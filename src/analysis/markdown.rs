use crate::analysis::sexp::{node_sexp, simplify_node};
use crate::analysis::walker::SourceFile;
use crate::analysis::parser::extract_nodes;
use tree_sitter::Tree;

/// Maximum total characters in the final Markdown summary fed to the LLM.
const MAX_SUMMARY_CHARS: usize = 12_000;

/// A parsed file's data — tree + source — passed into the Markdown builder.
/// This decouples the builder from the full `ParsedFile` struct.
pub struct ParsedFileRef<'a> {
    pub source_file: &'a SourceFile,
    pub tree: &'a Tree,
}

/// Convert a list of parsed files into a Markdown summary ready for LLM input.
/// Each node's S-expression is included as a fenced code block so the LLM
/// can see the actual syntax tree structure.
pub fn build_summary_from_refs(parsed_files: &[ParsedFileRef<'_>]) -> String {
    let mut md = String::with_capacity(4096);

    for pf in parsed_files {
        if md.len() > MAX_SUMMARY_CHARS {
            md.push_str("\n\n*(summary truncated — too many files)*\n");
            break;
        }
        append_file_summary(&mut md, pf);
    }

    md
}

fn append_file_summary(md: &mut String, pf: &ParsedFileRef<'_>) {
    let sf = pf.source_file;
    let file_start_len = md.len();

    // ── File header ──────────────────────────────────────────────────────────
    md.push_str(&format!(
        "\n## `{}` ({})\n",
        shorten_path(&sf.path),
        sf.language.name()
    ));

    let nodes = extract_nodes(pf.tree, &sf.language);

    if nodes.is_empty() {
        md.push_str("*(no named constructs found)*\n");
        return;
    }

    // ── Group by kind ────────────────────────────────────────────────────────
    let mut funcs: Vec<(String, String)> = Vec::new();   // (signature, sexp)
    let mut types: Vec<(String, String)> = Vec::new();
    let mut imports: Vec<(String, String)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for node in nodes {
        let simplified = simplify_node(node, &sf.source);

        if simplified.is_error {
            errors.push(format!(
                "- ⚠ Parse error at line {} — `{}`\n  ```sexp\n  {}\n  ```",
                simplified.line,
                node_sexp(node, &sf.source)
                    .chars()
                    .take(200)
                    .collect::<String>(),
                simplified.sexp
            ));
            continue;
        }

        let signature = format!(
            "- `{}` *(line {})*",
            simplified.text.replace('`', "'"),
            simplified.line
        );

        let entry = (signature, simplified.sexp);

        match simplified.kind.as_str() {
            k if k.contains("function") || k.contains("method") || k.contains("fn") => {
                funcs.push(entry);
            }
            k if k.contains("struct")
                || k.contains("class")
                || k.contains("enum")
                || k.contains("trait")
                || k.contains("interface")
                || k.contains("type")
                || k.contains("impl") =>
            {
                types.push(entry);
            }
            k if k.contains("import") || k.contains("use") || k.contains("include") => {
                imports.push(entry);
            }
            _ => {
                funcs.push(entry); // fallback bucket
            }
        }
    }

    // Emit each section with S-expression code blocks
    if !funcs.is_empty() {
        md.push_str("### Functions / Methods\n");
        for (sig, sexp) in &funcs {
            md.push_str(sig);
            md.push('\n');
            md.push_str("  ```sexp\n");
            // Limit each sexp block to 400 chars to stay within budget
            let sexp_short: String = sexp.chars().take(400).collect();
            md.push_str(&format!("  {}\n", sexp_short));
            md.push_str("  ```\n");
        }
    }

    if !types.is_empty() {
        md.push_str("### Types / Structs / Traits\n");
        for (sig, sexp) in &types {
            md.push_str(sig);
            md.push('\n');
            md.push_str("  ```sexp\n");
            let sexp_short: String = sexp.chars().take(400).collect();
            md.push_str(&format!("  {}\n", sexp_short));
            md.push_str("  ```\n");
        }
    }

    if !imports.is_empty() {
        md.push_str("### Imports\n");
        for (sig, _) in imports.iter().take(10) {
            // Imports are compact — no sexp needed
            md.push_str(sig);
            md.push('\n');
        }
        if imports.len() > 10 {
            md.push_str(&format!("- *(+{} more imports)*\n", imports.len() - 10));
        }
    }

    if !errors.is_empty() {
        md.push_str("### ⚠ Parse Errors\n");
        for e in &errors {
            md.push_str(e);
            md.push('\n');
        }
    }

    // Guard: if this single file blew up the summary, truncate it
    if md.len() - file_start_len > MAX_SUMMARY_CHARS / 3 {
        md.truncate(file_start_len + MAX_SUMMARY_CHARS / 3);
        md.push_str("\n*(file output truncated)*\n");
    }
}

/// Shorten a path to the last 3 components for readability.
fn shorten_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() <= 3 {
        return normalized;
    }
    format!("…/{}", parts[parts.len() - 3..].join("/"))
}

// ── Legacy shim (kept so any other callers still compile) ───────────────────
use crate::analysis::parser::ParsedFile;

#[allow(dead_code)]
pub fn build_summary(parsed_files: &[ParsedFile]) -> String {
    let refs: Vec<ParsedFileRef<'_>> = parsed_files
        .iter()
        .map(|pf| ParsedFileRef {
            source_file: &pf.source_file,
            tree: &pf.tree,
        })
        .collect();
    build_summary_from_refs(&refs)
}
