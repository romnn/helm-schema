use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{GuardValue, ValuesPath};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;

fn execute(action: &str) -> crate::value::DocumentExecution {
    let mut defines = DefineIndex::new();
    defines.add_file_source(
        "helpers.tpl",
        indoc! {r#"
        {{- define "common.name" -}}{{ .Values.nameOverride }}{{- end -}}
        {{- define "common.labels" -}}app: {{ .Values.labels.app }}{{- end -}}
        {{- define "partial.feature" -}}
          {{- if .Values.feature.gate -}}true
          {{- else -}}{{ if eq (randAlphaNum 1) "a" }}{{ .Values.feature.output }}{{ end }}{{- end -}}
        {{- end -}}
    "#},
    );
    let db = IrAnalysisDb::new(&defines);
    crate::value::execute_document(&format!("{{{{ {action} }}}}"), None, &db)
}

#[test]
fn dict_value_can_be_nested_helper_call() -> eyre::Result<()> {
    let execution = execute(r#"dict "name" (include "common.name" .)"#);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing dict output")?;
    sim_assert_eq!(have: site.projected_value(), want: AbstractValue::Dict(BTreeMap::from([
        ("name".to_string(), AbstractValue::Widened(BTreeSet::from([ValuesPath::parse("nameOverride")]))),
    ])));
    Ok(())
}

#[test]
fn printf_preserves_nested_helper_provenance_path() -> eyre::Result<()> {
    let execution = execute(r#"printf "%s-sfx" (include "common.name" .)"#);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing formatted output")?;
    sim_assert_eq!(have: site.value().inputs(), want: BTreeSet::from([ValuesPath::parse("nameOverride")]));
    sim_assert_eq!(have: site.value().identity(), want: None);
    Ok(())
}

/// Helm helper calls return text, so a map consumer requires explicit decoding.
#[test]
fn pipeline_merge_requires_decoding_nested_helper_text() {
    let execution = execute(r#"dict "base" "static" | merge (include "common.labels" .)"#);
    assert!(
        execution.text().next().is_none(),
        "merge accepted rendered helper text as a map"
    );
}

#[test]
fn integer_index_on_values_path_uses_array_item_wildcard_with_helper_context() -> eyre::Result<()> {
    let execution =
        execute(r#"dict "value" (index .Values.items 0) "name" (include "common.name" .)"#);
    let site = execution
        .sites()
        .filter(|site| site.rendered())
        .last()
        .ok_or_eyre("missing indexed output")?;
    sim_assert_eq!(have: site.projected_value(), want: AbstractValue::Dict(BTreeMap::from([
        ("name".to_string(), AbstractValue::Widened(BTreeSet::from([ValuesPath::parse("nameOverride")]))),
        ("value".to_string(), values_path!("items.*")),
    ])));
    Ok(())
}

#[test]
fn partial_helper_truth_marks_later_short_circuit_member_access_incomplete() {
    let execution = execute(r#"and (eq (include "partial.feature" .) "true") .Values.host.member"#);
    let captures = &execution.observed_facts().captures;
    let members = captures
        .iter()
        .filter(|capture| {
            matches!(
                capture.kind,
                crate::observed_facts::CaptureKind::MemberAccess { .. }
            )
        })
        .collect::<Vec<_>>();
    assert!(
        !members.is_empty(),
        "missing possible short circuit member failure"
    );
    assert!(
        members
            .iter()
            .any(|capture| capture.contains_approximation()),
        "unknown rendered helper output became an exact execution guard"
    );
}

/// A helper argument is invoked once even when semver parsing or selector folding misses.
#[test]
fn semver_selector_misses_evaluate_the_version_once() {
    struct Case {
        version: Option<GuardValue>,
        field: &'static str,
    }
    for Case { version, field } in [
        Case {
            version: Some(GuardValue::string("not-a-version")),
            field: "Major",
        },
        Case {
            version: Some(GuardValue::string("1.2.3")),
            field: "Unknown",
        },
        Case {
            version: Some(GuardValue::string("18446744073709551615.0.0")),
            field: "Major",
        },
        Case {
            version: None,
            field: "Major",
        },
        Case {
            version: Some(GuardValue::Bool(false)),
            field: "Major",
        },
    ] {
        let value = version.as_ref().map_or_else(
            || ".Values.version".to_string(),
            |value| match value {
                GuardValue::String(text) => format!("{text:?}"),
                value => value.to_string(),
            },
        );
        let helper = format!(
            "{{{{- define \"version\" -}}}}{{{{- $_ := set . \"called\" true -}}}}{{{{- {value} -}}}}{{{{- end -}}}}"
        );
        let mut defines = DefineIndex::new();
        defines.add_file_source("version.tpl", &helper);
        let db = IrAnalysisDb::new(&defines);
        let source = format!("{{{{ (semver (include \"version\" .)).{field} }}}}");
        let execution = crate::value::execute_document(&source, None, &db);
        let invocations = execution
            .all_sites()
            .filter(|site| site.source_path() == Some("version.tpl") && !site.rendered())
            .count();
        sim_assert_eq!(have: invocations, want: 1, "version={version:?} field={field}");
    }
}
