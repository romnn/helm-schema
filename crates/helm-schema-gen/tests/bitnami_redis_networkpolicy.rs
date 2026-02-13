#![recursion_limit = "512"]

mod common;

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(
        parser,
        &test_util::read_testdata("charts/bitnami-redis/templates/_helpers.tpl"),
    );
    idx
}

/// Full schema generation for networkpolicy using fused-Rust parser.
///
/// The generated schema should capture all `.Values.*` references from the
/// networkpolicy template and produce a well-structured JSON schema that a
/// devops engineer would recognize as describing the values.yaml structure.
#[test]
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&schema).expect("pretty json")
        );
    }

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "architecture": {
                "anyOf": [
                    { "enum": ["replication"] },
                    { "type": "string" }
                ]
            },
            "commonAnnotations": {
                "additionalProperties": { "type": "string" },
                "description": "Annotations is an unstructured key value map stored with a resource that may be set by external tools to store and retrieve arbitrary metadata. They are not queryable and should be preserved when modifying objects. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/annotations",
                "properties": {},
                "type": "object"
            },
            "commonLabels": {
                "additionalProperties": { "type": "string" },
                "description": "Map of string keys and values that can be used to organize and categorize (scope and select) objects. May match selectors of replication controllers and services. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels",
                "properties": {},
                "type": "object"
            },
            "master": {
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "additionalProperties": false,
                        "properties": {
                            "redis": {
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
            "metrics": {
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "additionalProperties": false,
                        "properties": {
                            "http": {
                                "oneOf": [
                                    { "type": "string" },
                                    { "type": "integer" }
                                ]
                            }
                        },
                        "type": "object"
                    },
                    "enabled": { "type": "boolean" }
                },
                "type": "object"
            },
            "networkPolicy": {
                "additionalProperties": false,
                "properties": {
                    "allowExternal": { "type": "boolean" },
                    "allowExternalEgress": { "type": "boolean" },
                    "enabled": { "type": "boolean" },
                    "extraEgress": {
                        "description": "egress is a list of egress rules to be applied to the selected pods. Outgoing traffic is allowed if there are no NetworkPolicies selecting the pod (and cluster policy otherwise allows the traffic), OR if the traffic matches at least one egress rule across all of the NetworkPolicy objects whose podSelector matches the pod. If this field is empty then this NetworkPolicy limits all outgoing traffic (and serves solely to ensure that the pods it selects are isolated by default). This field is beta-level in 1.8",
                        "items": {
                            "description": "NetworkPolicyEgressRule describes a particular set of traffic that is allowed out of pods matched by a NetworkPolicySpec's podSelector. The traffic must match both ports and to. This type is beta-level in 1.8",
                            "properties": {
                                "ports": {
                                    "description": "ports is a list of destination ports for outgoing traffic. Each item in this list is combined using a logical OR. If this field is empty or missing, this rule matches all ports (traffic not restricted by port). If this field is present and contains at least one item, then this rule allows traffic only if the traffic matches at least one port in the list.",
                                    "items": {
                                        "description": "NetworkPolicyPort describes a port to allow traffic on",
                                        "properties": {
                                            "endPort": {
                                                "description": "endPort indicates that the range of ports from port to endPort if set, inclusive, should be allowed by the policy. This field cannot be defined if the port field is not defined or if the port field is defined as a named (string) port. The endPort must be equal or greater than port.",
                                                "format": "int32",
                                                "type": "integer"
                                            },
                                            "port": {
                                                "oneOf": [
                                                    { "type": "string" },
                                                    { "type": "integer" }
                                                ]
                                            },
                                            "protocol": {
                                                "description": "protocol represents the protocol (TCP, UDP, or SCTP) which traffic must match. If not specified, this field defaults to TCP.",
                                                "type": "string"
                                            }
                                        },
                                        "type": "object"
                                    },
                                    "type": "array",
                                    "x-kubernetes-list-type": "atomic"
                                },
                                "to": {
                                    "description": "to is a list of destinations for outgoing traffic of pods selected for this rule. Items in this list are combined using a logical OR operation. If this field is empty or missing, this rule matches all destinations (traffic not restricted by destination). If this field is present and contains at least one item, this rule allows traffic only if the traffic matches at least one item in the to list.",
                                    "items": {
                                        "description": "NetworkPolicyPeer describes a peer to allow traffic to/from. Only certain combinations of fields are allowed",
                                        "properties": {
                                            "ipBlock": {
                                                "description": "IPBlock describes a particular CIDR (Ex. \"192.168.1.0/24\",\"2001:db8::/64\") that is allowed to the pods matched by a NetworkPolicySpec's podSelector. The except entry describes CIDRs that should not be included within this rule.",
                                                "properties": {
                                                    "cidr": {
                                                        "description": "cidr is a string representing the IPBlock Valid examples are \"192.168.1.0/24\" or \"2001:db8::/64\"",
                                                        "type": "string"
                                                    },
                                                    "except": {
                                                        "description": "except is a slice of CIDRs that should not be included within an IPBlock Valid examples are \"192.168.1.0/24\" or \"2001:db8::/64\" Except values will be rejected if they are outside the cidr range",
                                                        "items": { "type": "string" },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    }
                                                },
                                                "required": ["cidr"],
                                                "type": "object"
                                            },
                                            "namespaceSelector": {
                                                "description": "A label selector is a label query over a set of resources. The result of matchLabels and matchExpressions are ANDed. An empty label selector matches all objects. A null label selector matches no objects.",
                                                "properties": {
                                                    "matchExpressions": {
                                                        "description": "matchExpressions is a list of label selector requirements. The requirements are ANDed.",
                                                        "items": {
                                                            "description": "A label selector requirement is a selector that contains values, a key, and an operator that relates the key and values.",
                                                            "properties": {
                                                                "key": {
                                                                    "description": "key is the label key that the selector applies to.",
                                                                    "type": "string"
                                                                },
                                                                "operator": {
                                                                    "description": "operator represents a key's relationship to a set of values. Valid operators are In, NotIn, Exists and DoesNotExist.",
                                                                    "type": "string"
                                                                },
                                                                "values": {
                                                                    "description": "values is an array of string values. If the operator is In or NotIn, the values array must be non-empty. If the operator is Exists or DoesNotExist, the values array must be empty. This array is replaced during a strategic merge patch.",
                                                                    "items": { "type": "string" },
                                                                    "type": "array",
                                                                    "x-kubernetes-list-type": "atomic"
                                                                }
                                                            },
                                                            "required": ["key", "operator"],
                                                            "type": "object"
                                                        },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    },
                                                    "matchLabels": {
                                                        "additionalProperties": { "type": "string" },
                                                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                                        "type": "object"
                                                    }
                                                },
                                                "type": "object",
                                                "x-kubernetes-map-type": "atomic"
                                            },
                                            "podSelector": {
                                                "description": "A label selector is a label query over a set of resources. The result of matchLabels and matchExpressions are ANDed. An empty label selector matches all objects. A null label selector matches no objects.",
                                                "properties": {
                                                    "matchExpressions": {
                                                        "description": "matchExpressions is a list of label selector requirements. The requirements are ANDed.",
                                                        "items": {
                                                            "description": "A label selector requirement is a selector that contains values, a key, and an operator that relates the key and values.",
                                                            "properties": {
                                                                "key": {
                                                                    "description": "key is the label key that the selector applies to.",
                                                                    "type": "string"
                                                                },
                                                                "operator": {
                                                                    "description": "operator represents a key's relationship to a set of values. Valid operators are In, NotIn, Exists and DoesNotExist.",
                                                                    "type": "string"
                                                                },
                                                                "values": {
                                                                    "description": "values is an array of string values. If the operator is In or NotIn, the values array must be non-empty. If the operator is Exists or DoesNotExist, the values array must be empty. This array is replaced during a strategic merge patch.",
                                                                    "items": { "type": "string" },
                                                                    "type": "array",
                                                                    "x-kubernetes-list-type": "atomic"
                                                                }
                                                            },
                                                            "required": ["key", "operator"],
                                                            "type": "object"
                                                        },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    },
                                                    "matchLabels": {
                                                        "additionalProperties": { "type": "string" },
                                                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                                        "type": "object"
                                                    }
                                                },
                                                "type": "object",
                                                "x-kubernetes-map-type": "atomic"
                                            }
                                        },
                                        "type": "object"
                                    },
                                    "type": "array",
                                    "x-kubernetes-list-type": "atomic"
                                }
                            },
                            "type": "object"
                        },
                        "type": "array",
                        "x-kubernetes-list-type": "atomic"
                    },
                    "extraIngress": {
                        "description": "ingress is a list of ingress rules to be applied to the selected pods. Traffic is allowed to a pod if there are no NetworkPolicies selecting the pod (and cluster policy otherwise allows the traffic), OR if the traffic source is the pod's local node, OR if the traffic matches at least one ingress rule across all of the NetworkPolicy objects whose podSelector matches the pod. If this field is empty then this NetworkPolicy does not allow any traffic (and serves solely to ensure that the pods it selects are isolated by default)",
                        "items": {
                            "description": "NetworkPolicyIngressRule describes a particular set of traffic that is allowed to the pods matched by a NetworkPolicySpec's podSelector. The traffic must match both ports and from.",
                            "properties": {
                                "from": {
                                    "description": "from is a list of sources which should be able to access the pods selected for this rule. Items in this list are combined using a logical OR operation. If this field is empty or missing, this rule matches all sources (traffic not restricted by source). If this field is present and contains at least one item, this rule allows traffic only if the traffic matches at least one item in the from list.",
                                    "items": {
                                        "description": "NetworkPolicyPeer describes a peer to allow traffic to/from. Only certain combinations of fields are allowed",
                                        "properties": {
                                            "ipBlock": {
                                                "description": "IPBlock describes a particular CIDR (Ex. \"192.168.1.0/24\",\"2001:db8::/64\") that is allowed to the pods matched by a NetworkPolicySpec's podSelector. The except entry describes CIDRs that should not be included within this rule.",
                                                "properties": {
                                                    "cidr": {
                                                        "description": "cidr is a string representing the IPBlock Valid examples are \"192.168.1.0/24\" or \"2001:db8::/64\"",
                                                        "type": "string"
                                                    },
                                                    "except": {
                                                        "description": "except is a slice of CIDRs that should not be included within an IPBlock Valid examples are \"192.168.1.0/24\" or \"2001:db8::/64\" Except values will be rejected if they are outside the cidr range",
                                                        "items": { "type": "string" },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    }
                                                },
                                                "required": ["cidr"],
                                                "type": "object"
                                            },
                                            "namespaceSelector": {
                                                "description": "A label selector is a label query over a set of resources. The result of matchLabels and matchExpressions are ANDed. An empty label selector matches all objects. A null label selector matches no objects.",
                                                "properties": {
                                                    "matchExpressions": {
                                                        "description": "matchExpressions is a list of label selector requirements. The requirements are ANDed.",
                                                        "items": {
                                                            "description": "A label selector requirement is a selector that contains values, a key, and an operator that relates the key and values.",
                                                            "properties": {
                                                                "key": {
                                                                    "description": "key is the label key that the selector applies to.",
                                                                    "type": "string"
                                                                },
                                                                "operator": {
                                                                    "description": "operator represents a key's relationship to a set of values. Valid operators are In, NotIn, Exists and DoesNotExist.",
                                                                    "type": "string"
                                                                },
                                                                "values": {
                                                                    "description": "values is an array of string values. If the operator is In or NotIn, the values array must be non-empty. If the operator is Exists or DoesNotExist, the values array must be empty. This array is replaced during a strategic merge patch.",
                                                                    "items": { "type": "string" },
                                                                    "type": "array",
                                                                    "x-kubernetes-list-type": "atomic"
                                                                }
                                                            },
                                                            "required": ["key", "operator"],
                                                            "type": "object"
                                                        },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    },
                                                    "matchLabels": {
                                                        "additionalProperties": { "type": "string" },
                                                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                                        "type": "object"
                                                    }
                                                },
                                                "type": "object",
                                                "x-kubernetes-map-type": "atomic"
                                            },
                                            "podSelector": {
                                                "description": "A label selector is a label query over a set of resources. The result of matchLabels and matchExpressions are ANDed. An empty label selector matches all objects. A null label selector matches no objects.",
                                                "properties": {
                                                    "matchExpressions": {
                                                        "description": "matchExpressions is a list of label selector requirements. The requirements are ANDed.",
                                                        "items": {
                                                            "description": "A label selector requirement is a selector that contains values, a key, and an operator that relates the key and values.",
                                                            "properties": {
                                                                "key": {
                                                                    "description": "key is the label key that the selector applies to.",
                                                                    "type": "string"
                                                                },
                                                                "operator": {
                                                                    "description": "operator represents a key's relationship to a set of values. Valid operators are In, NotIn, Exists and DoesNotExist.",
                                                                    "type": "string"
                                                                },
                                                                "values": {
                                                                    "description": "values is an array of string values. If the operator is In or NotIn, the values array must be non-empty. If the operator is Exists or DoesNotExist, the values array must be empty. This array is replaced during a strategic merge patch.",
                                                                    "items": { "type": "string" },
                                                                    "type": "array",
                                                                    "x-kubernetes-list-type": "atomic"
                                                                }
                                                            },
                                                            "required": ["key", "operator"],
                                                            "type": "object"
                                                        },
                                                        "type": "array",
                                                        "x-kubernetes-list-type": "atomic"
                                                    },
                                                    "matchLabels": {
                                                        "additionalProperties": { "type": "string" },
                                                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                                        "type": "object"
                                                    }
                                                },
                                                "type": "object",
                                                "x-kubernetes-map-type": "atomic"
                                            }
                                        },
                                        "type": "object"
                                    },
                                    "type": "array",
                                    "x-kubernetes-list-type": "atomic"
                                },
                                "ports": {
                                    "description": "ports is a list of ports which should be made accessible on the pods selected for this rule. Each item in this list is combined using a logical OR. If this field is empty or missing, this rule matches all ports (traffic not restricted by port). If this field is present and contains at least one item, then this rule allows traffic only if the traffic matches at least one port in the list.",
                                    "items": {
                                        "description": "NetworkPolicyPort describes a port to allow traffic on",
                                        "properties": {
                                            "endPort": {
                                                "description": "endPort indicates that the range of ports from port to endPort if set, inclusive, should be allowed by the policy. This field cannot be defined if the port field is not defined or if the port field is defined as a named (string) port. The endPort must be equal or greater than port.",
                                                "format": "int32",
                                                "type": "integer"
                                            },
                                            "port": {
                                                "oneOf": [
                                                    { "type": "string" },
                                                    { "type": "integer" }
                                                ]
                                            },
                                            "protocol": {
                                                "description": "protocol represents the protocol (TCP, UDP, or SCTP) which traffic must match. If not specified, this field defaults to TCP.",
                                                "type": "string"
                                            }
                                        },
                                        "type": "object"
                                    },
                                    "type": "array",
                                    "x-kubernetes-list-type": "atomic"
                                }
                            },
                            "type": "object"
                        },
                        "type": "array",
                        "x-kubernetes-list-type": "atomic"
                    },
                    "ingressNSMatchLabels": {
                        "additionalProperties": { "type": "string" },
                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                        "properties": {},
                        "type": "object"
                    },
                    "ingressNSPodMatchLabels": {
                        "additionalProperties": { "type": "string" },
                        "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                        "properties": {},
                        "type": "object"
                    },
                    "metrics": {
                        "additionalProperties": false,
                        "properties": {
                            "allowExternal": { "type": "boolean" },
                            "ingressNSMatchLabels": {
                                "additionalProperties": { "type": "string" },
                                "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                "properties": {},
                                "type": "object"
                            },
                            "ingressNSPodMatchLabels": {
                                "additionalProperties": { "type": "string" },
                                "description": "matchLabels is a map of {key,value} pairs. A single {key,value} in the matchLabels map is equivalent to an element of matchExpressions, whose key field is \"key\", the operator is \"In\", and the values array contains only \"value\". The requirements are ANDed.",
                                "properties": {},
                                "type": "object"
                            }
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "sentinel": {
                "additionalProperties": false,
                "properties": {
                    "containerPorts": {
                        "additionalProperties": false,
                        "properties": {
                            "sentinel": {
                                "oneOf": [
                                    { "type": "string" },
                                    { "type": "integer" }
                                ]
                            }
                        },
                        "type": "object"
                    },
                    "enabled": { "type": "boolean" }
                },
                "type": "object"
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn schema_validates_values_yaml() {
    let src = test_util::read_testdata("charts/bitnami-redis/templates/networkpolicy.yaml");
    let values_yaml = test_util::read_testdata("charts/bitnami-redis/values.yaml");
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let errors = common::validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}
