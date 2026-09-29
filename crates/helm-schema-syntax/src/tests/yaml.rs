use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::{
    ActionId, ArmLayout, BodyLayout, HoleShape, HoleShapes, LayoutUncertainty, MAX_SKELETON_BYTES,
    RenderedBody, TemplatedDocument, Undecoded, UnknownShapes, YamlOwnership, arm_layout,
    arm_skeleton, parse_go_template, parse_yaml, render_body,
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

fn render<'src>(source: &'src str, shapes: &dyn HoleShapes) -> eyre::Result<RenderedBody<'src>> {
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
/// the ownership tree (`Kind key[first..last]`, children indented), block
/// headers, and the decoded documents or why they are withheld (hole
/// output withholds them and is not printed).
fn layouts(source: &str, shapes: &dyn HoleShapes) -> eyre::Result<String> {
    let body = render(source, shapes)?;
    let BodyLayout::Arms(arms) = &body.layout else {
        return Ok(format!("body {:?}\n", body.layout));
    };
    let mut out = String::new();
    for arm in arms {
        let pieces: Vec<String> = arm
            .pieces
            .iter()
            .map(|piece| format!("p{}", piece.0))
            .collect();
        let _ = match arm_skeleton(&body, arm) {
            Ok(skeleton) => writeln!(out, "arm [{}] {:?}", pieces.join(" "), skeleton.text),
            Err(reason) => writeln!(out, "arm [{}] no skeleton: {reason:?}", pieces.join(" ")),
        };
        match arm_layout(&body, arm) {
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
        let key = if node.key { " key" } else { "" };
        let _ = writeln!(out, "{}{:?}{key}{pieces}", "  ".repeat(depth), node.kind);
    }
    for block in &owned.blocks {
        let _ = writeln!(out, "  block n{} {:?}", block.node, block.header);
    }
    match &owned.decoded {
        Ok(documents) => {
            for document in documents {
                let _ = writeln!(out, "  decoded {}", serde_json::to_string(document)?);
            }
        }
        Err(Undecoded::Placeholders) => {}
        Err(reason) => {
            let _ = writeln!(out, "  withheld {reason:?}");
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
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    BlockScalar[1..2]
          block n7 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"data":{"note":"\ntail\n"}}
        arm [p0 p2 p3] "data:\n\n    note: plain\n\n      tail\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
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
                Scalar(Plain) key[0..0]
                BlockScalar[0..2]
          block n4 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"conf":"a=1\nsuffix\n"}
        arm [p0 p2] "conf: |\n  suffix\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..1]
                Scalar(Plain) key[0..0]
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
                Scalar(Plain) key[0..0]
                Sequence[0..2]
                  Item[0..2]
                    BlockScalar[0..2]
          block n6 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"items":["a=1\nsuffix\n"]}
        arm [p0 p2] "items:\n- |\n  suffix\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..1]
                Scalar(Plain) key[0..0]
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
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..3]
        arm [p0 p3 p4] "data:\n\n  key: none-suffix\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
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
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4 p5] "data:\n  other: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
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
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain) key[1..1]
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
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain) key[1..1]
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
                Scalar(Plain) key[0..0]
                FlowMapping[0..2]
                  FlowPair[0..2]
                    Scalar(Plain) key[0..0]
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
                Scalar(Plain) key[0..0]
                Mapping[1..5]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
                  Pair[3..4]
                    Scalar(Plain) key[3..3]
                    Scalar(Plain)[4..4]
        arm [p0 p1 p2 p5] "data:\n\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4 p5] "data:\n\n  other: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p5] "data:\n\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..0]
                Scalar(Plain) key[0..0]
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
                Scalar(Plain) key[0..0]
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
                    Scalar(Plain) key[0..0]
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
    // A tagged block scalar abstains like a tagged plain one.
    sim_assert_eq!(have: layouts("a: !!str |-\n  1\n", &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "a: !!str |-\n  1\n"
          uncertain TagOrAlias
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
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
          block n4 BlockHeader { folded: false, chomping: Strip, indentation: Some(2) }
          block n7 BlockHeader { folded: true, chomping: Keep, indentation: None }
          block n10 BlockHeader { folded: false, chomping: Clip, indentation: None }
          block n13 BlockHeader { folded: true, chomping: Keep, indentation: Some(1) }
          decoded {"a":"  x","b":"y\n\n","c":"z\n","d":"w\n"}
    "#});
    Ok(())
}

/// `inline-else/helm/{inline,own,trim}` (Helm 4.2.3, `phase2/helm/cells.log`): the else written
/// mid-line, on its own line, or trimmed renders `key: H` / `key: none` alike, so all three
/// spellings own the same arms.
#[test]
fn inline_own_and_trim_cells_render_the_same_arms() -> eyre::Result<()> {
    let want = indoc! {r#"
        arm [p0 p1 p2 p4] "data:\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4] "data:\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..1]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#};
    for source in [
        indoc! {r"
            data:
            {{- if .Values.flag }}
              key: {{ .Values.x }}{{else}}
              key: none{{end}}
        "},
        indoc! {r"
            data:
            {{- if .Values.flag }}
              key: {{ .Values.x }}
            {{- else }}
              key: none
            {{- end }}
        "},
        indoc! {r"
            data:
            {{- if .Values.flag }}
              key: {{ .Values.x }}{{- else }}
              key: none{{- end }}
        "},
    ] {
        sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: want);
    }
    Ok(())
}

/// `inline-else/helm/with`: an `if`/`else` inside `with` renders three arms; an empty `cfg`
/// renders `data:` alone, which Helm decodes to `data: null`.
#[test]
fn with_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{- with .Values.cfg }}
        {{- if .flag }}
          key: {{ .x }}{{else}}
          key: none{{end}}
        {{- end }}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p4] "data:\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4] "data:\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..1]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
        arm [p0 p4] "data:\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..0]
                Scalar(Plain) key[0..0]
          decoded {"data":null}
    "#});
    Ok(())
}

