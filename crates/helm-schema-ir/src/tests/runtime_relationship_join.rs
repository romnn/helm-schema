use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;

#[test]
fn decoded_capture_keeps_the_nested_provider_destination() {
    let helpers = indoc! {r#"
        {{- define "pod.template" -}}
        metadata:
          labels:
            app: test
        spec:
          {{- if not (kindIs "invalid" .Values.hostUsers) }}
          hostUsers: {{ .Values.hostUsers }}
          {{- end }}
        {{- end -}}
    "#};
    let source = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template: {{ include "pod.template" . | fromYaml | toYaml | nindent 4 }}
    "#};
    let mut defines = DefineIndex::new();
    defines.add_file_source("_helpers.tpl", helpers);
    let db = IrAnalysisDb::new(&defines);
    let execution = execute_document(source, Some("templates/deployment.yaml"), &db);
    let contract = crate::fragment_eval::project_execution_at_offset(
        source,
        Some("templates/deployment.yaml"),
        0,
        &db,
        &execution,
    )
    .finalize();
    let rows = contract
        .uses()
        .iter()
        .filter(|row| {
            row.source_expr.encode() == "hostUsers"
                && !row.path.0.is_empty()
                && !row.condition.is_never()
        })
        .map(|row| (row.path.0.clone(), row.kind))
        .collect::<Vec<_>>();
    let destination = vec![
        "spec".to_owned(),
        "template".to_owned(),
        "spec".to_owned(),
        "hostUsers".to_owned(),
    ];
    sim_assert_eq!(have: rows, want: vec![
        (destination.clone(), crate::ValueKind::Scalar),
        (destination.clone(), crate::ValueKind::GoFormattedMap),
        (destination, crate::ValueKind::GoFormattedSequence),
    ]);
}

#[test]
fn member_presence_keeps_null_distinct_from_an_absent_field() -> eyre::Result<()> {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(
        "{{ toYaml (dict \"present\" nil \"origin\" .Values.payload) }}",
        None,
        &db,
    );
    let site = execution
        .sites()
        .find(|site| site.rendered())
        .ok_or_eyre("output missing")?;
    for (key, present) in [("present", true), ("absent", false)] {
        let view = execution.member_relationships(site.value(), &[key.to_owned()], &mut 1024);
        sim_assert_eq!(have: view.presence.present, want: if present { helm_schema_core::Predicate::True } else { helm_schema_core::Predicate::False });
        sim_assert_eq!(have: view.presence.absent, want: if present { helm_schema_core::Predicate::False } else { helm_schema_core::Predicate::True });
        sim_assert_eq!(have: view.presence.unknown, want: helm_schema_core::Predicate::False);
        sim_assert_eq!(have: view.relationships.known.len(), want: 0);
    }
    Ok(())
}

#[test]
fn literal_serialized_header_keeps_the_provider_destination() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{ toYaml (dict "kind" "Pod") }}
        apiVersion: v1
        spec:
          hostUsers: {{ .Values.hostUsers }}
    "#};
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/pod.yaml"), &db);
    let signals = crate::fragment_eval::project_execution_at_offset(
        source,
        Some("templates/pod.yaml"),
        0,
        &db,
        &execution,
    )
    .finalize()
    .into_schema_signals();
    let evidence = signals
        .evidence_for(&helm_schema_core::ValuesPath::parse("hostUsers"))
        .ok_or_eyre("provider evidence missing")?;
    let actual = evidence
        .provider_schema_uses
        .iter()
        .chain(
            evidence
                .conditional_overlays
                .iter()
                .flat_map(|overlay| &overlay.evidence.provider_schema_uses),
        )
        .filter(|use_| use_.kind == crate::ValueKind::Scalar)
        .map(|use_| {
            (
                use_.resource.api_version.clone(),
                use_.resource.kind.clone(),
                use_.path.0.clone(),
            )
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(have: actual, want: vec![("v1".to_owned(), "Pod".to_owned(), vec!["spec".to_owned(), "hostUsers".to_owned()])]);
    Ok(())
}
