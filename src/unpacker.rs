use std::fs;
use std::io::{self, BufReader, Read};
use std::path::Path;
use anyhow::{Context, Result};
use crate::compress;
use crate::format::{ArchiveHeader, Entry, FLAG_ZSTD, FOOTER_MARKER, MAGIC, MARKER_IMAGE, MARKER_VIDEO, is_path_traversal};

fn open_input(input: &str) -> Result<Box<dyn Read>> {
    if input == "-" {
        Ok(Box::new(BufReader::new(io::stdin().lock())))
    } else {
        Ok(Box::new(BufReader::new(
            fs::File::open(input).with_context(|| format!("Open '{}'", input))?
        )))
    }
}

fn open_and_decompress(input: &str) -> Result<(ArchiveHeader, Box<dyn Read>)> {
    let reader = open_input(input)?;
    let mut pb_reader = BufReader::new(reader);
    let header = ArchiveHeader::read(&mut pb_reader)
        .context("Invalid archive header")?;

    if header.version != crate::format::VERSION {
        anyhow::bail!("Unsupported archive version: {:#06x}", header.version);
    }

    let payload_reader: Box<dyn Read> = if header.flags & FLAG_ZSTD != 0 {
        compress::create_decompressor(pb_reader)?
    } else {
        Box::new(pb_reader)
    };

    Ok((header, payload_reader))
}

/// Read entries sequentially until FOOTER_MARKER, then validate footer.
/// Returns the number of entries extracted.
fn extract_all<R: Read>(reader: &mut R, output_norm: &Path) -> Result<u32> {
    let mut count = 0u32;

    loop {
        // Read kind byte (first byte of every entry OR footer marker)
        let mut kind_buf = [0u8; 1];
        if reader.read_exact(&mut kind_buf).is_err() {
            anyhow::bail!("Unexpected EOF while reading entry {count}");
        }
        let kind = kind_buf[0];

        if kind == FOOTER_MARKER {
            break; // Reached the footer
        }

        if kind != MARKER_IMAGE && kind != MARKER_VIDEO {
            anyhow::bail!("Unknown entry kind byte: 0x{kind:02x} at entry {count}");
        }

        // Read path length
        let mut pl_buf = [0u8; 2];
        reader.read_exact(&mut pl_buf)?;
        let path_len = u16::from_le_bytes(pl_buf) as usize;

        // Read path
        let mut path_bytes = vec![0u8; path_len];
        reader.read_exact(&mut path_bytes)?;
        let path = String::from_utf8(path_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path at entry {count}"))?;

        // Read width, height
        let mut b4 = [0u8; 4];
        reader.read_exact(&mut b4)?;
        let width = u32::from_le_bytes(b4);
        reader.read_exact(&mut b4)?;
        let height = u32::from_le_bytes(b4);

        // Read filter type
        let mut b1 = [0u8; 1];
        reader.read_exact(&mut b1)?;
        let filter_type = b1[0];

        // Read data size
        let mut b8 = [0u8; 8];
        reader.read_exact(&mut b8)?;
        let data_size = u64::from_le_bytes(b8) as usize;

        // Read stored CRC32
        reader.read_exact(&mut b4)?;
        let stored_crc = u32::from_le_bytes(b4);

        // Read payload
        let mut data = vec![0u8; data_size];
        reader.read_exact(&mut data)?;

        // Path traversal protection (CWE-22): reject any path with `..` components
        if is_path_traversal(&path) {
            anyhow::bail!("Path traversal blocked: '{path}'");
        }

        // Validate CRC32
        let entry = Entry { kind, path: path.clone(), width, height, filter_type, data };
        let computed = entry.calculate_crc32()?;
        if stored_crc != computed {
            anyhow::bail!("CRC32 mismatch for '{path}': stored {stored_crc:#x}, computed {computed:#x}");
        }
        let dest = output_norm.join(&path);

        if kind == MARKER_IMAGE && width > 0 && height > 0 {
            // De-planarize and save as PNG
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Create dir {:?}", parent))?;
            }
            let restored_data = match filter_type {
                1 => {
                    let plane_len = (width as usize) * (height as usize);
                    let mut data = Vec::with_capacity(entry.data.len());
                    for chunk in entry.data.chunks_exact(plane_len) {
                        data.extend(crate::filter::Filter::Delta.reverse(chunk, width as usize));
                    }
                    data
                }
                2 => {
                    let plane_len = (width as usize) * (height as usize);
                    let mut data = Vec::with_capacity(entry.data.len());
                    for chunk in entry.data.chunks_exact(plane_len) {
                        data.extend(crate::filter::Filter::Paeth.reverse(chunk, width as usize));
                    }
                    data
                }
                _ => entry.data.clone(),
            };
            crate::image::save_planar(&dest, width, height, &restored_data)
                .with_context(|| format!("Save image {:?}", dest))?;
        } else {
            // Video or raw file: write bytes directly
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("Create dir {:?}", parent))?;
            }
            fs::write(&dest, &entry.data)
                .with_context(|| format!("Write {:?}", dest))?;
        }

        count += 1;
        let kind_str = if kind == MARKER_IMAGE { "Image" } else { "File" };
        eprintln!(" -> {kind_str} {path} ({} bytes)", entry.data.len());
    }

    // Read and validate footer
    let mut eb = [0u8; 4];
    reader.read_exact(&mut eb)?;
    let footer_count = u32::from_le_bytes(eb);

    let cc = { let mut h = crc32fast::Hasher::new(); h.update(&eb); h.finalize() };
    reader.read_exact(&mut eb)?;
    let footer_crc = u32::from_le_bytes(eb);
    if footer_crc != cc {
        anyhow::bail!("Footer CRC32: stored {footer_crc:#x}, computed {cc:#x}");
    }

    reader.read_exact(&mut eb)?;
    if eb != *MAGIC {
        anyhow::bail!("Invalid ending magic: {eb:?}");
    }

    if footer_count != count {
        anyhow::bail!("Entry count mismatch: footer says {footer_count}, extracted {count}");
    }

    Ok(count)
}

