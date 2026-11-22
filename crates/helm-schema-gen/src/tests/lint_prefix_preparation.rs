use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre;
use helm_schema_core::{ConditionalGuard, GuardConjunction, GuardPrefix, ValuesPath};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{AbsenceDefaults, ConditionPolarity};
use crate::schema_node::SchemaNode;
use crate::uncoalesced_root::{GuardEncoding, UncoalescedRootGate, create_override_tables};

#[test]
fn shared_floor_preparation_matches_flat_lint_checks_and_definitions() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("kid.enabled"),
        },
        ConditionalGuard::Absent {
            path: ValuesPath::parse("kid.missing"),
        },
        ConditionalGuard::Truthy {
            path: ValuesPath::parse("kid.*.nested.enabled"),
        },
        ConditionalGuard::Absent {
            path: ValuesPath::parse(r"kid.\*.nested.missing"),
        },
    ]);
    let same_tables = GuardPrefix::new(prefix.as_slice().to_vec());
    let rows = [
        ("first", prefix.clone()),
        ("second", prefix),
        ("first", same_tables),
    ]
    .map(|(name, prefix)| {
        GuardConjunction::prefixed(
            vec![ConditionalGuard::Truthy {
                path: ValuesPath::parse(&format!("kid.{name}.enabled")),
            }],
            prefix,
            Vec::new(),
        )
        .canonicalized()
    });
    let coalesced = json!({ "kid": {
        "enabled": true,
        "first": { "enabled": true, "nested": { "enabled": true }, "branch": { "required": "ok" } },
        "second": { "enabled": true, "required": "ok", "nested": { "enabled": true } },
        "*": { "nested": {} },
    } });
    let values = serde_yaml::to_value(&coalesced)?;
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::from([vec!["kid".to_owned()]]);
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &values,
        dependency_roots: &roots,
    };
    let anchors = [Vec::new(), vec!["kid".to_owned()]];
    let mut observed_relaxation = false;
    for root in [
        json!({}),
        json!({ "kid": {} }),
        json!({ "kid": null }),
        json!({ "kid": false }),
        json!({ "kid": [] }),
        json!({ "kid": { "enabled": true } }),
        json!({ "kid": { "enabled": true, "first": { "nested": null }, "*": false } }),
    ] {
        for polarity in [ConditionPolarity::Widen, ConditionPolarity::Narrow] {
            let definitions = BTreeMap::new();
            let mut shared = UncoalescedRootGate::new(Some(&root), &coalesced, &definitions);
            let mut flat = UncoalescedRootGate::new(Some(&root), &coalesced, &definitions);
            let encoding = GuardEncoding {
                values_yaml_doc: &values,
                absence,
                polarity,
            };
            for ancestor in &anchors {
                for row in &rows {
                    assert_shared_floor_row_matches_flat(
                        &mut shared,
                        &mut flat,
                        ancestor,
                        row,
                        encoding,
                    );
                }
            }
            sim_assert_eq!(have: shared.take_condition_definitions(), want: flat.take_condition_definitions());
            let expected = flat.into_relaxed();
            observed_relaxation |= !expected.is_empty();
            sim_assert_eq!(have: shared.into_relaxed(), want: expected);
        }
    }
    assert!(observed_relaxation);
    Ok(())
}

fn assert_shared_floor_row_matches_flat(
    shared: &mut UncoalescedRootGate<'_>,
    flat: &mut UncoalescedRootGate<'_>,
    ancestor: &[String],
    row: &GuardConjunction,
    encoding: GuardEncoding<'_>,
) {
    let GuardEncoding {
        values_yaml_doc,
        absence,
        polarity,
    } = encoding;
    let guards = row.to_vec();
    let local_then = json!({ "properties": {
        "first": { "properties": { "branch": {
            "required": ["required"], "properties": { "required": { "type": "string" } },
        } } },
    } });
    let then_schema = if ancestor.is_empty() {
        json!({ "properties": { "kid": local_then } })
    } else {
        local_then
    };
    let shared_condition =
        shared.planned_conjunction(row, ancestor, values_yaml_doc, absence, polarity);
    let flat_condition = SchemaNode::all_of(flat.condition_clauses(
        &guards,
        ancestor,
        values_yaml_doc,
        absence,
        polarity,
    ));
    let actual = shared.lint_safe_planned_then(
        ancestor,
        row,
        &shared_condition,
        then_schema.clone(),
        encoding,
    );
    let expected = flat.lint_safe_then(
        ancestor,
        &GuardConjunction::from(guards),
        &flat_condition,
        then_schema,
        encoding,
    );
    sim_assert_eq!(have: actual, want: expected);
}

