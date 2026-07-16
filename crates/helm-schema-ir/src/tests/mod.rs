mod contract;
mod contract_signals;
mod expr_eval;
mod expr_eval_helper_hooks;
mod fragment_expr_eval;
mod fragment_scope_eval;
mod resource_identity;
mod symbolic_local_state;

use crate::{Guard, SymbolicIrContext, ValueKind, YamlPath};
use helm_schema_core::{DYNAMIC_MAPPING_VALUE_SEGMENT, GuardDnf, Predicate};

/// The raw per-branch guard stacks of one helper meta (branch predicates in
/// canonical guard order plus the defaulted marker), the same lowering the
/// fragment projection's pathless reads use. Test-side replacement for the
/// retired emission-side `contract_guard_sets` (which additionally applied
/// the deleted sibling/suppression prune algebra).
pub(crate) fn raw_guard_sets(
    meta: &crate::helper_meta::HelperOutputMeta,
    source_expr: &str,
) -> Vec<Vec<Guard>> {
    let branches: Vec<Vec<Predicate>> = if meta.predicates.is_empty() {
        vec![Vec::new()]
    } else {
        meta.predicates
            .iter()
            .map(|branch| branch.iter().cloned().collect())
            .collect()
    };
    let mut condition = GuardDnf::from_contract_predicate_disjunction_preserving_evidence(branches);
    if meta.defaulted {
        condition = condition.conjoined_with_guards([Guard::Default {
            path: source_expr.to_string(),
        }]);
    }
    condition.guard_conjunctions()
}
use helm_schema_ast::DefineIndex;
use test_util::prelude::sim_assert_eq;

