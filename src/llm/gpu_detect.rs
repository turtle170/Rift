use std::process::Command;

/// The kind of GPU found in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuKind {
    /// Discrete / dedicated GPU with its own VRAM — use Vulkan llama.cpp.
    Discrete,
    /// Integrated GPU sharing system RAM — use CPU llama.cpp.
    Integrated,
    /// Detection failed; fall back to CPU to be safe.
    Unknown,
}

impl GpuKind {
    #[allow(dead_code)]
    pub fn is_discrete(&self) -> bool {
        matches!(self, GpuKind::Discrete)
    }

    pub fn label(&self) -> &'static str {
        match self {
            GpuKind::Discrete => "Discrete GPU (Vulkan acceleration)",
            GpuKind::Integrated => "Integrated GPU (CPU inference)",
            GpuKind::Unknown => "Unknown GPU (CPU inference, safe fallback)",
        }
    }
}

/// Detect whether the primary GPU is integrated or discrete.
///
/// Uses PowerShell to query `Win32_VideoController` WMI class.
/// Heuristics:
/// - If `AdapterRAM` (dedicated VRAM) < 256 MB → integrated
/// - If name matches known integrated patterns → integrated
/// - Otherwise → discrete
pub fn detect_gpu() -> GpuKind {
    match query_gpu_info() {
        Ok(gpus) => classify_gpus(&gpus),
        Err(_) => GpuKind::Unknown,
    }
}

#[derive(Debug)]
struct GpuInfo {
    name: String,
    /// AdapterRAM in bytes as reported by WMI (may be 0 for integrated)
    adapter_ram: u64,
}

/// Run PowerShell to get GPU name + AdapterRAM for all display adapters.
fn query_gpu_info() -> Result<Vec<GpuInfo>, String> {
    // Use PowerShell to query WMI. Output: "Name|AdapterRAM" per line.
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            r#"Get-WmiObject Win32_VideoController | Select-Object Name,AdapterRAM | ForEach-Object { "$($_.Name)|$($_.AdapterRAM)" }"#,
        ])
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err("PowerShell query failed".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '|').collect();
        if parts.len() < 2 {
            continue;
        }
        let name = parts[0].trim().to_string();
        let adapter_ram: u64 = parts[1].trim().parse().unwrap_or(0);
        if !name.is_empty() {
            gpus.push(GpuInfo { name, adapter_ram });
        }
    }

    Ok(gpus)
}

/// Classify a list of GPUs — if ANY discrete GPU is present, return Discrete.
fn classify_gpus(gpus: &[GpuInfo]) -> GpuKind {
    if gpus.is_empty() {
        return GpuKind::Unknown;
    }

    // If we find at least one discrete GPU, prefer Vulkan.
    for gpu in gpus {
        if is_discrete(gpu) {
            return GpuKind::Discrete;
        }
    }

    GpuKind::Integrated
}

/// Heuristics to determine if a GPU is discrete.
fn is_discrete(gpu: &GpuInfo) -> bool {
    let name_lower = gpu.name.to_lowercase();

    // ── Explicitly integrated GPU patterns ──────────────────────────────────
    let integrated_patterns = [
        // Intel
        "intel uhd",
        "intel hd graphics",
        "intel iris",
        "intel(r) uhd",
        "intel(r) hd",
        "intel(r) iris",
        // AMD APU / Radeon integrated
        "radeon graphics",      // AMD APU (e.g. "AMD Radeon Graphics")
        "vega 3",
        "vega 6",
        "vega 7",
        "vega 8",
        "vega 10",
        "radeon 610m",
        "radeon 680m",
        "radeon 780m",
        "radeon 890m",
        // ARM / Qualcomm
        "qualcomm adreno",
        "microsoft basic",
        "microsoft remote display",
        "vmware",
        "virtualbox",
        "basic render",
    ];

    for pattern in integrated_patterns {
        if name_lower.contains(pattern) {
            return false; // definitely integrated
        }
    }

    // ── Explicitly discrete GPU patterns ────────────────────────────────────
    let discrete_patterns = [
        "nvidia",
        "geforce",
        "quadro",
        "tesla",
        "rtx",
        "gtx",
        "radeon rx",   // "RX" distinguishes discrete from APU "Radeon Graphics"
        "radeon pro",
        "radeon vii",
        "arc ",        // Intel Arc discrete
        "intel arc",
    ];

    for pattern in discrete_patterns {
        if name_lower.contains(pattern) {
            return true;
        }
    }

    // ── AdapterRAM heuristic ─────────────────────────────────────────────────
    // WMI AdapterRAM is a 32-bit value (max ~4 GB). Integrated GPUs report 0
    // or share system RAM (reported as total system RAM, often 1–4 GB).
    // Discrete GPUs typically have ≥ 2 GB dedicated VRAM.
    //
    // Threshold: if reported RAM > 512 MB AND it looks like dedicated VRAM
    // (i.e., not suspiciously equal to a round system RAM size), treat as discrete.
    //
    // We use a conservative 512 MB threshold. Integrated GPUs almost never
    // have > 512 MB _dedicated_ VRAM; they use system RAM and report 0 or 128 MB.
    let vram_mb = gpu.adapter_ram / (1024 * 1024);
    if vram_mb > 512 {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvidia_is_discrete() {
        let gpu = GpuInfo { name: "NVIDIA GeForce RTX 4070".into(), adapter_ram: 12 * 1024 * 1024 * 1024 };
        assert!(is_discrete(&gpu));
    }

    #[test]
    fn test_intel_uhd_is_integrated() {
        let gpu = GpuInfo { name: "Intel(R) UHD Graphics 630".into(), adapter_ram: 0 };
        assert!(!is_discrete(&gpu));
    }

    #[test]
    fn test_amd_apu_is_integrated() {
        let gpu = GpuInfo { name: "AMD Radeon Graphics".into(), adapter_ram: 512 * 1024 * 1024 };
        assert!(!is_discrete(&gpu));
    }

    #[test]
    fn test_radeon_rx_is_discrete() {
        let gpu = GpuInfo { name: "AMD Radeon RX 6700 XT".into(), adapter_ram: 12 * 1024 * 1024 * 1024 };
        assert!(is_discrete(&gpu));
    }

    #[test]
    fn test_intel_arc_is_discrete() {
        let gpu = GpuInfo { name: "Intel Arc A770".into(), adapter_ram: 16 * 1024 * 1024 * 1024 };
        assert!(is_discrete(&gpu));
    }
}
