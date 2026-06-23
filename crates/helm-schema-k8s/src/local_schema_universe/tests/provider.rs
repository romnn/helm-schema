use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

fn resource(api_version: &str) -> ResourceRef {
    ResourceRef {
        api_version: api_version.to_string(),
        kind: "Widget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    }
}

fn widget_universe() -> LocalSchemaUniverse {
    LocalSchemaUniverse::from_crd_documents([json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": {"kind": "Widget"},
            "versions": [
                {
                    "name": "v1",
                    "served": true,
                    "schema": {
                        "openAPIV3Schema": {
                            "type": "object",
                            "properties": {
                                "spec": {
                                    "type": "object",
                                    "properties": {
                                        "size": {"type": "integer"}
                                    }
                                }
                            }
                        }
                    }
                }
            ]
        }
    })])
}

#[test]
fn resolves_served_crd_version_schema_from_universe() {
    let provider = ChartLocalCrdSchemaProvider::new(widget_universe());

    let schema = provider.lookup(
        &resource("example.com/v1"),
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );

    let ProviderLookupResult::Found { schema, .. } = schema else {
        panic!("chart-local provider should resolve spec.size");
    };
    sim_assert_eq!(
        have: schema.into_schema(),
        want: json!({"type": "integer"})
    );
}

#[test]
fn lookup_attaches_chart_local_provider_source() {
    let mut universe = LocalSchemaUniverse::default();
    universe.insert_crd_document_with_source(
        json!({
            "apiVersion": "apiextensions.k8s.io/v1",
            "kind": "CustomResourceDefinition",
            "spec": {
                "group": "example.com",
                "names": {"kind": "Widget"},
                "versions": [
                    {
                        "name": "v1",
                        "served": true,
                        "schema": {
                            "openAPIV3Schema": {
                                "type": "object",
                                "properties": {
                                    "spec": {
                                        "type": "object",
                                        "properties": {
                                            "size": {"type": "integer"}
                                        }
                                    }
                                }
                            }
                        }
                    }
                ]
            }
        }),
        "chart-static-crd",
        "/chart/crds/widgets.yaml",
    );
    let provider = ChartLocalCrdSchemaProvider::new(universe);

    let result = provider.lookup(
        &resource("example.com/v1"),
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );
    let ProviderLookupResult::Found { schema, .. } = result else {
        panic!("chart-local lookup should resolve spec.size");
    };
    let source = schema.source().expect("chart-local source should attach");

    sim_assert_eq!(have: source.origin(), want: ProviderOrigin::ChartLocalCrd);
    sim_assert_eq!(have: source.source_id(), want: "chart-static-crd");
    sim_assert_eq!(have: source.filename(), want: "/chart/crds/widgets.yaml");
    sim_assert_eq!(have: source.pointer(), want: "/properties/spec/properties/size");
}

#[test]
fn api_version_guess_uses_chart_local_crd_origin() {
    let provider = ChartLocalCrdSchemaProvider::new(widget_universe()).with_api_version_guess(true);

    sim_assert_eq!(
        have: provider.infer_api_version_candidates("Widget"),
        want: vec![ApiVersionCandidate {
            api_version: "example.com/v1".to_string(),
            source: InferenceSource::ChartLocalCrd,
            origin: ProviderOrigin::ChartLocalCrd,
        }]
    );
}
