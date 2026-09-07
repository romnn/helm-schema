use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::guard_algebra::minimize_disjunction_by;
use crate::{ConditionalGuard, Guard, Predicate, PredicateKind};

/// Disjunction of conjunctions of typed predicates.
///
/// Construction removes impossible conjunctions and normalizes exact
/// complementary resolution, absorption, and deduplication.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GuardDnf(BTreeSet<BTreeSet<Predicate>>);

impl Default for GuardDnf {
    fn default() -> Self {
        Self::unconditional()
    }
}

impl GuardDnf {
    /// Returns the formula that accepts every input.
    #[must_use]
    pub fn unconditional() -> Self {
        Self(BTreeSet::from([BTreeSet::new()]))
    }

    /// Returns the formula that accepts no input.
    #[must_use]
    pub fn never() -> Self {
        Self(BTreeSet::new())
    }

    /// Builds a normalized DNF from one predicate conjunction.
    #[must_use]
    pub fn from_conjunction(predicates: impl IntoIterator<Item = Predicate>) -> Self {
        Self::from_disjunction([predicates])
    }

    /// Builds a normalized DNF from one guard conjunction.
    #[must_use]
    pub fn from_guards(guards: impl IntoIterator<Item = Guard>) -> Self {
        Self::from_conjunction(guards.into_iter().map(Predicate::from))
    }

    /// Builds a normalized DNF from guard conjunction alternatives.
    #[must_use]
    pub fn from_guard_disjunction(
        conjunctions: impl IntoIterator<Item = impl IntoIterator<Item = Guard>>,
    ) -> Self {
        Self::from_disjunction(
            conjunctions
                .into_iter()
                .map(|guards| guards.into_iter().map(Predicate::from)),
        )
    }

    /// Canonicalizes a disjunction of conditional-guard conjunctions.
    #[must_use]
    pub fn normalize_conditional_guard_disjunction(
        conjunctions: impl IntoIterator<Item = impl IntoIterator<Item = ConditionalGuard>>,
    ) -> Vec<Vec<ConditionalGuard>> {
        let keys = conjunctions
            .into_iter()
            .map(|conjunction| {
                let mut key = conjunction.into_iter().collect::<Vec<_>>();
                key.sort();
                key.dedup();
                key
            })
            .collect();
        minimize_disjunction_by(keys, crate::guard_algebra::guards_are_complementary)
    }

    /// Builds and minimizes a DNF from predicate conjunction alternatives.
    #[must_use]
    pub fn from_disjunction(
        conjunctions: impl IntoIterator<Item = impl IntoIterator<Item = Predicate>>,
    ) -> Self {
        let keys = conjunctions
            .into_iter()
            .filter_map(normalize_conjunction)
            .collect::<Vec<_>>();
        let keys = minimize_disjunction_by(keys, predicates_are_complementary);
        Self(
            keys.into_iter()
                .map(|key| key.into_iter().collect())
                .collect(),
        )
    }

    /// Reports whether the formula accepts every input.
    #[must_use]
    pub fn is_unconditional(&self) -> bool {
        self.0.contains(&BTreeSet::new())
    }

