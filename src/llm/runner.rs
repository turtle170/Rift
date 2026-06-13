use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;

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
