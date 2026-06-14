use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::mpsc as tokio_mpsc;

use crate::analysis::{
    markdown::{build_summary_from_refs, ParsedFileRef},
    parser::{parse_file, ParsedFile},
    walker::{walk_path, SourceFile},
};
use crate::llm::runner::{build_prompt, spawn_llm};
use crate::pet::storage::load_pet;
use crate::tui::output::{run_output_tui, TuiMsg};

/// One "Claw" handles one chunk of source files in a Tokio task.
/// Rayon is used inside for parallel Tree-sitter parsing within the chunk.
struct ClawResult {
    parsed: Vec<ParsedFile>,
}

#[derive(Clone, Copy)]
pub enum AnalyzeMode {
    Analyze,
    Roast,
    Grill,
}

pub async fn run(path: &str, max_files: usize, mode: AnalyzeMode, engine: crate::pet::identity::AnalysisEngine) -> Result<()> {
    // ── Ensure pet is hatched ────────────────────────────────────────────────
    let pet = load_pet()?.ok_or_else(|| {
        anyhow::anyhow!(
            "No Rift pet found! Run \x1b[36mrift hatch\x1b[0m first to hatch your companion."
        )
    })?;

    let root = Path::new(path);
    if !root.exists() {
        bail!("Path does not exist: {}", path);
    }

    // TUI channel: all worker messages go here
    let (tui_tx, tui_rx) = tokio_mpsc::unbounded_channel::<TuiMsg>();

    let pet_clone = pet.clone();
    let path_owned = path.to_string();

    // ── Spawn orchestrator task ──────────────────────────────────────────────
    let tui_tx_clone = tui_tx.clone();
    tokio::spawn(async move {
        let result =
            orchestrate_claws(&pet_clone, &path_owned, max_files, mode, engine, tui_tx_clone.clone()).await;
        if let Err(e) = result {
            let _ = tui_tx_clone.send(TuiMsg::Output(format!("\n\x1b[31mError:\x1b[0m {e}")));
        }
        let _ = tui_tx_clone.send(TuiMsg::Done);
    });

    // ── Run TUI on main thread (blocking) ───────────────────────────────────
    run_output_tui(&pet, tui_rx)?;

    Ok(())
}

/// Orchestrate all Claw workers asynchronously, merge results, build Markdown, call LLM.
async fn orchestrate_claws(
    pet: &crate::pet::PetIdentity,
    path: &str,
    max_files: usize,
    mode: AnalyzeMode,
    engine: crate::pet::identity::AnalysisEngine,
    tui_tx: tokio_mpsc::UnboundedSender<TuiMsg>,
) -> Result<()> {
    match engine {
        crate::pet::identity::AnalysisEngine::Balanced => {
            orchestrate_balanced(pet, path, max_files, mode, tui_tx).await
        }
        crate::pet::identity::AnalysisEngine::Boost => {
            orchestrate_boost(pet, path, max_files, mode, tui_tx).await
        }
    }
}

async fn orchestrate_boost(
    pet: &crate::pet::PetIdentity,
    path: &str,
    max_files: usize,
    mode: AnalyzeMode,
    tui_tx: tokio_mpsc::UnboundedSender<TuiMsg>,
) -> Result<()> {
    crate::llm::runner::run_agent_loop(pet, path, max_files, mode, tui_tx).await
}

