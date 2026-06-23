use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::{ContractDocument, ContractDocumentUse};
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

#[test]
fn contract_document_serializes_stable_guard_shape() {
    let value_use = ContractDocumentUse {
        source_expr: "kid.enabled".to_string(),
        path: YamlPath(vec!["data".to_string(), "enabled".to_string()]),
        kind: ValueKind::Scalar,
        guards: vec![
            Guard::Or {
                paths: vec![
                    "global.kidEnabled".to_string(),
                    "kid.enabled".to_string(),
                    "tags.observability".to_string(),
                ],
            },
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: "kid.enabled".to_string(),
                    }],
                    vec![Guard::Eq {
                        path: "kid.mode".to_string(),
                        value: crate::GuardValue::string("prod"),
                    }],
                ],
            },
        ],
        resource: Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
        provenance: Vec::new(),
    };

    let actual = serde_json::to_value(ContractDocument {
        version: ContractDocument::VERSION,
        uses: vec![value_use.clone()],
    })
    .expect("serialize contract document");

    sim_assert_eq!(
        have: actual,
        want: json!({
            "version": 2,
            "uses": [{
                "source_expr": "kid.enabled",
                "path": ["data", "enabled"],
                "kind": "Scalar",
                "guards": [{
                    "type": "or",
                    "paths": ["global.kidEnabled", "kid.enabled", "tags.observability"]
                }, {
                    "type": "any_of",
                    "alternatives": [[{
                        "type": "truthy",
                        "path": "kid.enabled"
                    }], [{
                        "type": "eq",
                        "path": "kid.mode",
                        "value": "prod"
                    }]]
                }],
                "resource": {
                    "api_version": "v1",
                    "kind": "ConfigMap"
                }
            }]
        })
    );

    let decoded: ContractDocument =
        serde_json::from_value(actual).expect("deserialize contract document");
    sim_assert_eq!(have: decoded.uses, want: vec![value_use]);
}
