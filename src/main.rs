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

#[derive(clap::Args)]
struct ModeArgs {
    /// Mode for the analysis engine: 'balanced' (default, tree-sitter) or 'boost' (raw, agentic loop)
    #[arg(long, value_enum)]
    mode: Option<crate::pet::identity::AnalysisEngine>,
    /// Sets the engine mode and saves it as the default for future calls
    #[arg(long, value_enum)]
    mode_default: Option<crate::pet::identity::AnalysisEngine>,
}

#[derive(Subcommand)]
enum Commands {
    /// Hatch your Rift pet (run once to initialize)
    Hatch {
        /// Skip downloading the massive Qwen3 Boost model
        #[arg(long)]
        no_boost: bool,
    },
    /// Analyze code at the given path with your pet's commentary
    Analyze {
        /// Path to analyze (file or directory)
        path: String,
        /// Maximum files to analyze (default: 50)
        #[arg(short, long, default_value = "50")]
        max_files: usize,
        #[command(flatten)]
        mode_args: ModeArgs,
    },
    /// Roast the user on every mistake they make
    Roast {
        /// Path to analyze (file or directory)
        path: String,
        /// Maximum files to analyze (default: 50)
        #[arg(short, long, default_value = "50")]
        max_files: usize,
        #[command(flatten)]
        mode_args: ModeArgs,
    },
    /// Ask the user interrogating questions about their code
    Grill {
        /// Path to analyze (file or directory)
        path: String,
        /// Maximum files to analyze (default: 50)
        #[arg(short, long, default_value = "50")]
        max_files: usize,
        #[command(flatten)]
        mode_args: ModeArgs,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load pet to check/update mode preferences
    let mut pet = match pet::storage::load_pet()? {
        Some(p) => p,
        None => {
            if !matches!(cli.command, Commands::Hatch { .. }) {
                anyhow::bail!("No Rift pet found! Run `rift hatch` first to hatch your companion.");
            }
            // Dummy pet for Hatch (will be overwritten during hatch)
            crate::pet::identity::derive_identity("dummy")
        }
    };

    let mut mode_args = None;
    match &cli.command {
        Commands::Analyze { mode_args: m, .. } |
        Commands::Roast { mode_args: m, .. } |
        Commands::Grill { mode_args: m, .. } => {
            mode_args = Some(m);
        }
        _ => {}
    }

    if let Some(m) = mode_args {
        if let Some(ref md) = m.mode_default {
            pet.engine_mode = md.clone();
            pet::storage::save_pet(&pet)?;
        }
    }

    let engine_mode = if let Some(m) = mode_args {
        m.mode.clone().unwrap_or_else(|| pet.engine_mode.clone())
    } else {
        pet.engine_mode.clone()
    };

    match cli.command {
        Commands::Hatch { no_boost } => {
            commands::hatch::run(no_boost).await?;
        }
        Commands::Analyze { path, max_files, .. } => {
            commands::analyze::run(&path, max_files, commands::analyze::AnalyzeMode::Analyze, engine_mode).await?;
        }
        Commands::Roast { path, max_files, .. } => {
            commands::analyze::run(&path, max_files, commands::analyze::AnalyzeMode::Roast, engine_mode).await?;
        }
        Commands::Grill { path, max_files, .. } => {
            commands::analyze::run(&path, max_files, commands::analyze::AnalyzeMode::Grill, engine_mode).await?;
        }
    }

    Ok(())
}
