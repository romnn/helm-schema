use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::guard_algebra::minimize_disjunction_by;
use crate::{ConditionalGuard, Guard, Predicate};

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

    /// Project typed predicate branches to the contract-expressible guard
    /// vocabulary while retaining their disjunction as one row condition.
    #[must_use]
    pub fn from_contract_predicate_disjunction(
        conjunctions: impl IntoIterator<Item = impl IntoIterator<Item = Predicate>>,
    ) -> Self {
        Self::from_guard_disjunction(conjunctions.into_iter().map(|conjunction| {
            let predicates = conjunction.into_iter().collect::<Vec<_>>();
            Predicate::contract_guard_stack(&predicates)
        }))
    }

    /// Retain projected alternatives until their downstream evidence payloads
    /// can be compared; early resolution can erase a nullable or strict arm.
    #[must_use]
    pub fn from_contract_predicate_disjunction_preserving_evidence(
        conjunctions: impl IntoIterator<Item = impl IntoIterator<Item = Predicate>>,
    ) -> Self {
        let mut condition = Self::never();
        for conjunction in conjunctions {
            condition
                .union_preserving_disjuncts(Self::from_contract_predicate_conjunction(conjunction));
        }
        condition
    }

    /// Projects one predicate conjunction into contract guards.
    #[must_use]
    pub fn from_contract_predicate_conjunction(
        predicates: impl IntoIterator<Item = Predicate>,
    ) -> Self {
        Self::from_contract_predicate_disjunction([predicates])
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

    /// Adds alternatives without absorbing subsets that may carry distinct evidence.
    pub fn union_preserving_disjuncts(&mut self, other: Self) {
        // Rows are combined before downstream evidence payloads are known to
        // be equal, so absorption here could discard a more precise payload.
        self.0.extend(other.0);
    }

    /// Union conditions after their evidence payloads are known to be
    /// equal, re-normalizing so duplicate and subsumed disjuncts are
    /// absorbed (unlike [`Self::union_preserving_disjuncts`]).
    pub fn union_absorbing(&mut self, other: Self) {
        *self = Self::from_disjunction(std::mem::take(&mut self.0).into_iter().chain(other.0));
    }

    /// Rewrites every values path and re-normalizes the formula.
    pub fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
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

impl Serialize for GuardDnf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.guard_conjunctions().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for GuardDnf {
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
        match predicate {
            Predicate::True => true,
            Predicate::False => false,
            Predicate::And(predicates) => predicates
                .into_iter()
                .all(|predicate| push(predicate, normalized)),
            predicate => {
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
    Some(normalized.into_iter().collect())
}

fn predicates_are_contradictory(left: &Predicate, right: &Predicate) -> bool {
    if predicates_are_complementary(left, right) {
        return true;
    }

    matches!(
        (left, right),
        (
            Predicate::Guard(Guard::Eq {
                path: left_path,
                value: left_value,
            }),
            Predicate::Guard(Guard::NotEq {
                path: right_path,
                value: right_value,
            })
        ) | (
            Predicate::Guard(Guard::NotEq {
                path: left_path,
                value: left_value,
            }),
            Predicate::Guard(Guard::Eq {
                path: right_path,
                value: right_value,
            })
        ) if left_path == right_path && left_value == right_value
    )
}

fn predicates_are_complementary(left: &Predicate, right: &Predicate) -> bool {
    match (left, right) {
        (predicate, Predicate::Not(negated)) | (Predicate::Not(negated), predicate) => {
            predicate == negated.as_ref()
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "tests/guard_dnf.rs"]
mod tests;
