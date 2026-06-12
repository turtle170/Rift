use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

const PREFIXES: &[&str] = &[
    "Captain", "Admiral", "Cyber", "Mecha", "Rusty", "Salty", "Abstract", "Crusty", "Techy",
];

const ADJECTIVES: &[&str] = &[
    "Crimson",
    "Snapping",
    "Steely",
    "Clawy",
    "Nocturnal",
    "Binary",
    "Executable",
    "Niche",
];

const NOUNS: &[&str] = &[
    "Lobster",
    "Langoustine",
    "Homarus",
    "Claw",
    "Carapace",
    "Sifter",
    "Rifty",
];

/// All stats are 0–100.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetStats {
    /// How good at finding bugs (high = methodical & precise)
    pub debuggability: u8,
    /// How inquisitive / question-prone
    pub curiosity: u8,
    /// How chaotic / random the commentary is
    pub unpredictability: u8,
    /// How verbose the pet is
    pub chattiness: u8,
    /// How nitpicky about style/conventions
    pub pedantry: u8,
    /// How encouraging vs. harsh
    pub empathy: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetIdentity {
    pub guid: String,
    pub prefix: String,
    pub adjective: String,
    pub noun: String,
    pub stats: PetStats,
}

impl PetIdentity {
    pub fn name(&self) -> String {
        format!("{} {} {}", self.prefix, self.adjective, self.noun)
    }
}

/// Read the Windows Machine GUID from the registry.
pub fn read_machine_guid() -> Result<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let subkey = hklm
        .open_subkey(r"SOFTWARE\Microsoft\Cryptography")
        .context("Failed to open Cryptography registry key")?;
    let guid: String = subkey
        .get_value("MachineGuid")
        .context("Failed to read MachineGuid")?;
    Ok(guid.trim().to_string())
}

/// Derive a fully-populated [`PetIdentity`] from the machine GUID.
pub fn derive_identity(guid: &str) -> PetIdentity {
    let mut hasher = Sha256::new();
    hasher.update(guid.as_bytes());
    let hash = hasher.finalize();
    let b = hash.as_slice();

    let prefix = PREFIXES[b[0] as usize % PREFIXES.len()].to_string();
    let adjective = ADJECTIVES[b[1] as usize % ADJECTIVES.len()].to_string();
    let noun = NOUNS[b[2] as usize % NOUNS.len()].to_string();

    let stats = PetStats {
        debuggability: b[3],
        curiosity: b[4],
        unpredictability: b[5],
        chattiness: b[6],
        pedantry: b[7],
        empathy: b[8],
    };

    PetIdentity {
        guid: guid.to_string(),
        prefix,
        adjective,
        noun,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_identity_deterministic() {
        let guid = "12345678-1234-1234-1234-123456789abc";
        let a = derive_identity(guid);
        let b = derive_identity(guid);
        assert_eq!(a.name(), b.name());
        assert_eq!(a.stats.debuggability, b.stats.debuggability);
    }

    #[test]
    fn test_derive_identity_valid_ranges() {
        let guid = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let id = derive_identity(guid);
        assert!(PREFIXES.contains(&id.prefix.as_str()));
        assert!(ADJECTIVES.contains(&id.adjective.as_str()));
        assert!(NOUNS.contains(&id.noun.as_str()));
    }
}
