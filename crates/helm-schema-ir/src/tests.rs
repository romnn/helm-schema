use crate::{Guard, SymbolicIrContext, ValueKind, YamlPath};
use helm_schema_ast::{DefineIndex, TreeSitterParser};
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
        .project();

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
        .project();

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
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src, &idx)
        .project();

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
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src, &idx)
        .project();

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
    idx.add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&idx)
        .generate_contract_ir(src, &idx)
        .project();

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
