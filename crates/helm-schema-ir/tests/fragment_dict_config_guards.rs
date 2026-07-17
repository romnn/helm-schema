//! Guard survival for helper calls bound through `dict` contexts: call-site
//! guards must wrap spliced helper output, and helper-internal guards must
//! compose across nested control regions. Pins the B4 regression where both
//! were dropped (luup3 `common.*` dict-config pattern).

use helm_schema_ast::DefineIndex;
use helm_schema_ir::SymbolicIrContext;
use helm_schema_ir::fragment_eval::dump_document;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn assert_fragment_dump(source: &str, helpers: &str, expected: &str) {
    let mut idx = DefineIndex::new();
    if !helpers.is_empty() {
        idx.add_file_source("_helpers.tpl", helpers);
    }
    let document = SymbolicIrContext::new(&idx).eval_document_fragment(source);
    sim_assert_eq!(have: dump_document(&document), want: expected);
}

/// A helper spliced under `with` + `if` call-site guards keeps those guards
/// on its rendered placements.
#[test]
fn call_site_guards_wrap_spliced_dict_config_helper() {
    let helpers = indoc! {r#"
        {{- define "repro.ingress" -}}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
        {{- end }}
    "#};
    let source = indoc! {r#"
        {{- with .Values.ingress -}}
        {{- if .enabled -}}
        {{ include "repro.ingress" (dict "ctx" $ "config" .) }}
        {{- end }}
        {{- end }}
    "#};
    let expected = indoc! {r#"
        when (with(ingress) && truthy(ingress.enabled)):
          mapping:
            key "apiVersion":
              when always:
                scalar [text{"networking.k8s.io/v1"}]
            key "kind":
              when always:
                scalar [text{"Ingress"}]
            key "spec":
              when always:
                mapping:
                  key "ingressClassName":
                    when truthy(ingress.className):
                      splice ingress.className scalar
        reads:
          ingress [with(ingress)]
          ingress.enabled [truthy(ingress.enabled), with(ingress)]
          ingress.className [truthy(ingress.className), truthy(ingress.enabled), with(ingress)]
    "#};
    assert_fragment_dump(source, helpers, expected);
}

/// Inside a helper body, a read under `with .config.x` keeps the enclosing
/// `if .config.enabled` condition (conditions compose across nested regions).
#[test]
fn helper_internal_nested_with_keeps_outer_if_condition() {
    let helpers = indoc! {r#"
        {{- define "repro.pdb" -}}
        {{- if .config.enabled }}
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        spec:
          minAvailable: {{ .config.minAvailable }}
          {{- with .config.maxUnavailable }}
          maxUnavailable: {{ . }}
          {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let source = indoc! {r#"
        {{- include "repro.pdb" (dict "ctx" $ "config" .Values.podDisruptionBudget) }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "apiVersion":
              when truthy(podDisruptionBudget.enabled):
                scalar [text{"policy/v1"}]
            key "kind":
              when truthy(podDisruptionBudget.enabled):
                scalar [text{"PodDisruptionBudget"}]
            key "spec":
              when truthy(podDisruptionBudget.enabled):
                mapping:
                  key "minAvailable":
                    when always:
                      splice podDisruptionBudget.minAvailable scalar
                  key "maxUnavailable":
                    when truthy(podDisruptionBudget.maxUnavailable):
                      splice podDisruptionBudget.maxUnavailable scalar
        reads:
          podDisruptionBudget.enabled [truthy(podDisruptionBudget.enabled)]
          podDisruptionBudget.maxUnavailable [truthy(podDisruptionBudget.enabled), truthy(podDisruptionBudget.maxUnavailable)]
    "#};
    assert_fragment_dump(source, helpers, expected);
}

#[test]
fn literal_dotted_index_and_get_keys_stay_single_path_segments() {
    let source = indoc! {r#"
        data:
          direct: {{ index .Values "foo.bar" }}
          selected: {{ (get .Values "foo.bar").baz }}
    "#};
    let expected = indoc! {r#"
        when always:
          mapping:
            key "data":
              when always:
                mapping:
                  key "direct":
                    when always:
                      splice foo\.bar scalar
                  key "selected":
                    when always:
                      splice foo\.bar.baz scalar
    "#};

    assert_fragment_dump(source, "", expected);
}

#[test]
fn with_bound_nindented_dynamic_entries_attach_below_literal_key() {
    let source = indoc! {r#"
        spec:
        {{- with .Values.cfg }}
          config:
        {{- range $key, $value := . }}
        {{- $key | nindent 4 }}: {{ $value | quote }}
        {{- end }}
        {{- end }}
    "#};

    let expected = indoc! {r#"
        when always:
          mapping:
            key "spec":
              when (with(cfg) && range(cfg)):
                splice cfg fragment
              when always:
                mapping:
                  key "config":
                  key dynamic [splice cfg fragment range-key]:
                    when (with(cfg) && range(cfg)):
                      splice cfg.* scalar
        reads:
          cfg [with(cfg)]
          cfg [range(cfg), with(cfg)]
    "#};
    assert_fragment_dump(source, "", expected);

    let ir = SymbolicIrContext::new(&DefineIndex::new()).generate_contract_ir(source);
    let finalized = ir.finalize();
    assert!(
        finalized.uses().iter().any(|use_| {
            use_.source_expr == "cfg"
                && use_.kind == helm_schema_ir::ValueKind::Fragment
                && use_.path.0 == ["spec".to_string(), "config".to_string()]
        }),
        "the ranged map splice should project at spec.config: {finalized:#?}"
    );
}

#[test]
fn ranged_resource_with_bound_dynamic_entries_attach_below_literal_key() {
    let source = indoc! {r#"
        apiVersion: velero.io/v1
        kind: BackupStorageLocation
        spec:
        {{- range .Values.configuration.backupStorageLocation }}
          provider: {{ .provider }}
          objectStorage:
            bucket: {{ .bucket }}
        {{- with .config }}
          config:
        {{- range $key, $value := . }}
        {{- $key | nindent 4 }}: {{ $value | quote }}
        {{- end }}
        {{- end }}
        {{- end }}
    "#};

    let ir = SymbolicIrContext::new(&DefineIndex::new()).generate_contract_ir(source);
    let finalized = ir.finalize();
    assert!(
        finalized.uses().iter().any(|use_| {
            use_.source_expr == "configuration.backupStorageLocation.*.config"
                && use_.kind == helm_schema_ir::ValueKind::Fragment
                && use_.path.0 == ["spec".to_string(), "config".to_string()]
        }),
        "the ranged resource map splice should project at spec.config: {finalized:#?}"
    );
}

#[test]
fn velero_backup_location_config_attaches_below_config_key() {
    let source = test_util::read_testdata("charts/velero/templates/backupstoragelocation.yaml");
    let context = SymbolicIrContext::new(&DefineIndex::new());
    let fragment = context.eval_document_fragment(&source);
    let ir = context.generate_contract_ir(&source);
    let finalized = ir.finalize();
    assert!(
        finalized.uses().iter().any(|use_| {
            use_.source_expr == "configuration.backupStorageLocation.*.config"
                && use_.kind == helm_schema_ir::ValueKind::Fragment
                && use_.path.0 == ["spec".to_string(), "config".to_string()]
        }),
        "Velero's config map splice should project at spec.config:\n{}",
        dump_document(&fragment)
    );
}
