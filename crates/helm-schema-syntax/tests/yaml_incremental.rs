//! Incremental YAML parsing preserves the tree across interleaved inputs.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_syntax::parse_yaml;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[test]
fn growing_and_interleaved_yaml_has_the_same_tree_as_a_fresh_parse() -> eyre::Result<()> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter::Language::new(tree_sitter_yaml::LANGUAGE))?;
    let prefixes = [
        "note: été\n",
        "note: être\n",
        "spec:\n",
        indoc! {"
            spec:
              items: ["},
        "other: true\n",
        indoc! {"
            spec:
              items: [one,"},
        indoc! {"
            other: true
              trailing: false
        "},
        indoc! {"
            spec:
              items: [one, two]
        "},
        indoc! {"
            spec:
              items: [one, two]
              note: |
                indented
                content
            ---
            next: document
        "},
    ];
    for source in prefixes.into_iter().cycle().take(36) {
        let warm = parse_yaml(source).ok_or_eyre("shared parser did not return a tree")?;
        let fresh = parser
            .parse(source, None)
            .ok_or_eyre("fresh parser did not return a tree")?;
        sim_assert_eq!(have: warm.root_node().to_sexp(), want: fresh.root_node().to_sexp());
    }
    Ok(())
}
