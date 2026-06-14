use anyhow::{Context, Result};
use std::process::Command;
use std::path::Path;
use ignore::WalkBuilder;
use std::fs;

pub struct ExeClaw;

impl ExeClaw {
    pub fn run(exe_path: &str, args: &[&str], cwd: &Path) -> Result<String> {
        let output = Command::new(exe_path)
            .args(args)
            .current_dir(cwd)
            .output()
            .context(format!("Failed to execute {}", exe_path))?;

        let mut res = String::new();
        res.push_str(&String::from_utf8_lossy(&output.stdout));
        if !output.stderr.is_empty() {
            if !res.is_empty() {
                res.push('\n');
            }
            res.push_str("--- STDERR ---\n");
            res.push_str(&String::from_utf8_lossy(&output.stderr));
        }
        Ok(res)
    }
}

pub struct GrepClaw;

impl GrepClaw {
    pub fn run(pattern: &str, cwd: &Path) -> Result<String> {
        let mut results = String::new();
        let walker = WalkBuilder::new(cwd).build();
        let mut match_count = 0;

        for result in walker {
            if match_count >= 100 {
                results.push_str("\n...[Truncated: more than 100 matches]...\n");
                break;
            }
            if let Ok(entry) = result {
                if entry.file_type().map_or(false, |ft| ft.is_file()) {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        for (i, line) in content.lines().enumerate() {
                            if line.contains(pattern) {
                                let rel_path = entry.path().strip_prefix(cwd).unwrap_or(entry.path());
                                results.push_str(&format!("{}:{}: {}\n", rel_path.display(), i + 1, line.trim()));
                                match_count += 1;
                                if match_count >= 100 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No matches found for '{}'", pattern))
        } else {
            Ok(results)
        }
    }
}

pub struct ReadClaw;

impl ReadClaw {
    pub fn run(file_path: &str, cwd: &Path) -> Result<String> {
        let full_path = cwd.join(file_path);
        let content = fs::read_to_string(&full_path)
            .context(format!("Failed to read {}", full_path.display()))?;
        Ok(content)
    }
}