    /// Reports whether the formula accepts no input.
    #[must_use]
    pub fn is_never(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns normalized predicate conjunctions in stable order.
    #[must_use]
    pub fn disjuncts(&self) -> &BTreeSet<BTreeSet<Predicate>> {
        &self.0
    }

    /// Projects each predicate conjunction into serializable contract guards.
    #[must_use]
    pub fn guard_conjunctions(&self) -> Vec<Vec<Guard>> {
        let mut seen = BTreeSet::new();
        let mut projected = Vec::new();
        for conjunction in &self.0 {
            // Approximate predicates remain in the in-memory DNF consumed by
            // schema inference, where they force abstention. The serialized
            // inspection format cannot represent them, but it can retain the
            // exact ambient guards that still explain where the row lives.
            let mut guards =
                Predicate::contract_guard_stack(&conjunction.iter().cloned().collect::<Vec<_>>());
            Guard::canonicalize_all(&mut guards);
            if seen.insert(guards.clone()) {
                projected.push(guards);
            }
        }
        projected
    }

    /// Returns the sole projected guard conjunction, if exactly one exists.
    #[must_use]
    pub fn single_guard_conjunction(&self) -> Option<Vec<Guard>> {
        let [guards] = self.guard_conjunctions().try_into().ok()?;
        Some(guards)
    }

    /// Returns the conjunction of this formula and `other`.
    #[must_use]
    pub fn conjoined(&self, other: &Self) -> Self {
        Self::from_disjunction(self.0.iter().flat_map(|left| {
            other.0.iter().map(|right| {
                left.iter()
                    .chain(right)
                    .cloned()
                    .collect::<Vec<Predicate>>()
            })
        }))
    }

    /// Conjoins this formula with one guard conjunction.
    #[must_use]
    pub fn conjoined_with_guards(&self, guards: impl IntoIterator<Item = Guard>) -> Self {
        self.conjoined(&Self::from_guards(guards))
    }

    /// Union conditions after their evidence payloads are known to be equal,
    /// re-normalizing so duplicate and subsumed disjuncts are absorbed.
    pub fn union_absorbing(&mut self, other: Self) {
        let mut other_disjuncts = other.0;
        let Some(incoming) = other_disjuncts.pop_first() else {
            return;
        };
        if !other_disjuncts.is_empty()
            || self
                .0
                .iter()
                .any(|existing| conjunctions_resolve_complementary(existing, &incoming))
        {
            *self = Self::from_disjunction(
                std::mem::take(&mut self.0)
                    .into_iter()
                    .chain(std::iter::once(incoming))
                    .chain(other_disjuncts),
            );
            return;
        }
        if self.0.iter().any(|existing| existing.is_subset(&incoming)) {
            return;
        }
        self.0.retain(|existing| !incoming.is_subset(existing));
        self.0.insert(incoming);
    }

    /// Rewrites every values path and re-normalizes the formula.
    pub fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(crate::ValuesPath) -> crate::ValuesPath,
    {
        *self =
            Self::from_disjunction(std::mem::take(&mut self.0).into_iter().map(|conjunction| {
                conjunction
                    .into_iter()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect::<Vec<_>>()
            }));
    }
}

fn conjunctions_resolve_complementary(
    left: &BTreeSet<Predicate>,
    right: &BTreeSet<Predicate>,
) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut left_difference = left.difference(right);
    let Some(left_extra) = left_difference.next() else {
        return false;
    };
    if left_difference.next().is_some() {
        return false;
    }
    let mut right_difference = right.difference(left);
    let Some(right_extra) = right_difference.next() else {
        return false;
    };
    right_difference.next().is_none() && predicates_are_complementary(left_extra, right_extra)
}

impl Serialize for GuardDnf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.guard_conjunctions().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GuardDnf {
    /// Reconstructs only the public guard projection written by `Serialize`.
    /// Opaque predicate details are deliberately not a round-trip wire format.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let conjunctions = Vec::<Vec<Guard>>::deserialize(deserializer)?;
        Ok(Self::from_guard_disjunction(conjunctions))
    }
}

