use crate::{
    DefineIndex, TemplateExpr, TemplateHeader, contains_template_action, parse_action_expressions,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn template_header_parse_control_normalizes_control_keyword_prefix() {
    let expected = TemplateExpr::Field(vec![
        "Values".to_string(),
        "signoz".to_string(),
        "serviceAccount".to_string(),
        "create".to_string(),
    ]);

    for raw in [
        "if .Values.signoz.serviceAccount.create",
        "{{- if .Values.signoz.serviceAccount.create -}}",
    ] {
        let header = TemplateHeader::parse_control(raw);
        sim_assert_eq!(have: header.expr(), want: &expected, "raw={raw}");
    }
}

#[test]
fn template_header_parse_range_preserves_typed_include_call() {
    let header = TemplateHeader::parse_range("$i, $v := include \"items\" .");

    sim_assert_eq!(have: header.raw(), want: "$i, $v := include \"items\" .");

    let mut saw_include = false;
    header.expr().walk(|expr| {
        if let TemplateExpr::Call { function, args } = expr
            && function == "include"
            && matches!(args.first(), Some(TemplateExpr::Literal(lit)) if lit.as_string() == Some("items"))
        {
            saw_include = true;
        }
    });
    assert!(saw_include, "expected typed range header include call");
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
    let src = "metadata:\n  name: {{ .Values.name }}\n";

    assert!(contains_template_action(src).expect("parse template source"));
}

#[test]
fn template_action_detection_accepts_literal_yaml_comments() {
    let src = "# comment\nmetadata:\n  name: demo\n";

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
        have: idx.file_sources().map(|(path, _)| path).collect::<Vec<_>>(),
        want: vec!["templates/a.yaml", "templates/z.yaml"]
    );
}
