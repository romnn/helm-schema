use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::{
    ActionId, ArmLayout, BodyLayout, HoleShape, HoleShapes, RenderedBody, TemplatedDocument,
    UnknownShapes, YamlOwnership, arm_layout, parse_go_template, parse_yaml, render_body,
};

fn sexp(text: &str) -> eyre::Result<String> {
    Ok(parse_yaml(text)
        .ok_or_eyre("tree-sitter-yaml parse")?
        .root_node()
        .to_sexp())
}

/// tree-sitter-yaml 0.7.2 was developed against tree-sitter 0.25; the
/// workspace runs 0.27. Loading its language and parsing a mapping proves
/// the runtime accepts the grammar's ABI.
#[test]
fn tree_sitter_yaml_parses_on_the_workspace_runtime() -> eyre::Result<()> {
    sim_assert_eq!(have: sexp("a: 1\n")?, want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))))");
    Ok(())
}

/// Helm 4.2.3 reads YAML 1.1 (`phase2/helm/dialect.log`: `fromYaml` turns
/// `y`, `yes`, `on` into `true`, `010` into `8`, a `y:` key into `"true"`,
/// honours `!!str` and expands `<<` merges). The grammar resolves the same
/// scalars with the YAML 1.2 core schema, so its plain-scalar kinds disagree
/// with Helm and must never type a value: these trees pin that disagreement.
/// The grammar also leaves `!!str 1` an integer under its tag and the `<<`
/// merge an ordinary key over an alias, so tags and aliases are left
/// unresolved and must make a layout abstain.
#[test]
fn grammar_scalar_kinds_follow_yaml_1_2_not_helm() -> eyre::Result<()> {
    sim_assert_eq!(
        have: sexp("y: y\nyes: yes\non: on\noctal: 010\n")?,
        want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (string_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))))"
    );
    sim_assert_eq!(
        have: sexp("s: !!str 1\nb: &x {c: 1}\nd:\n  <<: *x\n")?,
        want: "(stream (document (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (tag) (plain_scalar (integer_scalar)))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (anchor (anchor_name)) (flow_mapping (flow_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (plain_scalar (integer_scalar))))))) (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (block_node (block_mapping (block_mapping_pair key: (flow_node (plain_scalar (string_scalar))) value: (flow_node (alias (alias_name)))))))))))"
    );
    Ok(())
}

/// Every output proved to be nonempty scalar text.
struct ScalarShapes;

impl HoleShapes for ScalarShapes {
    fn shape(&self, _action: ActionId) -> HoleShape {
        HoleShape::ScalarText { nonempty: true }
    }
}

fn render(source: &str, shapes: &dyn HoleShapes) -> eyre::Result<RenderedBody> {
    let tree = parse_go_template(source).ok_or_eyre("go-template parse")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    Ok(render_body(&document, tree.root_node(), shapes))
}

/// Every output proved to be scalar text that may be empty.
struct MaybeEmptyScalarShapes;

impl HoleShapes for MaybeEmptyScalarShapes {
    fn shape(&self, _action: ActionId) -> HoleShape {
        HoleShape::ScalarText { nonempty: false }
    }
}

/// One block per arm: its pieces, skeleton, and either the uncertainty or
/// the ownership tree (`Kind[first..last]`, children indented), block
/// headers and literal-decoded documents.
fn layouts(source: &str, shapes: &dyn HoleShapes) -> eyre::Result<String> {
    let body = render(source, shapes)?;
    let BodyLayout::Arms(arms) = &body.layout else {
        return Ok(format!("body {:?}\n", body.layout));
    };
    let mut out = String::new();
    for arm in arms {
        let (skeleton, layout) = arm_layout(&body, arm, source);
        let pieces: Vec<String> = arm
            .pieces
            .iter()
            .map(|piece| format!("p{}", piece.0))
            .collect();
        let _ = writeln!(out, "arm [{}] {:?}", pieces.join(" "), skeleton.text);
        match layout {
            ArmLayout::Uncertain(reason) => {
                let _ = writeln!(out, "  uncertain {reason:?}");
            }
            ArmLayout::Known(owned) => dump_ownership(&owned, &mut out)?,
        }
    }
    Ok(out)
}

fn dump_ownership(owned: &YamlOwnership, out: &mut String) -> eyre::Result<()> {
    let mut depths: Vec<usize> = Vec::new();
    for node in &owned.nodes {
        let depth = node
            .parent
            .and_then(|parent| depths.get(parent))
            .map_or(1, |depth| depth + 1);
        depths.push(depth);
        let pieces = node
            .pieces
            .map_or_else(String::new, |(first, last)| format!("[{first}..{last}]"));
        let _ = writeln!(out, "{}{:?}{pieces}", "  ".repeat(depth), node.kind);
    }
    for block in &owned.blocks {
        let _ = writeln!(out, "  block n{} {:?}", block.node, block.header);
    }
    if let Some(documents) = &owned.decoded {
        for document in documents {
            let _ = writeln!(out, "  decoded {}", serde_json::to_string(document)?);
        }
    }
    Ok(())
}

