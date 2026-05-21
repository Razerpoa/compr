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
        /// Memory budget in MB for compression (overrides default ~512 MB cap)
        #[arg(long)]
        mem: Option<u32>,
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

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Pack { input_dir, output, max, mem, level } => {
            let mut params = if max {
                compress::CompressParams::max()
            } else if let Some(mb) = mem {
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
