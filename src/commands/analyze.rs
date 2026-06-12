use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::mpsc;
use std::thread;

use crate::analysis::{markdown::build_summary, parser::parse_file, walker::walk_path};
use crate::llm::{
    runner::{build_prompt, spawn_llm},
};
use crate::pet::storage::{llama_cli_path, load_pet, model_path};
use crate::tui::output::run_output_tui;

pub async fn run(path: &str, max_files: usize) -> Result<()> {
    // ── Ensure pet is hatched ─────────────────────────────────────────────────
    let pet = load_pet()?.ok_or_else(|| {
        anyhow::anyhow!(
            "No Rift pet found! Run \x1b[36mrift hatch\x1b[0m first to hatch your companion."
        )
    })?;

    let root = Path::new(path);
    if !root.exists() {
        bail!("Path does not exist: {}", path);
    }

    let (status_tx, status_rx) = mpsc::channel::<String>();
    let (output_tx, output_rx) = mpsc::channel::<String>();

    let pet_clone = pet.clone();
    let path_owned = path.to_string();

    // ── Spawn analysis + LLM thread ───────────────────────────────────────────
    thread::spawn(move || {
        let result = analysis_thread(
            &pet_clone,
            &path_owned,
            max_files,
            status_tx.clone(),
            output_tx.clone(),
        );
        if let Err(e) = result {
            let _ = output_tx.send(format!("\n\x1b[31mError:\x1b[0m {e}"));
        }
        let _ = status_tx.send("__DONE__".to_string());
        let _ = output_tx.send("__DONE__".to_string());
    });

    // ── Run TUI on main thread ────────────────────────────────────────────────
    run_output_tui(&pet, status_rx, output_rx)?;

    Ok(())
}

fn analysis_thread(
    pet: &crate::pet::PetIdentity,
    path: &str,
    max_files: usize,
    status_tx: mpsc::Sender<String>,
    output_tx: mpsc::Sender<String>,
) -> Result<()> {
    let root = Path::new(path);

    // Step 1: Walk & parse
    let _ = status_tx.send(format!("Walking {}…", path));
    let source_files = walk_path(root, max_files)
        .with_context(|| format!("Failed to walk {path}"))?;

    if source_files.is_empty() {
        let _ = output_tx.send(format!(
            "No supported source files found in `{path}`.\n\
             Supported languages: Rust, Python, JS, TS, C, C++, Go, Java"
        ));
        return Ok(());
    }

    let _ = status_tx.send(format!(
        "Parsing {} files with Tree-sitter…",
        source_files.len()
    ));

    let mut parsed = Vec::new();
    for sf in &source_files {
        match parse_file(sf) {
            Ok(pf) => parsed.push(pf),
            Err(e) => {
                let _ = output_tx.send(format!("  ⚠ Skipping {}: {e}", sf.path));
            }
        }
    }

    // Step 2: Build Markdown summary
    let _ = status_tx.send("Building code summary…".to_string());
    let summary = build_summary(&parsed);

    // Step 3: Build prompt
    let prompt = build_prompt(pet, &summary);

    // Step 4: Spawn LLM
    let _ = status_tx.send(format!("Generating review with {}…", pet.name()));
    let llama_cli = llama_cli_path();
    let model = model_path();

    let llm = spawn_llm(&llama_cli, &model, &prompt)?;

    // Step 5: Stream output
    while let Ok(line) = llm.rx.recv() {
        let _ = output_tx.send(line);
    }

    Ok(())
}
