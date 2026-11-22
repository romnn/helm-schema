use std::collections::BTreeSet;
use std::sync::Arc;

use color_eyre::eyre;
use helm_schema_core::{ConditionalGuard, GuardConjunction, GuardPrefix, ValuesPath};
use serde_json::json;
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{AbsenceDefaults, ConditionFragmentCache, ConditionPolarity};
use crate::schema_node::SchemaNode;

fn not_truthy(path: &str) -> ConditionalGuard {
    ConditionalGuard::Not(Arc::new(ConditionalGuard::Truthy {
        path: ValuesPath::parse(path),
    }))
}

#[test]
fn shared_prefix_encoding_matches_flat_conditions_and_definition_order() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![
        not_truthy("service.middle"),
        ConditionalGuard::Absent {
            path: ValuesPath::parse("service.missing"),
        },
        ConditionalGuard::MatchesPattern {
            path: ValuesPath::parse("service.invalid"),
            pattern: "[".to_owned(),
        },
    ]);
    let rows = [
        GuardConjunction::prefixed(
            vec![not_truthy("service.before")],
            prefix.clone(),
            vec![not_truthy("service.z")],
        ),
        GuardConjunction::prefixed(
            vec![not_truthy("service.another")],
            prefix.clone(),
            vec![not_truthy("service.y")],
        ),
        GuardConjunction::prefixed(Vec::new(), prefix, Vec::new()),
    ]
    .map(GuardConjunction::canonicalized);
    let base_values = serde_yaml::from_str("service: {middle: {enabled: true}}")?;
    let other_values = serde_yaml::from_str("service: {middle: {}}")?;
    let empty = YamlValue::Null;
    let fallback = serde_yaml::from_str("service: {missing: true}")?;
    let root = Vec::new();
    let local = vec!["service".to_owned()];
    let dependencies = BTreeSet::from([local.clone()]);
    let no_dependencies = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &empty,
        dependency_roots: &dependencies,
    };
    let contexts = [
        (&root, &base_values, absence),
        (&root, &other_values, absence),
        (&local, &base_values, absence),
        (
            &root,
            &base_values,
            AbsenceDefaults {
                runtime_defaults: &fallback,
                ..absence
            },
        ),
        (
            &root,
            &base_values,
            AbsenceDefaults {
                dependency_refill: &fallback,
                ..absence
            },
        ),
        (
            &root,
            &base_values,
            AbsenceDefaults {
                dependency_roots: &no_dependencies,
                ..absence
            },
        ),
    ];
    for shared_references in [true, false] {
        let mut shared = ConditionFragmentCache::new();
        let mut flat = ConditionFragmentCache::new();
        if !shared_references {
            let schema = json!({ "$id": "urn:guard-condition-test" });
            shared.reserve(&schema);
            flat.reserve(&schema);
        }
        for polarity in [
            ConditionPolarity::Widen,
            ConditionPolarity::Narrow,
            ConditionPolarity::Exact,
        ] {
            for (ancestor, values, absence) in contexts {
                for row in &rows {
                    let actual = shared
                        .planned_conjunction(row, ancestor, values, absence, polarity)
                        .flat_schema();
                    let expected = flat.clauses(&row.to_vec(), ancestor, values, absence, polarity);
                    sim_assert_eq!(
                        have: actual.into_value(),
                        want: SchemaNode::all_of(expected).into_value(),
                    );
                    sim_assert_eq!(have: &shared.definitions, want: &flat.definitions);
                }
            }
        }
        sim_assert_eq!(have: shared.definitions.is_empty(), want: !shared_references);
    }
    Ok(())
}
