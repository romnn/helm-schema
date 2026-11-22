use std::collections::BTreeSet;
use std::sync::Arc;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    ConditionalGuard, Predicate, PredicateMemo, PredicateResiduals, RequirementConditionBuilder,
    ValuesPath,
};
use test_util::prelude::sim_assert_eq;

use super::{ActivatedRequirement, GuardedActivation, partition_activation};
use crate::condition_encoding::evaluate_guard_set_on_values;

fn truthy(path: &str) -> ConditionalGuard {
    ConditionalGuard::Truthy {
        path: ValuesPath::parse(path),
    }
}

fn activation(predicate: Predicate) -> GuardedActivation {
    GuardedActivation {
        predicate,
        guard_count: 1,
    }
}

#[test]
fn parity_activation_keeps_two_residuals_per_split() -> eyre::Result<()> {
    let width = 18;
    let paths = (0..width)
        .map(|index| format!("flag{index}"))
        .collect::<Vec<_>>();
    let mut builder = RequirementConditionBuilder::default();
    let mut root = RequirementConditionBuilder::TRUE;
    let mut parity = Predicate::False;
    for path in &paths {
        let test = truthy(path);
        let predicate = test.predicate();
        root = builder.split(test, root, root);
        parity = Predicate::Or(vec![
            Predicate::And(vec![parity.clone(), predicate.clone().negated()]),
            Predicate::And(vec![parity.negated(), predicate]),
        ]);
    }
    let graph = builder.freeze().root(root, true);
    let partitioned = partition_activation(&graph, &Predicate::True, &[activation(parity)])
        .ok_or_eyre("activation partition")?;
    sim_assert_eq!(have: partitioned.len(), want: 2);
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitioned
    {
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        eyre::ensure!(
            condition.reachable_ids().len() <= 2 * width + 2,
            "parity lost its shared residuals"
        );
        for enabled in [0, 1, 2, 7, width] {
            let values = serde_yaml::to_value(
                paths
                    .iter()
                    .enumerate()
                    .map(|(index, path)| (path, index < enabled))
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )?;
            let guard = ConditionalGuard::Requirement(condition.clone());
            sim_assert_eq!(have: evaluate_guard_set_on_values([&guard], &values), want: Some(active.contains(&0) == (enabled % 2 == 1)));
        }
    }
    Ok(())
}

#[test]
fn activation_retains_a_test_later_read_by_an_opaque_proof() -> eyre::Result<()> {
    let split = truthy("split");
    let active = truthy("active");
    let mut builder = RequirementConditionBuilder::default();
    let proof = builder.guard(ConditionalGuard::AnyOf(Arc::new(vec![
        ConditionalGuard::Not(Arc::new(split.clone())),
        active.clone(),
    ])));
    let root = builder.split(split.clone(), proof, proof);
    let graph = builder.freeze().root(root, true);
    let partitions =
        partition_activation(&graph, &Predicate::True, &[activation(active.predicate())])
            .ok_or_eyre("activation partition")?;
    sim_assert_eq!(have: partitions.len(), want: 2);
    let memo = PredicateMemo::new();
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitions
    {
        let proof = condition.predicate();
        let split_state = if active.contains(&0) {
            split.predicate()
        } else {
            split.predicate().negated()
        };
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        sim_assert_eq!(have: memo.exactly_implies(&proof, &split_state), want: true);
    }
    Ok(())
}

#[test]
fn ranked_activations_keep_only_the_highest_proved_document() -> eyre::Result<()> {
    let width = 18;
    let paths = (0..width)
        .map(|index| format!("flag{index}"))
        .collect::<Vec<_>>();
    let mut builder = RequirementConditionBuilder::default();
    let mut root = RequirementConditionBuilder::TRUE;
    for path in paths.iter().rev() {
        root = builder.split(truthy(path), root, root);
    }
    let made = builder.freeze().root(root, true);
    let activations = paths
        .iter()
        .map(|path| activation(truthy(path).predicate()))
        .collect::<Vec<_>>();
    let partitions = partition_activation(&made, &Predicate::True, &activations)
        .ok_or_eyre("ranked activations")?;
    sim_assert_eq!(have: partitions.len(), want: width + 1);
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitions
    {
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        eyre::ensure!(
            active.len() <= 1,
            "lower-ranked histories survived selection"
        );
        let guard = ConditionalGuard::Requirement(condition);
        for enabled in [0, 1, 2, 5, width] {
            let values = serde_yaml::to_value(
                paths
                    .iter()
                    .enumerate()
                    .map(|(index, path)| (path, index < enabled))
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )?;
            let expected = enabled
                .checked_sub(1)
                .map_or_else(BTreeSet::new, |index| BTreeSet::from([index]));
            sim_assert_eq!(have: evaluate_guard_set_on_values([&guard], &values), want: Some(active == expected));
        }
    }
    Ok(())
}