/// `inline-else/helm/{elseif,elseif-own}`: one arm per branch of the chain (`H`, `Z`, `none`),
/// whether each `else if` is written mid-line or on its own line.
#[test]
fn elseif_and_elseif_own_cells_render_the_same_arms() -> eyre::Result<()> {
    let want = indoc! {r#"
        arm [p0 p1 p2 p6] "data:\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4 p6] "data:\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p5 p6] "data:\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..1]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#};
    for source in [
        indoc! {r"
            data:
            {{- if .Values.a }}
              key: {{ .Values.x }}{{else if .Values.b}}
              key: {{ .Values.z }}{{else}}
              key: none{{end}}
        "},
        indoc! {r"
            data:
            {{- if .Values.a }}
              key: {{ .Values.x }}
            {{- else if .Values.b }}
              key: {{ .Values.z }}
            {{- else }}
              key: none
            {{- end }}
        "},
    ] {
        sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: want);
    }
    Ok(())
}

/// `inline-else/helm/range`: the `range` body repeats a value-dependent number of times, so it
/// claims no layout until 3.1.
#[test]
fn range_cell_is_uncertain() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{- range .Values.items }}
        {{- if .flag }}
          key-{{ .name }}: {{ .x }}{{else}}
          none-{{ .name }}: none{{end}}
        {{- end }}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r"
        body Uncertain(Range)
    "});
    Ok(())
}

/// `two-ends`: two regions closing on one line; the taken arm is `key: H-suffix`, and every other
/// arm renders `-suffix` under `data:`, which Helm also refuses to parse.
#[test]
fn two_ends_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.a}}{{if .Values.b}}
          key: {{ .Values.x }}{{end}}{{end}}-suffix
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3] "data:\n\n  key: h2-suffix\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..3]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..3]
        arm [p0 p3] "data:\n-suffix\n"
          uncertain Parse
    "#});
    Ok(())
}

