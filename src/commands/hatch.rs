use anyhow::Result;
use crossterm::style::{Print, ResetColor, SetForegroundColor};

use crossterm::{cursor, execute, terminal};
use std::io::{self, Write};
use std::time::Duration;
use std::thread;

use crate::llm::downloader::Downloader;
use crate::pet::identity::{derive_identity, read_machine_guid};
use crate::pet::storage::{llama_dir, load_pet, models_dir, save_pet};
use crate::tui::spinner::Spinner;

pub async fn run() -> Result<()> {
    // ── Check if already hatched ──────────────────────────────────────────────
    if let Some(pet) = load_pet()? {
        println!();
        print_pet_card(&pet.prefix, &pet.adjective, &pet.noun, &pet.stats)?;
        println!();
        println!("  Your Rift is already hatched! Run \x1b[36mrift analyze <path>\x1b[0m to review code.");
        println!();
        return Ok(());
    }

    // ── Read MachineGuid & derive identity ───────────────────────────────────
    println!();
    println!("  \x1b[36m🦞 Rift is hatching...\x1b[0m");
    println!();

    run_hatch_animation()?;

    let guid = read_machine_guid()?;
    let pet = derive_identity(&guid);

    // ── Display pet card ──────────────────────────────────────────────────────
    print_pet_card(&pet.prefix, &pet.adjective, &pet.noun, &pet.stats)?;
    println!();

    // ── Download llama.cpp (GPU-aware) ────────────────────────────────────────
    println!("  \x1b[33m⬇\x1b[0m  Acquiring llama.cpp...");
    let dl = Downloader::new()?;
    let gpu_kind = dl.download_llama(&llama_dir()).await?;
    println!("  \x1b[32m✓\x1b[0m  llama.cpp ready ({})", gpu_kind.label());
    println!();

    // ── Download Gemma 4 E4B Q6_K ─────────────────────────────────────────────
    println!("  \x1b[33m⬇\x1b[0m  Acquiring Gemma 4 E4B Q6_K model (~6.4 GB)...");
    println!("     \x1b[90mDownload is resumable — feel free to Ctrl+C and continue later.\x1b[0m");
    dl.download_model(&models_dir()).await?;
    println!("  \x1b[32m✓\x1b[0m  Model ready");
    println!();

    // ── Persist ────────────────────────────────────────────────────────────────
    save_pet(&pet)?;

    println!("  \x1b[32m✓\x1b[0m  Pet saved. Run \x1b[36mrift analyze <path>\x1b[0m to begin your first review!");
    println!();

    Ok(())
}

/// Play the hatching spinner animation for ~2 seconds.
fn run_hatch_animation() -> Result<()> {
    let mut spinner = Spinner::new();
    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;

    for _ in 0..50 {
        let frame = spinner.render();
        execute!(
            stdout,
            cursor::MoveToColumn(2),
            SetForegroundColor(frame.color),
            Print(&frame.symbol),
            Print("  hatching"),
            ResetColor,
        )?;
        stdout.flush()?;
        spinner.tick();
        thread::sleep(Duration::from_millis(80));
    }

    terminal::disable_raw_mode()?;
    execute!(stdout, Print("\r"), terminal::Clear(terminal::ClearType::CurrentLine))?;
    Ok(())
}

fn print_pet_card(
    prefix: &str,
    adjective: &str,
    noun: &str,
    stats: &crate::pet::identity::PetStats,
) -> Result<()> {
    let name = format!("{prefix} {adjective} {noun}");
    let width = name.len() + 8;
    let border: String = "─".repeat(width);

    println!("  ╭{border}╮");
    println!(
        "  │  \x1b[1;36m{:^w$}\x1b[0m  │",
        name,
        w = width - 4
    );
    println!("  ├{border}┤");
    println!(
        "  │  \x1b[32mdebuggability\x1b[0m  {:>3}   \x1b[33mcuriosity\x1b[0m      {:>3}  │",
        stats.debuggability, stats.curiosity
    );
    println!(
        "  │  \x1b[31munpredictability\x1b[0m {:>3}   \x1b[35mchattiness\x1b[0m    {:>3}  │",
        stats.unpredictability, stats.chattiness
    );
    println!(
        "  │  \x1b[34mpedantry\x1b[0m       {:>3}   \x1b[92mempathy\x1b[0m       {:>3}  │",
        stats.pedantry, stats.empathy
    );
    println!("  ╰{border}╯");

    Ok(())
}
