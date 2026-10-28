//! Exact and partial scalar semantics for symbolic Helm evaluation.
//!
//! Scalar values retain their branch predicates and runtime transforms so
//! truthiness, equality, pattern matching, and semantic-version comparison
//! all consume one typed representation.

use std::collections::BTreeSet;

use helm_schema_core::{Predicate, PredicateMemo};

/// Exact scalar content carried by a canonical value leaf.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarContent {
    /// A concrete Go-template scalar.
    Literal(helm_schema_core::GuardValue),
    /// A raw values-path identity before template rendering.
    Identity(helm_schema_core::ValuesPath),
    /// Text already rendered by a helper body.
    Rendered(Vec<ScalarRenderPart>),
    /// Output of `printf "%s"` over one raw scalar identity.
    ///
    /// Non-string inputs render Go's non-empty format-mismatch spelling, so
    /// only the empty-output preimage is exact without modeling that grammar.
    PrintfStringIdentity(helm_schema_core::ValuesPath),
    /// The number of segments produced by splitting one scalar value on a
    /// literal separator.
    SplitLength {
        value: Box<ScalarContent>,
        separator: String,
    },
    /// Go's `len` of one scalar value: the BYTE count of a string. JSON
    /// Schema counts characters, so character and byte bounds differ.
    Length(Box<ScalarContent>),
}

/// One exact contribution to helper-rendered scalar text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarRenderPart {
    Text(String),
    Identity {
        path: helm_schema_core::ValuesPath,
        /// The text is Go's total scalar rendering of the raw value. When
        /// false, an earlier string consumer has already restricted the
        /// runtime input to strings.
        stringified: bool,
        lexical_escapes: BTreeSet<crate::helper_meta::LexicalEscape>,
    },
}

impl ScalarContent {
    fn rendered_parts(&self) -> Option<Vec<ScalarRenderPart>> {
        let parts = match self {
            Self::Literal(value) => {
                let text = match value {
                    helm_schema_core::GuardValue::String(text)
                    | helm_schema_core::GuardValue::Float(text) => text.clone(),
                    helm_schema_core::GuardValue::Bool(value) => value.to_string(),
                    helm_schema_core::GuardValue::Int(value) => value.to_string(),
                    helm_schema_core::GuardValue::Null => return None,
                };
                vec![ScalarRenderPart::Text(text)]
            }
            Self::Identity(path) => vec![ScalarRenderPart::Identity {
                path: path.clone(),
                stringified: true,
                lexical_escapes: BTreeSet::new(),
            }],
            Self::Rendered(parts) => parts.clone(),
            Self::PrintfStringIdentity(_) | Self::SplitLength { .. } | Self::Length(_) => {
                return None;
            }
        };
        Some(parts)
    }
}

pub(crate) fn any_predicates_with_memo(
    predicates: Vec<helm_schema_core::Predicate>,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(helm_schema_core::Predicate::Or(predicates))
}

pub(crate) fn conjoin_predicates_with_memo(
    left: helm_schema_core::Predicate,
    right: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> Option<helm_schema_core::Predicate> {
    let predicate = memo.normalize(helm_schema_core::Predicate::And(vec![left, right]));
    (predicate != helm_schema_core::Predicate::False).then_some(predicate)
}

fn normalized_factored(
    predicate: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(predicate)
}

fn factored_negated(
    predicate: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(helm_schema_core::Predicate::Not(Box::new(predicate)))
}

/// Sound knowledge about the states in which a value is Helm-truthy or
/// Helm-falsy.
///
/// Partial conditions retain independently proven subsets of both polarities.
/// Consumers that require a complement still use [`Self::predicate`], which
/// exposes only an exact condition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum TruthCondition {
    /// Neither polarity has a known non-empty subset.
    #[default]
    Unknown,
    /// Sound subsets of the truthy and falsy domains, which need not be
    /// exhaustive.
    Partial {
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
    },
    /// The predicate is exact in both positive and negative polarity.
    Exact(helm_schema_core::Predicate),
}

impl TruthCondition {
    pub(crate) fn exact(predicate: helm_schema_core::Predicate) -> Self {
        Self::exact_with_memo(predicate, &PredicateMemo::new())
    }

    pub(crate) fn exact_with_memo(
        predicate: helm_schema_core::Predicate,
        memo: &PredicateMemo,
    ) -> Self {
        if predicate.contains_approximation() {
            Self::from_predicate_with_memo(predicate, memo)
        } else {
            Self::Exact(normalized_factored(predicate, memo))
        }
    }

    pub(crate) fn from_predicate(predicate: helm_schema_core::Predicate) -> Self {
        Self::from_predicate_with_memo(predicate, &PredicateMemo::new())
    }

    pub(crate) fn from_predicate_with_memo(
        predicate: helm_schema_core::Predicate,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        if !predicate.contains_approximation() {
            return Self::exact_with_memo(predicate, memo);
        }
        match predicate.kind() {
            helm_schema_core::PredicateKind::Approximate {
                marker: _,
                paths: _,
                role: _,
                sound_subset,
            } => {
                let when_true = sound_subset.cloned().unwrap_or(Predicate::False);
                Self::from_subsets_with_memo(when_true, Predicate::False, false, memo)
            }
            helm_schema_core::PredicateKind::Not(inner) => {
                Self::from_predicate_with_memo(inner.clone(), memo).negated_with_memo(memo)
            }
            helm_schema_core::PredicateKind::And(predicates) => Self::all_with_memo(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| Self::from_predicate_with_memo(predicate, memo)),
                memo,
            ),
            helm_schema_core::PredicateKind::Or(predicates) => Self::any_with_memo(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| Self::from_predicate_with_memo(predicate, memo)),
                memo,
            ),
            helm_schema_core::PredicateKind::True
            | helm_schema_core::PredicateKind::False
            | helm_schema_core::PredicateKind::Guard(_) => Self::Exact(predicate),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_subsets(
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
        complete: bool,
    ) -> Self {
        Self::from_subsets_with_memo(when_true, when_false, complete, &PredicateMemo::new())
    }

