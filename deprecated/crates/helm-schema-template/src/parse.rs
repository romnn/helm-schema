#[derive(Debug)]
pub struct Parsed {
    pub tree: tree_sitter::Tree,
    pub source: String,
}

// Parse the whole template (with YAML + {{...}}) using go-template grammar
pub fn parse_gotmpl_document(source: &str) -> Option<Parsed> {
    let mut parser = tree_sitter::Parser::new();
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;

    if std::env::var("HELM_SCHEMA_DEBUG_AST")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        let ast = crate::fmt::SExpr::parse_tree(&tree.root_node(), source);
        eprintln!("=====================\n{}\n", ast.to_string_pretty());
    }

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
