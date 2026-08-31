//! Destructured range headers: the assignment form (`range $i, $v = …`)
//! must expose the same key/value bindings as the declaring form. The
//! grammar used to hard-code `:=`, sending the `=` form into error
//! recovery and silently dropping the element binding.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{
    parse_go_template, range_destructured_key_variable, range_destructured_value_variable,
    range_has_destructured_variable_definition,
};
use test_util::prelude::sim_assert_eq;

fn first_range_action(tree: &tree_sitter::Tree) -> eyre::Result<tree_sitter::Node<'_>> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    root.named_children(&mut cursor)
        .find(|node| node.kind() == "range_action")
        .ok_or_eyre("no range_action in tree")
}

#[test]
fn assignment_form_binds_key_and_value_like_the_declaring_form() -> eyre::Result<()> {
    for source in [
        "{{ range $i, $v := .Values.items }}{{ $v }}{{ end }}",
        "{{ range $i, $v = .Values.items }}{{ $v }}{{ end }}",
    ] {
        let tree = parse_go_template(source).ok_or_eyre("parse failure")?;
        let range = first_range_action(&tree)?;
        assert!(
            range_has_destructured_variable_definition(range),
            "destructured header not detected in {source}"
        );
        sim_assert_eq!(
            have: range_destructured_key_variable(range, source),
            want: Some("i".to_string())
        );
        sim_assert_eq!(
            have: range_destructured_value_variable(range, source),
            want: Some("v".to_string())
        );
    }
    Ok(())
}
