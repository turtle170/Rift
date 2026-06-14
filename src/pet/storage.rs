use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::pet::PetIdentity;

#[derive(Debug, Serialize, Deserialize)]
struct PetFile {
    pet: PetIdentity,
}

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine APPDATA directory")?;
    let dir = base.join("rift");
    std::fs::create_dir_all(&dir).context("Cannot create rift config directory")?;
    Ok(dir)
}

fn pet_toml_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("pet.toml"))
}

/// Load the pet from `%APPDATA%\rift\pet.toml`. Returns `None` if not hatched yet.
pub fn load_pet() -> Result<Option<PetIdentity>> {
    let path = pet_toml_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let file: PetFile =
        toml::from_str(&contents).context("Failed to parse pet.toml — try `rift hatch` again")?;
    Ok(Some(file.pet))
}

/// Save the pet to `%APPDATA%\rift\pet.toml`.
pub fn save_pet(pet: &PetIdentity) -> Result<()> {
    let path = pet_toml_path()?;
    let file = PetFile { pet: pet.clone() };
    let contents = toml::to_string_pretty(&file).context("Failed to serialize pet")?;
    std::fs::write(&path, contents)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

/// Return the path to the rift data directory on D: drive.
pub fn data_dir() -> PathBuf {
    PathBuf::from(r"D:\rift")
}

pub fn llama_dir() -> PathBuf {
    data_dir().join("llama")
}

pub fn models_dir(pet: Option<&PetIdentity>) -> PathBuf {
    if let Some(p) = pet {
        if let Some(ref custom_dir) = p.custom_models_dir {
            return PathBuf::from(custom_dir);
        }
    }
    data_dir().join("models")
}

pub fn llama_cli_path() -> PathBuf {
    llama_dir().join("llama-cli.exe")
}

pub fn llama_server_path() -> PathBuf {
    llama_dir().join("llama-server.exe")
}

pub fn gemma_model_path(pet: Option<&PetIdentity>) -> PathBuf {
    models_dir(pet).join("gemma-4-E4B-it-Q6_K.gguf")
}

pub fn qwen3_model_path(pet: Option<&PetIdentity>) -> PathBuf {
    models_dir(pet).join("qwen3-coder-next-iq4_nl.gguf")
}
