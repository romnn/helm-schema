use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::{
    ConditionalGuard, GuardConjunction, GuardPrefix, RequirementCondition,
    RequirementConditionBuilder, RequirementConditionNode, RequirementDecision, Segment,
    ValuesPath,
};
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::RootPolicy;
use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, deleted_dependency_root_terminates,
    helm_truthy_definition_schema,
};
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::terminal_conditions::{
    ProofGuardRole, RequirementPartition, TerminalEmissions, TerminalPathCache,
    partition_requirement,
};
use crate::uncoalesced_root::UncoalescedRootGate;

fn truthy(path: &str) -> ConditionalGuard {
    ConditionalGuard::Truthy {
        path: ValuesPath::parse(path),
    }
}

fn collapsing_requirement(builder: &mut RequirementConditionBuilder) -> usize {
    let a = builder.guard(truthy("a"));
    let b = builder.guard(truthy("b"));
    let either = builder.union([a, b]);
    let both_contexts = builder.guard(truthy("both"));
    let neutral = builder.guard(truthy("neutral"));
    builder.and([either, both_contexts, neutral])
}

fn judgment(guard: &ConditionalGuard, role: ProofGuardRole) -> u8 {
    match (role, guard) {
        (ProofGuardRole::SplitTest, ConditionalGuard::Not(_)) => 0b1000,
        (ProofGuardRole::SplitTest, _) => 0b0100,
        (ProofGuardRole::Leaf, ConditionalGuard::Truthy { path }) => {
            match path.segments().next().and_then(Segment::literal) {
                Some("a") => 0b0001,
                Some("b") => 0b0010,
                Some("both") => 0b0011,
                _ => 0,
            }
        }
        (ProofGuardRole::Leaf, _) => 0,
    }
}

#[derive(Clone, Default)]
struct PartitionWork {
    recipes: Arc<AtomicU64>,
    max_node_contexts: Arc<AtomicU64>,
    materialized_recipes: Arc<AtomicU64>,
}

impl tracing::field::Visit for PartitionWork {
    fn record_debug(&mut self, _: &tracing::field::Field, _: &dyn std::fmt::Debug) {}

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "recipes" => self.recipes.store(value, Ordering::Relaxed),
            "max_node_contexts" => self.max_node_contexts.store(value, Ordering::Relaxed),
            "materialized_recipes" => self.materialized_recipes.store(value, Ordering::Relaxed),
            _ => {}
        }
    }
}

impl<S> tracing_subscriber::Layer<S> for PartitionWork
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _: &tracing::span::Id,
        _: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if attributes.metadata().name() == "partition_requirement" {
            attributes.record(&mut self.clone());
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if context
            .span(id)
            .is_some_and(|span| span.name() == "partition_requirement")
        {
            values.record(&mut self.clone());
        }
    }
}

#[test]
fn single_context_requirement_partitions_reuse_the_original_graph() -> eyre::Result<()> {
    use tracing_subscriber::prelude::*;

    let mut builder = RequirementConditionBuilder::default();
    let mut root = collapsing_requirement(&mut builder);
    for depth in 0..128 {
        let left = builder.guard(truthy(&format!("left-{depth}")));
        let right = builder.guard(truthy(&format!("right-{depth}")));
        let left = builder.and([left, root]);
        let right = builder.and([right, root]);
        root = builder.union([left, right]);
    }
    let graph = builder.freeze().root(root, true);
    let work = PartitionWork::default();
    let subscriber = tracing_subscriber::registry().with(work.clone());
    let partitions = tracing::subscriber::with_default(subscriber, || {
        partition_requirement(&graph, judgment, |left, right| left | right)
    })
    .ok_or_eyre("single context partition")?;
    let [RequirementPartition { context, condition }] = partitions.as_slice() else {
        eyre::bail!("expected one context after the branches join");
    };
    sim_assert_eq!(have: *context, want: 0b0011);
    sim_assert_eq!(have: condition.arena_id(), want: graph.arena_id());
    sim_assert_eq!(have: condition.root(), want: graph.root());
    assert!(condition.projection_is_exact());
    // The disjunction has two judgments that its enclosing conjunction merges.
    sim_assert_eq!(have: work.max_node_contexts.load(Ordering::Relaxed), want: 2);
    assert!(work.recipes.load(Ordering::Relaxed) >= 640);
    sim_assert_eq!(have: work.materialized_recipes.load(Ordering::Relaxed), want: 0);
    Ok(())
}

type EagerContexts = BTreeMap<u8, usize>;

