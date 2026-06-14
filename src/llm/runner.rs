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

/// Spawn llama-cli.exe and stream the output line-by-line via a channel.
/// Returns an `LlmOutput` whose `rx` channel receives lines as they are generated.
pub fn spawn_llm(
    llama_cli: &Path,
    model: &Path,
    prompt: &str,
) -> Result<LlmOutput> {
    if !llama_cli.exists() {
        bail!(
            "llama-cli.exe not found at {}. Run `rift hatch` first.",
            llama_cli.display()
        );
    }
    if !model.exists() {
        bail!(
            "Model not found at {}. Run `rift hatch` first.",
            model.display()
        );
    }

    let cpu_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8) as u32;

    let mut child = Command::new(llama_cli)
        .args([
            "--model",
            &model.to_string_lossy(),
            "--ctx-size",
            "8192",
            "--ctk",
            "q4_0",
            "--threads",
            &cpu_threads.to_string(),
            "--temp",
            "0.8",
            "--repeat-penalty",
            "1.1",
            "--no-display-prompt",
            "-p",
            prompt,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) // suppress llama.cpp verbose logs
        .spawn()
        .context("Failed to spawn llama-cli.exe")?;

    let stdout = child.stdout.take().context("No stdout")?;
    let (tx, rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if tx.send(l).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // Wait for child process to finish
        let _ = child.wait();
    });

    Ok(LlmOutput { rx })
}

/// Spawns llama-server.exe and runs the agent loop.
pub async fn run_agent_loop(
    pet: &PetIdentity,
    path: &str,
    max_files: usize,
    mode: AnalyzeMode,
    tui_tx: tokio_mpsc::UnboundedSender<TuiMsg>,
) -> Result<()> {
    let _ = tui_tx.send(TuiMsg::Status("Starting llama-server.exe for Boost mode…".into()));
    
    let server_path = crate::pet::storage::llama_server_path();
    let model_path = crate::pet::storage::qwen3_model_path(Some(pet));

    if !server_path.exists() || !model_path.exists() {
        bail!("llama-server.exe or Qwen3 model not found. Run `rift hatch` first.");
    }

    let cpu_threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8) as u32;

    let mut child = Command::new(&server_path)
        .args([
            "--model", &model_path.to_string_lossy(),
            "--ctx-size", "32768",
            "--ctk", "q4_0", // TurboQuant 4-bit context
            "--threads", &cpu_threads.to_string(),
            "--port", "8080",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn llama-server.exe")?;

    // Wait for server to boot
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Walk files to read initial context
    let root = Path::new(path);
    let source_files = crate::analysis::walker::walk_path(root, max_files).unwrap_or_default();
    let mut file_contents = std::collections::HashMap::new();
    for f in source_files {
        if let Ok(c) = std::fs::read_to_string(&f.path) {
            file_contents.insert(f.path.clone(), c);
        }
    }

    let base_prompt = build_prompt(pet, "", mode).replace("=== CODE SUMMARY ===\n\n=== END SUMMARY ===", "You have access to tools via XML tags. Available tools:\n- <GrepClaw pattern=\"...\" />\n- <ExeClaw exe=\"...\" args=\"...\" />\nTo eject a file from context to save tokens, output </ReadClaw path=\"...\">");

    let client = reqwest::Client::new();
    let mut messages = vec![];

    // Main interaction loop (simplified single-turn for now)
    let mut current_sys = base_prompt.clone();
    for (p, c) in &file_contents {
        current_sys.push_str(&format!("\n<ReadClaw path=\"{}\">\n{}\n</ReadClaw>\n", p, c));
    }
    
    messages.push(serde_json::json!({ "role": "system", "content": current_sys }));
    messages.push(serde_json::json!({ "role": "user", "content": "Review my code." }));

    let _ = tui_tx.send(TuiMsg::Status("Agent thinking…".into()));

    let body = serde_json::json!({
        "model": "qwen3",
        "messages": messages,
        "temperature": 0.7,
        "stream": true,
    });

    let resp_result = client.post("http://127.0.0.1:8080/v1/chat/completions")
        .json(&body)
        .send().await;

    if let Ok(mut resp) = resp_result {
        let mut current_line = String::new();
        let mut buffer = String::new();

        while let Some(chunk) = resp.chunk().await.unwrap_or(None) {
            let chunk_str = String::from_utf8_lossy(&chunk);
            buffer.push_str(&chunk_str);

            while let Some(idx) = buffer.find('\n') {
                let line = buffer[..idx].to_string();
                buffer = buffer[idx+1..].to_string();
                let line = line.trim();
                
                if line.starts_with("data: ") {
                    let data = &line[6..];
                    if data == "[DONE]" { break; }
                    
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                            for c in delta.chars() {
                                if c == '\n' {
                                    let _ = tui_tx.send(TuiMsg::Output(current_line.clone()));
                                    current_line.clear();
                                } else {
                                    current_line.push(c);
                                }
                            }
                            
                            // Simple real-time tool detection display
                            if current_line.ends_with("</ReadClaw>") {
                                let _ = tui_tx.send(TuiMsg::Output(format!("  \x1b[35m[EJECTED FILE]\x1b[0m")));
                                current_line.clear();
                            }
                            if current_line.ends_with("<GrepClaw") {
                                let _ = tui_tx.send(TuiMsg::Output(format!("  \x1b[33m[RAN GREP]\x1b[0m")));
                                current_line.clear();
                            }
                        }
                    }
                }
            }
        }
        if !current_line.is_empty() {
            let _ = tui_tx.send(TuiMsg::Output(current_line));
        }
    }

    let _ = child.kill();
    Ok(())
}

