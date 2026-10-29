//! Literal-node projection controls for templated YAML.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_syntax::{Node, TemplatedDocument};
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

#[test]
fn projects_block_and_flow_values_around_standalone_holes() -> eyre::Result<()> {
    let source = indoc! {r"
        root:
          literal: {enabled: true, names: [one, two]}
          {{ .Values.extra | toYaml | nindent 2 }}
          sequence:
            - name: first
              count: 2
            {{ .Values.items | toYaml | nindent 4 }}
            - name: second
              count: 3
    "};
    let document = TemplatedDocument::parse(source);
    let Node::Mapping(root) = document.roots().first().ok_or_eyre("root node")? else {
        return Err(eyre::eyre!("root node is not a mapping entry"));
    };

    sim_assert_eq!(
        have: document.literal_mapping_value(root),
        want: Some(json!({
            "literal": {"enabled": true, "names": ["one", "two"]},
            "sequence": [
                {"name": "first", "count": 2},
                {"name": "second", "count": 3},
            ],
        }))
    );

    Ok(())
}

#[test]
fn rejects_a_hole_inside_a_scalar_value() -> eyre::Result<()> {
    let source = indoc! {r"
        root:
          value: prefix-{{ .Values.suffix }}
    "};
    let document = TemplatedDocument::parse(source);
    let Node::Mapping(root) = document.roots().first().ok_or_eyre("root node")? else {
        return Err(eyre::eyre!("root node is not a mapping entry"));
    };

    sim_assert_eq!(have: document.literal_mapping_value(root), want: None);

    Ok(())
}

#[test]
fn block_scalars_read_as_in_their_document() -> eyre::Result<()> {
    // Helm reads each block scalar followed by its line break; a block that
    // needs its trailing blank lines or parent indentation stays unknown.
    let source = indoc! {r"
        root:
          clip: |
            a
            b
          strip: |-
            a
          folded: >
            a
            b
        kept:
          keep: |+
            a

        indented:
          explicit: |2
              a
    "};
    let document = TemplatedDocument::parse(source);
    let mut values = Vec::new();
    for node in document.roots() {
        let Node::Mapping(entry) = node else {
            return Err(eyre::eyre!("root node is not a mapping entry"));
        };
        values.push(document.literal_mapping_value(entry));
    }

    sim_assert_eq!(
        have: values,
        want: vec![
            Some(json!({"clip": "a\nb\n", "strip": "a", "folded": "a b\n"})),
            None,
            None,
        ]
    );

    Ok(())
}
