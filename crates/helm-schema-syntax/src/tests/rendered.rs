use std::fmt::Write as _;

use color_eyre::eyre::{self, OptionExt as _};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::{
    BodyLayout, HoleShapes, MAX_LAYOUT_STATES, RenderedBody, RenderedPiece, TemplatedDocument,
    UnknownShapes, parse_go_template, render_body,
};

fn render<'src>(source: &'src str, shapes: &dyn HoleShapes) -> eyre::Result<RenderedBody<'src>> {
    let tree = parse_go_template(source).ok_or_eyre("go-template parse")?;
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    Ok(render_body(&document, tree.root_node(), shapes))
}

/// The body's pieces and arms, one per line: `p<i> text "…"` / `p<i> hole
/// a<action> <shape>`, then each arm's pieces and choice paths.
fn dump_body(body: &RenderedBody<'_>) -> String {
    let mut out = String::new();
    for (index, piece) in body.pieces.iter().enumerate() {
        match piece {
            RenderedPiece::Text { span, .. } => {
                let text = body.source.get(span.start..span.end).unwrap_or_default();
                let _ = writeln!(out, "p{index} text {text:?}");
            }
            RenderedPiece::Hole { action, shape, .. } => {
                let _ = writeln!(out, "p{index} hole a{} {shape:?}", action.0);
            }
        }
    }
    match &body.layout {
        BodyLayout::Uncertain(reason) => {
            let _ = writeln!(out, "uncertain {reason:?}");
        }
        BodyLayout::Arms(arms) => {
            for arm in arms {
                let pieces: Vec<String> = arm.pieces.iter().map(|p| format!("p{}", p.0)).collect();
                let paths: Vec<String> = arm
                    .paths
                    .iter()
                    .map(|path| {
                        let choices: Vec<String> = path
                            .iter()
                            .map(|choice| format!("r{}={}", choice.region, choice.branch))
                            .collect();
                        format!("{{{}}}", choices.join(" "))
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "arm [{}] paths {}",
                    pieces.join(" "),
                    paths.join(" | ")
                );
            }
        }
    }
    out
}

fn dump(source: &str) -> eyre::Result<String> {
    Ok(dump_body(&render(source, &UnknownShapes)?))
}

/// An `if` / `else if` / `else` chain: one arm per branch, each with its own
/// literal and hole pieces; `{{-` and `-}}` trim the adjacent literal
/// whitespace, line breaks included, and nothing else.
#[test]
fn if_chain_renders_one_arm_per_branch_with_trimmed_literals() -> eyre::Result<()> {
    let source = indoc! {"
        a:
        {{- if .Values.x }}
          b: {{ .Values.y }}
        {{- else if .Values.z -}}
          c: 1
        {{ else }}
          d: 2
        {{- end }}
    "};
    sim_assert_eq!(have: dump(source)?, want: indoc! {r#"
        p0 text "a:"
        p1 text "\n  b: "
        p2 hole a1 Unknown
        p3 text "c: 1\n"
        p4 text "\n  d: 2"
        p5 text "\n"
        arm [p0 p1 p2 p5] paths {r0=0}
        arm [p0 p3 p5] paths {r0=1}
        arm [p0 p4 p5] paths {r0=2}
    "#});
    Ok(())
}

/// `if`/`with` without a bare `else` also render nothing; `define` renders
/// nothing where it is written, and `block` renders the output of its call
/// (a hole), never its written body.
#[test]
fn optional_regions_define_and_block_render_structurally() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- define "h" }}ignored{{ end }}
        a: {{ if .Values.x }}1{{ end }}
        {{ with .Values.y }}b: {{ . }}{{ end }}
        {{ block "c" . }}c: 3{{ end }}
    "#};
    sim_assert_eq!(have: dump(source)?, want: indoc! {r#"
        p0 text "ignored"
        p1 text "\na: "
        p2 text "1"
        p3 text "\n"
        p4 text "b: "
        p5 hole a5 Unknown
        p6 text "\n"
        p7 hole a7 Unknown
        p8 text "c: 3"
        p9 text "\n"
        arm [p1 p2 p3 p4 p5 p6 p7 p9] paths {r1=0 r2=0}
        arm [p1 p2 p3 p6 p7 p9] paths {r1=0 r2=1}
        arm [p1 p3 p4 p5 p6 p7 p9] paths {r1=1 r2=0}
        arm [p1 p3 p6 p7 p9] paths {r1=1 r2=1}
    "#});
    Ok(())
}

/// Branches that render the same pieces are one arm: the choice between
/// them cannot change the layout.
#[test]
fn identically_rendering_branches_are_one_arm() -> eyre::Result<()> {
    let source = indoc! {"
        {{- if .Values.x }}{{ $a := 1 }}{{ else }}{{ $a := 2 }}{{ end }}
        k: v
    "};
    sim_assert_eq!(have: dump(source)?, want: indoc! {r#"
        p0 text "\nk: v\n"
        arm [p0] paths {}
    "#});
    Ok(())
}

/// `range` repeats its body a value-dependent number of times, and a parse
/// recovery leaves the structure unknown: both bodies claim nothing.
#[test]
fn range_and_recovery_bodies_are_uncertain() -> eyre::Result<()> {
    sim_assert_eq!(
        have: dump("{{ range .Values.l }}- {{ . }}\n{{ end }}")?,
        want: indoc! {r#"
            p0 text "- "
            p1 hole a1 Unknown
            p2 text "\n"
            uncertain Range
        "#}
    );
    sim_assert_eq!(have: dump("a: {{ .Values.x \nb: 1\n")?, want: indoc! {r#"
        p0 text "a: "
        p1 text "\n"
        uncertain Recovery
    "#});
    Ok(())
}

/// A chain of `n` written branches.
fn chain(branches: usize) -> String {
    let mut source = String::from("{{ if eq .Values.x \"b0\" }}k: b0\n");
    for branch in 1..branches - 1 {
        let _ = writeln!(
            source,
            "{{{{ else if eq .Values.x \"b{branch}\" }}}}k: b{branch}"
        );
    }
    let _ = writeln!(source, "{{{{ else }}}}k: last");
    source.push_str("{{ end }}");
    source
}

/// Exactly [`MAX_LAYOUT_STATES`] alternatives stay enumerable; one more is a
/// loss of certainty for the whole body, never a truncated arm list.
#[test]
fn layout_states_widen_to_uncertain_past_the_bound() -> eyre::Result<()> {
    let at_bound_source = chain(MAX_LAYOUT_STATES);
    let at_bound = render(&at_bound_source, &UnknownShapes)?;
    let BodyLayout::Arms(arms) = &at_bound.layout else {
        eyre::bail!("{MAX_LAYOUT_STATES} branches must stay enumerable");
    };
    sim_assert_eq!(have: arms.len(), want: MAX_LAYOUT_STATES);

    let past_bound_source = chain(MAX_LAYOUT_STATES + 1);
    let past_bound = render(&past_bound_source, &UnknownShapes)?;
    sim_assert_eq!(
        have: past_bound.layout,
        want: BodyLayout::Uncertain(crate::LayoutUncertainty::Overflow)
    );
    Ok(())
}
