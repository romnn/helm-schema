#[derive(Debug)]
pub struct Parsed {
    pub tree: tree_sitter::Tree,
    pub source: String,
}

// NEW: parse the whole template (with YAML + {{...}}) using go-template grammar
pub fn parse_gotmpl_document(source: &str) -> Option<Parsed> {
    let mut parser = tree_sitter::Parser::new();
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    Some(Parsed {
        tree,
        source: source.to_string(),
    })
}

// Collect byte ranges of templating chunks (everything except plain text)
pub fn template_node_byte_ranges(parsed: &Parsed) -> Vec<std::ops::Range<usize>> {
    let root = parsed.tree.root_node();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // go-template grammar uses "text" for YAML slices; everything else is templating
        if child.is_named() && child.kind() != "text" {
            out.push(child.byte_range());
        }
    }
    out
}