/// The D5 witnesses (`round8-d5-land-evidence/helm/{rotation,suffix-mapping,
/// suffix-item}.log`, Helm 4.2.3). Each arm is literal-only, so its decoded
/// documents must equal what Helm's render decodes to: `note` is
/// `"\ntail\n"` / `"plain\ntail"`, `conf` and `items[0]` are
/// `"a=1\nsuffix\n"` / `"suffix\n"`.
#[test]
fn d5_witness_arms_decode_to_their_rendered_values() -> eyre::Result<()> {
    let rotation = indoc! {"
        data:
        {{ if .Values.a }}
          note: |
        {{ else }}
            note: plain
        {{ end }}
              tail
    "};
    sim_assert_eq!(have: layouts(rotation, &UnknownShapes)?, want: indoc! {r#"
        arm [p0 p1 p3] "data:\n\n  note: |\n\n      tail\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    BlockScalar[1..2]
          block n7 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"data":{"note":"\ntail\n"}}
        arm [p0 p2 p3] "data:\n\n    note: plain\n\n      tail\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[1..2]
          decoded {"data":{"note":"plain\ntail"}}
    "#});
    let suffix_mapping = indoc! {"
        conf: |
        {{- if .Values.a }}
          a=1
        {{- end }}
          suffix
    "};
    sim_assert_eq!(have: layouts(suffix_mapping, &UnknownShapes)?, want: indoc! {r#"
        arm [p0 p1 p2] "conf: |\n  a=1\n  suffix\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                BlockScalar[0..2]
          block n4 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"conf":"a=1\nsuffix\n"}
        arm [p0 p2] "conf: |\n  suffix\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..1]
                Scalar(Plain)[0..0]
                BlockScalar[0..1]
          block n4 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"conf":"suffix\n"}
    "#});
    let suffix_item = indoc! {"
        items:
        - |
        {{- if .Values.a }}
          a=1
        {{- end }}
          suffix
    "};
    sim_assert_eq!(have: layouts(suffix_item, &UnknownShapes)?, want: indoc! {r#"
        arm [p0 p1 p2] "items:\n- |\n  a=1\n  suffix\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                Sequence[0..2]
                  Item[0..2]
                    BlockScalar[0..2]
          block n6 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"items":["a=1\nsuffix\n"]}
        arm [p0 p2] "items:\n- |\n  suffix\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..1]
                Scalar(Plain)[0..0]
                Sequence[0..1]
                  Item[0..1]
                    BlockScalar[0..1]
          block n6 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"items":["suffix\n"]}
    "#});
    Ok(())
}

// The inline-else review cells (`inline-else/helm/matrix.log` and the
// reviewers' adjacent cells): each arm concatenates its own pieces across
// the skipped branch, so the text after `{{ end }}` completes the
// consequence's scalar (`key: h-suffix`) and content after `{{ else }}` is a
// mapping entry of the alternative.

/// `suffix`: the text after `{{end}}` completes the consequence's scalar across the skipped alternative (`key: h-suffix`); the alternative renders `key: none-suffix`.
#[test]
fn suffix_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{else}}
          key: none{{end}}-suffix
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p4] "data:\n\n  key: h2-suffix\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..3]
        arm [p0 p3 p4] "data:\n\n  key: none-suffix\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[1..2]
          decoded {"data":{"key":"none-suffix"}}
    "#});
    Ok(())
}

/// `midline-key`: content after `{{ else }}` on the same line is a mapping entry of the alternative (`other: h`).
#[test]
fn midline_key_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{ else }}  other: {{ .Values.z }}{{ end }}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p5] "data:\n\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4 p5] "data:\n  other: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..2]
    "#});
    Ok(())
}

/// `quoted-crossing`: the double-quoted scalar opens before the hole and closes after `{{end}}`; the empty arm renders a lone `"` that YAML cannot parse.
#[test]
fn quoted_crossing_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r#"
        data:
        {{if .Values.flag}}
          key: "{{ .Values.x }}{{end}}"
    "#};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3] "data:\n\n  key: \"h2\"\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain)[1..1]
                    Scalar(DoubleQuoted)[1..3]
        arm [p0 p3] "data:\n\"\n"
          uncertain Parse
    "#});
    Ok(())
}

/// `end-suffix`: `key: h-suffix` is one scalar (Helm renders `key: true-suffix` for `x=true`); the empty arm renders `-suffix` under `data:`, which YAML cannot parse.
#[test]
fn end_suffix_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{end}}-suffix
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3] "data:\n\n  key: h2-suffix\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..3]
        arm [p0 p3] "data:\n-suffix\n"
          uncertain Parse
    "#});
    Ok(())
}