    pub(crate) fn from_subsets_with_memo(
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
        complete: bool,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let when_true = normalized_factored(when_true, memo);
        let when_false = normalized_factored(when_false, memo);
        let disjoint = memo.exactly_implies(
            &Predicate::all(vec![when_true.clone(), when_false.clone()]),
            &Predicate::False,
        );
        let exhaustive = memo.exactly_implies(
            &Predicate::True,
            &Predicate::Or(vec![when_true.clone(), when_false.clone()]),
        );
        if complete && disjoint && exhaustive {
            return Self::Exact(when_true);
        }
        if when_true == Predicate::True {
            return Self::Exact(Predicate::True);
        }
        if when_false == Predicate::True {
            return Self::Exact(Predicate::False);
        }
        if when_true == Predicate::False && when_false == Predicate::False {
            return Self::Unknown;
        }
        Self::Partial {
            when_true,
            when_false,
        }
    }

    pub(crate) fn predicate(&self) -> Option<&helm_schema_core::Predicate> {
        match self {
            Self::Unknown | Self::Partial { .. } => None,
            Self::Exact(predicate) => Some(predicate),
        }
    }

    pub(crate) fn when_true(&self) -> helm_schema_core::Predicate {
        match self {
            Self::Unknown => helm_schema_core::Predicate::False,
            Self::Partial { when_true, .. } | Self::Exact(when_true) => when_true.clone(),
        }
    }

    pub(crate) fn when_false(&self) -> helm_schema_core::Predicate {
        self.when_false_with_memo(&PredicateMemo::new())
    }

    pub(crate) fn when_false_with_memo(&self, memo: &PredicateMemo) -> helm_schema_core::Predicate {
        match self {
            Self::Unknown => helm_schema_core::Predicate::False,
            Self::Partial { when_false, .. } => when_false.clone(),
            Self::Exact(when_true) => factored_negated(when_true.clone(), memo),
        }
    }

    pub(crate) fn negated_with_memo(&self, memo: &PredicateMemo) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Partial {
                when_true,
                when_false,
            } => Self::from_subsets_with_memo(when_false.clone(), when_true.clone(), false, memo),
            Self::Exact(predicate) => Self::Exact(factored_negated(predicate.clone(), memo)),
        }
    }

    pub(crate) fn all_with_memo(
        conditions: impl IntoIterator<Item = Self>,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let conditions = conditions.into_iter().collect::<Vec<_>>();
        if conditions.is_empty() {
            return Self::Exact(Predicate::True);
        }
        if conditions
            .iter()
            .any(|condition| condition.predicate() == Some(&Predicate::False))
        {
            return Self::Exact(Predicate::False);
        }
        let complete = conditions
            .iter()
            .all(|condition| condition.predicate().is_some());
        let when_true = conjoin_many(conditions.iter().map(Self::when_true), memo);
        let when_false = any_predicates_with_memo(
            conditions
                .iter()
                .map(|condition| condition.when_false_with_memo(memo))
                .collect(),
            memo,
        );
        Self::from_subsets_with_memo(when_true, when_false, complete, memo)
    }

    pub(crate) fn any_with_memo(
        conditions: impl IntoIterator<Item = Self>,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let conditions = conditions.into_iter().collect::<Vec<_>>();
        if conditions.is_empty() {
            return Self::Exact(Predicate::False);
        }
        if conditions
            .iter()
            .any(|condition| condition.predicate() == Some(&Predicate::True))
        {
            return Self::Exact(Predicate::True);
        }
        let complete = conditions
            .iter()
            .all(|condition| condition.predicate().is_some());
        let when_true =
            any_predicates_with_memo(conditions.iter().map(Self::when_true).collect(), memo);
        let when_false = conjoin_many(
            conditions
                .iter()
                .map(|condition| condition.when_false_with_memo(memo)),
            memo,
        );
        Self::from_subsets_with_memo(when_true, when_false, complete, memo)
    }
}

fn conjoin_many(
    predicates: impl IntoIterator<Item = helm_schema_core::Predicate>,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    let mut condition = helm_schema_core::Predicate::True;
    for predicate in predicates {
        let Some(joined) = conjoin_predicates_with_memo(condition, predicate, memo) else {
            return helm_schema_core::Predicate::False;
        };
        condition = joined;
    }
    condition
}

/// Rendered scalar content read from one executed emission.
pub(crate) struct ExecutedScalarView {
    projection: super::ScalarValueDispatch,
}

impl super::ExecutedSite {
    pub(crate) fn scalar_view(&self) -> Option<ExecutedScalarView> {
        self.rendered_scalar()
            .map(|projection| ExecutedScalarView { projection })
    }
}

impl ExecutedScalarView {
    /// Maps known renderable arms without asserting exhaustive coverage.
    /// Returns `None` when a retained content term cannot be rendered.
    pub(crate) fn map_rendered_arms<T>(
        &self,
        mut map: impl FnMut(&Predicate, &[ScalarRenderPart]) -> T,
    ) -> Option<Vec<T>> {
        let Self { projection } = self;
        let content = projection.content()?;
        let mut arms = Vec::with_capacity(content.arms().count());
        for (condition, value) in content.arms() {
            if *condition == Predicate::False {
                continue;
            }
            arms.push(map(condition, &value.rendered_parts()?));
        }
        Some(arms)
    }
}