#[test]
fn directly_proved_activation_survives_residual_solver_exhaustion() -> eyre::Result<()> {
    let guard = ConditionalGuard::AllOf(Arc::new(
        (0..160)
            .map(|index| truthy(&format!("flag{index:03}")))
            .collect(),
    ));
    let activation = activation(guard.predicate());
    let mut solver = PredicateResiduals::new([&activation.predicate]);
    eyre::ensure!(
        solver.import(&activation.predicate).is_none(),
        "expected the ascending conjunction to exhaust BDD work"
    );
    let mut builder = RequirementConditionBuilder::default();
    let root = builder.guard(guard);
    let graph = builder.freeze().root(root, true);
    let selections = partition_activation(&graph, &Predicate::True, &[activation])
        .ok_or_eyre("direct activation partition")?;
    let [selected] = selections.as_slice() else {
        eyre::bail!(
            "a direct activation has one document, got {}",
            selections.len()
        );
    };
    sim_assert_eq!(have: &selected.active, want: &BTreeSet::from([0]));
    sim_assert_eq!(have: &selected.unknown, want: &BTreeSet::new());
    Ok(())
}
fn assert_two_activation_regions(
    graph: &helm_schema_core::RequirementCondition,
    inactive: &str,
) -> eyre::Result<()> {
    let partitions = partition_activation(
        graph,
        &Predicate::True,
        &[activation(truthy("x00000").predicate())],
    )
    .ok_or_eyre("large activation partition")?;
    sim_assert_eq!(have: partitions.len(), want: 2);
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitions
    {
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        let guard = ConditionalGuard::Requirement(condition);
        let active_values =
            serde_yaml::to_value(std::collections::BTreeMap::from([("x00000", true)]))?;
        let inactive_values =
            serde_yaml::to_value(std::collections::BTreeMap::from([(inactive, true)]))?;
        sim_assert_eq!(have: evaluate_guard_set_on_values([&guard], &active_values), want: Some(active.contains(&0)));
        sim_assert_eq!(have: evaluate_guard_set_on_values([&guard], &inactive_values), want: Some(active.is_empty()));
    }
    Ok(())
}

#[test]
fn shallow_overlapping_unions_share_live_atoms() -> eyre::Result<()> {
    let width = 24_000;
    let mut builder = RequirementConditionBuilder::default();
    let shared_guards = (0..width)
        .map(|index| builder.guard(truthy(&format!("x{index:05}"))))
        .collect::<Vec<_>>();
    let shared = builder.union(shared_guards);
    let alternatives = (0..width)
        .map(|index| {
            let extra = builder.guard(truthy(&format!("y{index:05}")));
            builder.union([shared, extra])
        })
        .collect::<Vec<_>>();
    let root = builder.union(alternatives);
    assert_two_activation_regions(&builder.freeze().root(root, true), "y00000")
}

#[test]
fn deep_activation_union_uses_a_heap_work_stack() -> eyre::Result<()> {
    let depth = 20_000;
    let mut builder = RequirementConditionBuilder::default();
    let mut root = builder.guard(truthy("x00000"));
    for index in 1..=depth {
        let next = builder.guard(truthy(&format!("x{index:05}")));
        root = builder.union([root, next]);
    }
    assert_two_activation_regions(&builder.freeze().root(root, true), "x20000")
}

#[test]
fn conjunction_keeps_atoms_live_across_pending_siblings() -> eyre::Result<()> {
    let split = truthy("split");
    let source = truthy("source");
    let mut builder = RequirementConditionBuilder::default();
    let proof = builder.guard(ConditionalGuard::AnyOf(Arc::new(vec![
        ConditionalGuard::Not(Arc::new(split.clone())),
        source.clone(),
    ])));
    let decision = builder.split(
        split.clone(),
        RequirementConditionBuilder::TRUE,
        RequirementConditionBuilder::TRUE,
    );
    let root = builder.and([proof, decision]);
    let graph = builder.freeze().root(root, true);
    let partitions =
        partition_activation(&graph, &Predicate::True, &[activation(source.predicate())])
            .ok_or_eyre("conjunctive activation partition")?;
    sim_assert_eq!(have: partitions.len(), want: 2);
    let memo = PredicateMemo::new();
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitions
    {
        let test = if active.contains(&0) {
            split.predicate()
        } else {
            split.predicate().negated()
        };
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        sim_assert_eq!(have: memo.exactly_implies(&condition.predicate(), &test), want: true);
    }
    Ok(())
}

