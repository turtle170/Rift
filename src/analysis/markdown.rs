use crate::analysis::parser::{extract_nodes, ParsedFile};
use crate::analysis::sexp::{node_sexp, simplify_node};

/// Maximum total characters in the final Markdown summary fed to the LLM.
const MAX_SUMMARY_CHARS: usize = 12_000;

/// Convert a list of parsed files into a Markdown summary ready for LLM input.
pub fn build_summary(parsed_files: &[ParsedFile]) -> String {
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

fn append_file_summary(md: &mut String, pf: &ParsedFile) {
    let sf = &pf.source_file;
    let file_start_len = md.len();

    // ── File header ──────────────────────────────────────────────────────────
    md.push_str(&format!(
        "\n## `{}` ({})\n",
        shorten_path(&sf.path),
        sf.language.name()
    ));

    let nodes = extract_nodes(&pf.tree, &sf.language);

    if nodes.is_empty() {
        md.push_str("*(no named constructs found)*\n");
        return;
    }

    // ── Group by kind ────────────────────────────────────────────────────────
    let mut funcs: Vec<String> = Vec::new();
    let mut types: Vec<String> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for node in nodes {
        let simplified = simplify_node(node, &sf.source);

        if simplified.is_error {
            errors.push(format!(
                "- ⚠ Parse error at line {} — `{}`",
                simplified.line,
                node_sexp(node, &sf.source)
                    .chars()
                    .take(200)
                    .collect::<String>()
            ));
            continue;
        }

        let entry = format!(
            "- `{}` *(line {})*",
            simplified.text.replace('`', "'"),
            simplified.line
        );

        match simplified.kind.as_str() {
            // Functions / methods
            k if k.contains("function") || k.contains("method") || k.contains("fn") => {
                funcs.push(entry);
            }
            // Types / structs / classes / traits / enums
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
            // Imports / uses
            k if k.contains("import") || k.contains("use") || k.contains("include") => {
                imports.push(entry);
            }
            _ => {
                funcs.push(entry); // fallback bucket
            }
        }
    }

    if !funcs.is_empty() {
        md.push_str("### Functions / Methods\n");
        for f in &funcs {
            md.push_str(f);
            md.push('\n');
        }
    }

    if !types.is_empty() {
        md.push_str("### Types / Structs / Traits\n");
        for t in &types {
            md.push_str(t);
            md.push('\n');
        }
    }

    if !imports.is_empty() {
        md.push_str("### Imports\n");
        for i in imports.iter().take(10) {
            md.push_str(i);
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
