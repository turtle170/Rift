use anyhow::{bail, Context, Result};
use std::path::Path;
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use tokio::sync::mpsc as tokio_mpsc;

use crate::llm::runner::build_prompt;
use crate::pet::storage::load_pet;
use crate::tui::output::{run_interactive_tui, TuiMsg};

/// Run the interactive TUI session.
pub async fn run(path: Option<String>) -> Result<()> {
    let pet = load_pet()?.ok_or_else(|| {
        anyhow::anyhow!("No Rift pet found! Run `rift hatch` first.")
    })?;

    let server_path = crate::pet::storage::llama_server_path();
    let model_path = crate::pet::storage::qwen3_model_path(Some(&pet));

    if !server_path.exists() {
        bail!("llama-server.exe not found. Run `rift hatch` first.");
    }
    if !model_path.exists() {
        bail!(
            "Qwen3 model not found at {}.\nRun `rift hatch` (without --no-boost) first.",
            model_path.display()
        );
    }

    // Load optional path context
    let context_summary = if let Some(ref p) = path {
        let root = Path::new(p);
        if !root.exists() {
            bail!("Path does not exist: {}", p);
        }
        let source_files = crate::analysis::walker::walk_path(root, 30).unwrap_or_default();
        let mut ctx = String::new();
        for f in &source_files {
            if let Ok(c) = std::fs::read_to_string(&f.path) {
                ctx.push_str(&format!("\n<file path=\"{}\">\n{}\n</file>\n", f.path, c));
            }
        }
        ctx
    } else {
        String::new()
    };

    let (tui_tx, tui_rx) = tokio_mpsc::unbounded_channel::<TuiMsg>();
    // Sync channel for user input from TUI -> async task
    let (input_tx, input_rx) = std_mpsc::sync_channel::<String>(16);
    let input_rx = Arc::new(Mutex::new(input_rx));

    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8) as u32;

    // Start llama-server
    let mut child = std::process::Command::new(&server_path)
        .args([
            "--model", &model_path.to_string_lossy(),
            "--ctx-size", "32768",
            "--cache-type-k", "q4_0",
            "--threads", &cpu_threads.to_string(),
            "--port", "8081",
            "--mmap",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("Failed to spawn llama-server.exe")?;

    let _ = tui_tx.send(TuiMsg::Status("Starting server…".into()));

    // Wait for server to boot
    tokio::time::sleep(tokio::time::Duration::from_secs(8)).await;

    let _ = tui_tx.send(TuiMsg::Status("Ready. Type a message and press Enter.".into()));
    let _ = tui_tx.send(TuiMsg::Done);

    let tui_tx2 = tui_tx.clone();
    let pet_clone = pet.clone();
    let context_summary_clone = context_summary.clone();

    // Spawn async chat handler
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let sys_prompt = format!(
            "{}\n\nYou are in interactive mode. The user will ask you questions about their code. Answer concisely and helpfully.\n{}",
            build_prompt(&pet_clone, "", crate::commands::analyze::AnalyzeMode::Analyze),
            if context_summary_clone.is_empty() { "No codebase context provided.".to_string() } else { format!("Codebase context:\n{}", context_summary_clone) }
        );

        let mut messages: Vec<serde_json::Value> = vec![
            serde_json::json!({ "role": "system", "content": sys_prompt })
        ];

        loop {
            // Block waiting for user input (use tokio blocking task)
            let rx = Arc::clone(&input_rx);
            let user_msg = match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                Ok(Ok(m)) => m,
                _ => break,
            };

            messages.push(serde_json::json!({ "role": "user", "content": user_msg }));

            let _ = tui_tx2.send(TuiMsg::Status("Thinking…".into()));

            let body = serde_json::json!({
                "model": "qwen3",
                "messages": messages,
                "temperature": 0.7,
                "stream": true,
            });

            let resp = match client.post("http://127.0.0.1:8081/v1/chat/completions")
                .json(&body)
                .send().await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tui_tx2.send(TuiMsg::Output(format!("Error: {e}")));
                    let _ = tui_tx2.send(TuiMsg::Done);
                    continue;
                }
            };

            let mut current_line = String::new();
            let mut buffer = String::new();
            let mut full_reply = String::new();

            let mut resp = resp;
            while let Some(chunk) = resp.chunk().await.unwrap_or(None) {
                let s = String::from_utf8_lossy(&chunk).to_string();
                buffer.push_str(&s);
                while let Some(idx) = buffer.find('\n') {
                    let line = buffer[..idx].trim().to_string();
                    buffer = buffer[idx+1..].to_string();
                    if line.starts_with("data: ") {
                        let data = &line[6..];
                        if data == "[DONE]" { break; }
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                                full_reply.push_str(delta);
                                for c in delta.chars() {
                                    if c == '\n' {
                                        let _ = tui_tx2.send(TuiMsg::Output(current_line.clone()));
                                        current_line.clear();
                                    } else {
                                        current_line.push(c);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !current_line.is_empty() {
                let _ = tui_tx2.send(TuiMsg::Output(current_line.clone()));
            }

            // Add assistant reply to history
            messages.push(serde_json::json!({ "role": "assistant", "content": full_reply }));
            let _ = tui_tx2.send(TuiMsg::Done);
        }

        let _ = child.kill();
    });

    // Run TUI on main thread
    run_interactive_tui(&pet, tui_rx, input_tx)?;
    Ok(())
}
