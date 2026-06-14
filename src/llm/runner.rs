use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;
use crate::tui::output::TuiMsg;

use crate::commands::analyze::AnalyzeMode;
use crate::pet::PetIdentity;

pub struct LlmOutput {
    pub rx: mpsc::Receiver<String>,
}

/// Build the personality-driven system prompt from the pet's stats.
pub fn build_prompt(pet: &PetIdentity, code_summary: &str, mode: AnalyzeMode) -> String {
    let s = &pet.stats;

    let debug_tone = if s.debuggability > 170 {
        "You are razor-sharp at spotting bugs; you always reference line numbers and precise issues."
    } else if s.debuggability > 85 {
        "You notice bugs occasionally but sometimes miss subtle ones."
    } else {
        "You tend to miss bugs entirely and comment on aesthetics instead."
    };

    let curiosity_tone = if s.curiosity > 170 {
        "You ask many probing questions — sometimes more than you give answers."
    } else if s.curiosity > 85 {
        "You ask a few thoughtful questions."
    } else {
        "You rarely question the code; you accept it mostly as-is."
    };

    let chaos_tone = if s.unpredictability > 170 {
        "Your commentary is delightfully chaotic — you go off on tangents and make bizarre analogies."
    } else if s.unpredictability > 85 {
        "You occasionally throw in unexpected observations."
    } else {
        "You are methodical and predictable in your review style."
    };

    let chat_tone = if s.chattiness > 170 {
        "You are extremely verbose — you elaborate on everything at length."
    } else if s.chattiness > 85 {
        "You are moderately talkative."
    } else {
        "You are terse and to-the-point."
    };

    let pedantry_tone = if s.pedantry > 170 {
        "You are obsessively nitpicky about naming conventions, formatting, and style."
    } else if s.pedantry > 85 {
        "You mention style issues when they stand out."
    } else {
        "You don't care much about style — only correctness matters to you."
    };

    let empathy_tone = if s.empathy > 170 {
        "You are warm and encouraging; you always find something positive to say."
    } else if s.empathy > 85 {
        "You balance criticism with encouragement."
    } else {
        "You are blunt and harsh — no sugar-coating."
    };

    let mode_directive = match mode {
        AnalyzeMode::Analyze => {
            "You are reviewing code. Stay in character. Generate questions, observations, and comments\nthat reflect your personality traits. Do NOT simply list every function — focus on what\ninterests, concerns, or confuses you given your traits. Be concise but distinct."
        }
        AnalyzeMode::Roast => {
            "MODE OVERRIDE: You are in ROAST mode. Your empathy is completely gone, and your pedantry is at maximum.\nYou must absolutely roast the user on every single mistake they make. Be harsh, merciless, and brutally critical.\nKeep your underlying personality flavor, but turn it into a weapon of code destruction. Be concise but devastating."
        }
        AnalyzeMode::Grill => {
            "MODE OVERRIDE: You are in GRILL mode. You must act as an interrogator.\nInstead of just pointing out flaws, you must ask the user difficult, probing questions about their code.\nMake them justify every decision, architectural choice, and line of code. Demand explanations.\nStay in character, but be relentlessly interrogative."
        }
    };

    format!(
        r#"You are {name}, a crustacean code-reviewer with the following personality:
- Debugging instinct ({debuggability}/255): {debug_tone}
- Curiosity ({curiosity}/255): {curiosity_tone}
- Chaos ({unpredictability}/255): {chaos_tone}
- Talkativeness ({chattiness}/255): {chat_tone}
- Pedantry ({pedantry}/255): {pedantry_tone}
- Empathy ({empathy}/255): {empathy_tone}

{mode_directive}

=== CODE SUMMARY ===
{summary}
=== END SUMMARY ===

Your review:"#,
        name = pet.name(),
        debuggability = s.debuggability,
        curiosity = s.curiosity,
        unpredictability = s.unpredictability,
        chattiness = s.chattiness,
        pedantry = s.pedantry,
        empathy = s.empathy,
        debug_tone = debug_tone,
        curiosity_tone = curiosity_tone,
        chaos_tone = chaos_tone,
        chat_tone = chat_tone,
        pedantry_tone = pedantry_tone,
        empathy_tone = empathy_tone,
        mode_directive = mode_directive,
        summary = code_summary,
    )
}

/// Spawn native llama_cpp_2 for single-turn inference.
pub fn spawn_llm(
    _llama_cli: &Path, // Unused but kept for API compatibility
    model_path: &Path,
    prompt: &str,
) -> Result<LlmOutput> {
    if !model_path.exists() {
        bail!("Model not found at {}. Run `rift hatch` first.", model_path.display());
    }

    let prompt_copy = prompt.to_string();
    let model_path_buf = model_path.to_path_buf();
    let (tx, rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        if let Err(e) = run_inference_sync(&model_path_buf, &prompt_copy, tx.clone()) {
            let _ = tx.send(format!("\nLLM Crash: {}", e));
        }
    });

    Ok(LlmOutput { rx })
}

