use test_util::prelude::sim_assert_eq;

#[test]
fn go_template_loads_grammar() {
    let mut parser = tree_sitter::Parser::new();
    let language = tree_sitter::Language::new(super::go_template::language());
    sim_assert_eq!(have: language.name(), want: Some("gotmpl"));
    parser.set_language(&language).unwrap();
}
