//! Public semantic-contract parsing and path utility regressions.

use helm_schema_core::{
    ApiPresenceQuery, ConditionalGuard, ConditionalOverlayEvidence, ConditionalOverlayFlavor,
    ConditionalPathOverlay, ContractRequirementImplication, ContractRequirementTarget, ContractUse,
    ContractValuePathFacts, FailValueRequirement, Guard, MergeLayer, MergeLayerTransform,
    MergeLayersUse, ValueKind, ValuesPath, YamlPath, join_value_path, split_value_path,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn api_presence_query_parses_resource_and_group_version_literals() {
    sim_assert_eq!(
        have: ApiPresenceQuery::parse_helm_literal("policy/v1/PodDisruptionBudget"),
        want: Some(ApiPresenceQuery::Resource {
            api_version: "policy/v1".to_string(),
            kind: "PodDisruptionBudget".to_string(),
        })
    );
    sim_assert_eq!(
        have: ApiPresenceQuery::parse_helm_literal("monitoring.coreos.com/v1"),
        want: Some(ApiPresenceQuery::GroupVersion {
            api_version: "monitoring.coreos.com/v1".to_string(),
        })
    );
}

#[test]
fn value_path_currency_preserves_literal_dots_and_backslashes() {
    let segments = ["grafana.ini", "paths", r"data\root"];
    let path = join_value_path(segments);

    sim_assert_eq!(have: &path, want: r"grafana\.ini.paths.data\\root");
    sim_assert_eq!(
        have: split_value_path(&path),
        want: segments.map(str::to_string).to_vec()
    );
}

#[test]
fn conditional_guards_own_self_scope_classification() {
    let target = ValuesPath::from_segments(["parent", "literal.key"]);
    let parent = ValuesPath::parse("parent");
    let truthy = ConditionalGuard::Truthy {
        path: target.clone(),
    };
    let with = ConditionalGuard::With {
        path: target.clone(),
    };
    let not_absent = ConditionalGuard::Not(Box::new(ConditionalGuard::Absent {
        path: target.clone(),
    }));
    let opaque_member_presence = ["literal.key", r"literal\key", "*"]
        .map(|key| {
            ConditionalGuard::HasKey {
                path: parent.clone(),
                key: key.to_string(),
            }
            .is_self_presence_for(&ValuesPath::from_segments(["parent", key]))
        })
        .to_vec();

    sim_assert_eq!(
        have: (
            truthy.is_self_truthy_for(&target),
            with.is_self_truthy_for(&target),
            not_absent.is_self_presence_for(&target),
            truthy.is_self_presence_for(&target),
        ),
        want: (true, true, true, false),
    );
    sim_assert_eq!(have: opaque_member_presence, want: vec![true; 3]);
}

#[test]
fn conditional_signal_constructors_canonicalize_guard_conjunctions() {
    let truthy = ConditionalGuard::Truthy {
        path: ValuesPath::parse("selected"),
    };
    let absent = ConditionalGuard::Absent {
        path: ValuesPath::parse("fallback"),
    };
    let nested = ConditionalGuard::AnyOf(vec![absent.clone(), truthy.clone(), absent.clone()]);
    let overlay = ConditionalPathOverlay::new(
        vec![nested.clone(), truthy.clone(), nested],
        ConditionalOverlayEvidence::default(),
        false,
        ConditionalOverlayFlavor::Ordinary,
    );
    let implication = ContractRequirementImplication::new(
        vec![absent.clone(), truthy.clone(), absent],
        ContractRequirementTarget::Value,
        vec![
            FailValueRequirement::SchemaType("string".to_string()),
            FailValueRequirement::SchemaType("string".to_string()),
        ],
    );

    sim_assert_eq!(
        have: overlay.guards,
        want: vec![
            truthy.clone(),
            ConditionalGuard::AnyOf(vec![truthy.clone(), ConditionalGuard::Absent {
                path: ValuesPath::parse("fallback"),
            }]),
        ],
    );
    sim_assert_eq!(
        have: implication,
        want: ContractRequirementImplication {
            outer_guards: vec![truthy, ConditionalGuard::Absent {
                path: ValuesPath::parse("fallback"),
            }],
            target: ContractRequirementTarget::Value,
            requirements: vec![FailValueRequirement::SchemaType("string".to_string())],
        },
    );
}

#[test]
fn merge_layers_validate_position_and_preserve_legacy_wire_shape() {
    let layers = vec![
        MergeLayer {
            path: ValuesPath::parse("preferred"),
            transform: MergeLayerTransform::ParsedMap,
        },
        MergeLayer {
            path: ValuesPath::parse("fallback"),
            transform: MergeLayerTransform::Identity,
        },
    ];
    let valid = MergeLayersUse::new(layers.clone(), 1, true);
    let wire = valid
        .as_ref()
        .and_then(|merge| serde_json::to_string(merge).ok());
    let round_trip = wire
        .as_deref()
        .and_then(|wire| serde_json::from_str::<MergeLayersUse>(wire).ok());

    sim_assert_eq!(
        have: wire,
        want: Some(
            r#"{"layers":["preferred","fallback"],"position":1,"transforms":["ParsedMap","Identity"],"via_binding":true}"#
                .to_string()
        )
    );
    sim_assert_eq!(have: round_trip, want: valid);
    sim_assert_eq!(
        have: MergeLayersUse::new(layers, 2, false).is_none(),
        want: true
    );
    sim_assert_eq!(
        have: serde_json::from_str::<MergeLayersUse>(
            r#"{"layers":["a"],"position":0,"transforms":[],"via_binding":false}"#
        )
        .is_err(),
        want: true
    );
    sim_assert_eq!(
        have: serde_json::from_str::<MergeLayersUse>(
            r#"{"layers":["a"],"position":1,"transforms":["Identity"],"via_binding":false}"#
        )
        .is_err(),
        want: true
    );
}

#[test]
fn empty_path_facts_use_the_universal_identity() {
    let facts = ContractValuePathFacts::default();

    sim_assert_eq!(
        have: (
            facts.has_render_use,
            facts.all_render_uses_self_guarded.holds(),
            facts.all_render_uses_falsy_tolerant.holds(),
        ),
        want: (false, true, true)
    );
}

#[test]
fn render_use_merges_preserve_universal_quantification() {
    let mut contribution = ContractValuePathFacts::default();
    contribution.record_render_use(false, Some(false), Some(false));
    let mut merged = ContractValuePathFacts::default();
    merged.merge_render_use_facts(contribution);

    sim_assert_eq!(
        have: (
            merged.has_render_use,
            merged.all_render_uses_self_guarded.holds(),
            merged.all_render_uses_falsy_tolerant.holds(),
        ),
        want: (true, false, false)
    );
}

#[test]
fn contract_use_derived_deserialize_preserves_legacy_defaults() {
    let have = serde_json::from_value::<ContractUse>(serde_json::json!({
        "condition": [[{ "path": "enabled", "type": "truthy" }]],
        "kind": "Scalar",
        "path": [],
        "resource": null,
        "source_expr": "name",
    }))
    .ok();
    let want = ContractUse::new(
        helm_schema_core::ValuesPath::parse("name"),
        YamlPath::default(),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("enabled"),
        }],
        None,
    );

    sim_assert_eq!(have: have, want: Some(want));
}
