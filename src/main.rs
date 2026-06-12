mod commands;
mod pet;
mod analysis;
mod llm;
mod tui;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "rift",
    version = env!("CARGO_PKG_VERSION"),
    about = "🦞 Rift — your desktop pet code reviewer",
    long_about = "Rift hatches a unique crustacean companion derived from your machine's identity,\nthen uses local AI to review your code with personality."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Hatch your Rift pet (run once to initialize)
    Hatch,
    /// Analyze code at the given path with your pet's commentary
    Analyze {
        /// Path to analyze (file or directory)
        path: String,
        /// Maximum files to analyze (default: 50)
        #[arg(short, long, default_value = "50")]
        max_files: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Hatch => {
            commands::hatch::run().await?;
        }
        Commands::Analyze { path, max_files } => {
            commands::analyze::run(&path, max_files).await?;
        }
    }

    Ok(())
}
