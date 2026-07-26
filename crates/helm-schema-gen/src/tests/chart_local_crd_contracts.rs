use indoc::indoc;

use super::*;

/// Provider chain over a chart-shipped CRD document, the way the CLI builds
/// it: the chart-local universe answers first, the bundle behind it.
fn chart_local_crd_provider(document: &Value) -> Chain {
    let mut universe = helm_schema_k8s::LocalSchemaUniverse::default();
    for resource_schema in helm_schema_k8s::resource_schemas_from_crd_document_with_source(
        document,
        "chart-static-crd",
        "crds/widget.yaml".to_string(),
    ) {
        universe.insert_resource_schema(resource_schema);
    }
    Chain::new(vec![
        Box::new(helm_schema_k8s::ChartLocalCrdSchemaProvider::new(universe)),
        Box::new(
            KubernetesJsonSchemaProvider::new("v1.35.0")
                .with_cache_dir(bundle_cache_dir())
                .with_allow_download(false),
        ),
    ])
    .with_inference_enabled(true)
}

fn widget_crd(api_version: &str, preserve_unknown_fields: Option<bool>) -> Value {
    let mut document = serde_json::json!({
        "apiVersion": api_version,
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": { "kind": "Widget" },
            "versions": [{
                "name": "v1",
                "served": true,
                "schema": { "openAPIV3Schema": {
                    "type": "object",
                    "properties": {
                        "metadata": {
                            "type": "object",
                            "properties": { "name": { "type": "string" } },
                        },
                        "spec": {
                            "type": "object",
                            "properties": {
                                "sink": {
                                    "type": "object",
                                    "properties": { "port": { "type": "integer" } },
                                },
                                "rules": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": { "action": { "type": "string" } },
                                    },
                                },
                                "opaque": {
                                    "type": "object",
                                    "properties": { "action": { "type": "string" } },
                                    "x-kubernetes-preserve-unknown-fields": true,
                                },
                            },
                        },
                    },
                }},
            }],
        },
    });
    if let Some(preserve) = preserve_unknown_fields {
        document["spec"]["preserveUnknownFields"] = Value::Bool(preserve);
    }
    document
}

const WIDGET_TEMPLATE: &str = indoc! {r"
    apiVersion: example.com/v1
    kind: Widget
    metadata:
      name: test
      annotations:
        {{- toYaml .Values.annotations | nindent 4 }}
    spec:
      sink:
        {{- toYaml .Values.sink | nindent 4 }}
      rules:
        {{- toYaml .Values.rules | nindent 4 }}
      opaque:
        {{- toYaml .Values.opaque | nindent 4 }}
"};

const WIDGET_VALUES: &str = indoc! {"
    annotations: {}
    sink: {}
    rules: []
    opaque: {}
"};

/// A pruning CRD states its whole contract: the API server drops (and, under
/// the default strict field validation, rejects) anything the structural
/// schema does not declare, so a chart that ships the CRD must hand out the
/// same closed contract the CRDs catalog does. The three open readings —
/// `metadata`, a `x-kubernetes-preserve-unknown-fields` subtree, and the
/// document root — stay open.
#[test]
fn chart_shipped_crds_close_the_fields_their_schema_prunes() {
    let provider = chart_local_crd_provider(&widget_crd("apiextensions.k8s.io/v1", None));
    let signals = schema_signals_for(parse_ir(WIDGET_TEMPLATE));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &provider).with_values_yaml(Some(WIDGET_VALUES)),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "sink": { "port": 7 } }),
            true,
            "declared member",
        ),
        (
            serde_json::json!({ "sink": { "unknown": 7 } }),
            false,
            "pruned member",
        ),
        (
            serde_json::json!({ "rules": [{ "action": "keep" }] }),
            true,
            "declared item member",
        ),
        (
            serde_json::json!({ "rules": [{ "unknown": "keep" }] }),
            false,
            "pruned item member",
        ),
        (
            serde_json::json!({ "opaque": { "unknown": "keep" } }),
            true,
            "preserved subtree",
        ),
        (
            serde_json::json!({ "annotations": { "example.com/note": "keep" } }),
            true,
            "metadata is never pruned",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "pruning CRD {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A `v1beta1` CRD keeps unknown fields unless it opts into pruning, so its
/// schema is a partial description of what the resource accepts.
#[test]
fn non_pruning_crds_keep_their_undeclared_fields_open() {
    for (preserve_unknown_fields, want, label) in [
        (None, true, "v1beta1 default"),
        (Some(true), true, "explicit opt-out"),
        (Some(false), false, "explicit pruning"),
    ] {
        let provider = chart_local_crd_provider(&widget_crd(
            "apiextensions.k8s.io/v1beta1",
            preserve_unknown_fields,
        ));
        let signals = schema_signals_for(parse_ir(WIDGET_TEMPLATE));
        let schema = generate_values_schema(
            ValuesSchemaInput::new(&signals, &provider).with_values_yaml(Some(WIDGET_VALUES)),
        );
        let instance = serde_json::json!({ "sink": { "unknown": 7 } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}
