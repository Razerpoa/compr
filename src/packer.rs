use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use anyhow::{Context, Result};
use walkdir::WalkDir;
use crate::classify::{classify, EntryKind};
use crate::format::{ArchiveFooter, ArchiveHeader, Entry, MAGIC, MARKER_IMAGE, MARKER_VIDEO, VERSION};
use crate::image;

pub fn pack(input_dir: &str, output: &str) -> Result<()> {
    let input_path = Path::new(input_dir);
    if !input_path.is_dir() {
        anyhow::bail!("Error: '{}' is not a valid directory", input_dir);
    }

    let mut writer: Box<dyn Write> = if output == "-" {
        Box::new(BufWriter::new(io::stdout().lock()))
    } else {
        Box::new(BufWriter::new(
            fs::File::create(output).with_context(|| format!("Create '{}'", output))?
        ))
    };

    ArchiveHeader { magic: *MAGIC, version: VERSION, flags: 0 }
        .write(&mut writer)?;

    // Collect all entries, classify, then sort for folder-grouped ordering.
    // Sort order: parent directory → image before video → filename.
    // This places similar planar RGB data contiguously for ZSTD in Phase 3.
    let mut file_entries: Vec<(walkdir::DirEntry, EntryKind)> = Vec::new();
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
                if let Some(rel_str) = rel.to_str() {
                    eprintln!(" -> Skipping (unsupported): {}", rel_str);
                }
            }
        }
    }

    // Sort: parent directory path → kind (Image=0 before Video=1) → filename
    file_entries.sort_by(|(a, ak), (b, bk)| {
        a.path().parent().cmp(&b.path().parent())
            .then_with(|| (*ak).cmp(bk))
            .then_with(|| a.file_name().cmp(b.file_name()))
    });

    if file_entries.is_empty() {
        anyhow::bail!("Error: No supported files found in '{}'", input_dir);
    }

    let mut entry_count: u32 = 0;
    let mut total_bytes: u64 = 0;

    for (entry, kind) in &file_entries {
        let path = entry.path();
        let rel = path.strip_prefix(input_path).unwrap();
        let rel_str = rel.to_str()
            .with_context(|| format!("Non-UTF-8 path: {:?}", rel))?;

        match kind {
            EntryKind::Image => {
                let (w, h, planar) = image::load_planar(path)
                    .with_context(|| format!("Load image {:?}", path))?;
                let entry_data = Entry {
                    kind: MARKER_IMAGE,
                    path: rel_str.to_string(),
                    width: w,
                    height: h,
                    data: planar,
                };
                let written = entry_data.write(&mut writer)?;
                entry_count += 1;
                total_bytes += written;
                eprintln!(" -> Image: {} ({}x{})", rel_str, w, h);
            }
            EntryKind::Video => {
                let data = fs::read(path)
                    .with_context(|| format!("Read {:?}", path))?;
                let entry_data = Entry {
                    kind: MARKER_VIDEO,
                    path: rel_str.to_string(),
                    width: 0,
                    height: 0,
                    data,
                };
                let written = entry_data.write(&mut writer)?;
                entry_count += 1;
                total_bytes += written;
                eprintln!(" -> Video: {} ({} bytes)", rel_str, written);
            }
        }
    }

    ArchiveFooter {
        entry_count,
        crc32: ArchiveFooter::compute_crc32(entry_count),
    }.write(&mut writer)?;

    eprintln!("\nSummary: {} entries, {} bytes", entry_count, total_bytes);
    Ok(())
}