pub(super) const DENSE_GROUPS: usize = 12;

pub(super) fn dense_atom_groups() -> eyre::Result<Vec<Vec<usize>>> {
    let atom_count = 1 << DENSE_GROUPS;
    let mut groups = vec![Vec::new(); DENSE_GROUPS];
    for ordinal in 0..atom_count {
        let block = ordinal / 128;
        let slot = ordinal % 128;
        let group = if slot < 64 {
            slot % DENSE_GROUPS
        } else if slot < 64 + DENSE_GROUPS - 7 {
            (block >> (slot - 64)) & 1
        } else {
            1
        };
        groups
            .get_mut(group)
            .ok_or_eyre("dense atom group")?
            .push(ordinal);
    }
    Ok(groups)
}

#[test]
fn dense_subset_unions_preserve_both_activation_regions() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let mut groups = Vec::new();
    for group in dense_atom_groups()? {
        let guards = group
            .into_iter()
            .map(|ordinal| builder.guard(truthy(&format!("x{ordinal:05}"))))
            .collect::<Vec<_>>();
        groups.push(builder.union(guards));
    }
    let mut subsets = vec![RequirementConditionBuilder::FALSE];
    for mask in 1..(1_usize << DENSE_GROUPS) {
        let smallest = usize::try_from(mask.trailing_zeros())?;
        let rest = mask & (mask - 1);
        let group = *groups.get(smallest).ok_or_eyre("proof group")?;
        let predecessor = *subsets.get(rest).ok_or_eyre("proof subset")?;
        subsets.push(builder.union([predecessor, group]));
    }
    let root = builder.union(subsets);
    assert_two_activation_regions(&builder.freeze().root(root, true), "x00001")
}

#[derive(Clone)]
struct PartitionVisits(Arc<std::sync::atomic::AtomicU64>);

impl tracing::field::Visit for PartitionVisits {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "visits" {
            self.0.store(value, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for PartitionVisits {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _: &tracing::span::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() == "activation_partition_storage" {
            attributes.record(&mut self.clone());
        }
    }
}

#[test]
fn equivalent_suffix_frontiers_share_activation_requests() -> eyre::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use tracing_subscriber::prelude::*;

    let mut builder = RequirementConditionBuilder::default();
    let enabled = builder.guard(truthy("x00000"));
    let fallback = builder.guard(truthy("fallback"));
    let mut root = builder.union([enabled, fallback]);
    let mut suffix_values = std::collections::BTreeMap::new();
    for index in 0..18 {
        let test_a = truthy(&format!("a{index}"));
        let test_c = truthy(&format!("c{index}"));
        let path_b = format!("b{index}");
        let a = builder.guard(test_a.clone());
        let b = builder.guard(truthy(&path_b));
        let c = builder.guard(test_c.clone());
        suffix_values.insert(path_b, true);
        let either_ab = builder.union([a, b]);
        let either_bc = builder.union([b, c]);
        let suffix_a = builder.split(test_a, either_bc, either_bc);
        let suffix_c = builder.split(test_c, either_ab, either_ab);
        let proof_a = builder.and([root, suffix_a]);
        let proof_c = builder.and([root, suffix_c]);
        root = builder.union([proof_a, proof_c]);
    }
    let graph = builder.freeze().root(root, true);
    let visits = Arc::new(AtomicU64::new(0));
    let subscriber = tracing_subscriber::registry().with(PartitionVisits(Arc::clone(&visits)));
    let partitions = tracing::subscriber::with_default(subscriber, || {
        partition_activation(
            &graph,
            &Predicate::True,
            &[activation(truthy("x00000").predicate())],
        )
    })
    .ok_or_eyre("equivalent suffix partition")?;
    let count = visits.load(Ordering::Relaxed);
    let bound = u64::try_from(4 * graph.reachable_ids().len())?;
    eyre::ensure!(
        (1..=bound).contains(&count),
        "expected at most {bound} visits, got {count}"
    );
    sim_assert_eq!(have: partitions.len(), want: 2);
    for ActivatedRequirement {
        condition,
        active,
        unknown,
    } in partitions
    {
        sim_assert_eq!(have: unknown, want: BTreeSet::new());
        let guard = ConditionalGuard::Requirement(condition);
        for (path, selects) in [("x00000", true), ("fallback", false)] {
            let mut values = suffix_values.clone();
            values.insert(path.to_owned(), true);
            let values = serde_yaml::to_value(values)?;
            sim_assert_eq!(
                have: evaluate_guard_set_on_values([&guard], &values),
                want: Some(active.contains(&0) == selects),
            );
        }
    }
    Ok(())
}
