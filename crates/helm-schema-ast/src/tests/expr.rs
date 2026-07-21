use super::*;
use test_util::prelude::sim_assert_eq;

fn first(exprs: &[TemplateExpr]) -> &TemplateExpr {
    exprs.first().expect("at least one expression")
}

#[test]
fn parses_real_include_call() {
    let exprs = parse_action_expressions(r#"{{ include "common.labels" . }}"#);
    match first(&exprs) {
        TemplateExpr::Call { function, args } => {
            sim_assert_eq!(have: function, want: "include");
            sim_assert_eq!(have: args.len(), want: 2);
            sim_assert_eq!(
                have: args[0],
                want: TemplateExpr::Literal(Literal::String("common.labels".into()))
            );
            sim_assert_eq!(have: args[1], want: TemplateExpr::Field(Vec::new()));
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parses_template_action_as_call() {
    let exprs = parse_action_expressions(r#"{{ template "common.labels" . }}"#);
    match first(&exprs) {
        TemplateExpr::Call { function, args } => {
            sim_assert_eq!(have: function, want: "template");
            sim_assert_eq!(
                have: args[0],
                want: TemplateExpr::Literal(Literal::String("common.labels".into()))
            );
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn quoted_string_payload_is_a_literal_not_a_call() {
    // The bug class: `"include \"X\""` is a payload string, NOT a
    // call. Walking the parsed tree, we should find ONE Call node
    // (the outer `quote` call) whose first arg is a Literal::String
    // containing the literal text — never a Call to `include`.
    let exprs = parse_action_expressions(r#"{{ "include \"X\"" | quote }}"#);
    let pipeline = first(&exprs);
    let TemplateExpr::Pipeline(stages) = pipeline else {
        panic!("expected Pipeline, got {pipeline:?}");
    };
    let TemplateExpr::Literal(Literal::String(s)) = &stages[0] else {
        panic!("expected String literal as stage 0, got {:?}", stages[0]);
    };
    sim_assert_eq!(have: s, want: r#"include "X""#);

    // Confirm no Call to include exists anywhere in the tree.
    let mut saw_include = false;
    pipeline.walk(|e| {
        if let TemplateExpr::Call { function, .. } = e
            && function == "include"
        {
            saw_include = true;
        }
    });
    assert!(!saw_include, "phantom Call to include leaked through");
}

#[test]
fn parses_default_literal_dotvalues_prefix_form() {
    let exprs = parse_action_expressions(r#"{{ default 5 .Values.replicas }}"#);
    match first(&exprs) {
        TemplateExpr::Call { function, args } => {
            sim_assert_eq!(have: function, want: "default");
            sim_assert_eq!(have: args[0], want: TemplateExpr::Literal(Literal::Int(5)));
            sim_assert_eq!(
                have: args[1],
                want: TemplateExpr::Field(vec!["Values".into(), "replicas".into()])
            );
        }
        other => panic!("expected Call, got {other:?}"),
    }
}

#[test]
fn parses_pipeline_form_dotvalues_default() {
    let exprs = parse_action_expressions(r#"{{ .Values.replicas | default 5 }}"#);
    match first(&exprs) {
        TemplateExpr::Pipeline(stages) => {
            sim_assert_eq!(have: stages.len(), want: 2);
            sim_assert_eq!(
                have: stages[0],
                want: TemplateExpr::Field(vec!["Values".into(), "replicas".into()])
            );
            let TemplateExpr::Call { function, args } = &stages[1] else {
                panic!("expected default call in stage 1");
            };
            sim_assert_eq!(have: function, want: "default");
            sim_assert_eq!(have: args, want: &vec![TemplateExpr::Literal(Literal::Int(5))]);
        }
        other => panic!("expected Pipeline, got {other:?}"),
    }
}

#[test]
fn skips_comment_action() {
    let exprs = parse_action_expressions(r#"{{/* include "fake" */}}{{ include "real" . }}"#);
    // We collect comment actions as either nothing (skipped) or
    // they don't produce an extracted call. The only Call we should
    // see is "include" with arg "real".
    let mut include_args: Vec<String> = Vec::new();
    for e in &exprs {
        e.walk(|child| {
            if let TemplateExpr::Call { function, args } = child
                && function == "include"
                && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
            {
                include_args.push(name.clone());
            }
        });
    }
    sim_assert_eq!(have: include_args, want: vec!["real".to_string()]);
}

#[test]
fn flattens_nested_control_flow_bodies() {
    let body = r#"{{ if .X }}{{ include "a" . }}{{ end }}{{ include "b" . }}"#;
    let exprs = parse_action_expressions(body);
    let mut include_names: Vec<String> = Vec::new();
    for e in &exprs {
        e.walk(|child| {
            if let TemplateExpr::Call { function, args } = child
                && function == "include"
                && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
            {
                include_names.push(name.clone());
            }
        });
    }
    sim_assert_eq!(have: include_names, want: vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn raw_string_literal_decoded_verbatim() {
    let exprs = parse_action_expressions("{{ `a\\nb` }}");
    // Raw string contents are NOT escape-decoded.
    match first(&exprs) {
        TemplateExpr::Literal(Literal::RawString(s)) => {
            sim_assert_eq!(have: s, want: "a\\nb");
        }
        other => panic!("expected RawString, got {other:?}"),
    }
}

#[test]
fn variable_selector_chain() {
    // `$root.Values.foo` — Selector with a Variable operand.
    let exprs = parse_action_expressions(r#"{{ $root.Values.foo }}"#);
    match first(&exprs) {
        TemplateExpr::Selector { operand, path } => {
            sim_assert_eq!(have: **operand, want: TemplateExpr::Variable("root".into()));
            sim_assert_eq!(have: path, want: &vec!["Values".to_string(), "foo".to_string()]);
        }
        other => panic!("expected Selector, got {other:?}"),
    }
}

#[test]
fn deep_field_chain_collapses_to_single_field() {
    // `.A.B.C.D.E` — nested `selector_expression`s. Should collapse
    // into a single `Field` with the full path, NOT a five-deep
    // Selector chain.
    let exprs = parse_action_expressions(r#"{{ .A.B.C.D.E }}"#);
    sim_assert_eq!(
        have: first(&exprs),
        want: &TemplateExpr::Field(vec![
            "A".into(),
            "B".into(),
            "C".into(),
            "D".into(),
            "E".into()
        ]),
    );
}

#[test]
fn parens_around_field_prefix_preserve_nil_safe_receiver() {
    // The receiver boundary is semantically meaningful: a missing
    // `.Values.image` is tolerated, while a present non-map still fails the
    // `.tag` lookup. The selector retains that boundary and downstream
    // analysis can still derive the complete values path from both nodes.
    let exprs = parse_action_expressions(r#"{{ (.Values.image).tag }}"#);
    sim_assert_eq!(
        have: first(&exprs),
        want: &TemplateExpr::Selector {
            operand: Box::new(TemplateExpr::Parenthesized(Box::new(TemplateExpr::Field(vec![
                "Values".into(),
                "image".into(),
            ])))),
            path: vec!["tag".into()],
        },
    );
}

#[test]
fn nested_parens_around_field_prefix_preserve_every_boundary() {
    let exprs = parse_action_expressions(r#"{{ ((.Values.image)).tag }}"#);
    sim_assert_eq!(
        have: first(&exprs),
        want: &TemplateExpr::Selector {
            operand: Box::new(TemplateExpr::Parenthesized(Box::new(
                TemplateExpr::Parenthesized(Box::new(TemplateExpr::Field(vec![
                    "Values".into(),
                    "image".into(),
                ]))),
            ))),
            path: vec!["tag".into()],
        },
    );
}

#[test]
fn arbitrary_depth_parens_around_field_remain_structural() {
    let exprs = parse_action_expressions(r#"{{ (((.Values.image))).tag }}"#);
    let TemplateExpr::Selector { operand, path } = first(&exprs) else {
        panic!("expected selector");
    };
    sim_assert_eq!(have: path, want: &vec!["tag".to_string()]);
    let mut receiver = operand.as_ref();
    for _ in 0..3 {
        let TemplateExpr::Parenthesized(inner) = receiver else {
            panic!("expected three preserved parenthesis layers, got {operand:?}");
        };
        receiver = inner;
    }
    sim_assert_eq!(
        have: receiver,
        want: &TemplateExpr::Field(vec!["Values".into(), "image".into()]),
    );
}

#[test]
fn selectors_after_grouped_receiver_still_merge() {
    // The grouping boundary stays on the receiver, while adjacent suffix
    // selectors remain one path.
    let exprs = parse_action_expressions(r#"{{ (.Values).image.tag }}"#);
    sim_assert_eq!(
        have: first(&exprs),
        want: &TemplateExpr::Selector {
            operand: Box::new(TemplateExpr::Parenthesized(Box::new(TemplateExpr::Field(vec![
                "Values".into(),
            ])))),
            path: vec!["image".into(), "tag".into()],
        },
    );
}

#[test]
fn parens_around_pipeline_do_not_collapse_into_field() {
    // `(.Values.image | upper).tag` — the parens wrap a Pipeline,
    // not a pure path. Don't pretend the result is a Values path;
    // leave the Pipeline wrapped so downstream code sees that the
    // tag access is on the upper-cased operand, not on the raw
    // `.Values.image.tag`.
    let exprs = parse_action_expressions(r#"{{ (.Values.image | upper).tag }}"#);
    match first(&exprs) {
        TemplateExpr::Selector { operand, path } => {
            sim_assert_eq!(have: path, want: &vec!["tag".to_string()]);
            assert!(
                matches!(
                    operand.as_ref(),
                    TemplateExpr::Parenthesized(inner)
                        if matches!(inner.as_ref(), TemplateExpr::Pipeline(_))
                ),
                "expected Selector operand to be Parenthesized(Pipeline), got {operand:?}",
            );
        }
        other => panic!("expected Selector, got {other:?}"),
    }
}

#[test]
fn bare_dot_parses_as_empty_field_path() {
    let exprs = parse_action_expressions(r#"{{ . }}"#);
    sim_assert_eq!(have: exprs, want: vec![TemplateExpr::Field(Vec::new())]);
}

#[test]
fn deparen_strips_arbitrary_nesting_for_path_and_non_path_alike() {
    // Standalone form: `deparen` always returns the inner-most
    // non-parens node, regardless of what it is. Path or pipeline,
    // depth and ordering of the parens don't change the answer.
    let cases = [
        (r#"{{ .X.Y }}"#, "Field"),
        (r#"{{ (.X.Y) }}"#, "Field"),
        (r#"{{ ((.X.Y)) }}"#, "Field"),
        (r#"{{ (((.X.Y))) }}"#, "Field"),
        (r#"{{ ((((.X.Y)))) }}"#, "Field"),
    ];
    for (src, expected_kind) in cases {
        let exprs = parse_action_expressions(src);
        let kind = match first(&exprs).deparen() {
            TemplateExpr::Field(_) => "Field",
            TemplateExpr::Selector { .. } => "Selector",
            TemplateExpr::Pipeline(_) => "Pipeline",
            TemplateExpr::Call { .. } => "Call",
            other => panic!("unexpected node for `{src}`: {other:?}"),
        };
        sim_assert_eq!(have: kind, want: expected_kind, "deparen result mismatch for {src}");
        // And the path is the same `["X","Y"]` everywhere.
        let TemplateExpr::Field(path) = first(&exprs).deparen() else {
            panic!("expected Field after deparen for {src}");
        };
        sim_assert_eq!(have: path, want: &vec!["X".to_string(), "Y".to_string()]);
    }
}

#[test]
fn walk_already_recurses_through_parens_so_visitor_must_not_deparen() {
    // Visitor-level invariant guarded by this test:
    // `TemplateExpr::walk` calls the visitor on the
    // `Parenthesized` *and* on its inner node — visiting the
    // wrapper is a no-op for any extractor that matches on a
    // specific shape (Literal / Call / Pipeline), so the inner is
    // reached without callers having to `deparen`. Conversely, if
    // a visitor *did* deparen, nested forms like
    // `default 5 (default "x" .Values.X)` would emit the inner
    // hint twice (once when visiting the Parenthesized — deparened
    // back to the Call — and once when visiting the same Call
    // directly).
    let exprs = parse_action_expressions(r#"{{ default 5 (default "x" .Values.X) }}"#);
    let mut paren_visits = 0;
    let mut inner_call_visits = 0;
    for top in &exprs {
        top.walk(|node| match node {
            TemplateExpr::Parenthesized(_) => paren_visits += 1,
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                if matches!(
                    args.first(),
                    Some(TemplateExpr::Literal(Literal::String(_)))
                ) {
                    inner_call_visits += 1;
                }
            }
            _ => {}
        });
    }
    sim_assert_eq!(
        have: paren_visits,
        want: 1,
        "walk should visit the Parenthesized node exactly once",
    );
    sim_assert_eq!(
        have: inner_call_visits,
        want: 1,
        "walk should visit the inner `default \"x\" .Values.X` Call exactly once",
    );
}

#[test]
fn raw_range_variable_definition_exposes_children() {
    let src = "{{- range $key, $value := .Values.environment }}{{- end }}";
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("set go-template language");
    let tree = parser.parse(src, None).expect("parse source");

    let mut stack = vec![tree.root_node()];
    let mut range_var = None;
    while let Some(node) = stack.pop() {
        if node.kind() == "range_variable_definition" {
            range_var = Some(node);
            break;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }

    let range_var = range_var.expect("find range_variable_definition");
    let mut children = Vec::new();
    let mut cursor = range_var.walk();
    for (index, child) in range_var.named_children(&mut cursor).enumerate() {
        children.push((
            child.kind().to_string(),
            range_var
                .field_name_for_child(index as u32)
                .map(str::to_string),
            child.utf8_text(src.as_bytes()).unwrap_or("").to_string(),
        ));
    }

    if std::env::var("RANGE_VAR_DUMP").is_ok() {
        eprintln!("{children:#?}");
    }

    assert!(
        children
            .iter()
            .any(|(kind, _, text)| kind == "variable" && text == "$key"),
        "expected range variable definition to expose first bound variable, got {children:#?}",
    );
    assert!(
        children
            .iter()
            .any(|(kind, _, text)| kind == "variable" && text == "$value"),
        "expected range variable definition to expose second bound variable, got {children:#?}",
    );
    assert!(
        children.iter().any(|(kind, field, text)| {
            kind == "selector_expression"
                && field.as_deref() == Some("element")
                && text == ".Values.environment"
        }),
        "expected range variable definition to expose the ranged expression as the element field, got {children:#?}",
    );
}

#[test]
fn pipeline_with_intervening_call_no_default_match() {
    // `.Values.X | upper | default 5` — the windows pattern matcher
    // should NOT pair `.Values.X` with `default` because `upper`
    // intervenes (so `default` fires on `upper(.Values.X)`, not on
    // `.Values.X`). Only direct `.Values.X | default` pairings are
    // legal type-hint sites.
    let exprs = parse_action_expressions(r#"{{ .Values.X | upper | default 5 }}"#);
    let TemplateExpr::Pipeline(stages) = first(&exprs) else {
        panic!("expected pipeline");
    };
    sim_assert_eq!(have: stages.len(), want: 3);
    // First adjacent pair (Field, Call("upper")) — not a default.
    assert!(matches!(&stages[1], TemplateExpr::Call { function, .. } if function == "upper"));
    // Second adjacent pair (Call("upper"), Call("default")) — first
    // half is not a Field, so consumers should reject.
    assert!(matches!(&stages[2], TemplateExpr::Call { function, .. } if function == "default"),);
}

#[test]
fn range_destructure_walks_range_expression_not_variables() {
    // `{{ range $i, $v := include "common.items" . }}{{ end }}`
    // — the range_variable_definition's `range` field carries an
    // include call. Walking the parsed actions should surface the
    // include Call so helper-call extraction works inside range
    // headers.
    let body = r#"{{ range $i, $v := include "common.items" . }}{{ end }}"#;
    let exprs = parse_action_expressions(body);
    let mut found = false;
    for e in &exprs {
        e.walk(|child| {
            if let TemplateExpr::Call { function, args } = child
                && function == "include"
                && let Some(TemplateExpr::Literal(Literal::String(name))) = args.first()
                && name == "common.items"
            {
                found = true;
            }
        });
    }
    assert!(
        found,
        "include inside range destructure not extracted: {exprs:?}"
    );
}

#[test]
fn define_name_is_not_surfaced_as_top_level_literal() {
    // `{{ define "common.name" }}{{ include "X" . }}{{ end }}` —
    // the define's name `"common.name"` must NOT appear in the
    // top-level expression list as a stray Literal. Only the body's
    // `include` call should surface.
    let body = r#"{{ define "common.name" }}{{ include "X" . }}{{ end }}"#;
    let exprs = parse_action_expressions(body);
    // Filter: name literals are forbidden at top level.
    let strays: Vec<_> = exprs
        .iter()
        .filter(|e| matches!(e, TemplateExpr::Literal(Literal::String(s)) if s == "common.name"))
        .collect();
    assert!(
        strays.is_empty(),
        "define name leaked as top-level Literal: {strays:?}",
    );
    // Sanity: the include call IS surfaced.
    let mut saw_include = false;
    for e in &exprs {
        e.walk(|c| {
            if let TemplateExpr::Call { function, .. } = c
                && function == "include"
            {
                saw_include = true;
            }
        });
    }
    assert!(saw_include, "include inside define body not extracted");
}

#[test]
fn block_name_is_not_surfaced_but_argument_is() {
    // `{{ block "name" .Values.X }}…{{ end }}` — the block's `name`
    // string is noise; the `.Values.X` argument and body content
    // must still be reachable.
    let body = r#"{{ block "thing" .Values.X }}body{{ end }}"#;
    let exprs = parse_action_expressions(body);
    let strays: Vec<_> = exprs
        .iter()
        .filter(|e| matches!(e, TemplateExpr::Literal(Literal::String(s)) if s == "thing"))
        .collect();
    assert!(strays.is_empty(), "block name leaked: {strays:?}");
    let mut saw_arg = false;
    for e in &exprs {
        e.walk(|c| {
            if let TemplateExpr::Field(path) = c
                && path == &vec!["Values".to_string(), "X".to_string()]
            {
                saw_arg = true;
            }
        });
    }
    assert!(saw_arg, "block argument .Values.X not extracted: {exprs:?}");
}

#[test]
fn malformed_unicode_escape_is_preserved_verbatim() {
    // `\u12` is only two hex digits — Go's grammar requires four.
    // The decoder must not silently produce a wrong char; the input
    // bytes are preserved verbatim so callers can detect the issue.
    let exprs = parse_action_expressions(r#"{{ "\u12" }}"#);
    let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
        panic!("expected string literal");
    };
    sim_assert_eq!(have: s, want: r"\u12", "got {s:?}");
}

#[test]
fn well_formed_unicode_escapes_decode_correctly() {
    // `é` → 'é'. `\U0001F600` → '😀' (supplementary plane).
    let exprs = parse_action_expressions(r#"{{ "café \U0001F600" }}"#);
    let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
        panic!("expected string literal");
    };
    sim_assert_eq!(have: s, want: "café 😀");
}

#[test]
fn surrogate_code_point_is_not_silently_decoded() {
    // `\uD800` is the leading half of a UTF-16 surrogate pair —
    // not a valid Unicode scalar value. `char::from_u32` returns
    // None, so the decoder must preserve the raw escape bytes.
    let exprs = parse_action_expressions(r#"{{ "\uD800" }}"#);
    let TemplateExpr::Literal(Literal::String(s)) = first(&exprs) else {
        panic!("expected string literal");
    };
    sim_assert_eq!(have: s, want: r"\uD800");
}

#[test]
fn empty_body_returns_empty_list() {
    assert!(parse_action_expressions("").is_empty());
}

#[test]
fn body_with_no_actions_returns_empty() {
    assert!(parse_action_expressions("just plain yaml text\nkey: value\n").is_empty());
}

#[test]
fn negative_int_literal() {
    let exprs = parse_action_expressions(r#"{{ default -42 .Values.X }}"#);
    let TemplateExpr::Call { args, .. } = first(&exprs) else {
        panic!("expected Call");
    };
    sim_assert_eq!(have: args[0], want: TemplateExpr::Literal(Literal::Int(-42)));
}

#[test]
fn hex_int_literal() {
    let exprs = parse_action_expressions(r#"{{ default 0xFF .Values.X }}"#);
    let TemplateExpr::Call { args, .. } = first(&exprs) else {
        panic!("expected Call");
    };
    sim_assert_eq!(have: args[0], want: TemplateExpr::Literal(Literal::Int(0xFF)));
}

#[test]
fn fragment_render_semantics_ignore_string_literals() {
    let exprs = parse_action_expressions(r#"{{ printf "%s" "nindent" }}"#);
    assert!(
        !exprs.iter().any(TemplateExpr::renders_yaml_fragment),
        "string literals must not masquerade as fragment render functions: {exprs:?}"
    );
}

#[test]
fn fragment_render_semantics_distinguish_include_from_fragment_render() {
    let exprs = parse_action_expressions(r#"{{ include "name" . }}"#);
    assert!(
        !exprs.iter().any(TemplateExpr::renders_yaml_fragment),
        "bare include is not a definite fragment render: {exprs:?}"
    );
}

#[test]
fn fragment_indent_width_comes_from_last_indent_stage() {
    let exprs = parse_action_expressions(r#"{{ include "labels" . | nindent 4 }}"#);
    let width = exprs
        .iter()
        .rev()
        .find_map(TemplateExpr::fragment_indent_width);
    sim_assert_eq!(have: width, want: Some(4));
}

#[test]
fn assignment_rhs_include_is_not_treated_as_emitted_yaml_structure() {
    let exprs = parse_action_expressions(r#"{{ $name := include "common.names.fullname" . }}"#);
    assert!(
        !exprs.iter().any(TemplateExpr::renders_yaml_fragment),
        "assignment-side include must not be treated as a rendered fragment: {exprs:?}"
    );
}

#[test]
fn tpl_of_plain_value_inside_printf_is_not_definite_fragment_render() {
    let exprs = parse_action_expressions(r#"{{ printf "%s" (tpl .Values.auth.database $) }}"#);
    assert!(
        !exprs.iter().any(TemplateExpr::renders_yaml_fragment),
        "tpl's context does not prove that its string input renders YAML structure: {exprs:?}"
    );
}

#[test]
fn unspaced_argument_pipe_splits_the_enclosing_command() {
    // Go's tokenizer emits `)` and `|` as separate tokens regardless of
    // spacing: `print (include "a" .)| sha256sum` pipes the WHOLE print
    // command, never its last argument (redis-ha's checksum annotation).
    let spaced =
        parse_action_expressions(r#"{{ print (include "a" .) (include "b" .) | sha256sum }}"#);
    let unspaced =
        parse_action_expressions(r#"{{ print (include "a" .) (include "b" .)| sha256sum }}"#);
    sim_assert_eq!(have: &unspaced, want: &spaced);
    let TemplateExpr::Pipeline(stages) = &unspaced[0] else {
        panic!("expected Pipeline, got {unspaced:?}");
    };
    sim_assert_eq!(have: stages.len(), want: 2);
    assert!(
        matches!(&stages[0], TemplateExpr::Call { function, args } if function == "print" && args.len() == 2),
        "first stage is the whole print command: {stages:?}"
    );
    assert!(
        matches!(&stages[1], TemplateExpr::Call { function, .. } if function == "sha256sum"),
        "second stage is the digest: {stages:?}"
    );
}

#[test]
fn unspaced_selector_pipe_splits_the_enclosing_command() {
    // The same tokenization rule for a bare selector argument:
    // `default "x" .Values.y|quote` pipes the whole default call.
    let spaced = parse_action_expressions(r#"{{ default "x" .Values.y | quote }}"#);
    let unspaced = parse_action_expressions(r#"{{ default "x" .Values.y|quote }}"#);
    sim_assert_eq!(have: &unspaced, want: &spaced);
}

#[test]
fn parenthesized_pipeline_argument_stays_an_argument() {
    // A REAL pipeline argument is parenthesized; the unfold must leave it
    // in place.
    let exprs = parse_action_expressions(r#"{{ print (list 1 2 | join ",") }}"#);
    assert!(
        matches!(
            &exprs[0],
            TemplateExpr::Call { function, args }
                if function == "print"
                    && matches!(
                        args.first().map(TemplateExpr::deparen),
                        Some(TemplateExpr::Pipeline(_))
                    )
        ),
        "parenthesized pipeline argument survives: {exprs:?}"
    );
}
