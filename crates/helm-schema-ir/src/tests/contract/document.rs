use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::ContractDocument;
use crate::{ContractUse, Guard, ResourceRef, ValueKind, YamlPath};

#[test]
fn contract_document_serializes_stable_guard_shape() {
    let value_use = ContractUse {
        source_expr: "kid.enabled".to_string(),
        path: YamlPath(vec!["data".to_string(), "enabled".to_string()]),
        kind: ValueKind::Scalar,
        guards: vec![
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Eq {
                        path: "kid.mode".to_string(),
                        value: crate::GuardValue::string("prod"),
                    }],
                    vec![Guard::Truthy {
                        path: "kid.enabled".to_string(),
                    }],
                ],
            },
            Guard::Or {
                paths: vec![
                    "tags.observability".to_string(),
                    "kid.enabled".to_string(),
                    "global.kidEnabled".to_string(),
                ],
            },
        ],
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
        provenance: Vec::new(),
    };
    let earlier_use = ContractUse {
        source_expr: "alpha.enabled".to_string(),
        path: YamlPath(Vec::new()),
        kind: ValueKind::Scalar,
        guards: Vec::new(),
        resource: None,
        provenance: Vec::new(),
    };
    let document = ContractDocument::from_contract_uses(vec![value_use, earlier_use]);

    let actual = serde_json::to_value(document.clone()).expect("serialize contract document");

    sim_assert_eq!(
        have: actual,
        want: json!({
            "version": 2,
            "uses": [{
                "source_expr": "alpha.enabled",
                "path": [],
                "kind": "Scalar",
                "guards": [],
                "resource": null
            }, {
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
    sim_assert_eq!(have: decoded, want: document);
}
