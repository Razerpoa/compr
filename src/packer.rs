use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use anyhow::{Context, Result};
use walkdir::WalkDir;
use crate::format::{ArchiveFooter, ArchiveHeader, Entry, MAGIC, MARKER_VIDEO, VERSION};

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

    let mut entry_count: u32 = 0;
    let mut total_bytes: u64 = 0;

    // walkdir with sort_by gives deterministic depth-first traversal,
    // naturally grouping files by folder.
    for entry in WalkDir::new(input_path).sort_by(|a, b| a.file_name().cmp(b.file_name())) {
        let entry = entry?;
        if !entry.file_type().is_file() { continue; }

        let path = entry.path();
        let rel = path.strip_prefix(input_path)
            .with_context(|| format!("Strip prefix for {:?}", path))?;
        let rel_str = rel.to_str()
            .with_context(|| format!("Non-UTF-8 path: {:?}", rel))?;
        if rel_str.is_empty() { continue; }

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
        eprintln!(" -> {} ({} bytes)", rel_str, written);
    }

    if entry_count == 0 {
        anyhow::bail!("Error: No files found in '{}'", input_dir);
    }

    ArchiveFooter {
        entry_count,
        crc32: ArchiveFooter::compute_crc32(entry_count),
    }.write(&mut writer)?;

    eprintln!("\nSummary: {} entries, {} bytes", entry_count, total_bytes);
    Ok(())
}
