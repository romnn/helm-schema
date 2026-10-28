use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::{DefineIndex, TemplateExpr, parse_action_expressions};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
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
    sim_assert_eq!(
        have: result.projected_value(),
        want: AbstractValue::Dict(BTreeMap::from([
            ("name".to_string(), AbstractValue::StringSet(BTreeSet::from(["generated".to_string()]))),
            ("annotations".to_string(), values_path!("serviceAccount.annotations")),
        ]))
    );
    Ok(())
}

#[test]
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