fn eager_union_contexts(
    alternatives: BTreeMap<u8, Vec<usize>>,
    builder: &mut RequirementConditionBuilder,
) -> EagerContexts {
    alternatives
        .into_iter()
        .map(|(context, roots)| (context, builder.union(roots)))
        .collect()
}

fn eager_partition(
    graph: &RequirementCondition,
    observe: impl Fn(&ConditionalGuard, ProofGuardRole) -> u8,
) -> Option<Vec<RequirementPartition<u8>>> {
    let mut builder = RequirementConditionBuilder::default();
    let mut partitions = HashMap::<usize, EagerContexts>::new();
    for id in graph.reachable_ids() {
        let contexts = match graph.node(id)? {
            RequirementConditionNode::False => BTreeMap::new(),
            RequirementConditionNode::True => {
                BTreeMap::from([(0, RequirementConditionBuilder::TRUE)])
            }
            RequirementConditionNode::Guard(guard) => BTreeMap::from([(
                observe(guard, ProofGuardRole::Leaf),
                builder.guard(guard.clone()),
            )]),
            RequirementConditionNode::And(children) => {
                let mut joint = BTreeMap::from([(0, RequirementConditionBuilder::TRUE)]);
                for child in children {
                    let mut alternatives = BTreeMap::<u8, Vec<usize>>::new();
                    for (&left, &left_root) in &joint {
                        for (&right, &right_root) in partitions.get(child)? {
                            alternatives
                                .entry(left | right)
                                .or_default()
                                .push(builder.and([left_root, right_root]));
                        }
                    }
                    joint = eager_union_contexts(alternatives, &mut builder);
                }
                joint
            }
            RequirementConditionNode::Or(children) => {
                let mut alternatives = BTreeMap::<u8, Vec<usize>>::new();
                for child in children {
                    for (&context, &root) in partitions.get(child)? {
                        alternatives.entry(context).or_default().push(root);
                    }
                }
                eager_union_contexts(alternatives, &mut builder)
            }
            RequirementConditionNode::Decision(RequirementDecision {
                test,
                when_true,
                when_false,
            })
            | RequirementConditionNode::Split(RequirementDecision {
                test,
                when_true,
                when_false,
            }) => {
                let positive = observe(test, ProofGuardRole::SplitTest);
                let negative = observe(
                    &ConditionalGuard::Not(Arc::new(test.clone())),
                    ProofGuardRole::SplitTest,
                );
                let mut branches = BTreeMap::<u8, [Vec<usize>; 2]>::new();
                for (&context, &root) in partitions.get(when_true)? {
                    let [yes, _] = branches.entry(context | positive).or_default();
                    yes.push(root);
                }
                for (&context, &root) in partitions.get(when_false)? {
                    let [_, no] = branches.entry(context | negative).or_default();
                    no.push(root);
                }
                branches
                    .into_iter()
                    .map(|(context, [yes, no])| {
                        let when_true = builder.union(yes);
                        let when_false = builder.union(no);
                        (context, builder.split(test.clone(), when_true, when_false))
                    })
                    .collect()
            }
        };
        partitions.insert(id, contexts);
    }
    let roots = partitions.remove(&graph.root())?;
    let conditions = builder.freeze();
    Some(
        roots
            .into_iter()
            .map(|(context, id)| RequirementPartition {
                context,
                condition: conditions.root(id, false),
            })
            .collect(),
    )
}

#[derive(Debug, PartialEq, Eq)]
struct GuardObservation {
    guard: ConditionalGuard,
    is_split_test: bool,
}

fn record_observations(
    observed: &RefCell<Vec<GuardObservation>>,
) -> impl Fn(&ConditionalGuard, ProofGuardRole) -> u8 + '_ {
    |guard, role| {
        observed.borrow_mut().push(GuardObservation {
            guard: guard.clone(),
            is_split_test: matches!(role, ProofGuardRole::SplitTest),
        });
        judgment(guard, role)
    }
}

fn partition_wire(partitions: Vec<RequirementPartition<u8>>) -> eyre::Result<BTreeMap<u8, Value>> {
    partitions
        .into_iter()
        .map(|RequirementPartition { context, condition }| {
            Ok((context, serde_json::to_value(condition)?))
        })
        .collect()
}

