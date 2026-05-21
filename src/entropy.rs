use std::io::Read;
use anyhow::Result;

/// Compute Shannon entropy of a byte slice (bits per byte, 0.0–8.0).
pub fn shannon_entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let len = data.len() as f64;
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let mut entropy = 0.0;
    for &count in &counts {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// Print entropy info for all entries in a compr archive.
pub fn print_archive_entropy<R: Read>(reader: &mut R) -> Result<()> {
    use crate::format::{FOOTER_MARKER, MARKER_IMAGE, MARKER_VIDEO};

    let mut count = 0u32;
    let mut total_entropy = 0.0;

    loop {
        let mut kind = [0u8; 1];
        if reader.read_exact(&mut kind).is_err() {
            break;
        }
        if kind[0] == FOOTER_MARKER {
            break;
        }
        if kind[0] != MARKER_IMAGE && kind[0] != MARKER_VIDEO {
            anyhow::bail!("Bad kind 0x{:02x} at entry {count}", kind[0]);
        }

        let mut pl = [0u8; 2];
        reader.read_exact(&mut pl)?;
        let plen = u16::from_le_bytes(pl) as usize;
        let mut pb = vec![0u8; plen];
        reader.read_exact(&mut pb)?;
        let path = String::from_utf8(pb)
            .map_err(|_| anyhow::anyhow!("Invalid UTF-8 path at entry {count}"))?;

        let mut b4 = [0u8; 4];
        reader.read_exact(&mut b4)?; // width
        reader.read_exact(&mut b4)?; // height
        let mut b8 = [0u8; 8];
        reader.read_exact(&mut b8)?;
        let data_size = u64::from_le_bytes(b8) as usize;
        reader.read_exact(&mut b4)?; // CRC32

        // Read payload in chunks for entropy calculation
        let mut data = vec![0u8; data_size];
        reader.read_exact(&mut data)?;

        let ent = shannon_entropy(&data);
        let kind_str = if kind[0] == MARKER_IMAGE { "image" } else { "file" };
        println!("  {kind_str:5}  {path}  {ent:.4} bits/byte  ({data_size} bytes)");
        total_entropy += ent;
        count += 1;
    }

    if count > 0 {
        println!("\n  Average entropy: {:.4} bits/byte across {count} entries", total_entropy / count as f64);
    } else {
        println!("  (no entries)");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_uniform() {
        // All same byte = 0.0 entropy
        let data = vec![0x42u8; 1000];
        let e = shannon_entropy(&data);
        assert!(e.abs() < 1e-10, "uniform data should have 0 entropy, got {e}");
    }

    #[test]
    fn test_entropy_max() {
        // All 256 bytes equally distributed = 8.0 entropy
        let data: Vec<u8> = (0u8..=255).cycle().take(256 * 100).collect();
        let e = shannon_entropy(&data);
        assert!((e - 8.0).abs() < 0.01, "max entropy expected ~8.0, got {e}");
    }

    #[test]
    fn test_entropy_half() {
        // 50/50 split = 1.0 entropy
        let mut data = vec![0u8; 500];
        data.extend(vec![1u8; 500]);
        let e = shannon_entropy(&data);
        assert!((e - 1.0).abs() < 0.01, "50/50 bytes expected ~1.0, got {e}");
    }

    #[test]
    fn test_empty() {
        assert_eq!(shannon_entropy(&[]), 0.0);
    }
}
