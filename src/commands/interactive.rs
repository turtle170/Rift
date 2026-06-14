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

    let model_path = crate::pet::storage::qwen3_model_path(Some(&pet));
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

    let _ = tui_tx.send(TuiMsg::Status("Starting native engine…".into()));

    let tui_tx2 = tui_tx.clone();
    let pet_clone = pet.clone();
    let context_summary_clone = context_summary.clone();

    // Spawn async chat handler
    tokio::spawn(async move {
        let sys_prompt = format!(
            "{}\n\nYou are in interactive mode. The user will ask you questions about their code. Answer concisely and helpfully.\n{}",
            build_prompt(&pet_clone, "", crate::commands::analyze::AnalyzeMode::Analyze),
            if context_summary_clone.is_empty() { "No codebase context provided.".to_string() } else { format!("Codebase context:\n{}", context_summary_clone) }
        );

        let mut conversation_history = String::new();
        conversation_history.push_str(&format!("<|im_start|>system\n{}<|im_end|>\n", sys_prompt));

        let _ = tui_tx2.send(TuiMsg::Status("Ready. Type a message and press Enter.".into()));
        let _ = tui_tx2.send(TuiMsg::Done);

        loop {
            // Block waiting for user input
            let rx = Arc::clone(&input_rx);
            let user_msg = match tokio::task::spawn_blocking(move || rx.lock().unwrap().recv()).await {
                Ok(Ok(m)) => m,
                _ => break,
            };

            let _ = tui_tx2.send(TuiMsg::Status("Thinking…".into()));

            conversation_history.push_str(&format!("<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", user_msg));

            let prompt_copy = conversation_history.clone();
            let (tx_inner, rx_inner) = std_mpsc::channel::<String>();
            let model_path_buf = model_path.to_path_buf();
            
            // Spawn a blocking thread to run the native LLM
            let llm_handle = std::thread::spawn(move || {
                if let Err(e) = run_inference_interactive(&model_path_buf, &prompt_copy, tx_inner) {
                    println!("Error: {}", e);
                }
            });

            let mut current_line = String::new();
            let mut full_reply = String::new();

            for chunk in rx_inner {
                full_reply.push_str(&chunk);
                for c in chunk.chars() {
                    if c == '\n' {
                        let _ = tui_tx2.send(TuiMsg::Output(current_line.clone()));
                        current_line.clear();
                    } else {
                        current_line.push(c);
                    }
                }
            }
            if !current_line.is_empty() {
                let _ = tui_tx2.send(TuiMsg::Output(current_line));
            }

            conversation_history.push_str(&full_reply);
            conversation_history.push_str("<|im_end|>\n");
            
            let _ = llm_handle.join();
            let _ = tui_tx2.send(TuiMsg::Done);
        }
    });

    // Run TUI on main thread
    run_interactive_tui(&pet, tui_rx, input_tx)?;
    Ok(())
}

fn run_inference_interactive(model_path: &Path, prompt: &str, tx: std_mpsc::Sender<String>) -> Result<()> {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::LlamaModel;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::context::params::LlamaContextParams;
    use std::num::NonZeroU32;

    let backend = LlamaBackend::init()?;
    let model_params = LlamaModelParams::default();
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)?;

    let mut ctx_params = LlamaContextParams::default();
    ctx_params = ctx_params.with_n_ctx(Some(NonZeroU32::new(8192).unwrap()));
    let mut ctx = model.new_context(&backend, ctx_params)?;

    let tokens = model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)?;

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(8192, 1);
    let last_index = tokens.len().saturating_sub(1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == last_index;
        batch.add(token, i as i32, &[0], is_last).unwrap();
    }

    ctx.decode(&mut batch)?;

    let mut n_cur = tokens.len() as i32;

    loop {
        let mut candidates = ctx.token_data_array_ith(batch.n_tokens() - 1);
        let id_token = candidates.sample_token_greedy();

        if id_token == model.token_eos() || n_cur > 8192 {
            break;
        }

        let token_bytes = model.token_to_bytes(id_token, llama_cpp_2::model::Special::Tokenize)
            .unwrap_or_default();
        let token_str = String::from_utf8_lossy(&token_bytes);

        if tx.send(token_str.to_string()).is_err() {
            break;
        }

        batch.clear();
        batch.add(id_token, n_cur, &[0], true).unwrap();
        ctx.decode(&mut batch)?;
        n_cur += 1;
    }

    Ok(())
}
