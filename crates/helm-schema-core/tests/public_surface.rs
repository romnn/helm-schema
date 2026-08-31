//! Public semantic-contract parsing and path utility regressions.

use helm_schema_core::{
    ApiPresenceQuery, ConditionalGuard, ContractUse, ContractValuePathFacts, Guard, ValueKind,
    ValuesPath, YamlPath, join_value_path, split_value_path,
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
