use std::collections::BTreeMap;
use std::rc::Rc;

use color_eyre::eyre;
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use super::{
    CanonicalGuards, FALSE, FormNode, MAX_CANONICAL_NODES, MAX_CANONICAL_WORK, Polar, TRUE,
};
use crate::value::{Arena, Coverage, DecisionEdge, FrozenTest, State, Value, guard::RuntimeGuard};

fn legacy_compose(
    tables: &mut CanonicalGuards,
    decision: u64,
    parts: [Option<&RuntimeGuard>; 3],
) -> Option<u32> {
    let mut forms = [FALSE; 3];
    for (slot, part) in forms.iter_mut().zip(parts) {
        *slot = if let Some(part) = part {
            tables.known(part)?
        } else {
            tables.make(decision, FALSE, TRUE)
        };
    }
    tables.ite(forms, &mut { MAX_CANONICAL_WORK })
}

fn legacy_form(
    tables: &mut CanonicalGuards,
    guard: &RuntimeGuard,
    context: *const super::super::GuardQueries,
) -> Option<u32> {
    let mut pending = vec![(guard, false)];
    while let Some((guard, expanded)) = pending.pop() {
        let RuntimeGuard::Branch(node) = guard else {
            continue;
        };
        if !std::ptr::eq(Rc::as_ptr(&node.queries), context) {
            return None;
        }
        if tables.forms.contains_key(&node.id) {
            continue;
        }
        let parts = [node.test.runtime.as_ref(), Some(&node.yes), Some(&node.no)];
        if !expanded {
            pending.push((guard, true));
            pending.extend(parts.into_iter().flatten().map(|part| (part, false)));
            continue;
        }
        let form = legacy_compose(tables, node.test.id, parts);
        tables.forms.insert(node.id, form);
    }
    tables.known(guard)
}

fn legacy_compatible(
    tables: &mut CanonicalGuards,
    [(left, left_value), (right, right_value)]: [(&RuntimeGuard, bool); 2],
    context: *const super::super::GuardQueries,
) -> Option<bool> {
    tables.start_query();
    let left = legacy_form(tables, left, context)?;
    let right = legacy_form(tables, right, context)?;
    tables.joint(
        Polar {
            form: left,
            value: left_value,
        },
        Polar {
            form: right,
            value: right_value,
        },
        &mut { MAX_CANONICAL_WORK },
    )
}

fn node_shape(node: &FormNode) -> (u64, u32, u32) {
    let FormNode { decision, no, yes } = *node;
    (decision, no, yes)
}

fn assert_same_tables(actual: &CanonicalGuards, expected: &CanonicalGuards) {
    sim_assert_eq!(
        have: actual.nodes.iter().map(node_shape).collect::<Vec<_>>(),
        want: expected.nodes.iter().map(node_shape).collect::<Vec<_>>(),
    );
    let unique = |tables: &CanonicalGuards| {
        tables
            .unique
            .iter()
            .map(|(node, form)| (node_shape(node), *form))
            .collect::<BTreeMap<_, _>>()
    };
    sim_assert_eq!(have: unique(actual), want: unique(expected));
    sim_assert_eq!(have: &actual.forms, want: &expected.forms);
    // Each memoized composition keeps its answer and its full cold charge.
    let composed = |tables: &CanonicalGuards| {
        tables
            .composed
            .keys
            .iter()
            .map(|(operands, id)| {
                let entry = tables.composed.entries.get(id.index());
                let value = entry.and_then(super::WorkEntry::value);
                let cold =
                    entry.and_then(|entry| entry.work()?.cold.map(std::num::NonZeroUsize::get));
                (*operands, (value, cold))
            })
            .collect::<BTreeMap<_, _>>()
    };
    sim_assert_eq!(have: composed(actual), want: composed(expected));
}

fn atom(arena: &Arena) -> (Rc<FrozenTest>, RuntimeGuard) {
    let test = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    let guard = arena.runtime_path(&[DecisionEdge {
        test: Rc::clone(&test),
        taken: true,
    }]);
    (test, guard)
}

