use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::io::Write;
use std::path::Path;

use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

use crate::llm::gpu_detect::{detect_gpu, GpuKind};

// llama.cpp b9606 release assets
const LLAMA_VULKAN_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9606/llama-b9606-bin-win-vulkan-x64.zip";
const LLAMA_CPU_URL: &str =
    "https://github.com/ggml-org/llama.cpp/releases/download/b9606/llama-b9606-bin-win-cpu-x64.zip";

const GEMMA_MODEL_URL: &str =
    "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q6_K.gguf";

const QWEN3_MODEL_URL: &str =
    "https://huggingface.co/unsloth/Qwen3-Coder-Next-GGUF/resolve/main/Qwen3-Coder-Next-UD-IQ4_NL.gguf?download=true";

pub struct Downloader {
    client: Client,
    multi: MultiProgress,
}

impl Downloader {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("rift/0.1.0")
            .timeout(std::time::Duration::from_secs(3600))
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self {
            client,
            multi: MultiProgress::new(),
        })
    }

    /// Detect GPU, pick the right llama.cpp build, download + extract it.
    pub async fn download_llama(&self, dest_dir: &Path) -> Result<GpuKind> {
        std::fs::create_dir_all(dest_dir)?;

        let cli_path = dest_dir.join("llama-cli.exe");

        // Detect GPU every time so we can report the kind even if already downloaded.
        print!("  Detecting GPU... ");
        let _ = std::io::stdout().flush();
        let gpu = detect_gpu();
        println!("{}", gpu.label());

        if cli_path.exists() {
            println!("  ✓ llama-cli.exe already present, skipping download.");
            return Ok(gpu);
        }

        let (url, label) = match &gpu {
            GpuKind::Discrete => (LLAMA_VULKAN_URL, "llama.cpp b9606 (Vulkan — discrete GPU)"),
            _ => (LLAMA_CPU_URL, "llama.cpp b9606 (CPU — integrated/unknown GPU)"),
        };

        let zip_path = dest_dir.join("llama.zip");
        self.download_file(url, &zip_path, label).await?;

        println!("  Extracting llama.cpp...");
        extract_zip(&zip_path, dest_dir)?;
        std::fs::remove_file(&zip_path).ok();
        println!("  ✓ llama.cpp extracted to {}", dest_dir.display());

        Ok(gpu)
    }

    /// Download both Gemma and Qwen3 models to dest_dir, supporting resume.
    pub async fn download_models(&self, dest_dir: &Path, no_boost: bool) -> Result<()> {
        std::fs::create_dir_all(dest_dir)?;

        let gemma_path = dest_dir.join("gemma-4-E4B-it-Q6_K.gguf");
        if gemma_path.exists() && std::fs::metadata(&gemma_path)?.len() > 1_000_000_000 {
            println!("  ✓ Gemma Model already present, skipping download.");
        } else {
            self.download_file_resumable(GEMMA_MODEL_URL, &gemma_path, "Gemma 4 E4B Q6_K (~6.4 GB)").await?;
            println!("  ✓ Gemma Model downloaded to {}", gemma_path.display());
        }

        if !no_boost {
            let qwen_path = dest_dir.join("Qwen3-Coder-Next-UD-IQ4_NL.gguf");
            if qwen_path.exists() && std::fs::metadata(&qwen_path)?.len() > 1_000_000_000 {
                println!("  ✓ Qwen3 Model already present, skipping download.");
            } else {
                self.download_file_resumable(QWEN3_MODEL_URL, &qwen_path, "Qwen3 Coder Next IQ4_NL (~45.0 GB)").await?;
                println!("  ✓ Qwen3 Model downloaded to {}", qwen_path.display());
            }
        } else {
            println!("  ✓ Skipping Qwen3 Model download (--no-boost)");
        }

        Ok(())
    }

    async fn download_file(&self, url: &str, dest: &Path, label: &str) -> Result<()> {
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to GET {url}"))?;

        if !resp.status().is_success() {
            bail!("HTTP {} for {url}", resp.status());
        }

        let total = resp.content_length().unwrap_or(0);
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(progress_style());
        pb.set_message(label.to_string());

        let mut file = tokio::fs::File::create(dest)
            .await
            .with_context(|| format!("Cannot create {}", dest.display()))?;

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Stream error")?;
            pb.inc(chunk.len() as u64);
            file.write_all(&chunk).await.context("Write error")?;
        }

        pb.finish_with_message(format!("{label} ✓"));
        Ok(())
    }

    /// Download with HTTP Range support for resumable downloads.
    async fn download_file_resumable(&self, url: &str, dest: &Path, label: &str) -> Result<()> {
        let existing_size = if dest.exists() {
            std::fs::metadata(dest)?.len()
        } else {
            0
        };

        // HEAD to get total size
        let head = self.client.head(url).send().await
            .with_context(|| format!("HEAD request failed for {url}"))?;
        let total = head
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        if existing_size > 0 && total > 0 && existing_size == total {
            println!("  ✓ {label} already complete, skipping.");
            return Ok(());
        }

        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(progress_style());
        pb.set_message(label.to_string());
        pb.set_position(existing_size);

        let mut req = self.client.get(url);
        if existing_size > 0 {
            req = req.header("Range", format!("bytes={existing_size}-"));
        }
        let resp = req.send().await.with_context(|| format!("GET {url}"))?;

        if !resp.status().is_success() && resp.status().as_u16() != 206 {
            bail!("HTTP {} for {url}", resp.status());
        }

        let append = existing_size > 0 && resp.status().as_u16() == 206;
        let mut file = if append {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(dest)
                .await?
        } else {
            tokio::fs::File::create(dest).await?
        };

        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Stream read error")?;
            pb.inc(chunk.len() as u64);
            file.write_all(&chunk).await.context("Write error")?;
        }

        pb.finish_with_message(format!("{label} ✓"));
        Ok(())
    }
}

fn progress_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{msg}\n{spinner:.cyan} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  ")
}

fn extract_zip(zip_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .with_context(|| format!("Cannot open {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file).context("Invalid ZIP archive")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let outpath = dest_dir.join(entry.name());

        if entry.name().ends_with('/') {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }
    }

    Ok(())
}
