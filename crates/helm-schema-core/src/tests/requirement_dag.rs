use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::hash::{DefaultHasher, Hash, Hasher as _};
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::{RequirementCondition, RequirementConditionBuilder, RequirementConditionNode};
use crate::{ConditionalGuard, RequirementDecision, RequirementGuard, ValuesPath};

fn truthy(path: &str) -> ConditionalGuard {
    ConditionalGuard::Truthy {
        path: ValuesPath::parse(path),
    }
}

fn hash(value: &impl Hash) -> u64 {
    let mut state = DefaultHasher::new();
    value.hash(&mut state);
    state.finish()
}

fn diamond(reverse: bool) -> RequirementCondition {
    let mut builder = RequirementConditionBuilder::default();
    let shared = builder.guard(truthy("shared"));
    let left = |builder: &mut RequirementConditionBuilder| {
        builder.decision(truthy("left"), shared, RequirementConditionBuilder::FALSE)
    };
    let right = |builder: &mut RequirementConditionBuilder| {
        builder.decision(truthy("right"), shared, RequirementConditionBuilder::TRUE)
    };
    let (left, right) = if reverse {
        let right = right(&mut builder);
        builder.guard(truthy("unreachable"));
        (left(&mut builder), right)
    } else {
        (left(&mut builder), right(&mut builder))
    };
    let root = builder.decision(truthy("select"), left, right);
    builder.freeze().root(root, true)
}

#[test]
fn equivalent_arenas_export_the_same_reachable_graph() -> eyre::Result<()> {
    let left = diamond(false);
    let right = diamond(true);
    let expected = json!({
        "nodes": [
            {"guard": {"truthy": {"path": "shared"}}},
            {"decision": {
                "test": {"truthy": {"path": "left"}},
                "when_true": 2,
                "when_false": 0,
            }},
            {"decision": {
                "test": {"truthy": {"path": "right"}},
                "when_true": 2,
                "when_false": 1,
            }},
            {"decision": {
                "test": {"truthy": {"path": "select"}},
                "when_true": 3,
                "when_false": 4,
            }},
        ],
        "root": 5,
        "projection_exact": true,
    });
    sim_assert_eq!(have: &left, want: &right);
    sim_assert_eq!(have: left.cmp(&right), want: Ordering::Equal);
    sim_assert_eq!(have: hash(&left), want: hash(&right));
    sim_assert_eq!(have: left.predicate(), want: right.predicate());
    sim_assert_eq!(have: serde_json::to_value(&left)?, want: expected.clone());
    sim_assert_eq!(have: serde_json::to_value(&right)?, want: expected);
    sim_assert_eq!(have: left.reachable_ids().len(), want: 6);
    Ok(())
}

#[test]
fn commutative_children_have_intrinsic_order() -> eyre::Result<()> {
    let mut left = RequirementConditionBuilder::default();
    let zebra = left.guard(truthy("zebra"));
    let apple = left.guard(truthy("apple"));
    let left_root = left.and([zebra, apple]);
    sim_assert_eq!(have: left.and([apple, zebra, apple]), want: left_root);
    let left_root = left.freeze().root(left_root, true);

    let mut right = RequirementConditionBuilder::default();
    let apple = right.guard(truthy("apple"));
    let zebra = right.guard(truthy("zebra"));
    let right_root = right.and([apple, zebra]);
    let right_root = right.freeze().root(right_root, true);

    sim_assert_eq!(
        have: left_root.guards().cloned().collect::<Vec<_>>(),
        want: vec![truthy("apple"), truthy("zebra")],
    );
    sim_assert_eq!(have: &left_root, want: &right_root);
    sim_assert_eq!(
        have: serde_json::to_value(&left_root)?,
        want: serde_json::to_value(&right_root)?,
    );
    Ok(())
}

#[test]
fn shared_diamonds_keep_linear_graph_storage() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let mut shared = builder.guard(truthy("shared"));
    let levels = 40;
    for level in 0..levels {
        let left = builder.decision(
            truthy(&format!("left.{level}")),
            shared,
            RequirementConditionBuilder::FALSE,
        );
        let right = builder.decision(
            truthy(&format!("right.{level}")),
            shared,
            RequirementConditionBuilder::TRUE,
        );
        shared = builder.decision(truthy(&format!("select.{level}")), left, right);
    }
    let branch = builder.guard(truthy("branch"));
    let other = builder.and([branch, shared]);
    let arena = builder.freeze();
    let graph = arena.root(shared, true);
    let other = arena.root(other, false);
    let subroot = other.subroot(shared).ok_or_eyre("missing shared node")?;
    sim_assert_eq!(have: graph.arena_id(), want: other.arena_id());
    assert!(graph.projection_is_exact());
    assert!(!other.projection_is_exact());
    assert!(!subroot.projection_is_exact());
    sim_assert_eq!(have: subroot.arena_id(), want: graph.arena_id());
    sim_assert_eq!(have: graph.reachable_ids().len(), want: levels * 3 + 3);
    sim_assert_eq!(have: other.reachable_ids().len(), want: levels * 3 + 5);
    assert!(serde_json::to_string(&graph)?.len() < 20_000);
    assert!(graph.predicate().syntax_size() > levels * 3 + 3);
    Ok(())
}