async fn orchestrate_balanced(
    pet: &crate::pet::PetIdentity,
    path: &str,
    max_files: usize,
    mode: AnalyzeMode,
    tui_tx: tokio_mpsc::UnboundedSender<TuiMsg>,
) -> Result<()> {
    let root = Path::new(path);

    // Step 1: Walk (synchronous, fast)
    let _ = tui_tx.send(TuiMsg::Status(format!("Walking {}…", path)));
    let source_files: Vec<SourceFile> =
        walk_path(root, max_files).with_context(|| format!("Failed to walk {path}"))?;

    if source_files.is_empty() {
        let _ = tui_tx.send(TuiMsg::Output(format!(
            "No supported source files found in `{path}`.\n\
             Supported languages: Rust, Python, JS, TS, C, C++, Go, Java"
        )));
        return Ok(());
    }

    // Step 2: Chunk files — one Claw per file (up to 16 concurrent claws)
    let chunks: Vec<Vec<SourceFile>> = chunk_files(source_files, 16);
    let total_claws = chunks.len();

    let _ = tui_tx.send(TuiMsg::Status(format!(
        "Spawning {} Claws for {} files…",
        total_claws,
        chunks.iter().map(|c| c.len()).sum::<usize>()
    )));
    let _ = tui_tx.send(TuiMsg::ClawCount(total_claws));

    // Active claw counter (shared across tasks)
    let active_claws = Arc::new(AtomicUsize::new(total_claws));

    // Step 3: Spawn a Tokio task per chunk ("Claw")
    let mut handles = Vec::with_capacity(total_claws);
    for chunk in chunks {
        let tui_tx2 = tui_tx.clone();
        let active = Arc::clone(&active_claws);

        let handle = tokio::spawn(async move {
            // Each Claw uses Rayon to parse its chunk in parallel
            let parsed: Vec<ParsedFile> = chunk
                .into_par_iter()
                .filter_map(|sf| {
                    match parse_file(&sf) {
                        Ok(pf) => Some(pf),
                        Err(e) => {
                            let _ = tui_tx2
                                .send(TuiMsg::Output(format!("  ⚠ Skipping {}: {e}", sf.path)));
                            None
                        }
                    }
                })
                .collect();

            // Claw finished — decrement active count and report
            let remaining = active.fetch_sub(1, Ordering::SeqCst) - 1;
            let _ = tui_tx2.send(TuiMsg::ClawCount(remaining));

            ClawResult { parsed }
        });
        handles.push(handle);
    }

    // Step 4: Collect all results
    let mut all_parsed: Vec<ParsedFile> = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(result) => all_parsed.extend(result.parsed),
            Err(e) => {
                let _ = tui_tx.send(TuiMsg::Output(format!("  ⚠ Claw task panicked: {e}")));
            }
        }
    }

    let _ = tui_tx.send(TuiMsg::ClawCount(0));

    // Step 5: Build Markdown summary with embedded S-expressions
    let _ = tui_tx.send(TuiMsg::Status("Building S-expression Markdown…".to_string()));
    let refs: Vec<ParsedFileRef<'_>> = all_parsed
        .iter()
        .map(|pf| ParsedFileRef {
            source_file: &pf.source_file,
            tree: &pf.tree,
        })
        .collect();
    let summary = build_summary_from_refs(&refs);

    // Step 6: Build prompt
    let prompt = build_prompt(pet, &summary, mode);

    // Step 7: Spawn LLM (blocking subprocess — run in blocking thread)
    let _ = tui_tx.send(TuiMsg::Status(format!(
        "Generating review with {}…",
        pet.name()
    )));
    let llama_cli = crate::pet::storage::llama_cli_path();
    let model = crate::pet::storage::gemma_model_path(Some(pet));

    let llm = tokio::task::spawn_blocking(move || spawn_llm(&llama_cli, &model, &prompt))
        .await
        .context("LLM spawn thread panicked")??;

    // Step 8: Stream output
    while let Ok(line) = llm.rx.recv() {
        let _ = tui_tx.send(TuiMsg::Output(line));
    }

    Ok(())
}

/// Split `files` into `max_chunks` chunks (one per Claw).
/// Each chunk contains at least 1 file; empty chunks are dropped.
fn chunk_files(files: Vec<SourceFile>, max_chunks: usize) -> Vec<Vec<SourceFile>> {
    if files.is_empty() {
        return vec![];
    }
    let n = files.len();
    let chunk_size = ((n + max_chunks - 1) / max_chunks).max(1);
    files
        .into_iter()
        .collect::<Vec<_>>()
        .chunks(chunk_size)
        .map(|c| c.to_vec())
        .collect()
}
