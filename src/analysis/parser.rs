use anyhow::Result;
use tree_sitter::{Node, Parser, Tree};

use crate::analysis::walker::{Language, SourceFile};

/// A parsed source file with its Tree-sitter tree.
pub struct ParsedFile {
    pub source_file: SourceFile,
    pub tree: Tree,
}

/// Parse a source file with the appropriate Tree-sitter grammar.
pub fn parse_file(sf: &SourceFile) -> Result<ParsedFile> {
    let mut parser = Parser::new();

    let ts_lang = tree_sitter_language(&sf.language)?;
    parser
        .set_language(&ts_lang)
        .map_err(|e| anyhow::anyhow!("Tree-sitter language error: {e}"))?;

    let tree = parser
        .parse(&sf.source, None)
        .ok_or_else(|| anyhow::anyhow!("Tree-sitter parse returned None for {}", sf.path))?;

    Ok(ParsedFile {
        source_file: sf.clone(),
        tree,
    })
}

fn tree_sitter_language(lang: &Language) -> Result<tree_sitter::Language> {
    Ok(match lang {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
    })
}

/// Interesting node kinds to extract per language.
pub fn interesting_node_kinds(lang: &Language) -> &'static [&'static str] {
    match lang {
        Language::Rust => &[
            "function_item",
            "impl_item",
            "struct_item",
            "enum_item",
            "trait_item",
            "mod_item",
            "use_declaration",
            "ERROR",
        ],
        Language::Python => &[
            "function_definition",
            "async_function_definition",
            "class_definition",
            "import_statement",
            "import_from_statement",
            "ERROR",
        ],
        Language::JavaScript | Language::TypeScript => &[
            "function_declaration",
            "function_expression",
            "arrow_function",
            "class_declaration",
            "method_definition",
            "import_statement",
            "export_statement",
            "ERROR",
        ],
        Language::C | Language::Cpp => &[
            "function_definition",
            "declaration",
            "struct_specifier",
            "class_specifier",
            "namespace_definition",
            "preproc_include",
            "ERROR",
        ],
        Language::Go => &[
            "function_declaration",
            "method_declaration",
            "type_declaration",
            "import_declaration",
            "ERROR",
        ],
        Language::Java => &[
            "method_declaration",
            "class_declaration",
            "interface_declaration",
            "import_declaration",
            "ERROR",
        ],
    }
}

/// Extract all top-level interesting nodes from the tree.
pub fn extract_nodes<'a>(tree: &'a Tree, lang: &Language) -> Vec<Node<'a>> {
    let kinds = interesting_node_kinds(lang);
    let mut results = Vec::new();
    collect_nodes(tree.root_node(), kinds, &mut results, 0);
    results
}

fn collect_nodes<'a>(node: Node<'a>, kinds: &[&str], out: &mut Vec<Node<'a>>, depth: usize) {
    // Limit recursion to avoid stack overflow on pathological files
    if depth > 20 {
        return;
    }

    if kinds.contains(&node.kind()) {
        out.push(node);
        // Don't descend into matched nodes (we want top-level constructs)
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_nodes(child, kinds, out, depth + 1);
    }
}