/// `opens-and-closes`: a region opened and closed inside one value renders the value alone in
/// each arm; the key stays in the shared literal.
#[test]
fn opens_and_closes_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
          key: {{if .Values.flag}}{{ .Values.x }}{{else}}none{{end}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p3] "data:\n  key: h1\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[0..2]
                  Pair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
        arm [p0 p2 p3] "data:\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[0..2]
                  Pair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#});
    Ok(())
}

/// `nested-inline`: a region nested inline in another's consequence renders one arm per reachable
/// branch (`H`, `a`, `none`).
#[test]
fn nested_inline_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
          key: {{if .Values.a}}{{if .Values.b}}{{ .Values.x }}{{else}}a{{end}}{{else}}none{{end}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p4] "data:\n  key: h1\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[0..2]
                  Pair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
        arm [p0 p2 p4] "data:\n  key: a\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[0..2]
                  Pair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"a"}}
        arm [p0 p3 p4] "data:\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[0..2]
                  Pair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#});
    Ok(())
}

/// `flow`: a region inside a flow mapping keeps the mapping in every arm (`{key: H}`,
/// `{key: none}`).
#[test]
fn flow_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data: {key: {{if .Values.flag}}{{ .Values.x }}{{else}}none{{end}}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p3] "data: {key: h1}\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                FlowMapping[0..2]
                  FlowPair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
        arm [p0 p2 p3] "data: {key: none}\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                FlowMapping[0..2]
                  FlowPair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#});
    Ok(())
}

/// `yaml-comment`: YAML comments belong to the arm that renders them (` # set` beside the value,
/// `# unset` on its own line).
#[test]
fn yaml_comment_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }} # set{{else}}
          # unset
          key: none{{end}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3 p5] "data:\n\n  key: h2 # set\n"
          Document[0..4]
            Mapping[0..4]
              Pair[0..4]
                Scalar(Plain) key[0..0]
                Mapping[1..4]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
                  Comment[3..3]
        arm [p0 p4 p5] "data:\n\n  # unset\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Comment[1..1]
                Mapping[1..2]
                  Pair[1..1]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#});
    Ok(())
}

/// `trailing-comment`: the comment after `{{end}}` is in every arm; with the region skipped it
/// is all that follows `data:`, which Helm renders as `data: null`.
#[test]
fn trailing_comment_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{end}} # trailing
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3] "data:\n\n  key: h2 # trailing\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
                  Comment[3..3]
        arm [p0 p3] "data:\n # trailing\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..0]
                Scalar(Plain) key[0..0]
              Comment[1..1]
          decoded {"data":null}
    "#});
    Ok(())
}

/// `template-comment`: template comments render nothing, in either arm or after `{{end}}`.
#[test]
fn template_comment_cell_layouts() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{if .Values.flag}}
          key: {{ .Values.x }}{{/* the value */}}{{else}}
          key: none{{end}}{{/* done */}}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p4] "data:\n\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3 p4] "data:\n\n  key: none\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Mapping[1..2]
                  Pair[1..1]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[1..1]
          decoded {"data":{"key":"none"}}
    "#});
    Ok(())
}

