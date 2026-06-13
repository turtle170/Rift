use tree_sitter::Node;

/// Maximum S-expression characters to emit per node before truncating.
const MAX_SEXP_CHARS: usize = 2000;

/// A simplified, filtered representation of a Tree-sitter node.
pub struct FilteredNode {
    pub kind: String,
    pub text: String,   // First line / signature of the node
    pub sexp: String,   // Full condensed S-expression (for the LLM)
    pub line: usize,
    pub is_error: bool,
}

/// Convert a raw Tree-sitter node into a `FilteredNode`, extracting both
/// the first meaningful line (signature) and the condensed S-expression.
pub fn simplify_node<'a>(node: Node<'a>, source: &str) -> FilteredNode {
    let is_error = node.kind() == "ERROR" || node.has_error();
    let start = node.start_position();

    // Extract the text of the node, but cap at first 120 chars of the first line
    let node_text = node
        .utf8_text(source.as_bytes())
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();

    // Full S-expression, truncated intelligently
    let sexp = node_sexp(node, source);

    FilteredNode {
        kind: node.kind().to_string(),
        text: node_text,
        sexp,
        line: start.row + 1,
        is_error,
    }
}

/// Build a condensed S-expression string for a node.
/// Truncates at a paren boundary if too long.
pub fn node_sexp(node: Node<'_>, _source: &str) -> String {
    let full = node.to_sexp();
    if full.len() <= MAX_SEXP_CHARS {
        return full;
    }
    // Truncate intelligently at a paren boundary
    let truncated = &full[..MAX_SEXP_CHARS];
    let last_paren = truncated.rfind('(').unwrap_or(MAX_SEXP_CHARS);
    format!("{}…)", &full[..last_paren])
}
