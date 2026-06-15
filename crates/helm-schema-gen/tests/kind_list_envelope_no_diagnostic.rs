//! End-to-end regression for Kubernetes `kind: List` envelopes.
//!
//! Vendored alertmanager templates (`ingressperreplica.yaml`,
//! `serviceperreplica.yaml`) wrap real resources in a `kind: List`
//! envelope. The envelope itself is transparent structural plumbing:
//! `.Values.*` uses inside `items[*]` must be attributed to the inner
//! resource and rebased from `items[*].spec...` to the inner
//! resource's `spec...` path.

mod common;

use helm_schema_ast::DefineIndex;
use helm_schema_ir::{ResourceRef, SymbolicIrContext, YamlPath};
use helm_schema_k8s::{
    Chain, Diagnostic, DiagnosticSink, K8sSchemaProvider, ProviderLookupResult, ProviderOrigin,
    ProviderSchemaFragment,
};
use serde_json::{Value, json};

const KIND_LIST_TEMPLATE: &str = "\
apiVersion: v1
kind: List
items:
{{- range $index, $replica := until (.Values.replicas | int) }}
  - apiVersion: networking.k8s.io/v1
    kind: Ingress
    metadata:
      name: {{ $.Release.Name }}-{{ $index }}
    spec:
      rules:
        - host: {{ $.Values.host | quote }}
{{- end }}
";

const KIND_LIST_VALUES: &str = "\
replicas: 2
host: example.com
";

#[test]
fn kind_list_envelope_descends_into_inner_resource() {
    let idx = DefineIndex::new();
    let ir = SymbolicIrContext::new(&idx).generate_contract_ir(KIND_LIST_TEMPLATE, &idx);
    let projection = ir.clone().project();
    assert!(
        projection.uses().iter().any(|use_| {
            use_.source_expr == "host"
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "rules[*]".to_string(),
                        "host".to_string(),
                    ]
                && use_.resource.as_ref().is_some_and(|resource| {
                    resource.kind == "Ingress" && resource.api_version == "networking.k8s.io/v1"
                })
        }),
        "host use must be attributed to the inner Ingress with a rebased path: {:#?}",
        projection
    );

    let diagnostics = DiagnosticSink::new();
    let chain =
        Chain::new(vec![Box::new(FakeIngressProvider)]).with_diagnostic_sink(diagnostics.clone());

    let schema = common::generate_schema_with_values_yaml(ir, &chain, Some(KIND_LIST_VALUES));
    assert_eq!(
        schema.pointer("/properties/host/description"),
        Some(&Value::String("inner ingress host".to_string())),
        "host must be validated through the inner Ingress spec.rules[*].host schema; got {schema}"
    );

    let snapshot = diagnostics.snapshot();
    let list_missing = snapshot
        .iter()
        .filter(|d| {
            matches!(
                d,
                Diagnostic::MissingSchema { kind, .. } if kind == "List"
            )
        })
        .count();
    assert_eq!(
        list_missing, 0,
        "Chain must not emit MissingSchema(kind=List, ...) for the K8s List envelope; got diagnostics: {snapshot:?}"
    );
}

#[derive(Debug)]
struct FakeIngressProvider;

impl K8sSchemaProvider for FakeIngressProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        if is_ingress_host(resource, path) {
            Some(json!({
                "description": "inner ingress host",
                "type": "string"
            }))
        } else {
            None
        }
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        match self.schema_for_resource_path(resource, path) {
            Some(schema) => ProviderLookupResult::Found {
                schema: ProviderSchemaFragment::new(schema),
                resolved_k8s_version: None,
            },
            None if self.has_resource(resource) => ProviderLookupResult::PathUnresolved,
            None => ProviderLookupResult::NotOwned,
        }
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        resource.kind == "Ingress" && resource.api_version == "networking.k8s.io/v1"
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }
}

fn is_ingress_host(resource: &ResourceRef, path: &YamlPath) -> bool {
    resource.kind == "Ingress"
        && resource.api_version == "networking.k8s.io/v1"
        && path.0
            == [
                "spec".to_string(),
                "rules[*]".to_string(),
                "host".to_string(),
            ]
}