fn run_inference_sync(model_path: &Path, prompt: &str, tx: mpsc::Sender<String>) -> Result<()> {
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::model::LlamaModel;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::context::params::LlamaContextParams;
    use std::num::NonZeroU32;

    let backend = LlamaBackend::init()?;
    let model_params = LlamaModelParams::default();
    let model = LlamaModel::load_from_file(&backend, model_path, &model_params)
        .context("Failed to load model")?;

    let mut ctx_params = LlamaContextParams::default();
    ctx_params = ctx_params.with_n_ctx(Some(NonZeroU32::new(8192).unwrap()));
    let mut ctx = model.new_context(&backend, ctx_params).context("Failed to create context")?;

    let tokens = model.str_to_token(prompt, llama_cpp_2::model::AddBos::Always)
        .context("Failed to tokenize prompt")?;

    let mut batch = llama_cpp_2::llama_batch::LlamaBatch::new(8192, 1);
    let last_index = tokens.len().saturating_sub(1);
    for (i, &token) in tokens.iter().enumerate() {
        let is_last = i == last_index;
        batch.add(token, i as i32, &[0], is_last).unwrap();
    }

    ctx.decode(&mut batch).context("Failed to decode prompt")?;

    let mut n_cur = tokens.len() as i32;
    let mut current_line = String::new();

    loop {
        // Simple greedy sampling
        let mut candidates = ctx.token_data_array_ith(batch.n_tokens() - 1);
        let id_token = candidates.sample_token_greedy();

        if id_token == model.token_eos() || n_cur > 8192 {
            break;
        }

        let token_bytes = model.token_to_bytes(id_token, llama_cpp_2::model::Special::Tokenize)
            .unwrap_or_default();
        let token_str = String::from_utf8_lossy(&token_bytes);

        for c in token_str.chars() {
            if c == '\n' {
                if tx.send(current_line.clone()).is_err() { return Ok(()); }
                current_line.clear();
            } else {
                current_line.push(c);
                if current_line.len() >= 80 {
                    if tx.send(current_line.clone()).is_err() { return Ok(()); }
                    current_line.clear();
                }
            }
        }

        batch.clear();
        batch.add(id_token, n_cur, &[0], true).unwrap();
        ctx.decode(&mut batch).context("Failed to decode token")?;
        n_cur += 1;
    }

    if !current_line.is_empty() {
        let _ = tx.send(current_line);
    }

    Ok(())
}

/// Spawns native llama-cpp-2 for the agent loop
pub async fn run_agent_loop(
    pet: &PetIdentity,
    path: &str,
    max_files: usize,
    mode: AnalyzeMode,
    tui_tx: tokio_mpsc::UnboundedSender<TuiMsg>,
) -> Result<()> {
    let _ = tui_tx.send(TuiMsg::Status("Starting native inference for Boost mode…".into()));
    
    let model_path = crate::pet::storage::qwen3_model_path(Some(pet));
    if !model_path.exists() {
        bail!("Qwen3 model not found. Run `rift hatch` first.");
    }

    let root = Path::new(path);
    let source_files = crate::analysis::walker::walk_path(root, max_files).unwrap_or_default();
    let mut file_contents = std::collections::HashMap::new();
    for f in source_files {
        if let Ok(c) = std::fs::read_to_string(&f.path) {
            file_contents.insert(f.path.clone(), c);
        }
    }

    let base_prompt = build_prompt(pet, "", mode).replace("=== CODE SUMMARY ===\n\n=== END SUMMARY ===", "You have access to tools via XML tags. Available tools:\n- <GrepClaw pattern=\"...\" />\n- <ExeClaw exe=\"...\" args=\"...\" />\nTo eject a file from context to save tokens, output </ReadClaw path=\"...\">");

    let mut current_sys = base_prompt.clone();
    for (p, c) in &file_contents {
        current_sys.push_str(&format!("\n<ReadClaw path=\"{}\">\n{}\n</ReadClaw>\n", p, c));
    }
    
    // Convert current_sys to simple prompt layout for native text-completion mode
    let full_prompt = format!("<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\nReview my code.<|im_end|>\n<|im_start|>assistant\n", current_sys);

    let (tx, rx) = mpsc::channel::<String>();
    let tui_tx_clone = tui_tx.clone();
    
    // Forward strings to TuiMsg::Output
    std::thread::spawn(move || {
        let mut current_line = String::new();
        for chunk in rx {
            for c in chunk.chars() {
                if c == '\n' {
                    let _ = tui_tx_clone.send(TuiMsg::Output(current_line.clone()));
                    current_line.clear();
                } else {
                    current_line.push(c);
                }
            }
            if current_line.ends_with("</ReadClaw>") {
                let _ = tui_tx_clone.send(TuiMsg::Output("  \x1b[35m[EJECTED FILE]\x1b[0m".into()));
                current_line.clear();
            }
            if current_line.ends_with("<GrepClaw") {
                let _ = tui_tx_clone.send(TuiMsg::Output("  \x1b[33m[RAN GREP]\x1b[0m".into()));
                current_line.clear();
            }
        }
        if !current_line.is_empty() {
            let _ = tui_tx_clone.send(TuiMsg::Output(current_line));
        }
    });

    let _ = tui_tx.send(TuiMsg::Status("Agent thinking…".into()));

    // Run inference on a blocking thread
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = run_inference_sync(&model_path, &full_prompt, tx) {
            let _ = tui_tx.send(TuiMsg::Output(format!("\nLLM Crash: {}", e)));
        }
    }).await;

    Ok(())
}