#[test]
fn shared_terminal_boundaries_match_flat_paths_after_extras_are_added() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![ConditionalGuard::HasKey {
        path: ValuesPath::parse("kid"),
        key: "leaf".to_owned(),
    }]);
    let rows = [
        None,
        Some("kid-aux.guard"),
        Some("kid.other"),
        Some("kid.*.enabled"),
        Some(r"kid.\*.enabled"),
    ]
    .map(|path| {
        let extras = path
            .into_iter()
            .map(|path| ConditionalGuard::Truthy {
                path: ValuesPath::parse(path),
            })
            .collect();
        GuardConjunction::prefixed(extras, prefix.clone(), Vec::new()).canonicalized()
    });
    let coalesced = json!({
        "kid": {"leaf": true, "other": true, "*": {"enabled": true}, "entry": {"enabled": true}},
        "kid-aux": {"guard": true},
    });
    let values = serde_yaml::to_value(&coalesced)?;
    let empty = serde_yaml::Value::Null;
    let roots = BTreeSet::from([vec!["kid".to_owned()], vec!["kid-aux".to_owned()]]);
    let same_roots = roots.clone();
    let other_roots = BTreeSet::from([vec!["kid-aux".to_owned()]]);
    for raw in [
        json!({}),
        json!({"kid": null}),
        json!({"kid": {"leaf": null, "*": {"enabled": null}}}),
    ] {
        let definitions = BTreeMap::new();
        let mut shared = UncoalescedRootGate::new(Some(&raw), &coalesced, &definitions);
        let mut flat = UncoalescedRootGate::new(Some(&raw), &coalesced, &definitions);
        for dependency_roots in [&roots, &other_roots, &same_roots] {
            let absence = AbsenceDefaults {
                runtime_defaults: &empty,
                dependency_refill: &values,
                dependency_roots,
            };
            let polarity = ConditionPolarity::Narrow;
            let encoding = GuardEncoding {
                values_yaml_doc: &values,
                absence,
                polarity,
            };
            for row in &rows {
                let condition = shared.planned_conjunction(row, &[], &values, absence, polarity);
                let actual =
                    shared.lint_safe_planned_then(&[], row, &condition, json!(false), encoding);
                let guards = row.to_vec();
                let condition = SchemaNode::all_of(flat.condition_clauses(
                    &guards,
                    &[],
                    &values,
                    absence,
                    polarity,
                ));
                let expected =
                    flat.lint_safe_then(&[], &guards.into(), &condition, json!(false), encoding);
                sim_assert_eq!(have: actual, want: expected);
            }
        }
        sim_assert_eq!(have: shared.take_condition_definitions(), want: flat.take_condition_definitions());
        let actual = shared.into_relaxed();
        sim_assert_eq!(have: &actual, want: &flat.into_relaxed());
        if raw == json!({}) {
            let expected = [
                vec!["kid.leaf"],
                vec!["kid", "kid-aux.guard", "kid.leaf"],
                vec!["kid.leaf", "kid.other"],
            ]
            .map(|paths| paths.into_iter().map(ValuesPath::parse).collect::<Vec<_>>());
            let actual = actual
                .iter()
                .take(3)
                .map(|fact| fact.deciding_paths.clone())
                .collect::<Vec<_>>();
            sim_assert_eq!(have: actual, want: expected.to_vec());
        }
    }
    Ok(())
}

#[test]
fn table_creation_commutes_and_preserves_existing_scalar_null_and_array_members() {
    let coalesced = json!({ "kid": {
        "first": { "nested": { "leaf": {} } },
        "second": { "nested": [1] },
        "*": { "nested": {} },
    } });
    let paths = [
        "",
        "kid",
        "kid.first",
        "kid.first.nested",
        "kid.first.nested.leaf",
        "kid.second",
        "kid.second.nested",
        "kid.*",
        "kid.*.nested.leaf",
        r"kid.\*.nested",
        "kid.missing",
    ]
    .map(ValuesPath::parse);
    for floor in [
        json!({}),
        json!({ "kid": {} }),
        json!({ "kid": null }),
        json!({ "kid": 7 }),
        json!({ "kid": [] }),
        json!({ "kid": { "first": { "nested": null }, "second": false, "*": [] } }),
    ] {
        for anchor in &paths {
            for first in &paths {
                for second in &paths {
                    let mut flat = Cow::Borrowed(&floor);
                    let mut reordered = Cow::Borrowed(&floor);
                    for path in [anchor, first, second] {
                        create_override_tables(
                            &mut flat,
                            &coalesced,
                            &path.segments().collect::<Vec<_>>(),
                        );
                    }
                    for path in [second, first, anchor, second] {
                        create_override_tables(
                            &mut reordered,
                            &coalesced,
                            &path.segments().collect::<Vec<_>>(),
                        );
                    }
                    sim_assert_eq!(have: reordered.as_ref(), want: flat.as_ref());
                }
            }
        }
    }
    let floor = json!({});
    let mut created = Cow::Borrowed(&floor);
    let path = ValuesPath::parse("kid.*.nested");
    create_override_tables(
        &mut created,
        &coalesced,
        &path.segments().collect::<Vec<_>>(),
    );
    sim_assert_eq!(have: created.as_ref(), want: &json!({"kid": {"first": {"nested": {}}, "second": {}, "*": {"nested": {}}}}));
}
