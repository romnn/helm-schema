mod abstract_value_output_projection;
mod contract;
mod contract_signals;
mod document_projection {
    use helm_schema_ast::{
        AttributionIndex, DefineIndex, OutputSlot, OutputSlotKind, build_attribution_index,
    };

    use crate::{ValueKind, YamlPath, analysis_db::IrAnalysisDb};

    struct DocumentTracker<'a> {
        source: &'a str,
        analysis_db: IrAnalysisDb,
        attribution: AttributionIndex,
    }

    impl<'a> DocumentTracker<'a> {
        fn new(source: &'a str, defines: &'a DefineIndex) -> Self {
            Self {
                source,
                analysis_db: IrAnalysisDb::new(defines),
                attribution: AttributionIndex::default(),
            }
        }

        fn reset_for_tree(&mut self, tree: &tree_sitter::Tree) {
            self.attribution = build_attribution_index(self.source, tree.root_node())
                .with_resource_spans(crate::resource_identity::collect_resource_spans(
                    self.source,
                    &self.analysis_db,
                ));
        }

        fn control_site_for_node(
            &self,
            node: tree_sitter::Node<'_>,
        ) -> helm_schema_ast::ControlSite {
            self.attribution
                .control_site_for_node(node)
                .unwrap_or_default()
        }

        fn output_slot_for_action(&self, node: tree_sitter::Node<'_>) -> OutputSlot {
            self.attribution
                .output_slot_for_node(node)
                .unwrap_or_else(|| OutputSlot {
                    kind: ValueKind::Scalar,
                    path: YamlPath(Vec::new()),
                    resource: None,
                    slot: OutputSlotKind::Opaque,
                })
        }
    }

    mod helper_contract;
    mod tracker;
}
mod expr_eval;
mod expr_eval_helper_hooks;
mod fragment_expr_eval;
mod fragment_range_scope;
mod fragment_scope_eval;
mod resource_identity;
mod symbolic_local_state;

use crate::{Guard, SymbolicIrContext, ValueKind, YamlPath};
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
        .generate_contract_ir(src, &idx)
        .finalize();

    assert!(ir.uses().iter().any(|u| u.source_expr == "enabled"
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
    assert!(ir.uses().iter().any(|u| u.source_expr == "name"
        && u.path == YamlPath(vec!["foo".to_string()])
        && u.kind == ValueKind::Scalar
        && u.guards
            == vec![Guard::Truthy {
                path: "enabled".to_string()
            }]));
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
        .generate_contract_ir(src, &idx)
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
        .generate_contract_ir(src, &idx)
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
        .generate_contract_ir(src, &idx)
        .finalize();

    let name_use = ir
        .uses()
        .iter()
        .find(|use_| use_.source_expr == "serviceName")
        .expect("serviceName use");

    sim_assert_eq!(
        have: name_use.guards,
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
        .generate_contract_ir(src, &idx)
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
            .guards
            .iter()
            .any(|guard| matches!(guard, Guard::Truthy { path } if path == "commonLabels"))),
        "commonLabels is the custom-label source, not a guard for the pathless common.names.name dependency: {pathless_name_override_uses:#?}"
    );
    let own_default_branch = [
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
        name_override_uses
            .iter()
            .any(|use_| { use_.path == YamlPath(Vec::new()) && use_.guards == own_default_branch }),
        "expected pathless nameOverride dependency to keep its own branch guards: {name_override_uses:#?}"
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
                .guards
                .iter()
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
        .generate_contract_ir(src, &idx)
        .finalize();

    assert!(
        ir.uses().iter().any(|use_| {
            use_.source_expr == "nameOverride"
                && use_.path == YamlPath(vec!["metadata".to_string(), "name".to_string()])
                && use_.guards.contains(&Guard::Default {
                    path: "nameOverride".to_string(),
                })
        }),
        "expected transitive helper default to survive into rendered contract use, got {:?}",
        ir.uses()
    );
}
