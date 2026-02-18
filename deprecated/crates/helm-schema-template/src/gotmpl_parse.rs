use tree_sitter::{Parser, Tree};

#[derive(Debug)]
pub struct GotmplParse {
    pub tree: Tree,
    pub source: String,
}

pub fn parse_gotmpl_expr(source: &str) -> Option<GotmplParse> {
    let mut parser = Parser::new();
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    Some(GotmplParse {
        tree,
        source: source.to_owned(),
    })
}