#[test]
fn ordered_atoms_preserve_legacy_composition_tables() {
    let state = State::new(Value::nil());
    let arena = &state.heap.arena;
    let context = Rc::as_ptr(&arena.guards.borrow().queries);
    let (lower, low) = atom(arena);
    let (upper, high) = atom(arena);
    let false_guard = RuntimeGuard::False;
    let true_guard = RuntimeGuard::True;
    let mut actual = CanonicalGuards::default();
    let mut expected = CanonicalGuards::default();
    for guard in [&low, &high] {
        sim_assert_eq!(
            have: actual.form(guard, context),
            want: legacy_form(&mut expected, guard, context),
        );
    }

    let fresh = arena.decision(Predicate::False, Predicate::False, Coverage::Partial);
    assert!(!actual.unique.contains_key(&FormNode {
        decision: fresh.id,
        no: FALSE,
        yes: TRUE,
    }));
    for (decision, parts) in [
        (fresh.id, [None, Some(&low), Some(&low)]),
        (upper.id, [None, Some(&true_guard), Some(&false_guard)]),
        (upper.id, [None, Some(&low), Some(&low)]),
        (upper.id, [None, Some(&low), Some(&false_guard)]),
        (upper.id, [None, Some(&low), Some(&false_guard)]),
        (lower.id, [None, Some(&high), Some(&true_guard)]),
        (upper.id, [Some(&low), Some(&high), Some(&false_guard)]),
    ] {
        sim_assert_eq!(
            have: actual.compose(decision, parts),
            want: legacy_compose(&mut expected, decision, parts),
        );
        assert_same_tables(&actual, &expected);
    }
}

#[test]
fn compatibility_preserves_tables_through_resets_aliases_and_repeated_roots() {
    let state = State::new(Value::nil());
    let arena = &state.heap.arena;
    let context = Rc::as_ptr(&arena.guards.borrow().queries);
    let (old_test, old_atom) = atom(arena);
    let mut actual = CanonicalGuards::default();
    let mut expected = CanonicalGuards::default();
    let mut resets = 0;
    for index in 0..100 {
        let (_, atom) = atom(arena);
        let ordered = arena.intersect_runtime_guards(&old_atom, &atom);
        let unordered =
            arena
                .guards
                .borrow_mut()
                .select(Rc::clone(&old_test), atom, RuntimeGuard::False);
        let alias_test = arena.budget_test(ordered.clone());
        let (yes, no) = if index % 2 == 0 {
            (RuntimeGuard::True, RuntimeGuard::False)
        } else {
            (RuntimeGuard::False, RuntimeGuard::True)
        };
        let alias = arena.guards.borrow_mut().select(alias_test, yes, no);
        assert!(matches!(&alias, RuntimeGuard::Branch(node) if node.test.runtime.is_some()));
        for operands in [
            [(&ordered, true), (&unordered, false)],
            [(&ordered, true), (&ordered, false)],
            [(&alias, true), (&old_atom, false)],
            [(&alias, false), (&RuntimeGuard::True, true)],
        ] {
            resets += usize::from(actual.nodes.len() >= MAX_CANONICAL_NODES);
            sim_assert_eq!(
                have: actual.compatible(operands, context),
                want: legacy_compatible(&mut expected, operands, context),
            );
            assert_same_tables(&actual, &expected);
        }
    }
    assert!(resets > 0);
}

#[test]
fn cached_forms_keep_exhaustion_and_check_context_before_identity() -> eyre::Result<()> {
    let local = State::new(Value::nil());
    let foreign = State::new(Value::nil());
    let local_arena = &local.heap.arena;
    let context = Rc::as_ptr(&local_arena.guards.borrow().queries);
    let (_, local_guard) = atom(local_arena);
    let (_, foreign_guard) = atom(&foreign.heap.arena);
    let RuntimeGuard::Branch(node) = &local_guard else {
        eyre::bail!("expected an ordinary atom");
    };
    sim_assert_eq!(have: local_guard.id(), want: foreign_guard.id());
    let mut actual = CanonicalGuards::default();
    let mut expected = CanonicalGuards::default();
    sim_assert_eq!(
        have: actual.form(&local_guard, context),
        want: legacy_form(&mut expected, &local_guard, context),
    );
    sim_assert_eq!(have: actual.form(&foreign_guard, context), want: None);
    sim_assert_eq!(have: legacy_form(&mut expected, &foreign_guard, context), want: None);
    actual.forms.insert(node.id, None);
    expected.forms.insert(node.id, None);
    sim_assert_eq!(have: actual.form(&local_guard, context), want: None);
    sim_assert_eq!(have: legacy_form(&mut expected, &local_guard, context), want: None);
    assert_same_tables(&actual, &expected);
    Ok(())
}
