#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

fn build_cert_manager_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/cert-manager/templates/_helpers.tpl"),
    );
    idx
}

#[test]
fn schema_fused_rust() {
    let src = test_util::read_testdata("charts/cert-manager/templates/service.yaml");
    let values_yaml = test_util::read_testdata("charts/cert-manager/values.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_cert_manager_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "prometheus": {
                "additionalProperties": false,
                "properties": {
                    "enabled": {
                        "type": "boolean"
                    },
                    "podmonitor": {
                        "additionalProperties": false,
                        "properties": {
                            "enabled": {
                                "type": "boolean"
                            }
                        },
                        "type": "object"
                    },
                    "servicemonitor": {
                        "additionalProperties": false,
                        "properties": {
                            "targetPort": {
                                "oneOf": [
                                    { "type": "string" },
                                    { "type": "integer" }
                                ]
                            }
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "serviceAnnotations": {
                "additionalProperties": {
                    "type": "string"
                },
                "description": "Annotations is an unstructured key value map stored with a resource that may be set by external tools to store and retrieve arbitrary metadata. They are not queryable and should be preserved when modifying objects. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/annotations",
                "type": "object"
            },
            "serviceIPFamilies": {
                "description": "IPFamilies is a list of IP families (e.g. IPv4, IPv6) assigned to this service. This field is usually assigned automatically based on cluster configuration and the ipFamilyPolicy field. If this field is specified manually, the requested family is available in the cluster, and ipFamilyPolicy allows it, it will be used; otherwise creation of the service will fail. This field is conditionally mutable: it allows for adding or removing a secondary IP family, but it does not allow changing the primary IP family of the Service. Valid values are \"IPv4\" and \"IPv6\".  This field only applies to Services of types ClusterIP, NodePort, and LoadBalancer, and does apply to \"headless\" services. This field will be wiped when updating a Service to type ExternalName.\n\nThis field may hold a maximum of two entries (dual-stack families, in either order).  These families must correspond to the values of the clusterIPs field, if specified. Both clusterIPs and ipFamilies are governed by the ipFamilyPolicy field.",
                "items": {
                    "type": "string"
                },
                "type": "array",
                "x-kubernetes-list-type": "atomic"
            },
            "serviceIPFamilyPolicy": {
                "description": "IPFamilyPolicy represents the dual-stack-ness requested or required by this Service. If there is no value provided, then this field will be set to SingleStack. Services can be \"SingleStack\" (a single IP family), \"PreferDualStack\" (two IP families on dual-stack configured clusters or a single IP family on single-stack clusters), or \"RequireDualStack\" (two IP families on dual-stack configured clusters, otherwise fail). The ipFamilies and clusterIPs fields depend on the value of this field. This field will be wiped when updating a service to type ExternalName.",
                "type": "string"
            },
            "serviceLabels": {
                "additionalProperties": {
                    "type": "string"
                },
                "description": "Map of string keys and values that can be used to organize and categorize (scope and select) objects. May match selectors of replication controllers and services. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels",
                "type": "object"
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
}