#[test]
fn materialized_requirement_partitions_match_eager_shapes_and_observations() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let collapsed = collapsing_requirement(&mut builder);
    let fallback = builder.guard(truthy("otherwise"));
    let decision = builder.decision(truthy("dispatch"), collapsed, fallback);
    let split = builder.split(truthy("same-child"), decision, decision);
    let neutral = builder.guard(truthy("neutral"));
    let root = builder.union([RequirementConditionBuilder::TRUE, split, neutral]);
    let graph = builder.freeze().root(root, true);
    let observed = RefCell::default();
    let actual = partition_requirement(&graph, record_observations(&observed), |left, right| {
        left | right
    })
    .ok_or_eyre("lazy proof partition")?;
    let eager_observed = RefCell::default();
    let eager = eager_partition(&graph, record_observations(&eager_observed))
        .ok_or_eyre("eager proof partition")?;
    // The collapsed conjunction must retain its binary recipe inside a multi-context result.
    assert!(actual.len() > 1);
    sim_assert_eq!(have: partition_wire(actual)?, want: partition_wire(eager)?);
    sim_assert_eq!(have: observed.into_inner(), want: eager_observed.into_inner());
    Ok(())
}

#[test]
fn terminal_path_summaries_match_unshared_guards_across_refill_documents() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![
        truthy("kid.enabled"),
        ConditionalGuard::Absent {
            path: ValuesPath::parse("kid.nested.missing"),
        },
    ]);
    let literal_star = ConditionalGuard::Truthy {
        path: ValuesPath::from_segments([
            Segment::Literal("kid".to_owned()),
            Segment::Literal("*".to_owned()),
            Segment::Literal("enabled".to_owned()),
        ]),
    };
    let rows = [
        GuardConjunction::prefixed(vec![truthy("kid.nested.more")], prefix.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![truthy("kid.*.enabled")], prefix.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![literal_star], prefix.clone(), Vec::new()),
        GuardConjunction::prefixed(vec![truthy("kid.global.enabled")], prefix, Vec::new()),
        GuardConjunction::prefixed(
            vec![truthy("extra.enabled")],
            GuardPrefix::new(vec![ConditionalGuard::AllOf(Vec::new().into())]),
            Vec::new(),
        ),
    ]
    .map(GuardConjunction::canonicalized);
    let documents = [
        serde_yaml::to_value(
            json!({"kid":{"enabled":true,"nested":{"more":true},"*":{"enabled":true}}}),
        )?,
        serde_yaml::to_value(json!({"kid":{"enabled":false,"nested":{"more":true}}}))?,
        serde_yaml::to_value(json!({"kid":{"enabled":true,"nested":{"missing":null}}}))?,
    ];
    let roots = BTreeSet::from([
        vec!["kid".to_owned()],
        vec!["kid".to_owned(), "nested".to_owned()],
    ]);
    let empty = YamlValue::Null;
    let mut cache = TerminalPathCache::default();
    for values in documents.iter().cycle().take(6) {
        let absence = AbsenceDefaults {
            runtime_defaults: &empty,
            dependency_refill: values,
            dependency_roots: &roots,
        };
        for row in &rows {
            let expected = deleted_dependency_root_terminates(row, absence);
            sim_assert_eq!(have: cache.deleted_dependency_root(row,absence), want: expected);
        }
    }
    sim_assert_eq!(have: cache.ancestor(&rows[0],&documents[0]), want: vec!["kid".to_owned()]);
    sim_assert_eq!(have: cache.ancestor(&rows[4],&documents[0]), want: vec!["extra".to_owned()]);
    let absent = GuardConjunction::from(vec![ConditionalGuard::Absent {
        path: ValuesPath::parse("kid.missing"),
    }]);
    sim_assert_eq!(have: cache.ancestor(&absent,&empty), want: Vec::<String>::new());
    sim_assert_eq!(have: cache.ancestor(&absent,&documents[0]), want: vec!["kid".to_owned()]);
    Ok(())
}

