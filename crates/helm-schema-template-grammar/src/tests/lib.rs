use test_util::prelude::sim_assert_eq;

#[test]
fn yaml_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::yaml::language());
    sim_assert_eq!(have: language.name(), want: Some("yaml"));
    parser.set_language(&language).unwrap();
}

#[test]
fn go_template_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::go_template::language());
    sim_assert_eq!(have: language.name(), want: Some("gotmpl"));
    parser.set_language(&language).unwrap();
}

#[test]
fn helm_template_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::helm_template::language());
    sim_assert_eq!(have: language.name(), want: Some("helm_template"));
    parser.set_language(&language).unwrap();
}
