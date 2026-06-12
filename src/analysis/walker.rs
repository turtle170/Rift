use anyhow::Result;
use ignore::WalkBuilder;
use std::path::Path;

/// A source file ready for Tree-sitter parsing.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub language: Language,
    pub source: String,
}

/// Languages with Tree-sitter grammar support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
    Java,
}

impl Language {
    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Go => "Go",
            Language::Java => "Java",
        }
    }

    fn from_extension(ext: &str) -> Option<Language> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            "py" => Some(Language::Python),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "ts" | "tsx" => Some(Language::TypeScript),
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(Language::Cpp),
            "go" => Some(Language::Go),
            "java" => Some(Language::Java),
            _ => None,
        }
    }
}

/// Walk `root` path (respecting .gitignore) and collect up to `max_files` source files.
pub fn walk_path(root: &Path, max_files: usize) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(true)        // skip hidden files/dirs
        .git_ignore(true)    // respect .gitignore
        .git_global(true)
        .git_exclude(true)
        .ignore(true)
        .max_filesize(Some(512 * 1024)) // skip files > 512 KB
        .build();

    for entry in walker {
        if files.len() >= max_files {
            break;
        }

        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let language = match Language::from_extension(ext) {
            Some(l) => l,
            None => continue,
        };

        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue, // skip binary or unreadable files
        };

        // Skip mostly-empty files
        if source.trim().len() < 10 {
            continue;
        }

        files.push(SourceFile {
            path: path.to_string_lossy().to_string(),
            language,
            source,
        });
    }

    Ok(files)
}
