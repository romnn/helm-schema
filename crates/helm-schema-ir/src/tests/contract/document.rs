use color_eyre::eyre;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::ContractDocument;
use crate::{ContractUse, Guard, ResourceRef, ValueKind, YamlPath};

#[test]
fn contract_document_serializes_stable_guard_shape() -> eyre::Result<()> {
    let value_use = ContractUse::new(
        helm_schema_core::ValuesPath::parse("kid.enabled"),
        YamlPath(vec!["data".to_string(), "enabled".to_string()]),
        ValueKind::Scalar,
        vec![
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: helm_schema_core::ValuesPath::parse("kid.enabled"),
                    }],
                    vec![Guard::Eq {
                        path: helm_schema_core::ValuesPath::parse("kid.mode"),
                        value: crate::GuardValue::string("prod"),
                    }],
                ],
            },
            Guard::Or {
                paths: vec![
                    helm_schema_core::ValuesPath::parse("global.kidEnabled"),
                    helm_schema_core::ValuesPath::parse("kid.enabled"),
                    helm_schema_core::ValuesPath::parse("tags.observability"),
                ],
            },
        ],
        Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
    );
    let earlier_use = ContractUse::new(
        helm_schema_core::ValuesPath::parse("alpha.enabled"),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        Vec::new(),
        None,
    );
    let document = ContractDocument::from_contract_uses(vec![value_use, earlier_use]);

    let actual = serde_json::to_value(document.clone())?;

    sim_assert_eq!(
        have: actual,
        want: json!({
            "version": 3,
            "uses": [{
                "source_expr": "alpha.enabled",
                "path": [],
                "kind": "Scalar",
                "condition": [[]],
                "resource": null
            }, {
                "source_expr": "kid.enabled",
                "path": ["data", "enabled"],
                "kind": "Scalar",
                "condition": [[{
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
                }]],
                "resource": {
                    "api_version": "v1",
                    "kind": "ConfigMap"
                }
            }]
        })
    );

    let decoded: ContractDocument = serde_json::from_value(actual)?;
    sim_assert_eq!(have: decoded, want: document);
    Ok(())
}
