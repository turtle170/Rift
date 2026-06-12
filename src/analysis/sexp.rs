use tree_sitter::Node;

/// Maximum S-expression characters to emit per file before truncating.
const MAX_SEXP_CHARS: usize = 2000;

/// A simplified, filtered representation of a Tree-sitter node.
pub struct FilteredNode {
    pub kind: String,
    pub text: String,   // First line / signature of the node
    pub line: usize,
    pub is_error: bool,
}

/// Convert a raw Tree-sitter node into a `FilteredNode`, extracting just
/// the first meaningful line (signature) rather than the full subtree.
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

    FilteredNode {
        kind: node.kind().to_string(),
        text: node_text,
        line: start.row + 1,
        is_error,
    }
}

/// Build a condensed S-expression string for a node (for error reporting).
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