pub fn unpack(input: &str, output_dir: &str) -> Result<()> {
    let (_header, mut reader) = open_and_decompress(input)?;

    let output_path = Path::new(output_dir);
    fs::create_dir_all(output_path)
        .with_context(|| format!("Create output dir '{output_dir}'"))?;
    let output_norm = output_path.canonicalize()
        .with_context(|| format!("Canonicalize '{output_dir}'"))?;

    let count = extract_all(&mut reader, &output_norm)?;
    eprintln!("\nUnpacked: {count} entries");
    Ok(())
}

pub fn list_entries(input: &str) -> Result<()> {
    let (_header, mut reader) = open_and_decompress(input)?;

    let mut count = 0u32;
    loop {
        let mut kind = [0u8; 1];
        if reader.read_exact(&mut kind).is_err() { break; }
        if kind[0] == FOOTER_MARKER { break; }
        if kind[0] != MARKER_IMAGE && kind[0] != MARKER_VIDEO {
            anyhow::bail!("Bad kind 0x{:02x}", kind[0]);
        }
        let mut pl = [0u8; 2];
        reader.read_exact(&mut pl)?;
        let plen = u16::from_le_bytes(pl) as usize;
        let mut pb = vec![0u8; plen];
        reader.read_exact(&mut pb)?;
        let path = String::from_utf8(pb).map_err(|_| anyhow::anyhow!("Bad path"))?;
        // Skip width, height, filter_type, data_size, CRC32, payload
        let mut b4 = [0u8; 4];
        reader.read_exact(&mut b4)?;
        reader.read_exact(&mut b4)?;
        let mut b1 = [0u8; 1];
        reader.read_exact(&mut b1)?;
        let mut b8 = [0u8; 8];
        reader.read_exact(&mut b8)?;
        let ds = u64::from_le_bytes(b8) as usize;
        reader.read_exact(&mut b4)?; // CRC32
        // Skip payload
        let mut skip = vec![0u8; ds.min(1024 * 1024)]; // skip in chunks
        let mut remaining = ds;
        while remaining > 0 {
            let chunk_size = remaining.min(skip.len());
            reader.read_exact(&mut skip[..chunk_size])?;
            remaining -= chunk_size;
        }
        let kind_str = if kind[0] == MARKER_IMAGE { "image" } else { "file" };
        println!("  {kind_str:5}  {path}  ({ds} bytes)");
        count += 1;
    }
    println!("\nTotal: {count} entries");
    Ok(())
}