#[test]
fn path_mapping_rebuilds_reachable_nodes_and_proofs() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let first = builder.guard(truthy("first"));
    let second = builder.guard(truthy("second"));
    let root = builder.or([first, second]);
    let graph = builder.freeze().root(root, false);
    let mut mapped_paths = Vec::new();
    let guard = ConditionalGuard::Requirement(graph).map_value_paths(&mut |path| {
        mapped_paths.push(path);
        ValuesPath::parse("mapped")
    });
    sim_assert_eq!(
        have: mapped_paths,
        want: vec![ValuesPath::parse("first"), ValuesPath::parse("second")],
    );
    sim_assert_eq!(have: guard.value_paths(), want: BTreeSet::from([ValuesPath::parse("mapped")]));
    assert!(guard.is_self_truthy_for(&ValuesPath::parse("mapped")));
    assert!(!guard.is_self_truthy_for(&ValuesPath::parse("first")));
    let ConditionalGuard::Requirement(graph) = guard else {
        eyre::bail!("the mapped condition must remain a requirement graph");
    };
    assert!(!graph.projection_is_exact());
    sim_assert_eq!(have: graph.reachable_ids().len(), want: 1);
    Ok(())
}

#[test]
fn bounded_decisions_remain_guard_units() {
    let bounded = ConditionalGuard::Decision(Arc::new(RequirementGuard {
        decisions: vec![RequirementDecision {
            test: truthy("required"),
            when_true: RequirementConditionBuilder::TRUE,
            when_false: RequirementConditionBuilder::FALSE,
        }],
        root: 2,
        projection_exact: true,
    }));
    let mut builder = RequirementConditionBuilder::default();
    let root = builder.guard(bounded.clone());
    let arena = builder.freeze();
    let graph = arena.root(root, true);
    sim_assert_eq!(have: graph.reachable_ids().len(), want: 1);
    sim_assert_eq!(have: graph.node(root), want: Some(&RequirementConditionNode::Guard(bounded)));
    let guard = ConditionalGuard::Requirement(graph);
    assert!(guard.is_self_truthy_for(&ValuesPath::parse("required")));
}

#[test]
fn bounded_decision_terminals_keep_their_boolean_meaning() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let falsity = builder.guard(ConditionalGuard::Decision(Arc::new(RequirementGuard {
        decisions: Vec::new(),
        root: RequirementConditionBuilder::FALSE,
        projection_exact: true,
    })));
    let truth = builder.guard(ConditionalGuard::Decision(Arc::new(RequirementGuard {
        decisions: Vec::new(),
        root: RequirementConditionBuilder::TRUE,
        projection_exact: true,
    })));
    let disjunction = builder.or([falsity, truth]);
    let conjunction = builder.and([falsity, truth]);
    sim_assert_eq!(have: disjunction, want: RequirementConditionBuilder::TRUE);
    sim_assert_eq!(have: conjunction, want: RequirementConditionBuilder::FALSE);
    let arena = builder.freeze();
    sim_assert_eq!(
        have: serde_json::to_value(arena.root(falsity, true))?,
        want: json!({"nodes": [], "root": 0, "projection_exact": true}),
    );
    sim_assert_eq!(
        have: serde_json::to_value(arena.root(truth, true))?,
        want: json!({"nodes": [], "root": 1, "projection_exact": true}),
    );
    Ok(())
}

#[test]
fn equal_split_successors_keep_both_execution_contexts() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let body = builder.guard(truthy("body"));
    let test = truthy("switch");
    let split = builder.split(test.clone(), body, body);
    let decision = builder.decision(test.clone(), body, body);
    let never = builder.split(
        test,
        RequirementConditionBuilder::FALSE,
        RequirementConditionBuilder::FALSE,
    );
    sim_assert_eq!(have: decision, want: body);
    sim_assert_eq!(have: never, want: RequirementConditionBuilder::FALSE);
    let arena = builder.freeze();
    let graph = arena.root(split, true);
    let mapped = graph.map_value_paths(&mut |path| {
        if path == ValuesPath::parse("switch") {
            ValuesPath::parse("mapped")
        } else {
            path
        }
    });
    let expected = json!({
        "nodes": [
            {"guard": {"truthy": {"path": "body"}}},
            {"split": {
                "test": {"truthy": {"path": "mapped"}},
                "when_true": 2,
                "when_false": 2,
            }},
        ],
        "root": 3,
        "projection_exact": true,
    });
    sim_assert_eq!(have: serde_json::to_value(&mapped)?, want: expected);
    assert!(ConditionalGuard::Requirement(mapped).is_self_truthy_for(&ValuesPath::parse("body")));
    Ok(())
}

#[test]
fn proof_union_preserves_arms_for_independent_policy_judgment() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let conditional = builder.guard(truthy("conditional"));
    let always = RequirementConditionBuilder::TRUE;
    let never = RequirementConditionBuilder::FALSE;
    let union = builder.union([conditional, always, conditional, never]);
    sim_assert_eq!(have: builder.or([conditional, always]), want: always);
    let graph = builder.freeze().root(union, true);
    let mapped = graph.map_value_paths(&mut |_| ValuesPath::parse("mapped"));
    sim_assert_eq!(
        have: serde_json::to_value(mapped)?,
        want: json!({
            "nodes": [
                {"guard": {"truthy": {"path": "mapped"}}},
                {"or": [1, 2]},
            ],
            "root": 3,
            "projection_exact": true,
        }),
    );
    Ok(())
}