#[test]
fn shared_terminal_encodability_matches_independent_guard_checks() -> eyre::Result<()> {
    let prefix = GuardPrefix::new(vec![
        truthy("service.enabled"),
        ConditionalGuard::IntGt {
            path: ValuesPath::parse("service.port"),
            bound: 0,
        },
    ]);
    let mut rows = [
        GuardConjunction::prefixed(vec![truthy("service.before")], prefix.clone(), Vec::new()),
        GuardConjunction::prefixed(
            vec![ConditionalGuard::MatchesPattern {
                path: ValuesPath::parse("service.invalid"),
                pattern: "[".to_owned(),
            }],
            prefix,
            Vec::new(),
        ),
    ]
    .map(GuardConjunction::canonicalized)
    .to_vec();
    for tail in [
        truthy("service.appended"),
        ConditionalGuard::MatchesPattern {
            path: ValuesPath::parse("service.appended"),
            pattern: "[".to_owned(),
        },
    ] {
        let mut appended = rows[0].clone();
        appended.push(tail);
        rows.push(appended);
    }
    let values = serde_yaml::to_value(json!({"service":{"enabled":true,"port":42}}))?;
    let empty = YamlValue::Null;
    let roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        runtime_defaults: &empty,
        dependency_refill: &values,
        dependency_roots: &roots,
    };
    let mut shared = ConditionFragmentCache::default();
    let mut flat = ConditionFragmentCache::default();
    for ancestor in [
        vec![],
        vec!["service".to_owned()],
        vec!["other".to_owned()],
        vec![],
    ] {
        for row in &rows {
            let expected = row
                .iter()
                .all(|guard| flat.encodes_fully(guard, &ancestor, &values, absence));
            sim_assert_eq!(have: shared.conjunction_encodes_fully(row,&ancestor,&values,absence),want:expected);
            sim_assert_eq!(have: &shared.definitions,want:&flat.definitions);
        }
    }
    Ok(())
}

#[test]
fn terminal_rows_coalesce_after_lint_and_preserve_addressed_array_slots() -> eyre::Result<()> {
    for foreign in [
        None,
        Some("#/allOf/1/if"),
        Some("file:///values.schema.json#/allOf/1/if"),
        Some("values.schema.json#/allOf/1/if"),
    ] {
        let values = YamlValue::Null;
        let roots = BTreeSet::new();
        let definitions = BTreeMap::new();
        let coalesced = json!({});
        let gate = UncoalescedRootGate::new(None, &coalesced, &definitions);
        let oracle = UncoalescedRootGate::new(None, &coalesced, &definitions);
        let mut actual = SchemaDocument::new_root_object(RootPolicy::Open);
        let mut legacy = actual.clone();
        if let Some(reference) = foreign {
            let property = json!({"$ref":reference});
            gate.reserve_condition_names(&property);
            oracle.reserve_condition_names(&property);
            actual.insert_path_schema(&["probe".to_owned()], SchemaNode::foreign(property.clone()));
            legacy.insert_path_schema(&["probe".to_owned()], SchemaNode::foreign(property));
        }
        let absence = AbsenceDefaults {
            runtime_defaults: &values,
            dependency_refill: &values,
            dependency_roots: &roots,
        };
        let mut pending = TerminalEmissions::default();
        let mut conditions = Vec::new();
        for name in ["a", "b"] {
            let row = GuardConjunction::from(vec![truthy(name)]);
            let plan =
                gate.planned_conjunction(&row, &[], &values, absence, ConditionPolarity::Narrow);
            let condition = SchemaNode::all_of(oracle.condition_clauses(
                &row.to_vec(),
                &[],
                &values,
                absence,
                ConditionPolarity::Narrow,
            ));
            conditions.push(condition.clone().into_value());
            oracle.append_conditional(
                &mut legacy,
                &[],
                condition,
                SchemaNode::foreign(json!(false)),
            );
            pending.push(&gate, &[], plan, json!(false));
        }
        pending.append(&mut actual, &gate);
        let actual = actual.into_value();
        let legacy = legacy.into_value();
        if foreign.is_some() {
            sim_assert_eq!(have:&actual,want:&legacy);
        } else {
            sim_assert_eq!(have:&actual,want:&json!({"type":"object","properties":{},"allOf":[{
                "if":{"anyOf":conditions},"then":false,
            }]}));
        }
        let validator = |mut schema: Value| {
            schema["definitions"] = json!({"t":helm_truthy_definition_schema()});
            jsonschema::options()
                .with_base_uri("file:///values.schema.json")
                .build(&schema)
        };
        let actual_validator = validator(actual)?;
        let legacy_validator = validator(legacy)?;
        for document in [
            json!({}),
            json!({"a":true}),
            json!({"b":true}),
            json!({"probe":{}}),
        ] {
            sim_assert_eq!(have:actual_validator.is_valid(&document),want:legacy_validator.is_valid(&document));
        }
    }
    Ok(())
}