pub fn archive_info(input: &str) -> Result<()> {
    let (_header, mut reader) = open_and_decompress(input)?;
    // Scan to the footer to get the official entry count
    loop {
        let mut kind = [0u8; 1];
        if reader.read_exact(&mut kind).is_err() { break; }
        if kind[0] == FOOTER_MARKER { break; }
        if kind[0] != MARKER_IMAGE && kind[0] != MARKER_VIDEO {
            anyhow::bail!("Bad kind 0x{:02x}", kind[0]);
        }
        let mut pl = [0u8; 2];
        reader.read_exact(&mut pl)?;
        let plen = u16::from_le_bytes(pl) as usize;
        let mut _pb = vec![0u8; plen];
        reader.read_exact(&mut _pb)?;
        let mut b4 = [0u8; 4];
        reader.read_exact(&mut b4)?; reader.read_exact(&mut b4)?; // w, h
        let mut b1 = [0u8; 1];
        reader.read_exact(&mut b1)?; // filter_type
        let mut b8 = [0u8; 8];
        reader.read_exact(&mut b8)?;
        let ds = u64::from_le_bytes(b8) as usize;
        reader.read_exact(&mut b4)?; // CRC32
        let mut remaining = ds;
        let mut skip = vec![0u8; 1024 * 1024];
        while remaining > 0 {
            let cs = remaining.min(skip.len());
            reader.read_exact(&mut skip[..cs])?;
            remaining -= cs;
        }
    }
    // Read footer entry count
    let mut eb = [0u8; 4];
    reader.read_exact(&mut eb)?;
    let count = u32::from_le_bytes(eb);
    println!("Archive: compr v0.1.0");
    println!("Entries:  {count}");
    println!("Format:   streaming solid archive");
    Ok(())
}

pub fn verify_archive(input: &str) -> Result<()> {
    let (_header, mut reader) = open_and_decompress(input)?;
    let mut count = 0u32;
    loop {
        let mut kind = [0u8; 1];
        if reader.read_exact(&mut kind).is_err() { break; }
        if kind[0] == FOOTER_MARKER { break; }
        if kind[0] != MARKER_IMAGE && kind[0] != MARKER_VIDEO {
            anyhow::bail!("Bad kind 0x{:02x}", kind[0]);
        }
        let mut pl = [0u8; 2];
        reader.read_exact(&mut pl)?;
        let plen = u16::from_le_bytes(pl) as usize;
        let mut pb = vec![0u8; plen];
        reader.read_exact(&mut pb)?;
        let path = String::from_utf8(pb).map_err(|_| anyhow::anyhow!("Bad path"))?;
        let mut b4 = [0u8; 4];
        reader.read_exact(&mut b4)?; let w = u32::from_le_bytes(b4);
        reader.read_exact(&mut b4)?; let h = u32::from_le_bytes(b4);
        let mut b1 = [0u8; 1];
        reader.read_exact(&mut b1)?; let filter_type = b1[0];
        let mut b8 = [0u8; 8];
        reader.read_exact(&mut b8)?; let ds = u64::from_le_bytes(b8) as usize;
        reader.read_exact(&mut b4)?; let sc = u32::from_le_bytes(b4);
        let mut data = vec![0u8; ds];
        reader.read_exact(&mut data)?;
        let entry = Entry { kind: kind[0], path: path.clone(), width: w, height: h, filter_type, data };
        let cc = entry.calculate_crc32()?;
        if sc != cc {
            anyhow::bail!("CRC32 FAIL: '{path}' — stored {sc:#x}, computed {cc:#x}");
        }
        count += 1;
        eprintln!(" OK  {path}");
    }
    // Validate footer
    let mut eb = [0u8; 4];
    reader.read_exact(&mut eb)?;
    let fc = u32::from_le_bytes(eb);
    let cc = { let mut h = crc32fast::Hasher::new(); h.update(&eb); h.finalize() };
    reader.read_exact(&mut eb)?;
    if u32::from_le_bytes(eb) != cc {
        anyhow::bail!("Footer CRC32 mismatch");
    }
    reader.read_exact(&mut eb)?;
    if eb != *MAGIC { anyhow::bail!("Bad ending magic"); }
    // Verify entry count matches footer
    if fc != count {
        anyhow::bail!("Entry count mismatch: footer says {fc}, verified {count}");
    }
    eprintln!("\nAll {count} entries valid, footer OK");
    Ok(())
}

pub fn archive_entropy(input: &str) -> Result<()> {
    let (_header, mut reader) = open_and_decompress(input)?;
    println!("Archive entropy (per entry):");
    crate::entropy::print_archive_entropy(&mut reader)
}
