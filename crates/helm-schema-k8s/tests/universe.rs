//! Pruning contract that chart-local CRD extraction stamps onto resource schemas.
//!
//! The Kubernetes API server prunes every object field a structural schema
//! does not declare. `x-kubernetes-preserve-unknown-fields` keeps the unknown
//! fields of its own node (and of the items of a preserving array), but the
//! declared properties below it are pruned by their own schemas again. The
//! resource root and `x-kubernetes-embedded-resource` nodes never prune
//! `apiVersion`, `kind`, or `metadata`.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_k8s::resource_schemas_from_crd_document_with_source;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

/// Returns the resource schema extracted from a `v1` CRD serving `schema`.
fn pruned_resource_schema(schema: &Value) -> eyre::Result<Value> {
    let document = json!({
        "apiVersion": "apiextensions.k8s.io/v1",
        "kind": "CustomResourceDefinition",
        "spec": {
            "group": "example.com",
            "names": {"kind": "Widget"},
            "versions": [
                {"name": "v1", "served": true, "schema": {"openAPIV3Schema": schema}}
            ]
        }
    });
    let mut resource_schemas =
        resource_schemas_from_crd_document_with_source(&document, "chart-local", String::new());
    sim_assert_eq!(have: resource_schemas.len(), want: 1);
    let resource_schema = resource_schemas
        .pop()
        .ok_or_eyre("the CRD serves one version")?;
    Ok(resource_schema.schema)
}

#[test]
fn declared_descendants_under_a_preserved_parent_are_pruned() -> eyre::Result<()> {
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "spec": {
                "type": "object",
                "properties": {
                    "config": {
                        "type": "object",
                        "x-kubernetes-preserve-unknown-fields": true,
                        "properties": {
                            "child": {
                                "type": "object",
                                "properties": {"known": {"type": "string"}}
                            }
                        }
                    }
                }
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "spec": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "config": {
                        "type": "object",
                        "x-kubernetes-preserve-unknown-fields": true,
                        "properties": {
                            "child": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {"known": {"type": "string"}}
                            }
                        }
                    }
                }
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn preserved_node_without_properties_stays_open() -> eyre::Result<()> {
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "spec": {
                "type": "object",
                "properties": {
                    "config": {"type": "object", "x-kubernetes-preserve-unknown-fields": true}
                }
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "spec": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "config": {"type": "object", "x-kubernetes-preserve-unknown-fields": true}
                }
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn nested_preserved_nodes_stay_open_and_their_declared_objects_close() -> eyre::Result<()> {
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "outer": {
                "type": "object",
                "x-kubernetes-preserve-unknown-fields": true,
                "properties": {
                    "inner": {
                        "type": "object",
                        "x-kubernetes-preserve-unknown-fields": true,
                        "properties": {
                            "leaf": {
                                "type": "object",
                                "properties": {"x": {"type": "string"}}
                            }
                        }
                    },
                    "sibling": {
                        "type": "object",
                        "properties": {"y": {"type": "integer"}}
                    }
                }
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "outer": {
                "type": "object",
                "x-kubernetes-preserve-unknown-fields": true,
                "properties": {
                    "inner": {
                        "type": "object",
                        "x-kubernetes-preserve-unknown-fields": true,
                        "properties": {
                            "leaf": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {"x": {"type": "string"}}
                            }
                        }
                    },
                    "sibling": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"y": {"type": "integer"}}
                    }
                }
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn items_of_a_preserved_array_keep_unknown_fields_but_declared_objects_close() -> eyre::Result<()> {
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "rules": {
                "type": "array",
                "x-kubernetes-preserve-unknown-fields": true,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "match": {
                            "type": "object",
                            "properties": {"path": {"type": "string"}}
                        }
                    }
                }
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "rules": {
                "type": "array",
                "x-kubernetes-preserve-unknown-fields": true,
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "match": {
                            "type": "object",
                            "additionalProperties": false,
                            "properties": {"path": {"type": "string"}}
                        }
                    }
                }
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn embedded_resources_keep_type_and_object_meta_open_but_close_declared_objects() -> eyre::Result<()>
{
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "template": {
                "type": "object",
                "x-kubernetes-embedded-resource": true,
                "properties": {
                    "apiVersion": {"type": "string"},
                    "kind": {"type": "string"},
                    "metadata": {
                        "type": "object",
                        "properties": {"name": {"type": "string"}}
                    },
                    "spec": {
                        "type": "object",
                        "properties": {"replicas": {"type": "integer"}}
                    }
                }
            },
            "preservedTemplate": {
                "type": "object",
                "x-kubernetes-embedded-resource": true,
                "x-kubernetes-preserve-unknown-fields": true,
                "properties": {
                    "metadata": {
                        "type": "object",
                        "properties": {"name": {"type": "string"}}
                    },
                    "spec": {
                        "type": "object",
                        "properties": {"replicas": {"type": "integer"}}
                    }
                }
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "template": {
                "type": "object",
                "x-kubernetes-embedded-resource": true,
                "properties": {
                    "apiVersion": {"type": "string"},
                    "kind": {"type": "string"},
                    "metadata": {
                        "type": "object",
                        "properties": {"name": {"type": "string"}}
                    },
                    "spec": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"replicas": {"type": "integer"}}
                    }
                }
            },
            "preservedTemplate": {
                "type": "object",
                "x-kubernetes-embedded-resource": true,
                "x-kubernetes-preserve-unknown-fields": true,
                "properties": {
                    "metadata": {
                        "type": "object",
                        "properties": {"name": {"type": "string"}}
                    },
                    "spec": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {"replicas": {"type": "integer"}}
                    }
                }
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

#[test]
fn resource_root_keeps_type_and_object_meta_open() -> eyre::Result<()> {
    let have = pruned_resource_schema(&json!({
        "type": "object",
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {
                "type": "object",
                "properties": {"name": {"type": "string"}}
            },
            "spec": {
                "type": "object",
                "properties": {"size": {"type": "integer"}}
            }
        }
    }))?;

    let want = json!({
        "type": "object",
        "properties": {
            "apiVersion": {"type": "string"},
            "kind": {"type": "string"},
            "metadata": {
                "type": "object",
                "properties": {"name": {"type": "string"}}
            },
            "spec": {
                "type": "object",
                "additionalProperties": false,
                "properties": {"size": {"type": "integer"}}
            }
        }
    });
    sim_assert_eq!(have: have, want: want);
    Ok(())
}
