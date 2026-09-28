use crate::{
    DefineIndex, TemplateExpr, TemplateHeader, contains_template_action, parse_action_expressions,
    render_printf_scalar_values,
};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::GuardValue;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[test]
fn template_header_from_node_converts_the_parsed_condition() -> eyre::Result<()> {
    let expected = TemplateExpr::Field(vec![
        "Values".to_string(),
        "signoz".to_string(),
        "serviceAccount".to_string(),
        "create".to_string(),
    ]);

    for source in [
        "{{ if .Values.signoz.serviceAccount.create }}x{{ end }}",
        "{{- if .Values.signoz.serviceAccount.create -}}x{{ end }}",
        "{{if(.Values.signoz.serviceAccount.create)}}x{{end}}",
    ] {
        let tree = crate::parse_go_template(source).ok_or_eyre("tree-sitter parse")?;
        let mut cursor = tree.root_node().walk();
        let action = tree
            .root_node()
            .named_children(&mut cursor)
            .find(|node| node.kind() == "if_action")
            .ok_or_eyre("if action")?;
        let condition = action
            .child_by_field_name("condition")
            .ok_or_eyre("condition")?;
        let header = TemplateHeader::from_node(condition, source);
        sim_assert_eq!(have: header.expr().deparen(), want: &expected, "source={source}");
    }
    Ok(())
}

#[test]
fn range_header_from_source_lowers_the_parsed_range_clause() -> eyre::Result<()> {
    let source = r#"{{ range $i, $v := include "items" . }}{{ $v }}{{ end }}"#;
    let tree = crate::parse_go_template(source).ok_or_eyre("tree-sitter parse")?;
    let mut cursor = tree.root_node().walk();
    let range = tree
        .root_node()
        .named_children(&mut cursor)
        .find(|node| node.kind() == "range_action")
        .ok_or_eyre("range action")?;
    let header = crate::range_header_from_source(range, source).ok_or_eyre("range header")?;
    sim_assert_eq!(have: header.raw(), want: r#"include "items" ."#);
    sim_assert_eq!(
        have: header.expr(),
        want: &TemplateExpr::Call {
            function: "include".to_string(),
            args: vec![
                TemplateExpr::Literal(crate::Literal::String("items".to_string())),
                TemplateExpr::Field(Vec::new()),
            ],
        }
    );
    Ok(())
}

/// Lowering an action from the retained document tree yields exactly what
/// parsing the action's own text yields, for every action that renders or
/// binds: the retained facts replace the text parse without changing it.
#[test]
fn lowered_action_expressions_match_parsing_each_action_text() -> eyre::Result<()> {
    let sources = [
        indoc! {r#"
            metadata:
              name: {{ include "app.fullname" . | trunc 63 }}
              labels: {{- toYaml .Values.labels | nindent 4 }}
            {{- $root := . }}
            {{ $count = add $count 1 -}}
            data:
              {{- range $key, $value := .Values.data }}
              {{ $key }}: {{ $value | quote }}
              {{- end }}
              {{- with .Values.extra }}
              extra: {{ . }}
              {{- else }}
              extra: {{ template "app.extra" $root }}
              {{- end }}
        "#},
        indoc! {r#"
            key: {{ if .Values.flag }}prefix {{-3}} {{ .Values.x }}{{ end }}
            script: |-
              {{if(.Values.flag)}}{{ .Values.x }}{{end}}
            multi: {{ printf "%s-%s"
              .Values.a
              .Values.b }}
            {{/* a comment */}}
            {{- define "helper" -}}
            {{ default "x" .Values.y }}
            {{- end -}}
        "#},
        indoc! {r"
            broken: {{ .Values.a | }}
            unclosed: {{ .Values.b
            {{ if }}
        "},
    ];
    for source in sources {
        let tree = crate::parse_go_template(source).ok_or_eyre("tree-sitter parse")?;
        let document =
            helm_schema_syntax::TemplatedDocument::parse_with_root(source, tree.root_node());
        let lowered = crate::ParsedActions::lower(tree.root_node(), source, document.actions());
        for action in document.actions() {
            if !matches!(
                action.kind,
                helm_schema_syntax::ActionKind::Output { .. }
                    | helm_schema_syntax::ActionKind::Assign
                    | helm_schema_syntax::ActionKind::TemplateComment
            ) {
                continue;
            }
            let text = source
                .get(action.span.start..action.span.end)
                .ok_or_eyre("action span")?;
            let parsed = lowered.at(action.span).ok_or_eyre("lowered action")?;
            sim_assert_eq!(
                have: parsed.expressions.to_vec(),
                want: parse_action_expressions(text),
                "action={text}"
            );
        }
    }
    Ok(())
}

#[test]
fn parse_action_expressions_types_pipeline_actions() {
    let exprs = parse_action_expressions("{{ .Values.name | quote }}");
    let [TemplateExpr::Pipeline(stages)] = exprs.as_slice() else {
        panic!("expected one parsed pipeline expression");
    };

    assert!(matches!(
        stages.as_slice(),
        [
            TemplateExpr::Field(path),
            TemplateExpr::Call { function, args }
        ] if path == &vec!["Values".to_string(), "name".to_string()]
            && function == "quote"
            && args.is_empty()
    ));
}

#[test]
fn template_action_detection_finds_inline_output_action() {
    let src = indoc! {"
        metadata:
          name: {{ .Values.name }}
    "};

    assert!(contains_template_action(src).expect("parse template source"));
}

#[test]
fn template_action_detection_accepts_literal_yaml_comments() {
    let src = indoc! {"
        # comment
        metadata:
          name: demo
    "};

    assert!(!contains_template_action(src).expect("parse template source"));
}

#[test]
fn define_index_tracks_file_sources_deterministically() {
    let mut idx = DefineIndex::new();
    idx.add_file_source("templates/z.yaml", "kind: ConfigMap\n");
    idx.add_file_source("templates/a.yaml", "kind: Service\n");

    sim_assert_eq!(
        have: idx.get_file("templates/z.yaml"),
        want: Some("kind: ConfigMap\n")
    );
    sim_assert_eq!(
        have: idx.file_sources().map(|(path, ..)| path).collect::<Vec<_>>(),
        want: vec!["templates/a.yaml", "templates/z.yaml"]
    );
}

#[test]
fn printf_renders_exact_typed_semver_components() {
    sim_assert_eq!(
        have: render_printf_scalar_values(
            "%d.%d.0",
            &[GuardValue::Int(1), GuardValue::Int(35)]
        ),
        want: Some("1.35.0".to_string()),
    );
    sim_assert_eq!(
        have: render_printf_scalar_values("%d", &[GuardValue::string("35")]),
        want: None,
        "a string must not masquerade as a decimal argument"
    );
}
