use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use anyhow::{Context, Result};
use walkdir::WalkDir;
use crate::classify::{classify, EntryKind};
use crate::compress::{self, CompressParams};
use crate::format::{ArchiveFooter, ArchiveHeader, Entry, FLAG_ZSTD, MAGIC, MARKER_IMAGE, MARKER_VIDEO, VERSION};
use crate::image;

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

pub fn pack(input_dir: &str, output: &str, params: &CompressParams) -> Result<()> {
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

    // Sort: parent directory path → kind (Image=0 before Video=1) → filename
    eprintln!("Phase: sorting entries by folder (images before videos) ...");
    file_entries.sort_by(|(a, ak), (b, bk)| {
        a.path().parent().cmp(&b.path().parent())
            .then_with(|| (*ak).cmp(bk))
            .then_with(|| a.file_name().cmp(b.file_name()))
    });

    // Write header (always uncompressed so readers can detect compression flag)
    eprintln!("Phase: writing archive header ...");
    ArchiveHeader { magic: *MAGIC, version: VERSION, flags: FLAG_ZSTD }
        .write(&mut raw_writer)?;

    // Wrap with ZSTD compressor — entries + footer will be compressed
    let thread_str = if params.threads == 0 { "auto".to_string() } else { params.threads.to_string() };
    eprintln!("Phase: compressing {} entries (level={}, window=2^{}MiB, LDM={}, threads={}) ...",
        file_entries.len(), params.level, params.window_log, params.ldm, thread_str,
    );
    let mut writer = compress::create_compressor(raw_writer, params)?;

    let mut entry_count: u32 = 0;
    let mut raw_input_bytes: u64 = 0;

    for (entry, kind) in &file_entries {
        let path = entry.path();
        let rel = path.strip_prefix(input_path).unwrap();
        let rel_str = rel.to_str()
            .with_context(|| format!("Non-UTF-8 path: {:?}", rel))?;

        match kind {
            EntryKind::Image => {
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
            EntryKind::Video => {
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
                let _written = entry_data.write(&mut writer)?;
                entry_count += 1;
                eprintln!(" -> Video: {} ({} bytes)", rel_str, data_len);
            }
        }
    }

    eprintln!("Phase: writing footer ...");
    ArchiveFooter {
        entry_count,
        crc32: ArchiveFooter::compute_crc32(entry_count),
    }.write(&mut writer)?;

    // Flush ZSTD encoder by dropping the writer (triggers auto_finish)
    drop(writer);

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