/// An opener whose condition spans lines and a closer split across lines trim and branch like
/// their one-line spellings.
#[test]
fn multiline_actions_render_like_single_line_ones() -> eyre::Result<()> {
    let source = indoc! {r"
        data:
        {{- if and
              .Values.a
              .Values.b }}
          key: {{ .Values.x }}
        {{- end
        }}
    "};
    sim_assert_eq!(have: layouts(source, &ScalarShapes)?, want: indoc! {r#"
        arm [p0 p1 p2 p3] "data:\n  key: h2\n"
          Document[0..3]
            Mapping[0..3]
              Pair[0..3]
                Scalar(Plain) key[0..0]
                Mapping[1..3]
                  Pair[1..2]
                    Scalar(Plain) key[1..1]
                    Scalar(Plain)[2..2]
        arm [p0 p3] "data:\n"
          Document[0..1]
            Mapping[0..1]
              Pair[0..0]
                Scalar(Plain) key[0..0]
          decoded {"data":null}
    "#});
    Ok(())
}

/// A `---` indented inside a block scalar is content; at column zero it starts the next document.
/// Helm's manifest splitter also needs the marker right after a line break.
#[test]
fn document_markers_split_only_at_column_zero() -> eyre::Result<()> {
    let source = indoc! {r"
        a: |
          ---
          x
        ---
        b: 1
    "};
    sim_assert_eq!(have: layouts(source, &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "a: |\n  ---\n  x\n---\nb: 1\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          block n4 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"a":"---\nx\n"}
          decoded {"b":1}
    "#});
    Ok(())
}

/// Each cell's layouts, one block per cell under its source.
fn cell_layouts(cells: &[&str], shapes: &dyn HoleShapes) -> eyre::Result<String> {
    let mut out = String::new();
    for cell in cells {
        let _ = writeln!(out, "# {cell:?}");
        out.push_str(&layouts(cell, shapes)?);
    }
    Ok(out)
}

/// P1 (`phase2/helm/cells-rework1.log`, `flow-empty*`): Helm renders
/// `items: [{{ .Values.x }}]` as `[abc]` for `x=abc` and `[]` for `x=""`, so an
/// output that may be empty can remove a flow sequence element; a mapping
/// value or block item that empties keeps its entry with a null value.
#[test]
fn empty_output_may_not_remove_a_flow_sequence_element() -> eyre::Result<()> {
    let cells = [
        "items: [{{ .Values.x }}]\n",
        "items: [a, {{ .Values.x }}]\n",
        "items: {k: {{ .Values.x }}}\n",
        indoc! {r"
            items:
            - {{ .Values.x }}
        "},
    ];
    sim_assert_eq!(have: cell_layouts(&cells, &MaybeEmptyScalarShapes)?, want: indoc! {r#"
        # "items: [{{ .Values.x }}]\n"
        arm [p0 p1 p2] "items: [h1]\n"
          uncertain Substitution
        # "items: [a, {{ .Values.x }}]\n"
        arm [p0 p1 p2] "items: [a, h1]\n"
          uncertain Substitution
        # "items: {k: {{ .Values.x }}}\n"
        arm [p0 p1 p2] "items: {k: h1}\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                FlowMapping[0..2]
                  FlowPair[0..1]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[1..1]
        # "items:\n- {{ .Values.x }}\n"
        arm [p0 p1 p2] "items:\n- h1\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..0]
                Sequence[0..2]
                  Item[0..1]
                    Scalar(Plain)[1..1]
    "#});
    sim_assert_eq!(
        have: layouts("items: [{{ .Values.x }}]\n", &ScalarShapes)?,
        want: indoc! {r#"
            arm [p0 p1 p2] "items: [h1]\n"
              Document[0..2]
                Mapping[0..2]
                  Pair[0..2]
                    Scalar(Plain) key[0..0]
                    FlowSequence[0..2]
                      Scalar(Plain)[1..1]
        "#}
    );
    Ok(())
}

/// P4 (`cells-rework1.log`, `merge-*`): an empty `x` turns `<x<` into the
/// merge key, and Helm merges a mapping or sequence value into the enclosing
/// mapping (`{"injected":1,"own":2}`) but refuses a scalar or null value; a
/// quoted `"<<"` is an ordinary key. A key that is or can become `<<` over a
/// collection makes the arm uncertain.
#[test]
fn merge_keys_over_collections_are_uncertain() -> eyre::Result<()> {
    let maybe_empty = [
        indoc! {r"
            <{{ .Values.x }}<: {injected: 1}
            own: 2
        "},
        "<{{ .Values.x }}<: v\n",
    ];
    sim_assert_eq!(have: cell_layouts(&maybe_empty, &MaybeEmptyScalarShapes)?, want: indoc! {r#"
        # "<{{ .Values.x }}<: {injected: 1}\nown: 2\n"
        arm [p0 p1 p2] "<h1<: {injected: 1}\nown: 2\n"
          uncertain MergeKey
        # "<{{ .Values.x }}<: v\n"
        arm [p0 p1 p2] "<h1<: v\n"
          Document[0..2]
            Mapping[0..2]
              Pair[0..2]
                Scalar(Plain) key[0..2]
                Scalar(Plain)[2..2]
    "#});
    sim_assert_eq!(
        have: layouts(
            indoc! {r"
                {{ .Values.x }}:
                  injected: 1
            "},
            &ScalarShapes
        )?,
        want: indoc! {r#"
            arm [p0 p1] "h0:\n  injected: 1\n"
              uncertain MergeKey
        "#}
    );
    let literal = [
        indoc! {r"
            <<: {c: 1}
            e: 2
        "},
        indoc! {r"
            <<: [{c: 1}, {d: 2}]
            e: 2
        "},
        indoc! {r#"
            "<<": {c: 1}
            e: 2
        "#},
        indoc! {r"
            <<: 1
            e: 2
        "},
    ];
    sim_assert_eq!(have: cell_layouts(&literal, &UnknownShapes)?, want: indoc! {r#"
        # "<<: {c: 1}\ne: 2\n"
        arm [p0] "<<: {c: 1}\ne: 2\n"
          uncertain MergeKey
        # "<<: [{c: 1}, {d: 2}]\ne: 2\n"
        arm [p0] "<<: [{c: 1}, {d: 2}]\ne: 2\n"
          uncertain MergeKey
        # "\"<<\": {c: 1}\ne: 2\n"
        arm [p0] "\"<<\": {c: 1}\ne: 2\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(DoubleQuoted) key[0..0]
                FlowMapping[0..0]
                  FlowPair[0..0]
                    Scalar(Plain) key[0..0]
                    Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          decoded {"<<":{"c":1},"e":2}
        # "<<: 1\ne: 2\n"
        arm [p0] "<<: 1\ne: 2\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(MergeKey)
    "#});
    Ok(())
}

/// P2 (`helm/dialect-agree.log`): where no plain scalar diverges between
/// YAML 1.1 and 1.2, a literal arm's decoded documents equal Helm's
/// `fromYaml` values.
#[test]
fn literal_decoding_matches_helm_where_the_dialects_agree() -> eyre::Result<()> {
    let agree = indoc! {r#"
        s: abc
        i: 10
        f: 1.5
        b: true
        nul: ~
        qy: "yes"
        qo: 'on'
        block: |
          x
        "y": 1
        seq: [a, 1]
    "#};
    sim_assert_eq!(have: layouts(agree, &UnknownShapes)?, want: indoc! {r#"
        arm [p0] "s: abc\ni: 10\nf: 1.5\nb: true\nnul: ~\nqy: \"yes\"\nqo: 'on'\nblock: |\n  x\n\"y\": 1\nseq: [a, 1]\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(DoubleQuoted)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(SingleQuoted)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                BlockScalar[0..0]
              Pair[0..0]
                Scalar(DoubleQuoted) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                FlowSequence[0..0]
                  Scalar(Plain)[0..0]
                  Scalar(Plain)[0..0]
          block n25 BlockHeader { folded: false, chomping: Clip, indentation: None }
          decoded {"b":true,"block":"x\n","f":1.5,"i":10,"nul":null,"qo":"on","qy":"yes","s":"abc","seq":["a",1],"y":1}
    "#});
    Ok(())
}

/// P2 (`helm/dialect.log`, `helm/dialect-agree.log`): Helm reads `y`, `yes`
/// and `on` as `true`, `010` as 8, the key `y` as `"true"`, the key `n` as
/// `"false"` and `1:` as `"1"`, so those arms withhold their decoded values
/// with the divergence; a duplicate key decodes to the later value, as Helm's.
#[test]
fn dialect_sensitive_literals_are_withheld() -> eyre::Result<()> {
    let diverge = [
        "a: y\n",
        "a: yes\n",
        "a: on\n",
        "a: 010\n",
        "y: 1\n",
        "n: ~\n",
        "1: a\n",
        indoc! {r"
            a: 1
            a: 2
        "},
    ];
    sim_assert_eq!(have: cell_layouts(&diverge, &UnknownShapes)?, want: indoc! {r#"
        # "a: y\n"
        arm [p0] "a: y\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Boolean)
        # "a: yes\n"
        arm [p0] "a: yes\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Boolean)
        # "a: on\n"
        arm [p0] "a: on\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Boolean)
        # "a: 010\n"
        arm [p0] "a: 010\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Number)
        # "y: 1\n"
        arm [p0] "y: 1\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Boolean)
        # "n: ~\n"
        arm [p0] "n: ~\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(Boolean)
        # "1: a\n"
        arm [p0] "1: a\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          withheld Dialect(NonStringKey)
        # "a: 1\na: 2\n"
        arm [p0] "a: 1\na: 2\n"
          Document[0..0]
            Mapping[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
              Pair[0..0]
                Scalar(Plain) key[0..0]
                Scalar(Plain)[0..0]
          decoded {"a":2}
    "#});
    Ok(())
}

/// P3 (`cells-rework1.log`, `block-*`): a competing definition of the name
/// wins over a `block`'s written body, in the same file or another, so the
/// block renders an unresolved call.
#[test]
fn block_renders_an_unresolved_call() -> eyre::Result<()> {
    let source = "{{ define \"h\" }}overridden: value{{ end }}{{ block \"h\" . }} {{ end }}\n";
    sim_assert_eq!(have: layouts(source, &UnknownShapes)?, want: indoc! {r#"
        arm [p1 p3] "h0\n"
          uncertain RawHole
    "#});
    Ok(())
}

/// P5: a body's pieces resolve only in the source it was rendered from; a
/// span outside it, or a piece the body does not hold, is never read as text.
#[test]
fn pieces_resolve_only_in_their_own_source() -> eyre::Result<()> {
    let source = indoc! {r"
        a: 1
        b: 2
    "};
    let body = render(source, &UnknownShapes)?;
    let BodyLayout::Arms(arms) = &body.layout else {
        eyre::bail!("a literal body has one arm");
    };
    let arm = arms.first().ok_or_eyre("one arm")?;
    let shorter = RenderedBody {
        source: "a: 1\n",
        ..body.clone()
    };
    sim_assert_eq!(
        have: arm_layout(&shorter, arm),
        want: ArmLayout::Uncertain(LayoutUncertainty::Provenance)
    );
    let foreign = crate::RenderedArm {
        paths: arm.paths.clone(),
        pieces: vec![crate::PieceId(body.pieces.len())],
    };
    sim_assert_eq!(
        have: arm_layout(&body, &foreign),
        want: ArmLayout::Uncertain(LayoutUncertainty::Provenance)
    );
    Ok(())
}

/// P6: a skeleton of [`MAX_SKELETON_BYTES`] is parsed; one byte more is an
/// overflow and is never built or parsed.
#[test]
fn skeleton_bytes_are_bounded() -> eyre::Result<()> {
    let at_bound = format!("a: {}\n", "x".repeat(MAX_SKELETON_BYTES - 4));
    let past_bound = format!("a: {}\n", "x".repeat(MAX_SKELETON_BYTES - 3));
    for (source, known) in [(&at_bound, true), (&past_bound, false)] {
        let body = render(source, &UnknownShapes)?;
        let BodyLayout::Arms(arms) = &body.layout else {
            eyre::bail!("a literal body has one arm");
        };
        let arm = arms.first().ok_or_eyre("one arm")?;
        let layout = arm_layout(&body, arm);
        sim_assert_eq!(have: matches!(layout, ArmLayout::Known(_)), want: known);
        if !known {
            sim_assert_eq!(have: layout, want: ArmLayout::Uncertain(LayoutUncertainty::Overflow));
        }
    }
    Ok(())
}
