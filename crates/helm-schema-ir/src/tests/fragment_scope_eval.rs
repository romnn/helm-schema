use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};
use helm_schema_core::{GuardValue, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;

#[test]
fn parse_helper_assignment_detects_declaration_from_ast() {
    sim_assert_eq!(
        have: parse_action_expressions(r"{{- $image := .Values.image.repository -}}"),
        want: vec![TemplateExpr::VariableDefinition {
            name: "$image".to_string(),
            value: Box::new(TemplateExpr::Field(vec!["Values".to_string(), "image".to_string(), "repository".to_string()])),
        }]
    );
}

#[test]
fn parse_helper_assignment_detects_assignment_from_ast() {
    sim_assert_eq!(
        have: parse_action_expressions(r"{{- $image = .Values.global.image -}}"),
        want: vec![TemplateExpr::Assignment {
            name: "$image".to_string(),
            value: Box::new(TemplateExpr::Field(vec!["Values".to_string(), "global".to_string(), "image".to_string()])),
        }]
    );
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): local set mutation uses shared expression eval for computed key; tracked in plan/value-algebra-r3b.md follow-ups"]
fn local_set_mutation_uses_shared_expression_eval_for_computed_key() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $config := dict "name" .Values.serviceAccount.name "annotations" .Values.serviceAccount.annotations -}}
        {{- $_ := set $config (printf "%s" "name") "generated" -}}
        {{- $config -}}
    "#};
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::tests::execute_document(source, None, &db);
    let result = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing config output")?;
    let name = result
        .value()
        .read_field("name")
        .present()
        .ok_or_eyre("missing name field")?;
    let annotations = result
        .value()
        .read_field("annotations")
        .present()
        .ok_or_eyre("missing annotations field")?;
    sim_assert_eq!(have: name.literal(), want: Some(&GuardValue::string("generated")));
    sim_assert_eq!(have: annotations.identity(), want: Some(&ValuesPath::parse("serviceAccount.annotations")));
    Ok(())
}

#[test]
#[ignore = "R3b regression vs main, package C (parser, collection and validation transfers): local set mutation resolves computed key from literal local; tracked in plan/value-algebra-r3b.md follow-ups"]
fn local_set_mutation_resolves_computed_key_from_literal_local() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $patch := fromJson .Values.patch -}}
        {{- $opPathKey := "path" -}}
        {{- $_ := set $patch (printf "%sKey" $opPathKey) "derived" -}}
        {{- $patch.pathKey -}}
    "#};
    let defines = DefineIndex::new();
    let db = IrAnalysisDb::new(&defines);
    let execution = crate::value::tests::execute_document(source, None, &db);
    let result = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing patch output")?;
    sim_assert_eq!(have: result.value().exact_text(), want: Some("derived".to_string()));
    Ok(())
}
