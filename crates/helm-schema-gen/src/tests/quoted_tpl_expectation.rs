//! The Helm identity contract of action-free `tpl` input over an independently placed raw slot.
//! Unknown template programs retain no lexical claim; the expected contract never reads analyzed output.

use color_eyre::eyre;

#[path = "quoted_tpl_reference.rs"]
mod reference;

use helm_schema_core::{
    ConditionalGuard, ContractSchemaSignals, Guard, Predicate, PredicateKind, PredicateMemo,
    ValuePattern, ValuesPath,
};

pub(super) fn action_free_contract(
    raw: &ContractSchemaSignals,
    key: &str,
    stringified: bool,
) -> eyre::Result<ContractSchemaSignals> {
    if stringified {
        return Ok(formatted_contract(raw, key));
    }
    let path = ValuesPath::parse(key);
    let action_free = Predicate::from(Guard::MatchesPattern {
        path,
        pattern: r"\{\{".to_owned(),
        templated: false,
    })
    .negated();
    let memo = PredicateMemo::new();
    let mut clauses = raw.terminal_clauses().to_vec();
    for clause in &mut clauses {
        let mut expected = Vec::new();
        for original in clause.iter() {
            let predicate = gate_language(&original.predicate(), &action_free, &memo);
            match predicate.kind() {
                PredicateKind::And(items) => {
                    expected.extend(items.iter().map(guard).collect::<eyre::Result<Vec<_>>>()?);
                }
                _ => expected.push(guard(&predicate)?),
            }
        }
        *clause = expected;
        ConditionalGuard::canonicalize_conjunction(clause);
    }
    clauses.sort();
    clauses.dedup();
    Ok(ContractSchemaSignals::new(
        raw.schema_evidence_by_value_path().clone(),
        clauses,
    ))
}

fn gate_language(
    predicate: &Predicate,
    action_free: &Predicate,
    memo: &PredicateMemo,
) -> Predicate {
    match predicate.kind() {
        PredicateKind::Guard(Guard::MatchesPattern { .. } | Guard::MatchesValue { .. }) => {
            memo.normalize(Predicate::all(vec![predicate.clone(), action_free.clone()]))
        }
        PredicateKind::Not(inner) if has_language(inner) => {
            memo.normalize(Predicate::all(vec![predicate.clone(), action_free.clone()]))
        }
        PredicateKind::And(items) => memo.normalize(Predicate::all(
            items
                .iter()
                .map(|item| gate_language(item, action_free, memo))
                .collect(),
        )),
        PredicateKind::Or(items) => memo.normalize(Predicate::Or(
            items
                .iter()
                .map(|item| gate_language(item, action_free, memo))
                .collect(),
        )),
        _ => predicate.clone(),
    }
}

fn has_language(predicate: &Predicate) -> bool {
    match predicate.kind() {
        PredicateKind::Guard(Guard::MatchesPattern { .. } | Guard::MatchesValue { .. }) => true,
        PredicateKind::Not(inner) => has_language(inner),
        PredicateKind::And(items) | PredicateKind::Or(items) => items.iter().any(has_language),
        _ => false,
    }
}

fn guard(predicate: &Predicate) -> eyre::Result<ConditionalGuard> {
    Ok(match predicate.kind() {
        PredicateKind::Guard(value) => ConditionalGuard::try_from(value)
            .map_err(|()| eyre::eyre!("unsupported oracle guard: {value:?}"))?,
        PredicateKind::Not(inner) => ConditionalGuard::Not(guard(inner)?.into()),
        PredicateKind::And(items) => ConditionalGuard::AllOf(
            items
                .iter()
                .map(guard)
                .collect::<eyre::Result<Vec<_>>>()?
                .into(),
        ),
        PredicateKind::Or(items) => ConditionalGuard::AnyOf(
            items
                .iter()
                .map(guard)
                .collect::<eyre::Result<Vec<_>>>()?
                .into(),
        ),
        PredicateKind::True => ConditionalGuard::AllOf(Vec::new().into()),
        PredicateKind::False => ConditionalGuard::AnyOf(Vec::new().into()),
        PredicateKind::Approximate { .. } => eyre::bail!("oracle contract must be exact"),
    })
}

fn formatted_contract(raw: &ContractSchemaSignals, key: &str) -> ContractSchemaSignals {
    use reference::LexicalState;
    let path = ValuesPath::parse(key);
    let memo = PredicateMemo::new();
    let nil = Predicate::Or(vec![
        Predicate::from(Guard::Eq {
            path: path.clone(),
            value: helm_schema_core::GuardValue::Null,
        }),
        Predicate::from(Guard::Absent { path: path.clone() }),
    ]);
    let action_free = memo.normalize(Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::MatchesValue {
            path: path.clone(),
            pattern: ValuePattern::TemplateActionFree,
        }),
    ]));
    let mut states = reference::formatted(&path, &memo);
    for condition in states.values_mut() {
        *condition = memo.normalize(condition.clone());
        *condition = memo.normalize(Predicate::all(vec![
            condition.clone(),
            nil.clone().negated(),
        ]));
    }
    let states = states
        .into_iter()
        .map(|(state, condition)| {
            (
                state,
                memo.normalize(Predicate::all(vec![action_free.clone(), condition])),
            )
        })
        .collect();
    let mut states = reference::literal(states, "\"", &memo);
    let mut invalid = Vec::new();
    for state in [
        LexicalState::Invalid,
        LexicalState::DirectiveInvalid,
        LexicalState::BlockInvalid,
        LexicalState::ClosedCollection,
        LexicalState::CollectionInvalid,
    ] {
        invalid.push(states.remove(&state).unwrap_or(Predicate::False));
    }
    let pending = states
        .into_iter()
        .filter(|(state, _)| state.is_unterminated())
        .map(|(_, condition)| condition)
        .collect();
    let failure = memo.normalize(Predicate::Or(vec![
        Predicate::Or(invalid),
        Predicate::Or(pending),
    ]));
    let mut expected = helm_schema_ir::ContractIr::from_contract_uses(Vec::new());
    expected.add_terminal_fail_condition(failure);
    let expected = expected.finalize().into_schema_signals();
    ContractSchemaSignals::new(
        raw.schema_evidence_by_value_path().clone(),
        expected.terminal_clauses().to_vec(),
    )
}