/// `flow-crossing`: the flow mapping and its quoted value close after `{{end}}`; the empty arm renders a lone `"}`.
#[test]
fn flow_crossing_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{if .Values.flag}}
        data: {key: "{{ .Values.x }}{{end}}"}
    "#};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2] "\ndata: {key: \"h1\"}\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain)[0..0]
                FlowMapping[0..2]
                  FlowPair[0..2]
                    Scalar(Plain)[0..0]
                    Scalar(DoubleQuoted)[0..2]
        arm [p2] "\"}\n"
          uncertain Parse
    "#});
    Ok(())
}

/// `end-new-if`: two sequential regions render four arms; with both taken, `key` and `other` are sibling entries.
#[test]
fn end_new_if_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{ end }}{{if .Values.g}}
          other: {{ .Values.z }}{{end}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3 p4 p5] "data:\n\n  key: h2\n  other: h4\n"
          Document[0..5]
            Mapping[0..5]
              Pair[0..5]
                Scalar(Plain)[0..0]
                Mapping[1..5]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..2]
                  Pair[3..4]
                    Scalar(Plain)[3..3]
                    Scalar(Plain)[4..4]
        arm [p0 p1 p2 p5] "data:\n\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4 p5] "data:\n\n  other: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain)[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain)[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p5] "data:\n\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..0]
                Scalar(Plain)[0..0]
          decoded {"data":null}
    "#});
    Ok(())
}

/// A hole of unknown shape leaves its own arm uncertain; the sibling arm
/// that renders literal text only keeps its known ownership.
#[test]
fn known_arm_beside_an_uncertain_arm() -> eyre::Result<()> {
    let source = indoc! {"
        {{ if .Values.a }}k: v
        {{ else }}k: {{ .Values.x }}
        {{ end }}"};
    sim_assert_eq!(have: layouts(source, &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "k: v\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain)[0..0]
                Scalar(Plain)[0..0]
          decoded {"k":"v"}
        arm [p1 p2 p3] "k: h1\n"
          uncertain RawHole
    "#});
    Ok(())
}

/// An output that may be empty is certain only if the empty output leaves
/// the structure in place: a value-only placeholder may vanish, but `k:` glued
/// to the next word stops being a mapping key.
#[test]
fn possibly_empty_output_must_preserve_structure() -> eyre::Result<()> {
    sim_assert_eq!(
        have: layouts("k: {{ .Values.x }}\n", &MaybeEmptyScalarShapes)?,
        want: indoc! {r#"
        arm [p0 p1 p2] "k: h1\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..1]
                Scalar(Plain)[0..0]
                Scalar(Plain)[1..1]
    "#}
    );
    sim_assert_eq!(
        have: layouts("k:{{ .Values.x }} v\n", &MaybeEmptyScalarShapes)?,
        want: indoc! {r#"
        arm [p0 p1 p2] "k:h1 v\n"
          uncertain Substitution
    "#}
    );
    Ok(())
}

/// Unresolved tags and aliases, and a skeleton the grammar cannot parse,
/// claim no layout.
#[test]
fn tags_aliases_and_parse_errors_are_uncertain() -> eyre::Result<()> {
    sim_assert_eq!(
        have: layouts("a: !!str 1\nb: &x {c: 1}\nd: *x\n", &UnknownShapes)?,
        want: indoc! {r#"
        arm [p0] "a: !!str 1\nb: &x {c: 1}\nd: *x\n"
          uncertain TagOrAlias
    "#}
    );
    sim_assert_eq!(have: layouts("a: [1,\nb: 2\n", &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "a: [1,\nb: 2\n"
          uncertain Parse
    "#});
    Ok(())
}

/// Block headers decode as YAML defines them: style, then chomping and an
/// indentation indicator in either order.
#[test]
fn block_headers_decode_style_chomping_and_indentation() -> eyre::Result<()> {
    let source = indoc! {"
        a: |2-
            x
        b: >+
          y

        c: |
          z
        d: >1+ # note
         w
    "};
    sim_assert_eq!(have: layouts(source, &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "a: |2-\n    x\nb: >+\n  y\n\nc: |\n  z\nd: >1+ # note\n w\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain)[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain)[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain)[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain)[0..0]
                BlockScalar[0..0]
          block n4 BlockHeader { folded: false, chomping: Strip, indentation: Some(2) }
          block n7 BlockHeader { folded: true, chomping: Keep, indentation: None }
          block n10 BlockHeader { folded: false, chomping: Clip, indentation: None }
          block n13 BlockHeader { folded: true, chomping: Keep, indentation: Some(1) }
          decoded {"a":"  x","b":"y\n\n","c":"z\n","d":"w\n"}
    "#});
    Ok(())
}
