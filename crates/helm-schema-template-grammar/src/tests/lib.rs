#[test]
fn yaml_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::yaml::language());
    parser.set_language(&language).unwrap();
}

#[test]
fn go_template_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::go_template::language());
    parser.set_language(&language).unwrap();
}

#[test]
fn helm_template_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::helm_template::language());
    parser.set_language(&language).unwrap();
}