fn normalize_conjunction(
    predicates: impl IntoIterator<Item = Predicate>,
) -> Option<Vec<Predicate>> {
    fn push(predicate: Predicate, normalized: &mut BTreeSet<Predicate>) -> bool {
        match predicate.kind() {
            PredicateKind::True => true,
            PredicateKind::False => false,
            PredicateKind::And(predicates) => predicates
                .iter()
                .cloned()
                .all(|predicate| push(predicate, normalized)),
            PredicateKind::Or(predicates) if disjunction_is_tautology(predicates) => true,
            _ => {
                if normalized
                    .iter()
                    .any(|other| predicates_are_contradictory(&predicate, other))
                {
                    return false;
                }
                normalized.insert(predicate);
                true
            }
        }
    }

    let mut normalized = BTreeSet::new();
    for predicate in predicates {
        if !push(predicate, &mut normalized) {
            return None;
        }
    }
    let absorbed_disjunctions = normalized
        .iter()
        .filter(|predicate| match predicate.kind() {
            PredicateKind::Or(alternatives) => alternatives
                .iter()
                .any(|alternative| normalized.contains(alternative)),
            _ => false,
        })
        .cloned()
        .collect::<Vec<_>>();
    for predicate in absorbed_disjunctions {
        normalized.remove(&predicate);
    }
    let exact = Predicate::all(
        normalized
            .iter()
            .filter(|predicate| !predicate.contains_approximation())
            .cloned()
            .collect(),
    );
    normalized.retain(|predicate| {
        let PredicateKind::Approximate {
            sound_subset: Some(sound_subset),
            ..
        } = predicate.kind()
        else {
            return true;
        };
        !crate::predicate_bdd::exact_implies(&exact, sound_subset)
    });
    Some(normalized.into_iter().collect())
}

fn disjunction_is_tautology(predicates: &[Predicate]) -> bool {
    predicates.iter().any(|predicate| {
        predicates
            .iter()
            .any(|other| predicates_are_complementary(predicate, other))
    })
}

fn predicates_are_contradictory(left: &Predicate, right: &Predicate) -> bool {
    if predicates_are_complementary(left, right) {
        return true;
    }

    if matches!(
        (left.kind(), right.kind()),
        (
            PredicateKind::Guard(Guard::Eq {
                path: left_path,
                value: left_value,
            }),
            PredicateKind::Guard(Guard::NotEq {
                path: right_path,
                value: right_value,
            })
        ) | (
            PredicateKind::Guard(Guard::NotEq {
                path: left_path,
                value: left_value,
            }),
            PredicateKind::Guard(Guard::Eq {
                path: right_path,
                value: right_value,
            })
        ) if left_path == right_path && left_value == right_value
    ) {
        return true;
    }

    match (left.kind(), right.kind()) {
        (
            PredicateKind::Guard(Guard::Eq {
                path: left_path,
                value: left_value,
            }),
            PredicateKind::Guard(Guard::Eq {
                path: right_path,
                value: right_value,
            }),
        ) => left_path == right_path && left_value != right_value,
        (
            PredicateKind::Guard(Guard::MatchesPattern {
                path: pattern_path, ..
            }),
            PredicateKind::Guard(Guard::Eq {
                path: value_path,
                value,
            }),
        )
        | (
            PredicateKind::Guard(Guard::Eq {
                path: value_path,
                value,
            }),
            PredicateKind::Guard(Guard::MatchesPattern {
                path: pattern_path, ..
            }),
        ) => pattern_path == value_path && !matches!(value, crate::GuardValue::String(_)),
        (
            PredicateKind::Guard(Guard::TypeIs {
                path: type_path,
                schema_type,
            }),
            PredicateKind::Guard(Guard::Eq {
                path: value_path,
                value,
            }),
        )
        | (
            PredicateKind::Guard(Guard::Eq {
                path: value_path,
                value,
            }),
            PredicateKind::Guard(Guard::TypeIs {
                path: type_path,
                schema_type,
            }),
        ) => type_path == value_path && !guard_value_has_schema_type(value, schema_type),
        _ => false,
    }
}

fn guard_value_has_schema_type(value: &crate::GuardValue, schema_type: &str) -> bool {
    match value {
        crate::GuardValue::String(_) => schema_type == "string",
        crate::GuardValue::Bool(_) => schema_type == "boolean",
        crate::GuardValue::Int(_) => matches!(schema_type, "integer" | "number"),
        crate::GuardValue::Float(_) => schema_type == "number",
        crate::GuardValue::Null => schema_type == "null",
    }
}

fn predicates_are_complementary(left: &Predicate, right: &Predicate) -> bool {
    if let PredicateKind::Not(negated) = right.kind() {
        return left == negated;
    }
    matches!(left.kind(), PredicateKind::Not(negated) if negated == right)
}

#[cfg(test)]
#[path = "tests/guard_dnf.rs"]
mod tests;