#[derive(Clone, Default)]
struct ProofBounds(Arc<AtomicU64>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for ProofBounds {
    fn on_new_span(
        &self,
        attributes: &tracing::span::Attributes<'_>,
        _id: &tracing::span::Id,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if matches!(
            attributes.metadata().name(),
            "requirement_partition_bound"
                | "predicate_residual_operation_bound"
                | "predicate_residual_inventory_bound"
        ) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn partition_factors(builder: &mut RequirementConditionBuilder, count: usize) -> usize {
    let factors = (0..count)
        .map(|index| {
            let choice = builder.guard(truthy(&format!("bit{index}")));
            builder.union([RequirementConditionBuilder::TRUE, choice])
        })
        .collect::<Vec<_>>();
    builder.and(factors)
}

fn partition_bits(guard: &ConditionalGuard, _: ProofGuardRole) -> u32 {
    let ConditionalGuard::Truthy { path } = guard else {
        return 0;
    };
    path.encode()
        .strip_prefix("bit")
        .and_then(|suffix| suffix.parse::<u32>().ok())
        .map_or(0, |index| 1 << index)
}

#[test]
fn lint_conjunction_partition_counts_its_local_cross_product_bound() {
    use tracing_subscriber::prelude::*;

    let mut builder = RequirementConditionBuilder::default();
    let root = partition_factors(&mut builder, 14);
    let graph = builder.freeze().root(root, true);
    let count = ProofBounds::default();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(count.clone()), || {
        assert!(
            partition_requirement(&graph, partition_bits, |left, right| left | right)
                .is_none_or(|parts| parts.is_empty())
        );
    });
    sim_assert_eq!(have: count.0.load(Ordering::Relaxed), want: 1);
}

#[test]
fn capped_lint_conjunction_keeps_an_independent_abort_arm() -> eyre::Result<()> {
    use helm_schema_core::Predicate;
    use tracing_subscriber::prelude::*;

    let mut builder = RequirementConditionBuilder::default();
    let costly = partition_factors(&mut builder, 14);
    let abort = builder.guard(truthy("abort"));
    let enabled = builder.guard(truthy("enabled"));
    let cheap = builder.and([abort, enabled]);
    let root = builder.union([costly, cheap]);
    let graph = builder.freeze().root(root, true);
    let count = ProofBounds::default();
    let parts = tracing::subscriber::with_default(
        tracing_subscriber::registry().with(count.clone()),
        || partition_requirement(&graph, partition_bits, |left, right| left | right),
    )
    .ok_or_eyre("the independently provable abort arm was withdrawn")?;
    let [RequirementPartition { context, condition }] = parts.as_slice() else {
        eyre::bail!("expected just the cheap abort arm");
    };
    sim_assert_eq!(have: *context, want: 0);
    sim_assert_eq!(have: count.0.load(Ordering::Relaxed), want: 1);
    sim_assert_eq!(
        have: ConditionalGuard::Requirement(condition.clone()).predicate().normalize_boolean(),
        want: Predicate::And(vec![
            Predicate::truthy_path("abort"),
            Predicate::truthy_path("enabled"),
        ]).normalize_boolean(),
    );
    assert!(!condition.projection_is_exact());
    Ok(())
}

#[test]
fn activation_residual_operation_counts_exhaustion_before_returning_unknown() {
    use helm_schema_core::{Predicate, PredicateResiduals};
    use tracing_subscriber::prelude::*;

    let too_large = Predicate::And(
        (0..100)
            .map(|index| Predicate::truthy_path(format!("value-{index:03}")))
            .collect(),
    );
    let count = ProofBounds::default();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(count.clone()), || {
        let mut solver = PredicateResiduals::new([&too_large]);
        assert!(solver.atoms(&too_large).is_some());
        assert!(solver.import(&too_large).is_none());
    });
    sim_assert_eq!(have: count.0.load(Ordering::Relaxed), want: 1);
}

#[test]
fn activation_residual_inventory_counts_work_and_depth_exhaustion() {
    use helm_schema_core::{Predicate, PredicateResiduals};
    use tracing_subscriber::prelude::*;

    let work = Predicate::And(vec![Predicate::True; 17_000]);
    let depth = (0..520).fold(Predicate::True, |child, _| Predicate::Not(Box::new(child)));
    let opaque = Predicate::approximate("unresolved", BTreeSet::new());
    let good = Predicate::truthy_path("valid");
    let count = ProofBounds::default();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(count.clone()), || {
        let mut solver = PredicateResiduals::new([&work, &depth, &opaque, &good, &work]);
        sim_assert_eq!(have: count.0.load(Ordering::Relaxed), want: 2);
        for predicate in [&work, &depth, &opaque] {
            assert!(solver.atoms(predicate).is_none());
            assert!(solver.import(predicate).is_none());
        }
        assert!(solver.import(&good).is_some());
    });
    sim_assert_eq!(have: count.0.load(Ordering::Relaxed), want: 2);
}
