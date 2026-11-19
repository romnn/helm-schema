use std::rc::Rc;

use ahash::AHashMap;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use super::{CanonicalGuards, FALSE};
use crate::value::guard::{GuardQueries, GuardWorkBudget, RuntimeGuard};
use crate::value::{Arena, Coverage, FrozenTest};

fn steady_query(
    tables: &mut CanonicalGuards,
    guard: &RuntimeGuard,
    context: *const GuardQueries,
    skip_feasible: bool,
) -> bool {
    if skip_feasible {
        return false;
    }
    match guard {
        RuntimeGuard::False => return true,
        RuntimeGuard::True => return false,
        RuntimeGuard::Branch(node) if node.alias_free => return false,
        RuntimeGuard::Branch(_) => {}
    }
    // The small ordinary unit-test table would erase the production history being examined.
    // These tables remain below the production generation limit throughout the test.
    assert!(tables.nodes.len() < 1 << 22);
    if let Some(form) = tables.form(guard, context) {
        return form == FALSE;
    }
    guard.satisfiable(
        &RuntimeGuard::True,
        true,
        true,
        None,
        &mut GuardWorkBudget::new(crate::value::guard::MAX_PROJECTION_WORK),
    ) == Some(false)
}

struct History {
    reference: CanonicalGuards,
    optimized: CanonicalGuards,
    context: *const GuardQueries,
}

impl History {
    fn query(&mut self, guard: &RuntimeGuard, stage: &str) {
        let expected = steady_query(&mut self.reference, guard, self.context, false);
        let actual = steady_query(
            &mut self.optimized,
            guard,
            self.context,
            stage == "feasible warmup",
        );
        sim_assert_eq!(have: actual, want: expected, "stage={stage}, reference_nodes={}, optimized_nodes={}", self.reference.nodes.len(), self.optimized.nodes.len());
    }

    fn warm_select(
        &mut self,
        arena: &Arena,
        test: &Rc<FrozenTest>,
        yes: &RuntimeGuard,
        no: &RuntimeGuard,
    ) -> RuntimeGuard {
        for attempt in 0..64 {
            let root = {
                let mut store = arena.guards.borrow_mut();
                store.working.clear();
                store.select(Rc::clone(test), yes.clone(), no.clone())
            };
            self.query(&root, "feasible warmup");
            if self.reference.known(&root).is_some() || attempt == 63 {
                return root;
            }
            // Fresh root IDs with retained children exercise partial work from failed queries.
            // A completed or failed composition must keep the same boundary on every retry.
        }
        RuntimeGuard::False
    }
}

fn build_conjunction(
    arena: &Arena,
    clauses: &[Rc<FrozenTest>],
    reversed: bool,
    history: &mut History,
) -> eyre::Result<RuntimeGuard> {
    let order = if reversed {
        clauses.iter().rev().collect::<Vec<_>>()
    } else {
        clauses.iter().collect::<Vec<_>>()
    };
    let mut root = RuntimeGuard::True;
    for test in order {
        root = history.warm_select(arena, test, &root, &RuntimeGuard::False);
        eyre::ensure!(
            root != RuntimeGuard::False,
            "warmup failed to finish a feasible conjunction"
        );
    }
    Ok(root)
}

/// Skipping feasible queries preserves later bounded refutations below the production table limit.
#[test]
fn witness_queries_preserve_production_sized_canonical_histories() -> eyre::Result<()> {
    for pairs in [14, 15, 16] {
        let arena = Arena::default();
        let inputs = (0..pairs * 2)
            .map(|_| arena.decision(Predicate::False, Predicate::False, Coverage::Partial))
            .collect::<Vec<_>>();
        let mut clauses = Vec::new();
        for index in 0..pairs {
            let x = inputs.get(index).ok_or_eyre("x input missing")?;
            let y = inputs.get(index + pairs).ok_or_eyre("y input missing")?;
            let pair = {
                let mut store = arena.guards.borrow_mut();
                let y = store.select(Rc::clone(y), RuntimeGuard::True, RuntimeGuard::False);
                store.select(Rc::clone(x), RuntimeGuard::True, y)
            };
            clauses.push(arena.budget_test(pair));
        }
        let context = Rc::as_ptr(&arena.guards.borrow().queries);
        let mut history = History {
            reference: CanonicalGuards::default(),
            optimized: CanonicalGuards::default(),
            context,
        };
        let left = build_conjunction(&arena, &clauses, false, &mut history)?;
        let right = build_conjunction(&arena, &clauses, true, &mut history)?;
        let right_test = arena.budget_test(right);
        let opposite = history.warm_select(
            &arena,
            &right_test,
            &RuntimeGuard::False,
            &RuntimeGuard::True,
        );
        eyre::ensure!(
            opposite != RuntimeGuard::False,
            "negation warmup did not finish"
        );
        let left_test = arena.budget_test(left);
        for _ in 0..64 {
            let contradiction = {
                let mut store = arena.guards.borrow_mut();
                store.working.clear();
                store.select(Rc::clone(&left_test), opposite.clone(), RuntimeGuard::False)
            };
            history.query(&contradiction, "equivalent guards under opposite outcomes");
        }
    }
    Ok(())
}