/// Simple template IR generation test.
#[test]
fn simple_template_ir() {
    let src = r"{{- if .Values.enabled }}
foo: {{ .Values.name }}
{{- end }}
";
    let idx = DefineIndex::new();
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    assert!(ir.uses().iter().any(|u| u.source_expr == "enabled"
        && u.single_guard_conjunction()
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
    assert!(ir.uses().iter().any(|u| u.source_expr == "name"
        && u.path == YamlPath(vec!["foo".to_string()])
        && u.kind == ValueKind::Scalar
        && u.single_guard_conjunction()
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
}

#[test]
fn direct_tpl_files_get_executes_json_template_source() {
    let src = r#"
apiVersion: v1
kind: Secret
data:
  clients.json: {{ tpl (.Files.Get "config/client-auth.json") . | b64enc }}
"#;
    let file = r#"
{{- range $user := .Values.users }}
{{ $user.username }}: {{ $user.password }}
{{- end }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("config/client-auth.json", file);
    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    for path in ["users.*.username", "users.*.password"] {
        assert!(
            ir.uses().iter().any(|use_| use_.source_expr == path),
            "the tpl-executed file should contribute {path}: {ir:#?}"
        );
    }
}

#[test]
fn base_path_include_executes_implicit_template_source() {
    let src = r#"
apiVersion: v1
kind: ConfigMap
data:
  initialize: |-
    {{ include (print $.Template.BasePath "/_create.txt") . | nindent 4 }}
"#;
    let partial = r#"
{{- range $bucket := .Values.buckets }}
create {{ $bucket.name }}
{{- end }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("templates/_create.txt", partial);
    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    assert!(
        ir.uses()
            .iter()
            .any(|use_| use_.source_expr == "buckets.*.name"),
        "the implicit template body should contribute its member access: {ir:#?}"
    );
}

#[test]
fn dynamic_mapping_value_projects_structural_member_path() {
    let src = r"
apiVersion: v1
kind: ConfigMap
data:
  {{- range $key, $value := .Values.entries }}
  {{ $key }}: {{ $value }}
  {{- end }}
";
    let idx = DefineIndex::new();
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    assert!(ir.uses().iter().any(|use_| {
        use_.source_expr == "entries.*"
            && use_.path
                == YamlPath(vec![
                    "data".to_string(),
                    DYNAMIC_MAPPING_VALUE_SEGMENT.to_string(),
                ])
            && use_.kind == ValueKind::Scalar
    }));
}

#[test]
fn document_output_projection_preserves_resource_claim() {
    let src = r"
apiVersion: v1
kind: Service
metadata:
  name: {{ .Values.serviceName }}
";
    let idx = DefineIndex::new();
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    let name_use = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "serviceName")
        .expect("serviceName use");

    sim_assert_eq!(
        have: name_use.path,
        want: YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    let resource = name_use.resource.as_ref().expect("resource claim");
    sim_assert_eq!(have: resource.api_version, want: "v1");
    sim_assert_eq!(have: resource.kind, want: "Service");
}

#[test]
fn scalar_helper_document_projection_preserves_resource_claim() {
    let helpers = r#"
{{- define "common.serviceName" -}}
{{ .Values.serviceName }}
{{- end -}}
"#;
    let src = r#"
apiVersion: v1
kind: Service
metadata:
  name: {{ include "common.serviceName" . }}
"#;
    let mut idx = DefineIndex::new();
    idx.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    let name_use = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "serviceName")
        .expect("serviceName use");

    sim_assert_eq!(
        have: name_use.path,
        want: YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    let resource = name_use.resource.as_ref().expect("resource claim");
    sim_assert_eq!(have: resource.api_version, want: "v1");
    sim_assert_eq!(have: resource.kind, want: "Service");
}

#[test]
fn document_guard_survives_helper_sibling_claim_scoping() {
    let helpers = r#"
{{- define "guarded.port" -}}
{{- if .enabled -}}
{{- .port -}}
{{- end -}}
{{- end -}}
"#;
    let src = r#"
{{- if .Values.enabled }}
apiVersion: v1
kind: Service
spec:
  value: {{ include "guarded.port" (dict "enabled" .Values.enabled "port" .Values.port) }}
{{- end }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    let port = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "port" && !use_.path.0.is_empty())
        .unwrap_or_else(|| panic!("expected rendered port use: {ir:#?}"));
    assert!(
        port.single_guard_conjunction().contains(&Guard::Truthy {
            path: "enabled".to_string(),
        }),
        "the document guard executes outside the helper's sibling claims: {port:#?}"
    );
}

#[test]
fn document_branch_guard_survives_local_helper_reassignment() {
    let helpers = r#"
{{- define "selected.value" -}}
{{- .Values.payload -}}
{{- end -}}
"#;
    let src = r#"
{{- $selected := "" -}}
{{- if eq .Values.mode "active" -}}
{{- $selected = include "selected.value" . -}}
{{- end -}}
{{- if $selected }}
apiVersion: v1
kind: ConfigMap
data:
  value: {{ $selected }}
{{- end }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    let payload = ir
        .uses()
        .iter()
        .find(|use_| {
            use_.source_expr == "payload"
                && use_.path == YamlPath(vec!["data".to_string(), "value".to_string()])
        })
        .unwrap_or_else(|| panic!("expected rendered payload use: {ir:#?}"));
    assert!(
        payload.single_guard_conjunction().contains(&Guard::Eq {
            path: "mode".to_string(),
            value: helm_schema_core::GuardValue::string("active"),
        }),
        "the assignment branch must remain on the local's rendered value: {payload:#?}"
    );
}

#[test]
fn document_local_coalesce_preserves_ordered_candidate_selection() {
    let src = r#"
{{- $selected := coalesce .Values.primary .Values.fallback -}}
apiVersion: v1
kind: Secret
data:
  value: {{ $selected | b64enc | quote }}
"#;
    let ir = SymbolicIrContext::new(&DefineIndex::new())
        .generate_contract_ir(src)
        .finalize();

    let primary = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "primary" && !use_.path.0.is_empty())
        .unwrap_or_else(|| panic!("expected rendered primary use: {ir:#?}"));
    sim_assert_eq!(
        have: primary.condition.guard_conjunctions(),
        want: vec![vec![
            Guard::Truthy {
                path: "primary".to_string(),
            },
            Guard::Default {
                path: "primary".to_string(),
            },
        ]]
    );

    let fallback = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "fallback" && !use_.path.0.is_empty())
        .unwrap_or_else(|| panic!("expected rendered fallback use: {ir:#?}"));
    sim_assert_eq!(
        have: fallback.condition.guard_conjunctions(),
        want: vec![vec![
            Guard::Truthy {
                path: "fallback".to_string(),
            },
            Guard::Not {
                path: "primary".to_string(),
            },
            Guard::Default {
                path: "fallback".to_string(),
            },
        ]]
    );
}

#[test]
fn helper_type_dispatch_keeps_or_candidate_selection_on_each_row() {
    let helpers = r#"
{{- define "selected.value" -}}
{{- $selected := or .Values.primary .Values.fallback -}}
{{- if $selected -}}
{{- $type := typeOf $selected -}}
{{- if eq $type "string" -}}
{{ tpl $selected . }}
{{- else -}}
{{ toYaml $selected }}
{{- end -}}
{{- end -}}
{{- end -}}
"#;
    let src = r#"
apiVersion: v1
kind: ConfigMap
data:
  value: {{ include "selected.value" . | quote }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    let primary = ir
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == "primary" && !use_.path.0.is_empty())
        .collect::<Vec<_>>();
    assert!(!primary.is_empty(), "expected primary rows: {ir:#?}");
    assert!(
        primary.iter().all(|use_| {
            use_.condition
                .disjuncts()
                .iter()
                .all(|branch| branch.contains(&Predicate::truthy_path("primary")))
        }),
        "every rendered primary row must retain its selection predicate: {primary:#?}"
    );

    let fallback = ir
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == "fallback" && !use_.path.0.is_empty())
        .collect::<Vec<_>>();
    assert!(!fallback.is_empty(), "expected fallback rows: {ir:#?}");
    assert!(
        fallback.iter().all(|use_| {
            use_.condition
                .disjuncts()
                .iter()
                .all(|branch| branch.contains(&Predicate::truthy_path("primary").negated()))
        }),
        "every rendered fallback row must retain the earlier empty predicate: {fallback:#?}"
    );
}

#[test]
fn opaque_include_guard_abstains_from_provider_schema_evidence() {
    let helpers = r#"
{{- define "resource.enabled" -}}
{{- if .Values.enabled -}}
true
{{- end -}}
{{- end -}}
"#;
    let src = r#"
{{- if include "resource.enabled" . }}
apiVersion: v1
kind: ConfigMap
data:
  value: {{ .Values.payload }}
{{- end }}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("<inline:0>", helpers);
    let finalized = SymbolicIrContext::new(&index)
        .generate_contract_ir(src)
        .finalize();

    let payload = finalized
        .uses()
        .iter()
        .find(|use_| {
            use_.source_expr == "payload"
                && use_.path == YamlPath(vec!["data".to_string(), "value".to_string()])
        })
        .unwrap_or_else(|| panic!("expected rendered payload use: {finalized:#?}"));
    assert!(
        payload
            .condition
            .disjuncts()
            .iter()
            .all(|conjunction| conjunction.iter().any(Predicate::contains_approximation)),
        "the unlowerable include result must remain approximate in memory: {payload:#?}"
    );
    assert!(
        finalized
            .schema_signals()
            .schema_evidence_by_value_path()
            .get("payload")
            .is_none_or(|evidence| evidence.provider_schema_uses.is_empty()),
        "an approximate resource guard must not leak provider constraints: {finalized:#?}"
    );
}

#[test]
fn scalar_helper_document_projection_preserves_scope_guard() {
    let helpers = r#"
{{- define "common.serviceName" -}}
{{ .Values.serviceName }}
{{- end -}}
"#;
    let src = r#"
apiVersion: v1
kind: Service
metadata:
  {{- if .Values.enabled }}
  name: {{ include "common.serviceName" . }}
  {{- end }}
"#;
    let mut idx = DefineIndex::new();
    idx.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    let name_use = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "serviceName")
        .expect("serviceName use");

    sim_assert_eq!(
        have: name_use.single_guard_conjunction(),
        want: vec![Guard::Truthy {
            path: "enabled".to_string()
        }]
    );
}

#[test]
fn labels_helper_does_not_apply_custom_label_guard_to_name_helper_dependency() {
    let src = r#"
{{- if .Values.networkPolicy.enabled }}
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: {{ template "common.names.fullname" . }}
  labels: {{- include "common.labels.standard" ( dict "customLabels" .Values.commonLabels "context" $ ) | nindent 4 }}
spec:
  podSelector:
    matchLabels: {{- include "common.labels.matchLabels" ( dict "customLabels" .Values.commonLabels "context" $ ) | nindent 6 }}
{{- end }}
"#;
    let mut idx = DefineIndex::new();
    let loaded = test_util::DefineSourceSpec {
        helper_templates: &[],
        helper_template_dirs: &[("charts/common/templates", "tpl")],
        file_sources: &[],
    }
    .load();
    for (idx_num, source) in loaded.helper_templates.into_iter().enumerate() {
        idx.add_file_source(&format!("<inline:{idx_num}>"), &source);
    }
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    let name_override_uses = ir
        .uses()
        .iter()
        .filter(|use_| use_.source_expr == "nameOverride")
        .collect::<Vec<_>>();

    let pathless_name_override_uses = name_override_uses
        .iter()
        .filter(|use_| use_.path.0.is_empty())
        .collect::<Vec<_>>();
    assert!(
        pathless_name_override_uses.iter().all(|use_| !use_
            .condition
            .guard_conjunctions()
            .iter()
            .flatten()
            .any(|guard| matches!(guard, Guard::Truthy { path } if path == "commonLabels"))),
        "commonLabels is the custom-label source, not a guard for the pathless common.names.name dependency: {pathless_name_override_uses:#?}"
    );
    let selected_default_branch = [
        Guard::Truthy {
            path: "nameOverride".to_string(),
        },
        Guard::Truthy {
            path: "networkPolicy.enabled".to_string(),
        },
        Guard::Default {
            path: "nameOverride".to_string(),
        },
    ];
    assert!(
        name_override_uses.iter().any(|use_| {
            use_.path == YamlPath(Vec::new())
                && use_
                    .condition
                    .guard_conjunctions()
                    .iter()
                    .any(|guards| guards == &selected_default_branch)
        }),
        "expected pathless nameOverride dependency to keep its selected default branch under the document execution guard: {name_override_uses:#?}"
    );
    let app_name_path = YamlPath(vec![
        "metadata".to_string(),
        "labels".to_string(),
        "app.kubernetes.io/name".to_string(),
    ]);
    assert!(
        name_override_uses
            .iter()
            .filter(|use_| use_.path == app_name_path)
            .all(|use_| !use_
                .condition
                .guard_conjunctions()
                .iter()
                .flatten()
                .any(|guard| matches!(guard, Guard::Not { path } if path == "nameOverride"))),
        "a customLabels branch should not keep nameOverride=false after common.names.name is projected: {name_override_uses:#?}"
    );
}

#[test]
fn transitive_scalar_helper_default_projects_default_guard() {
    let helpers = r#"
{{- define "liba.fullname" -}}
{{- include "libb.name" . -}}
{{- end -}}

{{- define "libb.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}
"#;
    let src = r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "liba.fullname" . }}
"#;
    let mut idx = DefineIndex::new();
    idx.add_file_source("<inline:0>", helpers);
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src)
        .finalize();

    assert!(
        ir.uses().iter().any(|use_| {
            use_.source_expr == "nameOverride"
                && use_.path == YamlPath(vec!["metadata".to_string(), "name".to_string()])
                && use_.single_guard_conjunction().contains(&Guard::Default {
                    path: "nameOverride".to_string(),
                })
        }),
        "expected transitive helper default to survive into rendered contract use, got {:?}",
        ir.uses()
    );
}

#[test]
fn nonempty_choice_list_range_preserves_computed_mutation() {
    let helpers = r#"
{{- define "mutate.patch" -}}
{{- $patch := .Values.patch -}}
{{- $keys := list "path" -}}
{{- if .Values.copy -}}
{{- $keys = append $keys "from" -}}
{{- end -}}
{{- range $key := $keys -}}
{{- $_ := set $patch (printf "%sKey" $key) "derived" -}}
{{- end -}}
{{- $patch.pathKey -}}
{{- end -}}
"#;
    let mut index = DefineIndex::new();
    index.add_file_source("<inline:0>", helpers);

    let ir = SymbolicIrContext::new(&index)
        .generate_contract_ir(r#"{{ include "mutate.patch" . }}"#)
        .finalize();

    assert!(
        ir.uses()
            .iter()
            .all(|use_| use_.source_expr != "patch.pathKey"),
        "the guaranteed path iteration sets pathKey before its later read: {ir:#?}"
    );
}
