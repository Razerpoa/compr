use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use anyhow::{Context, Result};
use walkdir::WalkDir;
use crate::classify::{classify, EntryKind};
use crate::compress::{self, CompressParams};
use crate::format::{ArchiveFooter, ArchiveHeader, Entry, FLAG_SREP, FLAG_ZSTD, MAGIC, MARKER_IMAGE, MARKER_SOLID_BLOCK, MARKER_VIDEO, VERSION};
use crate::image;
use crate::sort::{sort_entries, SortMode};

/// Format byte count as human-readable string (bytes, KB, MB, GB).
fn human_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let nf = n as f64;
    if nf >= GB {
        format!("{:.1} GB", nf / GB)
    } else if nf >= MB {
        format!("{:.1} MB", nf / MB)
    } else if nf >= KB {
        format!("{:.1} KB", nf / KB)
    } else {
        format!("{} bytes", n)
    }
}

pub fn pack(input_dir: &str, output: &str, params: &CompressParams, sort_mode: SortMode) -> Result<()> {
    let input_path = Path::new(input_dir);
    if !input_path.is_dir() {
        anyhow::bail!("Error: '{}' is not a valid directory", input_dir);
    }

    eprintln!("Packing: {} → {}", input_dir, output);
    eprintln!("Phase: scanning {} ...", input_dir);

    let mut raw_writer: Box<dyn Write> = if output == "-" {
        Box::new(BufWriter::new(io::stdout().lock()))
    } else {
        Box::new(BufWriter::new(
            fs::File::create(output).with_context(|| format!("Create '{}'", output))?
        ))
    };

    // Collect all entries, classify, then sort for folder-grouped ordering.
    // Sort order: parent directory → image before video → filename.
    // This places similar planar RGB data contiguously for ZSTD.
    let mut file_entries: Vec<(walkdir::DirEntry, EntryKind)> = Vec::new();
    let mut skipped = 0u32;
    for entry in WalkDir::new(input_path).sort_by(|a, b| a.file_name().cmp(b.file_name())) {
        let entry = entry?;
        if !entry.file_type().is_file() { continue; }
        let path = entry.path();
        let rel = path.strip_prefix(input_path)
            .with_context(|| format!("Strip prefix for {:?}", path))?;
        if rel.as_os_str().is_empty() { continue; }

        match classify(path) {
            Some(EntryKind::Image) => file_entries.push((entry, EntryKind::Image)),
            Some(EntryKind::Video) => file_entries.push((entry, EntryKind::Video)),
            None => {
                skipped += 1;
                if let Some(rel_str) = rel.to_str() {
                    eprintln!(" -> Skipping (unsupported): {}", rel_str);
                }
            }
        }
    }

    let image_count = file_entries.iter().filter(|(_, k)| *k == EntryKind::Image).count();
    let video_count = file_entries.iter().filter(|(_, k)| *k == EntryKind::Video).count();
    eprintln!("Phase: found {} images and {} videos ({} skipped)", image_count, video_count, skipped);

    if file_entries.is_empty() {
        anyhow::bail!("Error: No supported files found in '{}'", input_dir);
    }

    // Sort entries according to the chosen strategy.
    let sort_label = match sort_mode {
        SortMode::Folder => "folder-grouped (images before videos)",
        SortMode::Color  => "color (dominant hue → saturation → value)",
    };
    eprintln!("Phase: sorting entries by {} ...", sort_label);
    sort_entries(&mut file_entries, sort_mode);

    // Write header (always uncompressed so readers can detect compression flag)
    eprintln!("Phase: writing archive header ...");
    let mut flags = FLAG_ZSTD;
    if params.srep {
        flags |= FLAG_SREP;
    }
    ArchiveHeader { magic: *MAGIC, version: VERSION, flags }
        .write(&mut raw_writer)?;

    let mut entry_count: u32 = 0;
    let mut raw_input_bytes: u64 = 0;

    // Phase 1: Write uncompressed video entries
    let video_entries: Vec<&(walkdir::DirEntry, EntryKind)> = file_entries.iter()
        .filter(|(_, k)| *k == EntryKind::Video).collect();

    if !video_entries.is_empty() {
        eprintln!("Phase: writing {} videos (uncompressed) ...", video_entries.len());
        for (entry, _) in &video_entries {
            let path = entry.path();
            let rel = path.strip_prefix(input_path).unwrap();
            let rel_str = rel.to_str()
                .with_context(|| format!("Non-UTF-8 path: {:?}", rel))?;

            let data = fs::read(path)
                .with_context(|| format!("Read {:?}", path))?;
            let data_len = data.len();
            raw_input_bytes += data_len as u64;
            let entry_data = Entry {
                kind: MARKER_VIDEO,
                path: rel_str.to_string(),
                width: 0,
                height: 0,
                filter_type: 0,
                data,
            };
            entry_data.write(&mut raw_writer)?;
            entry_count += 1;
            eprintln!(" -> Video: {} ({} bytes)", rel_str, data_len);
        }
    }

    let image_entries: Vec<&(walkdir::DirEntry, EntryKind)> = file_entries.iter()
        .filter(|(_, k)| *k == EntryKind::Image).collect();

    if !image_entries.is_empty() {
        // Phase 2: Start compressed solid block for images
        raw_writer.write_all(&[MARKER_SOLID_BLOCK])?;
        raw_writer.flush()?;

        let thread_str = if params.threads == 0 { "auto".to_string() } else { params.threads.to_string() };
        let srep_str = if params.srep { "+SREP" } else { "" };
        eprintln!("Phase: compressing {} images (ZSTD{} level={}, window=2^{}MiB, LDM={}, threads={}) ...",
            image_entries.len(), srep_str, params.level, params.window_log, params.ldm, thread_str,
        );

        {
            let mut writer = compress::create_compressor(raw_writer, params)?;

            for (entry, _) in &image_entries {
            let path = entry.path();
            let rel = path.strip_prefix(input_path).unwrap();
            let rel_str = rel.to_str()
                .with_context(|| format!("Non-UTF-8 path: {:?}", rel))?;

            let (w, h, planar) = image::load_planar(path)
                .with_context(|| format!("Load image {:?}", path))?;
            let plane_len = (w as usize) * (h as usize);
            let mut filtered = Vec::with_capacity(planar.len());
            for chunk in planar.chunks_exact(plane_len) {
                filtered.extend(crate::filter::Filter::Paeth.apply(chunk, w as usize));
            }
            let entry_data = Entry {
                kind: MARKER_IMAGE,
                path: rel_str.to_string(),
                width: w,
                height: h,
                filter_type: 2,
                data: filtered,
            };
            let _written = entry_data.write(&mut writer)?;
            entry_count += 1;
            raw_input_bytes += (w as u64) * (h as u64) * 3;
            eprintln!(" -> Image: {} ({}x{})", rel_str, w, h);
        }

            eprintln!("Phase: writing footer ...");
            ArchiveFooter {
                entry_count,
                crc32: ArchiveFooter::compute_crc32(entry_count),
            }.write(&mut writer)?;

            // Flush compressor by dropping the writer
        }
    } else {
        // No images, but we still need a footer for the archive to be valid
        eprintln!("Phase: writing footer (uncompressed) ...");
        ArchiveFooter {
            entry_count,
            crc32: ArchiveFooter::compute_crc32(entry_count),
        }.write(&mut raw_writer)?;
    }

    // For file output, we can check the compressed size.
    if output != "-" {
        let meta = fs::metadata(output)?;
        let compressed_size = meta.len();
        let ratio = if raw_input_bytes > 0 {
            (compressed_size as f64 / raw_input_bytes as f64) * 100.0
        } else {
            0.0
        };
        eprintln!("\nDone: {} entries | raw: {} → archive: {} ({:.1}%)",
            entry_count, human_bytes(raw_input_bytes), human_bytes(compressed_size), ratio);
    } else {
        eprintln!("\nDone: {} entries, {} raw (stdout stream)", entry_count, human_bytes(raw_input_bytes));
    }
    Ok(())
}
