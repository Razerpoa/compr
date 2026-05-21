use clap::Parser;
use clap::Subcommand;

mod classify;
mod compress;
mod entropy;
mod filter;
mod format;
mod image;
mod packer;
mod unpacker;

#[derive(Parser)]
#[command(name = "compr", version, about = "Streaming solid archive compression")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pack a directory into a compr archive
    Pack {
        input_dir: String,
        output: String,
        /// Enable max compression (--ultra, --long=31, all cores, no memory cap)
        #[arg(long)]
        max: bool,
        /// Memory budget for compression (e.g. 512, 1G, 256M)
        #[arg(long)]
        mem: Option<String>,
        /// ZSTD compression level (1-22, default 19)
        #[arg(long)]
        level: Option<i32>,
    },
    /// Unpack a compr archive into a directory
    Unpack { input: String, output_dir: String },
    /// List entries in a compr archive
    List { archive: String },
    /// Show archive information
    Info { archive: String },
    /// Verify all CRC32 checksums
    Verify { archive: String },
    /// Show Shannon entropy for each entry in an archive
    Entropy { archive: String },
}

fn parse_mem_budget(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty memory budget".to_string());
    }
    let suffix = s.chars().last().unwrap();
    if suffix.is_alphabetic() {
        let num_part = s[..s.len() - 1].trim();
        let val: f64 = num_part.parse().map_err(|_| format!("invalid number: {}", num_part))?;
        match suffix {
            'g' | 'G' => Ok((val * 1024.0) as u32),
            'm' | 'M' => Ok(val as u32),
            'k' | 'K' => Ok((val / 1024.0) as u32),
            _ => Err(format!("unknown suffix: '{}'", suffix)),
        }
    } else {
        let val: u32 = s.parse().map_err(|_| format!("invalid number: {}", s))?;
        Ok(val)
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Pack { input_dir, output, max, mem, level } => {
            let mut params = if max {
                compress::CompressParams::max()
            } else if let Some(mem_str) = mem {
                let mb = parse_mem_budget(&mem_str)
                    .map_err(|e| anyhow::anyhow!("Invalid --mem value: {}", e))?;
                compress::CompressParams::eco(mb)
            } else {
                compress::CompressParams::default()
            };
            if let Some(lvl) = level {
                params.level = lvl;
            }
            packer::pack(&input_dir, &output, &params)?
        }
        Commands::Unpack { input, output_dir } => unpacker::unpack(&input, &output_dir)?,
        Commands::List { archive } => unpacker::list_entries(&archive)?,
        Commands::Info { archive } => unpacker::archive_info(&archive)?,
        Commands::Verify { archive } => unpacker::verify_archive(&archive)?,
        Commands::Entropy { archive } => unpacker::archive_entropy(&archive)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mem_budget() {
        assert_eq!(parse_mem_budget("512").unwrap(), 512);
        assert_eq!(parse_mem_budget("1G").unwrap(), 1024);
        assert_eq!(parse_mem_budget("2g").unwrap(), 2048);
        assert_eq!(parse_mem_budget("256M").unwrap(), 256);
        assert_eq!(parse_mem_budget("128m").unwrap(), 128);
        assert_eq!(parse_mem_budget("1.5G").unwrap(), 1536);
        assert!(parse_mem_budget("abc").is_err());
        assert!(parse_mem_budget("1.5X").is_err());
    }
}
