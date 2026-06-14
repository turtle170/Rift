use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Client;
use std::io::Write;
use std::path::Path;

use tokio::io::AsyncWriteExt;

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

