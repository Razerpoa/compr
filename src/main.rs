use clap::{Parser, Subcommand};

mod classify;
mod entropy;
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
    Pack { input_dir: String, output: String },
    /// Unpack a compr archive into a directory
    Unpack { input: String, output_dir: String },
    /// List entries in a compr archive
    List { archive: String },
    /// Show archive information
    Info { archive: String },
    /// Verify all CRC32 checksums
    Verify { archive: String },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Pack { input_dir, output } => packer::pack(&input_dir, &output)?,
        Commands::Unpack { input, output_dir } => unpacker::unpack(&input, &output_dir)?,
        Commands::List { archive } => unpacker::list_entries(&archive)?,
        Commands::Info { archive } => unpacker::archive_info(&archive)?,
        Commands::Verify { archive } => unpacker::verify_archive(&archive)?,
    }
    Ok(())
}
