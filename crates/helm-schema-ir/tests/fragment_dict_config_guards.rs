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
          ingress.enabled [with(ingress), truthy(ingress.enabled)]
          ingress.className [with(ingress), truthy(ingress.enabled), truthy(ingress.className)]
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