struct Random(u64);

impl Random {
    fn next(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % u64::try_from(bound).unwrap_or(u64::MAX)).unwrap_or_default()
    }
}

fn cold_memo<K: Copy + Eq + std::hash::Hash, T: Copy>(
    memo: &super::ProofMemo<K, T>,
) -> super::ProofMemo<K, T> {
    super::ProofMemo {
        keys: memo.keys.clone(),
        storage: Rc::clone(&memo.storage),
        certificate_allowance: memo.certificate_allowance,
        entries: memo
            .entries
            .iter()
            .map(|entry| match entry {
                super::WorkEntry::Pending => super::WorkEntry::Pending,
                super::WorkEntry::Complete { value: _, work } => super::WorkEntry::Complete {
                    value: None,
                    work: work.clone(),
                },
            })
            .collect(),
        completed: Vec::new(),
    }
}

fn cold_table(table: &CanonicalGuards) -> CanonicalGuards {
    CanonicalGuards {
        nodes: table.nodes.clone(),
        unique: table.unique.clone(),
        forms: AHashMap::new(),
        composed: cold_memo(&table.composed),
        compatible: cold_memo(&table.compatible),
        memo_capacity: table.memo_capacity,
    }
}

fn truth_table(tables: &CanonicalGuards, form: u32) -> Vec<bool> {
    (0_u64..256)
        .map(|assignment| {
            let mut form = form;
            while let Some(node) = tables.node(form) {
                form = if assignment & (1 << node.decision) == 0 {
                    node.no
                } else {
                    node.yes
                };
            }
            form == super::TRUE
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct IteObservation {
    result: Option<Vec<bool>>,
    remaining: usize,
}

fn observe_ite(
    tables: &mut CanonicalGuards,
    operands: [u32; 3],
    allowance: usize,
) -> IteObservation {
    let mut remaining = allowance;
    let result = tables.ite(operands, &mut remaining);
    IteObservation {
        result: result.map(|form| truth_table(tables, form)),
        remaining,
    }
}

/// Every allowance has the same boundary under completed, partial and evicted ITE histories.
#[test]
fn canonical_ite_answers_and_charges_ignore_memo_histories() {
    let mut random = Random(0x5acf_98be_19a7_624d);
    for _ in 0..12 {
        let mut seed = CanonicalGuards::default();
        let mut forms = vec![super::FALSE, super::TRUE];
        // Every arm tests a smaller decision, so the seed is already an ordered diagram.
        for decision in 0..8 {
            let earlier = forms.len();
            for _ in 0..24 {
                let no = forms[random.next(earlier)];
                let yes = forms[random.next(earlier)];
                forms.push(seed.make(decision, no, yes));
            }
        }
        let queries = (0..96)
            .map(|_| {
                [
                    forms[random.next(forms.len())],
                    forms[random.next(forms.len())],
                    forms[random.next(forms.len())],
                ]
            })
            .collect::<Vec<_>>();
        let mut warm = cold_table(&seed);
        let mut shuffled = cold_table(&seed);
        let mut tiny = cold_table(&seed);
        tiny.memo_capacity = 1;
        tiny.composed.certificate_allowance = 0;
        tiny.compatible.certificate_allowance = 0;
        for &operands in &queries {
            observe_ite(&mut warm, operands, super::MAX_CANONICAL_WORK);
        }
        for _ in 0..queries.len() * 2 {
            let operands = queries[random.next(queries.len())];
            observe_ite(&mut shuffled, operands, random.next(128));
        }
        for &operands in &queries {
            let expected = truth_table(&seed, operands[0])
                .into_iter()
                .zip(truth_table(&seed, operands[1]))
                .zip(truth_table(&seed, operands[2]))
                .map(|((test, yes), no)| if test { yes } else { no })
                .collect::<Vec<_>>();
            for allowance in [0, 1, 2, 8, 32, 128, super::MAX_CANONICAL_WORK] {
                let cold = observe_ite(&mut cold_table(&seed), operands, allowance);
                if let Some(result) = &cold.result {
                    sim_assert_eq!(have: result, want: &expected);
                }
                sim_assert_eq!(have: observe_ite(&mut warm, operands, allowance), want: cold);
                sim_assert_eq!(have: observe_ite(&mut shuffled, operands, allowance), want: cold);
                // Eviction also happens inside a composition, while only one entry survives.
                sim_assert_eq!(have: observe_ite(&mut tiny, operands, allowance), want: cold);
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CompatibilityObservation {
    result: Option<bool>,
    remaining: usize,
}

fn observe_compatibility(
    tables: &mut CanonicalGuards,
    operands: [super::Polar; 2],
    allowance: usize,
) -> CompatibilityObservation {
    let [left, right] = operands;
    let mut remaining = allowance;
    let result = tables.joint(left, right, &mut remaining);
    CompatibilityObservation { result, remaining }
}

/// Shared compatibility results preserve answers and charges across every memo history.
#[test]
fn canonical_compatibility_answers_and_charges_ignore_query_order() {
    let mut random = Random(0xc651_783d_9e74_2fa0);
    let mut tables = CanonicalGuards::default();
    let mut forms = vec![super::FALSE, super::TRUE];
    for decision in 0..8 {
        let earlier = forms.len();
        for _ in 0..48 {
            forms.push(tables.make(
                decision,
                forms[random.next(earlier)],
                forms[random.next(earlier)],
            ));
        }
    }
    let mut shuffled = cold_table(&tables);
    let mut tiny = cold_table(&tables);
    tiny.memo_capacity = 1;
    tiny.composed.certificate_allowance = 0;
    tiny.compatible.certificate_allowance = 0;
    for _ in 0..512 {
        let left = super::Polar {
            form: forms[random.next(forms.len())],
            value: random.next(2) == 0,
        };
        let right = super::Polar {
            form: forms[random.next(forms.len())],
            value: random.next(2) == 0,
        };
        let operands = [left, right];
        let expected = truth_table(&tables, left.form)
            .into_iter()
            .zip(truth_table(&tables, right.form))
            .any(|(a, b)| a == left.value && b == right.value);
        observe_compatibility(&mut tables, operands, super::MAX_CANONICAL_WORK);
        observe_compatibility(&mut shuffled, [right, left], random.next(128));
        for allowance in [0, 1, 2, 8, 32, super::MAX_CANONICAL_WORK] {
            let cold = observe_compatibility(&mut cold_table(&tables), operands, allowance);
            if let Some(result) = cold.result {
                sim_assert_eq!(have: result, want: expected);
            }
            sim_assert_eq!(have: observe_compatibility(&mut tables, operands, allowance), want: cold);
            sim_assert_eq!(have: observe_compatibility(&mut shuffled, operands, allowance), want: cold);
            sim_assert_eq!(have: observe_compatibility(&mut tiny, operands, allowance), want: cold);
            tiny.compatible.clear();
            sim_assert_eq!(have: observe_compatibility(&mut tiny, operands, allowance), want: cold);
        }
    }
}

fn evaluate_guard(guard: &RuntimeGuard, assignment: u64) -> bool {
    match guard {
        RuntimeGuard::False => false,
        RuntimeGuard::True => true,
        RuntimeGuard::Branch(node) => {
            let taken = node.test.runtime.as_ref().map_or_else(
                || assignment & (1 << node.test.id) != 0,
                |test| evaluate_guard(test, assignment),
            );
            evaluate_guard(if taken { &node.yes } else { &node.no }, assignment)
        }
    }
}

/// Cached forms, compatibility, atom restriction and table generations preserve bounded answers.
#[test]
fn canonical_guard_queries_ignore_warm_shuffled_and_tiny_histories() {
    let mut random = Random(0x942d_c8f7_165b_a630);
    for _ in 0..8 {
        let arena = Arena::default();
        let inputs = (0..8)
            .map(|_| arena.decision(Predicate::False, Predicate::False, Coverage::Partial))
            .collect::<Vec<_>>();
        let mut guards = vec![RuntimeGuard::False, RuntimeGuard::True];
        for _ in 0..128 {
            let test = if random.next(2) == 0 {
                Rc::clone(&inputs[random.next(inputs.len())])
            } else {
                arena.budget_test(guards[random.next(guards.len())].clone())
            };
            let yes = guards[random.next(guards.len())].clone();
            let no = guards[random.next(guards.len())].clone();
            guards.push(arena.guards.borrow_mut().select(test, yes, no));
        }
        let context = Rc::as_ptr(&arena.guards.borrow().queries);
        let mut warm = CanonicalGuards::default();
        let mut shuffled = CanonicalGuards::default();
        let mut tiny = CanonicalGuards {
            memo_capacity: 1,
            ..CanonicalGuards::default()
        };
        for guard in &guards {
            warm.form(guard, context);
        }
        for _ in 0..guards.len() * 2 {
            let left = &guards[random.next(guards.len())];
            let right = &guards[random.next(guards.len())];
            shuffled.compatible(
                [(left, random.next(2) == 0), (right, random.next(2) == 0)],
                context,
            );
        }
        for _ in 0..256 {
            let left = &guards[random.next(guards.len())];
            let right = &guards[random.next(guards.len())];
            let left_value = random.next(2) == 0;
            let right_value = random.next(2) == 0;
            let operands = [(left, left_value), (right, right_value)];
            let cold = CanonicalGuards::default().compatible(operands, context);
            let exact = (0..256).any(|assignment| {
                evaluate_guard(left, assignment) == left_value
                    && evaluate_guard(right, assignment) == right_value
            });
            if let Some(result) = cold {
                sim_assert_eq!(have: result, want: exact);
            }
            sim_assert_eq!(have: warm.compatible(operands, context), want: cold);
            sim_assert_eq!(have: shuffled.compatible(operands, context), want: cold);
            // Force generation eviction between queries without changing guard contents.
            tiny.nodes.clear();
            tiny.unique.clear();
            tiny.forms.clear();
            tiny.composed.clear();
            tiny.compatible.clear();
            sim_assert_eq!(have: tiny.compatible(operands, context), want: cold);
            let atom = super::super::DecisionEdgeKey {
                decision: u64::try_from(random.next(8)).unwrap_or_default(),
                taken: random.next(2) == 0,
            };
            let cold = CanonicalGuards::default().possible_atom(left, &atom, left_value, context);
            sim_assert_eq!(have: warm.possible_atom(left, &atom, left_value, context), want: cold);
            sim_assert_eq!(have: shuffled.possible_atom(left, &atom, left_value, context), want: cold);
            sim_assert_eq!(have: tiny.possible_atom(left, &atom, left_value, context), want: cold);
            // The production query wrappers also remain stable when their answer caches evict.
            let cached = left.compatible(left_value, right, right_value);
            arena
                .guards
                .borrow()
                .queries
                .compatibilities
                .borrow_mut()
                .clear();
            arena.guards.borrow().queries.atoms.borrow_mut().clear();
            sim_assert_eq!(have: left.compatible(left_value, right, right_value), want: cached);
        }
    }
}

/// A completed hit pays its entire cold construction, including the exact exhaustion boundary.
#[test]
fn canonical_completed_composition_charges_its_cold_work() {
    let mut tables = CanonicalGuards::default();
    let variable = tables.make(0, super::FALSE, super::TRUE);
    let operands = [variable, super::FALSE, super::TRUE];
    let cold = observe_ite(&mut tables, operands, 8);
    sim_assert_eq!(have: cold.remaining, want: 7);
    sim_assert_eq!(have: tables.composed.get(&operands).and_then(|proof| proof.work()?.cold.map(std::num::NonZeroUsize::get)), want: Some(1));
    sim_assert_eq!(have: observe_ite(&mut tables, operands, 8), want: cold);
    let warm_boundary = observe_ite(&mut tables, operands, 1);
    tables.composed.clear();
    sim_assert_eq!(have: observe_ite(&mut tables, operands, 1), want: warm_boundary);
    sim_assert_eq!(have: warm_boundary.remaining, want: 0);
    assert!(warm_boundary.result.is_some());
    let warm_exhausted = observe_ite(&mut tables, operands, 0);
    tables.composed.clear();
    sim_assert_eq!(have: observe_ite(&mut tables, operands, 0), want: warm_exhausted);
    sim_assert_eq!(have: warm_exhausted.result, want: None);
}

/// Every persisted result, including descendants of an exhausted parent, pays its cold charge.
#[test]
fn every_memoized_composition_hit_pays_its_full_cold_charge() {
    let mut random = Random(0xa4e7_51c9_82bf_360d);
    let mut tables = CanonicalGuards::default();
    let mut forms = vec![super::FALSE, super::TRUE];
    for decision in 0..8 {
        let earlier = forms.len();
        for _ in 0..32 {
            forms.push(tables.make(
                decision,
                forms[random.next(earlier)],
                forms[random.next(earlier)],
            ));
        }
    }
    for _ in 0..256 {
        let operands = [
            forms[random.next(forms.len())],
            forms[random.next(forms.len())],
            forms[random.next(forms.len())],
        ];
        let allowance = random.next(256);
        observe_ite(&mut tables, operands, allowance);
        let pair = [
            super::Polar {
                form: operands[0],
                value: random.next(2) == 0,
            },
            super::Polar {
                form: operands[1],
                value: random.next(2) == 0,
            },
        ];
        observe_compatibility(&mut tables, pair, allowance);
    }
    assert!(tables.composed.completed.len() > 256);
    assert!(tables.compatible.completed.len() > 256);
    // Disable every memo for the cold observation, then retain exactly the entry under test.
    for (&operands, &id) in &tables.composed.keys {
        let completed = tables.composed.entries[id.index()].clone();
        let Some(value) = completed.value() else {
            continue;
        };
        let mut reference = cold_table(&tables);
        reference.memo_capacity = 0;
        let mut remaining = super::MAX_CANONICAL_WORK;
        sim_assert_eq!(have: reference.ite(operands, &mut remaining), want: Some(value));
        let work = super::MAX_CANONICAL_WORK - remaining;
        for allowance in [work - 1, work, work + 1] {
            let mut cold = cold_table(&tables);
            cold.memo_capacity = 0;
            let mut warm = cold_table(&tables);
            warm.composed.entries[id.index()] = completed.clone();
            warm.composed.completed.push(id);
            let mut cold_work = allowance;
            let mut warm_work = allowance;
            let expected = cold.ite(operands, &mut cold_work);
            sim_assert_eq!(have: warm.ite(operands, &mut warm_work), want: expected);
            sim_assert_eq!(have: warm_work, want: cold_work);
            sim_assert_eq!(have: allowance - warm_work, want: allowance.min(work));
            if allowance >= work {
                sim_assert_eq!(have: expected, want: Some(value));
                sim_assert_eq!(have: warm.composed.get(&operands).and_then(|entry| entry.work()?.cold.map(std::num::NonZeroUsize::get)), want: Some(work));
            } else {
                sim_assert_eq!(have: expected, want: None);
            }
        }
    }
    for (&operands, &id) in &tables.compatible.keys {
        let completed = tables.compatible.entries[id.index()].clone();
        let Some(value) = completed.value() else {
            continue;
        };
        let mut reference = cold_table(&tables);
        reference.memo_capacity = 0;
        let observed = observe_compatibility(&mut reference, operands, super::MAX_CANONICAL_WORK);
        sim_assert_eq!(have: observed.result, want: Some(value));
        let work = super::MAX_CANONICAL_WORK - observed.remaining;
        for allowance in [work - 1, work, work + 1] {
            let mut cold = cold_table(&tables);
            cold.memo_capacity = 0;
            let mut warm = cold_table(&tables);
            warm.compatible.entries[id.index()] = completed.clone();
            warm.compatible.completed.push(id);
            let expected = observe_compatibility(&mut cold, operands, allowance);
            sim_assert_eq!(have: observe_compatibility(&mut warm, operands, allowance), want: expected);
            sim_assert_eq!(have: allowance - expected.remaining, want: allowance.min(work));
            if allowance >= work {
                sim_assert_eq!(have: expected.result, want: Some(value));
                sim_assert_eq!(have: warm.compatible.get(&operands).and_then(|entry| entry.work()?.cold.map(std::num::NonZeroUsize::get)), want: Some(work));
            } else {
                sim_assert_eq!(have: expected.result, want: None);
            }
        }
    }
}

/// Completed children of a failed parent remain useful, but cannot reduce the retry's charge.
#[test]
fn exhausted_parent_preserves_only_fully_charged_children() {
    let mut tables = CanonicalGuards::default();
    let x = tables.make(0, super::FALSE, super::TRUE);
    let y = tables.make(1, super::FALSE, super::TRUE);
    let z = tables.make(2, super::FALSE, super::TRUE);
    let operands = [x, y, z];
    let child = [x, y, super::TRUE];
    let mut work = 3;
    sim_assert_eq!(have: tables.ite(operands, &mut work), want: None);
    sim_assert_eq!(have: work, want: 0);
    assert!(tables.composed.get(&operands).is_none());
    sim_assert_eq!(have: tables.composed.get(&child).and_then(|proof| proof.work()?.cold.map(std::num::NonZeroUsize::get)), want: Some(2));
    let mut cold = cold_table(&tables);
    cold.memo_capacity = 0;
    for allowance in [3, 4, 5] {
        let expected = observe_ite(&mut cold, operands, allowance);
        sim_assert_eq!(have: observe_ite(&mut tables, operands, allowance), want: expected);
        sim_assert_eq!(have: allowance - expected.remaining, want: allowance.min(4));
    }
}

// The original query-local DAG walks are independent oracles for the allowance contract.
fn reference_ite(
    tables: &mut CanonicalGuards,
    [test, yes, no]: [u32; 3],
    memo: &mut AHashMap<[u32; 3], u32>,
    work: &mut usize,
) -> Option<u32> {
    match (test, yes, no) {
        (super::TRUE, _, _) => return Some(yes),
        (super::FALSE, _, _) => return Some(no),
        _ if yes == no => return Some(yes),
        (_, super::TRUE, super::FALSE) => return Some(test),
        _ => {}
    }
    if let Some(form) = memo.get(&[test, yes, no]) {
        return Some(*form);
    }
    *work = work.checked_sub(1)?;
    let decision = [test, yes, no]
        .into_iter()
        .filter_map(|form| tables.top(form))
        .max()?;
    let (test_no, test_yes) = tables.cofactors(test, decision);
    let (yes_no, yes_yes) = tables.cofactors(yes, decision);
    let (no_no, no_yes) = tables.cofactors(no, decision);
    let high = reference_ite(tables, [test_yes, yes_yes, no_yes], memo, work)?;
    let low = reference_ite(tables, [test_no, yes_no, no_no], memo, work)?;
    let form = tables.make(decision, low, high);
    memo.insert([test, yes, no], form);
    Some(form)
}

fn reference_joint(
    tables: &CanonicalGuards,
    left: super::Polar,
    right: super::Polar,
    refuted: &mut ahash::AHashSet<[super::Polar; 2]>,
    work: &mut usize,
) -> Option<bool> {
    for (constant, other) in [(left, right), (right, left)] {
        if constant.form <= super::TRUE {
            if (constant.form == super::TRUE) != constant.value {
                return Some(false);
            }
            return Some(other.form > super::TRUE || (other.form == super::TRUE) == other.value);
        }
    }
    if left.form == right.form {
        return Some(left.value == right.value);
    }
    if refuted.contains(&[left, right]) {
        return Some(false);
    }
    *work = work.checked_sub(1)?;
    let decision = tables.top(left.form)?.max(tables.top(right.form)?);
    let (left_no, left_yes) = tables.cofactors(left.form, decision);
    let (right_no, right_yes) = tables.cofactors(right.form, decision);
    for (a, b) in [(left_yes, right_yes), (left_no, right_no)] {
        if reference_joint(
            tables,
            super::Polar {
                form: a,
                value: left.value,
            },
            super::Polar {
                form: b,
                value: right.value,
            },
            refuted,
            work,
        )? {
            return Some(true);
        }
    }
    refuted.insert([left, right]);
    Some(false)
}

/// Sharing preserves the original distinct-subproblem charge, including overlapping siblings.
#[test]
fn shared_proofs_match_the_original_cold_dag_walks() {
    let mut random = Random(0x83bf_e617_2ca9_540d);
    let mut tables = CanonicalGuards::default();
    let mut forms = vec![super::FALSE, super::TRUE];
    for decision in 0..8 {
        let earlier = forms.len();
        for _ in 0..32 {
            forms.push(tables.make(
                decision,
                forms[random.next(earlier)],
                forms[random.next(earlier)],
            ));
        }
    }
    let mut tiny = cold_table(&tables);
    tiny.memo_capacity = 1;
    tiny.composed.certificate_allowance = 0;
    tiny.compatible.certificate_allowance = 0;
    for _ in 0..256 {
        let operands = [
            forms[random.next(forms.len())],
            forms[random.next(forms.len())],
            forms[random.next(forms.len())],
        ];
        let pair = [
            super::Polar {
                form: operands[0],
                value: random.next(2) == 0,
            },
            super::Polar {
                form: operands[1],
                value: random.next(2) == 0,
            },
        ];
        for allowance in [0, 1, 8, 32, 256, super::MAX_CANONICAL_WORK] {
            let mut reference = cold_table(&tables);
            let mut remaining = allowance;
            let result = reference_ite(
                &mut reference,
                operands,
                &mut AHashMap::new(),
                &mut remaining,
            );
            let expected = IteObservation {
                result: result.map(|form| truth_table(&reference, form)),
                remaining,
            };
            sim_assert_eq!(have: observe_ite(&mut tables, operands, allowance), want: expected);
            sim_assert_eq!(have: observe_ite(&mut tiny, operands, allowance), want: expected);
            let [left, right] = pair;
            let mut remaining = allowance;
            let result = reference_joint(
                &reference,
                left,
                right,
                &mut ahash::AHashSet::new(),
                &mut remaining,
            );
            let expected = CompatibilityObservation { result, remaining };
            sim_assert_eq!(have: observe_compatibility(&mut tables, pair, allowance), want: expected);
            sim_assert_eq!(have: observe_compatibility(&mut tiny, pair, allowance), want: expected);
        }
    }
}

/// Compressed ranges preserve every hole and count each work identity once.
#[test]
fn work_range_unions_match_distinct_identity_sets() -> eyre::Result<()> {
    let mut random = Random(0xf19a_507d_62c8_3be4);
    for _ in 0..32 {
        let storage = Rc::new(super::WorkStorage::default());
        let mut ledger = super::WorkSet::default();
        let mut expected = ahash::AHashSet::new();
        for _ in 0..256 {
            let a = u32::try_from(random.next(512)).unwrap_or_default();
            let b = u32::try_from(random.next(512)).unwrap_or_default();
            let first = a.min(b);
            let last = a.max(b).min(first + 16);
            let before = expected.len();
            expected.extend(first..=last);
            let previous = ledger.clone();
            ledger = ledger
                .union(
                    &super::WorkSet::Span(super::WorkSpan {
                        first: super::WorkId(first),
                        last: super::WorkId(last),
                    }),
                    &storage,
                    &mut { usize::MAX },
                )
                .ok_or_eyre("unlimited radix union must complete")?;
            sim_assert_eq!(have: ledger.count(), want: expected.len());
            sim_assert_eq!(have: ledger.count() - previous.count(), want: expected.len() - before);
            for id in 0..512 {
                sim_assert_eq!(have: ledger.contains(super::WorkId(id)), want: expected.contains(&id));
            }
        }
    }
    Ok(())
}

/// Interleaved histories keep dependency storage linear even when every closure is fragmented.
#[test]
fn fragmented_prefixes_share_work_instead_of_copying_transitive_sets() -> eyre::Result<()> {
    let mut table = CanonicalGuards::default();
    let mut prefix = super::TRUE;
    for decision in 0..16_384 {
        let variable = table.make(decision, super::FALSE, super::TRUE);
        let mut allowance = super::MAX_CANONICAL_WORK;
        let next = table
            .ite([variable, prefix, super::FALSE], &mut allowance)
            .ok_or_eyre("a conjunction prefix must complete")?;
        prefix = next;
        let mut allowance = super::MAX_CANONICAL_WORK;
        assert!(
            table
                .ite([prefix, super::FALSE, super::TRUE], &mut allowance)
                .is_some()
        );
        sim_assert_eq!(have: super::MAX_CANONICAL_WORK - allowance, want: usize::try_from(decision + 1).unwrap_or_default());
    }
    assert!(table.composed.entries.len() <= 2 * 16_384);
    let edges = table
        .composed
        .entries
        .iter()
        .filter_map(|entry| entry.work())
        .map(|work| work.children.into_iter().flatten().count())
        .sum::<usize>();
    assert!(edges <= 2 * table.composed.entries.len());
    assert!(table.composed.storage.live.get() <= 32 * table.composed.entries.len());
    Ok(())
}

/// Expanded work certificates have a physical allowance; completed values still share their DAG.
#[test]
fn certificate_exhaustion_keeps_every_completed_composition_shared() -> eyre::Result<()> {
    let mut table = CanonicalGuards {
        composed: super::ProofMemo {
            certificate_allowance: 128,
            ..super::ProofMemo::default()
        },
        ..CanonicalGuards::default()
    };
    let mut left = super::TRUE;
    let mut right = super::TRUE;
    let length = 2_048_u64;
    for decision in 0..length {
        for (arm, offset) in [(&mut left, 0), (&mut right, 1)] {
            let variable = table.make(2 * decision + offset, super::FALSE, super::TRUE);
            *arm = table
                .ite([variable, *arm, super::FALSE], &mut {
                    super::MAX_CANONICAL_WORK
                })
                .ok_or_eyre("a conjunction prefix must complete")?;
            table
                .ite([*arm, super::FALSE, super::TRUE], &mut {
                    super::MAX_CANONICAL_WORK
                })
                .ok_or_eyre("a negated conjunction prefix must complete")?;
        }
    }
    let mut selectors = Vec::new();
    for decision in 0..length {
        selectors.push(table.make(2 * length + decision, left, right));
    }
    let mut dispatch = *selectors.first().ok_or_eyre("selectors are nonempty")?;
    for (offset, &selector) in selectors.iter().skip(1).enumerate() {
        dispatch = table.make(3 * length + u64::try_from(offset)?, dispatch, selector);
    }
    let before = table.composed.storage.live.get();
    let mut remaining = super::MAX_CANONICAL_WORK;
    let result = table
        .ite([dispatch, super::FALSE, super::TRUE], &mut remaining)
        .ok_or_eyre("the dispatcher proof fits its logical allowance")?;
    let charge = usize::try_from(4 * length - 1)?;
    sim_assert_eq!(have: remaining, want: super::MAX_CANONICAL_WORK - charge);
    // Only the root can retain query scratch in addition to quota-charged certificate nodes.
    assert!(
        table.composed.storage.live.get() <= before + table.composed.certificate_allowance + charge
    );
    let mut unexpanded = None;
    for selector in selectors {
        let operands = [selector, super::FALSE, super::TRUE];
        let entry = table
            .composed
            .get(&operands)
            .ok_or_eyre("every completed selector remains shared")?;
        if entry.work().is_some_and(|work| work.fast.is_none()) {
            unexpanded = Some(operands);
        }
    }
    let operands = unexpanded.ok_or_eyre("the physical allowance forces a work-DAG certificate")?;
    let mut cold = cold_table(&table);
    cold.memo_capacity = 0;
    let mut cold_remaining = super::MAX_CANONICAL_WORK;
    let expected = cold.ite(operands, &mut cold_remaining);
    let mut warm_remaining = super::MAX_CANONICAL_WORK;
    sim_assert_eq!(have: table.ite(operands, &mut warm_remaining), want: expected);
    sim_assert_eq!(have: warm_remaining, want: cold_remaining);
    let mut allowance = charge;
    sim_assert_eq!(have: table.ite([dispatch, super::FALSE, super::TRUE], &mut allowance), want: Some(result));
    sim_assert_eq!(have: allowance, want: 0);
    let mut allowance = charge - 1;
    sim_assert_eq!(have: table.ite([dispatch, super::FALSE, super::TRUE], &mut allowance), want: None);
    sim_assert_eq!(have: allowance, want: 0);
    Ok(())
}

/// Sparse unions preserve high identity bits and leave shared older versions unchanged.
#[test]
fn radix_work_sets_preserve_sparse_high_bits_and_shared_versions() -> eyre::Result<()> {
    let mut random = Random(0xf317_459c_6a82_e0bd);
    let storage = Rc::new(super::WorkStorage::default());
    let mut ledger = super::WorkSet::default();
    let mut expected = ahash::AHashSet::new();
    for _ in 0..4_096 {
        let first = u32::try_from(random.next(1 << 22)).unwrap_or_default();
        let last = first + u32::try_from(random.next(32)).unwrap_or_default();
        let previous = ledger.clone();
        let previous_count = expected.len();
        let samples = (0..8)
            .map(|_| {
                let id = u32::try_from(random.next(1 << 22)).unwrap_or_default();
                (id, expected.contains(&id))
            })
            .collect::<Vec<_>>();
        ledger = ledger
            .union(
                &super::WorkSet::Span(super::WorkSpan {
                    first: super::WorkId(first),
                    last: super::WorkId(last),
                }),
                &storage,
                &mut { usize::MAX },
            )
            .ok_or_eyre("unlimited radix union must complete")?;
        expected.extend(first..=last);
        sim_assert_eq!(have: ledger.count(), want: expected.len());
        sim_assert_eq!(have: previous.count(), want: previous_count);
        for (id, was_present) in samples {
            sim_assert_eq!(have: previous.contains(super::WorkId(id)), want: was_present);
            sim_assert_eq!(have: ledger.contains(super::WorkId(id)), want: expected.contains(&id));
        }
        for id in first..=last {
            assert!(ledger.contains(super::WorkId(id)));
        }
        assert!(storage.live.get() < 2 * ledger.count());
    }
    Ok(())
}

/// A full-allowance cold proof fits independently of the test thread's call stack.
#[test]
fn cold_proofs_use_bounded_frames_at_the_full_depth_limit() -> eyre::Result<()> {
    let mut table = CanonicalGuards::default();
    let mut left = table.make(0, super::FALSE, super::TRUE);
    let mut right = table.make(1, super::FALSE, super::TRUE);
    for decision in 2..=u64::try_from(super::MAX_CANONICAL_WORK)? {
        left = table.make(decision, super::FALSE, left);
        right = table.make(decision, super::FALSE, right);
    }
    let mut allowance = super::MAX_CANONICAL_WORK;
    assert!(
        table
            .ite([left, super::FALSE, super::TRUE], &mut allowance)
            .is_some()
    );
    sim_assert_eq!(have: allowance, want: 0);
    let identities = table.composed.entries.len();
    table.composed.clear_certificates();
    let mut allowance = super::MAX_CANONICAL_WORK - 1;
    sim_assert_eq!(have: table.ite([left, super::FALSE, super::TRUE], &mut allowance), want: None);
    sim_assert_eq!(have: allowance, want: 0);
    sim_assert_eq!(have: table.composed.entries.len(), want: identities);
    let polar_left = super::Polar {
        form: left,
        value: true,
    };
    let polar_right = super::Polar {
        form: right,
        value: true,
    };
    let mut allowance = super::MAX_CANONICAL_WORK;
    sim_assert_eq!(have: table.joint(polar_left, polar_right, &mut allowance), want: Some(true));
    sim_assert_eq!(have: allowance, want: 0);
    table.compatible.clear();
    let mut allowance = super::MAX_CANONICAL_WORK - 1;
    sim_assert_eq!(have: table.joint(polar_left, polar_right, &mut allowance), want: None);
    sim_assert_eq!(have: allowance, want: 0);
    Ok(())
}

/// Physical quotas charge fresh branches, preserve shared sources, and debit dropped partial work.
#[test]
fn radix_allocation_quotas_count_shared_branches_once() -> eyre::Result<()> {
    let storage = Rc::new(super::WorkStorage::default());
    let singleton = |id| {
        super::WorkSet::Span(super::WorkSpan {
            first: super::WorkId(id),
            last: super::WorkId(id),
        })
    };
    let retained = singleton(32)
        .union(&singleton(64), &storage, &mut { usize::MAX })
        .ok_or_eyre("the shared source must fit")?;
    sim_assert_eq!(have: storage.live.get(), want: 1);
    let shared = retained.clone();
    sim_assert_eq!(have: storage.live.get(), want: 1);
    drop(shared);
    let mut allowance = 1;
    assert!(
        singleton(0)
            .union(&retained, &storage, &mut allowance)
            .is_none()
    );
    sim_assert_eq!(have: allowance, want: 0);
    sim_assert_eq!(have: storage.live.get(), want: 1);
    assert!(retained.contains(super::WorkId(32)));
    assert!(retained.contains(super::WorkId(64)));
    drop(retained);
    sim_assert_eq!(have: storage.live.get(), want: 0);

    // A uniquely owned branch can change its leaf without allocating a replacement branch.
    let unique = singleton(0)
        .union(&singleton(64), &storage, &mut { usize::MAX })
        .ok_or_eyre("the unique source must fit")?;
    let mut allowance = 0;
    let changed = unique
        .union(&singleton(1), &storage, &mut allowance)
        .ok_or_eyre("in-place reuse consumes no allocation quota")?;
    sim_assert_eq!(have: changed.count(), want: 3);
    sim_assert_eq!(have: storage.live.get(), want: 1);
    sim_assert_eq!(have: allowance, want: 0);
    drop(changed);
    sim_assert_eq!(have: storage.live.get(), want: 0);
    Ok(())
}
