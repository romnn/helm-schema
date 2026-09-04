use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Deref;
use std::sync::Arc;

use crate::{Guard, GuardValue, ValuesPath};

/// How an inexact predicate participates in later semantic projection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ApproximationRole {
    /// Ordinary control flow whose exact relation is unavailable.
    #[default]
    Control,
    /// A sound subset identifies when one candidate supplies an expression's
    /// returned value.
    OutputSelection,
}

/// Borrowed shape of a typed Boolean formula.
#[derive(Clone, Copy, Debug)]
pub enum PredicateKind<'a> {
    /// Formula that holds for every input.
    True,
    /// Formula that holds for no input.
    False,
    /// A control condition whose exact relation could not be lowered.
    ///
    /// The paths remain available for diagnostics and conservative attribution, but consumers
    /// must not turn this marker into a narrowing schema condition.
    Approximate {
        /// Stable description of the expression shape that could not be lowered.
        marker: &'a str,
        /// Values paths mentioned by the unlowerable expression.
        paths: &'a BTreeSet<ValuesPath>,
        /// Whether the subset describes ordinary execution or returned-value
        /// selection.
        role: ApproximationRole,
        /// A predicate that IMPLIES the real condition (a sound subset).
        /// Usable only in POSITIVE polarity where firing less often is safe
        /// — a fail-arm's outer condition — never through a negation, which
        /// would invert the containment.
        sound_subset: Option<&'a Predicate>,
    },
    /// Exactly lowerable atomic guard.
    Guard(&'a Guard),
    /// Logical negation of a predicate.
    Not(&'a Predicate),
    /// Conjunction of every enclosed predicate.
    And(&'a [Predicate]),
    /// Disjunction of the enclosed predicates.
    Or(&'a [Predicate]),
}

/// Typed Boolean formula recovered from template control flow.
#[derive(Clone)]
pub struct Predicate(PredicateStorage);

#[derive(Clone)]
enum PredicateStorage {
    True,
    False,
    Shared(Arc<PredicateNode>),
}

struct PredicateNode {
    kind: PredicateNodeKind,
    structural_hash: u64,
    contains_approximation: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum PredicateNodeKind {
    Approximate {
        marker: String,
        paths: BTreeSet<ValuesPath>,
        role: ApproximationRole,
        sound_subset: Option<Predicate>,
    },
    Guard(Guard),
    Not(Predicate),
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
}

impl PredicateNode {
    fn new(kind: PredicateNodeKind) -> Self {
        let mut hasher = DefaultHasher::new();
        hash_predicate_node(&kind, &mut hasher);
        let contains_approximation = match &kind {
            PredicateNodeKind::Approximate { .. } => true,
            PredicateNodeKind::Guard(_) => false,
            PredicateNodeKind::Not(predicate) => predicate.contains_approximation(),
            PredicateNodeKind::And(predicates) | PredicateNodeKind::Or(predicates) => {
                predicates.iter().any(Predicate::contains_approximation)
            }
        };
        Self {
            kind,
            structural_hash: hasher.finish(),
            contains_approximation,
        }
    }
}

fn hash_predicate_node(kind: &PredicateNodeKind, hasher: &mut DefaultHasher) {
    match kind {
        PredicateNodeKind::Approximate {
            marker,
            paths,
            role,
            sound_subset,
        } => {
            2_u8.hash(hasher);
            marker.hash(hasher);
            paths.hash(hasher);
            role.hash(hasher);
            sound_subset
                .as_ref()
                .map(Predicate::structural_hash)
                .hash(hasher);
        }
        PredicateNodeKind::Guard(guard) => {
            3_u8.hash(hasher);
            guard.hash(hasher);
        }
        PredicateNodeKind::Not(predicate) => {
            4_u8.hash(hasher);
            predicate.structural_hash().hash(hasher);
        }
        PredicateNodeKind::And(predicates) => {
            5_u8.hash(hasher);
            predicates.len().hash(hasher);
            for predicate in predicates {
                predicate.structural_hash().hash(hasher);
            }
        }
        PredicateNodeKind::Or(predicates) => {
            6_u8.hash(hasher);
            predicates.len().hash(hasher);
            for predicate in predicates {
                predicate.structural_hash().hash(hasher);
            }
        }
    }
}

impl PartialEq for PredicateNode {
    fn eq(&self, other: &Self) -> bool {
        self.structural_hash == other.structural_hash && self.kind == other.kind
    }
}

impl Eq for PredicateNode {}

impl Predicate {
    /// Formula that holds for every input.
    #[expect(
        non_upper_case_globals,
        reason = "preserves the former enum constructor API"
    )]
    pub const True: Self = Self(PredicateStorage::True);

    /// Formula that holds for no input.
    #[expect(
        non_upper_case_globals,
        reason = "preserves the former enum constructor API"
    )]
    pub const False: Self = Self(PredicateStorage::False);

    /// Creates a control condition whose exact relation could not be lowered.
    #[must_use]
    #[expect(non_snake_case, reason = "preserves the former enum constructor API")]
    pub fn Approximate(
        marker: String,
        paths: BTreeSet<ValuesPath>,
        role: ApproximationRole,
        sound_subset: Option<Box<Self>>,
    ) -> Self {
        Self::shared(PredicateNodeKind::Approximate {
            marker,
            paths,
            role,
            sound_subset: sound_subset.map(|predicate| *predicate),
        })
    }

    /// Creates an exactly lowerable atomic guard.
    #[must_use]
    #[expect(non_snake_case, reason = "preserves the former enum constructor API")]
    pub fn Guard(guard: Guard) -> Self {
        Self::shared(PredicateNodeKind::Guard(guard))
    }

    /// Creates the logical negation of a predicate.
    #[must_use]
    #[expect(non_snake_case, reason = "preserves the former enum constructor API")]
    pub fn Not(inner: impl Into<Box<Self>>) -> Self {
        Self::shared(PredicateNodeKind::Not(*inner.into()))
    }

    /// Creates a conjunction of predicates without normalizing it.
    #[must_use]
    #[expect(non_snake_case, reason = "preserves the former enum constructor API")]
    pub fn And(predicates: Vec<Self>) -> Self {
        Self::shared(PredicateNodeKind::And(predicates))
    }

    /// Creates a disjunction of predicates without normalizing it.
    #[must_use]
    #[expect(non_snake_case, reason = "preserves the former enum constructor API")]
    pub fn Or(predicates: Vec<Self>) -> Self {
        Self::shared(PredicateNodeKind::Or(predicates))
    }

    /// Returns the formula's exhaustive borrowed shape.
    #[must_use]
    pub fn kind(&self) -> PredicateKind<'_> {
        match &self.0 {
            PredicateStorage::True => PredicateKind::True,
            PredicateStorage::False => PredicateKind::False,
            PredicateStorage::Shared(node) => match &node.kind {
                PredicateNodeKind::Approximate {
                    marker,
                    paths,
                    role,
                    sound_subset,
                } => PredicateKind::Approximate {
                    marker,
                    paths,
                    role: *role,
                    sound_subset: sound_subset.as_ref(),
                },
                PredicateNodeKind::Guard(guard) => PredicateKind::Guard(guard),
                PredicateNodeKind::Not(inner) => PredicateKind::Not(inner),
                PredicateNodeKind::And(predicates) => PredicateKind::And(predicates),
                PredicateNodeKind::Or(predicates) => PredicateKind::Or(predicates),
            },
        }
    }

    fn shared(kind: PredicateNodeKind) -> Self {
        Self(PredicateStorage::Shared(Arc::new(PredicateNode::new(kind))))
    }

    fn structural_hash(&self) -> u64 {
        match &self.0 {
            PredicateStorage::True => 0,
            PredicateStorage::False => 1,
            PredicateStorage::Shared(node) => node.structural_hash,
        }
    }
}

impl fmt::Debug for Predicate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            PredicateKind::True => formatter.write_str("True"),
            PredicateKind::False => formatter.write_str("False"),
            PredicateKind::Approximate {
                marker,
                paths,
                role,
                sound_subset,
            } => formatter
                .debug_struct("Approximate")
                .field("marker", &marker)
                .field("paths", paths)
                .field("role", &role)
                .field("sound_subset", &sound_subset)
                .finish(),
            PredicateKind::Guard(guard) => formatter.debug_tuple("Guard").field(guard).finish(),
            PredicateKind::Not(inner) => formatter.debug_tuple("Not").field(inner).finish(),
            PredicateKind::And(predicates) => {
                formatter.debug_tuple("And").field(&predicates).finish()
            }
            PredicateKind::Or(predicates) => {
                formatter.debug_tuple("Or").field(&predicates).finish()
            }
        }
    }
}

impl PartialEq for Predicate {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (PredicateStorage::True, PredicateStorage::True)
            | (PredicateStorage::False, PredicateStorage::False) => true,
            (PredicateStorage::Shared(left), PredicateStorage::Shared(right)) => {
                Arc::ptr_eq(left, right) || left == right
            }
            _ => false,
        }
    }
}

impl Eq for Predicate {}

impl AsRef<Self> for Predicate {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl PartialOrd for Predicate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Predicate {
    fn cmp(&self, other: &Self) -> Ordering {
        predicate_kind_rank(self.kind())
            .cmp(&predicate_kind_rank(other.kind()))
            .then_with(|| match (self.kind(), other.kind()) {
                (
                    PredicateKind::Approximate {
                        marker: left_marker,
                        paths: left_paths,
                        role: left_role,
                        sound_subset: left_subset,
                    },
                    PredicateKind::Approximate {
                        marker: right_marker,
                        paths: right_paths,
                        role: right_role,
                        sound_subset: right_subset,
                    },
                ) => left_marker
                    .cmp(right_marker)
                    .then_with(|| left_paths.cmp(right_paths))
                    .then_with(|| left_role.cmp(&right_role))
                    .then_with(|| left_subset.cmp(&right_subset)),
                (PredicateKind::Guard(left), PredicateKind::Guard(right)) => left.cmp(right),
                (PredicateKind::Not(left), PredicateKind::Not(right)) => left.cmp(right),
                (PredicateKind::And(left), PredicateKind::And(right))
                | (PredicateKind::Or(left), PredicateKind::Or(right)) => left.cmp(right),
                _ => Ordering::Equal,
            })
    }
}

impl Hash for Predicate {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.structural_hash().hash(state);
    }
}

fn predicate_kind_rank(kind: PredicateKind<'_>) -> u8 {
    match kind {
        PredicateKind::True => 0,
        PredicateKind::False => 1,
        PredicateKind::Approximate { .. } => 2,
        PredicateKind::Guard(_) => 3,
        PredicateKind::Not(_) => 4,
        PredicateKind::And(_) => 5,
        PredicateKind::Or(_) => 6,
    }
}

/// Canonical logical conjunction of typed predicates.
///
/// Construction flattens nested conjunctions, removes `True`, and stores the
/// remaining predicates in the same structural order used by [`Predicate`].
#[derive(Clone, Default)]
pub struct Conjunction(Vec<Predicate>);

impl Conjunction {
    /// Builds a canonical conjunction from predicate parts.
    #[must_use]
    pub fn new(predicates: impl IntoIterator<Item = Predicate>) -> Self {
        fn append(predicate: Predicate, out: &mut Vec<Predicate>) {
            match predicate.kind() {
                PredicateKind::True => {}
                PredicateKind::And(items) => {
                    for item in items.iter().cloned() {
                        append(item, out);
                    }
                }
                _ => out.push(predicate),
            }
        }

        let mut canonical = Vec::new();
        for predicate in predicates {
            append(predicate, &mut canonical);
        }
        canonical.sort();
        canonical.dedup();
        Self(canonical)
    }

    /// Returns the canonical predicate slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Predicate] {
        &self.0
    }

    /// Adds predicate parts and restores canonical form.
    pub fn extend(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        *self = Self::new(std::mem::take(&mut self.0).into_iter().chain(predicates));
    }

    /// Adds one predicate and restores canonical form.
    pub fn push(&mut self, predicate: Predicate) {
        self.extend([predicate]);
    }

    /// Prepends predicate parts and restores canonical form.
    pub fn prepend(&mut self, predicates: impl IntoIterator<Item = Predicate>) {
        *self = Self::new(predicates.into_iter().chain(std::mem::take(&mut self.0)));
    }

    /// Retains matching predicates while preserving canonical form.
    pub fn retain(&mut self, keep: impl FnMut(&Predicate) -> bool) {
        self.0.retain(keep);
    }

    /// Consumes the conjunction into its canonical predicate vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<Predicate> {
        self.0
    }
}

impl fmt::Debug for Conjunction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl PartialEq for Conjunction {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Conjunction {}

impl PartialOrd for Conjunction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Conjunction {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl Hash for Conjunction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Deref for Conjunction {
    type Target = [Predicate];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl From<Vec<Predicate>> for Conjunction {
    fn from(predicates: Vec<Predicate>) -> Self {
        Self::new(predicates)
    }
}

impl PartialEq<Vec<Predicate>> for Conjunction {
    fn eq(&self, other: &Vec<Predicate>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl PartialEq<Conjunction> for Vec<Predicate> {
    fn eq(&self, other: &Conjunction) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl FromIterator<Predicate> for Conjunction {
    fn from_iter<T: IntoIterator<Item = Predicate>>(iter: T) -> Self {
        Self::new(iter)
    }
}

impl IntoIterator for Conjunction {
    type Item = Predicate;
    type IntoIter = std::vec::IntoIter<Predicate>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a Conjunction {
    type Item = &'a Predicate;
    type IntoIter = std::slice::Iter<'a, Predicate>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl From<Guard> for Predicate {
    fn from(guard: Guard) -> Self {
        match guard {
            Guard::Not { path } => Self::Not(Box::new(Self::truthy_values_path(path))),
            Guard::Or { paths } => {
                Self::Or(paths.into_iter().map(Self::truthy_values_path).collect())
            }
            Guard::AnyOf { alternatives } => Self::Or(
                alternatives
                    .into_iter()
                    .map(|alternative| Self::all(alternative.into_iter().map(Self::from).collect()))
                    .collect(),
            ),
            Guard::NotTypeIs { path, schema_type } => {
                Self::Not(Box::new(Self::Guard(Guard::TypeIs { path, schema_type })))
            }
            guard => Self::Guard(guard),
        }
    }
}

impl Predicate {
    /// Creates an atomic truthiness predicate for a values path.
    pub fn truthy_path(path: impl Into<String>) -> Self {
        Self::truthy_values_path(ValuesPath::parse(&path.into()))
    }

    /// Creates the exact predicate for Sprig's `kindIs "invalid"` over a values path.
    pub fn invalid_kind_path(path: impl Into<String>) -> Self {
        let path = ValuesPath::parse(&path.into());
        Self::Or(vec![
            Self::from(Guard::Absent { path: path.clone() }),
            Self::from(Guard::Eq {
                path,
                value: GuardValue::Null,
            }),
        ])
    }

    /// Marks an unlowerable condition without inventing a relation between its paths.
    pub fn approximate(marker: impl Into<String>, paths: BTreeSet<String>) -> Self {
        Self::Approximate(
            marker.into(),
            paths
                .into_iter()
                .map(|path| ValuesPath::parse(&path))
                .collect(),
            ApproximationRole::Control,
            None,
        )
    }

    /// Marks an unlowerable condition that still admits a bounded sound
    /// strengthening: `guards` hold only in states where the real condition
    /// holds too.
    pub fn approximate_with_sound_subset(
        marker: impl Into<String>,
        paths: BTreeSet<String>,
        sound_subset: Vec<Guard>,
    ) -> Self {
        let sound_subset = match sound_subset.as_slice() {
            [] => None,
            _ => Some(Box::new(Self::all(
                sound_subset.into_iter().map(Self::from).collect(),
            ))),
        };
        Self::Approximate(
            marker.into(),
            paths
                .into_iter()
                .map(|path| ValuesPath::parse(&path))
                .collect(),
            ApproximationRole::Control,
            sound_subset,
        )
    }

    /// Marks an unlowerable condition with a typed predicate that implies it.
    #[must_use]
    pub fn approximate_with_sound_predicate(
        marker: impl Into<String>,
        paths: BTreeSet<String>,
        sound_subset: Self,
    ) -> Self {
        let sound_subset = (!matches!(sound_subset.kind(), PredicateKind::False)
            && !sound_subset.contains_approximation())
        .then(|| Box::new(sound_subset.normalize_boolean()));
        Self::Approximate(
            marker.into(),
            paths
                .into_iter()
                .map(|path| ValuesPath::parse(&path))
                .collect(),
            ApproximationRole::Control,
            sound_subset,
        )
    }

    /// Marks an inexact returned-value selection with a typed predicate that
    /// proves when the candidate supplies the result.
    #[must_use]
    pub fn approximate_output_selection(
        marker: impl Into<String>,
        paths: BTreeSet<String>,
        sound_subset: Self,
    ) -> Self {
        let sound_subset = (!matches!(sound_subset.kind(), PredicateKind::False)
            && !sound_subset.contains_approximation())
        .then(|| Box::new(sound_subset.normalize_boolean()));
        Self::Approximate(
            marker.into(),
            paths
                .into_iter()
                .map(|path| ValuesPath::parse(&path))
                .collect(),
            ApproximationRole::OutputSelection,
            sound_subset,
        )
    }

    /// Normalizes a conjunction, collapsing empty and singleton formulas.
    #[must_use]
    pub fn all(predicates: Vec<Self>) -> Self {
        match predicates.as_slice() {
            [] => Self::True,
            [predicate] => predicate.clone(),
            _ => Self::And(predicates),
        }
    }

    /// Returns the logical complement without retaining redundant double negation.
    #[must_use]
    pub fn negated(&self) -> Self {
        match self.kind() {
            PredicateKind::True => Self::False,
            PredicateKind::False => Self::True,
            PredicateKind::Not(inner) => inner.clone(),
            _ => Self::Not(Box::new(self.clone())),
        }
    }

    /// Canonicalizes an exact Boolean formula without distributive expansion.
    ///
    /// Opaque approximate predicates are returned unchanged. Exact formulas
    /// use a bounded decision diagram internally and fall back to the input
    /// formula if neither normal form stays bounded.
    #[must_use]
    pub fn normalize_boolean(self) -> Self {
        crate::predicate_bdd::normalize(self)
    }

    /// Reports whether this exact Boolean formula entails `consequent`.
    ///
    /// Opaque approximations never prove entailment. The bounded decision
    /// diagram may also abstain when either formula exceeds its limits.
    #[must_use]
    pub fn exactly_implies(&self, consequent: &Self) -> bool {
        crate::predicate_bdd::exact_implies(self, consequent)
    }

    /// Reports whether the predicate is the constant `true` or `false` formula.
    #[must_use]
    pub fn is_trivial(&self) -> bool {
        matches!(self.kind(), PredicateKind::True | PredicateKind::False)
    }

    /// Whether this predicate contains a condition that could not be lowered exactly.
    #[must_use]
    pub fn contains_approximation(&self) -> bool {
        match &self.0 {
            PredicateStorage::True | PredicateStorage::False => false,
            PredicateStorage::Shared(node) => node.contains_approximation,
        }
    }

    /// Returns every values path referenced by the formula.
    #[must_use]
    pub fn value_paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        self.collect_value_paths(&mut paths);
        paths
    }

    /// Expands header predicates into the context-selection facts active in their bodies.
    pub fn with_context_predicates(self) -> Vec<Self> {
        match self.kind() {
            PredicateKind::True => Vec::new(),
            PredicateKind::False => vec![Self::False],
            PredicateKind::Approximate { .. }
            | PredicateKind::Guard(
                Guard::Range { .. }
                | Guard::RangeKeyPrefix { .. }
                | Guard::RangeKeyEquals { .. }
                | Guard::RangeKeyMatches { .. }
                | Guard::Absent { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. }
                | Guard::IntGt { .. }
                | Guard::IntLt { .. }
                | Guard::AtMostOneMember { .. }
                | Guard::MinMembers { .. }
                | Guard::HasKey { .. }
                | Guard::NotHasKey { .. }
                | Guard::ContainsEquals { .. }
                | Guard::ContainsMemberEquals { .. }
                | Guard::ContainsTruthyMember { .. },
            ) => vec![self.clone()],
            PredicateKind::And(predicates) => predicates
                .iter()
                .cloned()
                .flat_map(Self::with_context_predicates)
                .collect(),
            PredicateKind::Guard(Guard::Truthy { path }) => {
                vec![Self::from(Guard::With { path: path.clone() })]
            }
            PredicateKind::Or(predicates) => {
                let paths: Option<Vec<ValuesPath>> = predicates
                    .iter()
                    .map(|predicate| match predicate.kind() {
                        PredicateKind::Guard(Guard::Truthy { path }) => Some(path.clone()),
                        _ => None,
                    })
                    .collect();
                let Some(paths) = paths else {
                    return vec![self.clone()];
                };
                let mut out: Vec<Self> = paths
                    .iter()
                    .map(|path| Self::from(Guard::With { path: path.clone() }))
                    .collect();
                out.push(Self::Or(
                    paths.into_iter().map(Self::truthy_values_path).collect(),
                ));
                out
            }
            PredicateKind::Not(inner) => match inner.kind() {
                PredicateKind::Guard(Guard::Truthy { path }) => {
                    vec![Self::from(Guard::With { path: path.clone() }), self.clone()]
                }
                _ => vec![self.clone()],
            },
            PredicateKind::Guard(Guard::Eq { path, value }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::Eq {
                    path: path.clone(),
                    value: value.clone(),
                }),
            ],
            PredicateKind::Guard(Guard::MatchesPattern {
                path,
                pattern,
                templated,
            }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: pattern.clone(),
                    templated: *templated,
                }),
            ],
            PredicateKind::Guard(Guard::NotMatchesPattern { path, pattern }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::NotMatchesPattern {
                    path: path.clone(),
                    pattern: pattern.clone(),
                }),
            ],
            PredicateKind::Guard(Guard::NotEq { path, value }) => vec![
                Self::from(Guard::With { path: path.clone() }),
                Self::from(Guard::NotEq {
                    path: path.clone(),
                    value: value.clone(),
                }),
            ],
        }
    }

    /// Returns values paths whose branch structure permits them to be absent.
    #[must_use]
    pub fn conditionally_optional_paths(&self) -> BTreeSet<ValuesPath> {
        let mut paths = BTreeSet::new();
        self.collect_conditionally_optional_paths(&mut paths);
        paths
    }

    /// Projects this formula exactly into the contract guard vocabulary.
    /// `None` means some predicate node has no exact guard spelling.
    #[must_use]
    pub fn contract_guards(&self) -> Option<Vec<Guard>> {
        flatten_contract_guards(self, false)
    }

    fn collect_value_paths(&self, out: &mut BTreeSet<ValuesPath>) {
        match self.kind() {
            PredicateKind::True | PredicateKind::False => {}
            PredicateKind::Approximate { paths, .. } => out.extend(paths.iter().cloned()),
            PredicateKind::Guard(guard) => {
                for path in guard.value_paths() {
                    out.insert(path);
                }
            }
            PredicateKind::Not(inner) => inner.collect_value_paths(out),
            PredicateKind::And(predicates) | PredicateKind::Or(predicates) => {
                for predicate in predicates {
                    predicate.collect_value_paths(out);
                }
            }
        }
    }

    fn collect_conditionally_optional_paths(&self, out: &mut BTreeSet<ValuesPath>) {
        match self.kind() {
            PredicateKind::Guard(Guard::NotEq { path, .. } | Guard::Absent { path }) => {
                out.insert(path.clone());
            }
            PredicateKind::Not(inner) => match inner.kind() {
                PredicateKind::Guard(Guard::Truthy { path }) => {
                    out.insert(path.clone());
                }
                _ => inner.collect_conditionally_optional_paths(out),
            },
            PredicateKind::Or(predicates) => {
                for predicate in predicates {
                    out.extend(predicate.value_paths());
                }
            }
            PredicateKind::And(predicates) => {
                for predicate in predicates {
                    predicate.collect_conditionally_optional_paths(out);
                }
            }
            PredicateKind::True
            | PredicateKind::False
            | PredicateKind::Approximate { .. }
            | PredicateKind::Guard(
                Guard::Truthy { .. }
                | Guard::Eq { .. }
                | Guard::MatchesPattern { .. }
                | Guard::NotMatchesPattern { .. }
                | Guard::RangeKeyPrefix { .. }
                | Guard::RangeKeyEquals { .. }
                | Guard::RangeKeyMatches { .. }
                | Guard::Range { .. }
                | Guard::With { .. }
                | Guard::Default { .. }
                | Guard::TypeIs { .. }
                | Guard::NotTypeIs { .. }
                | Guard::Not { .. }
                | Guard::Or { .. }
                | Guard::AnyOf { .. }
                | Guard::IntGt { .. }
                | Guard::IntLt { .. }
                | Guard::AtMostOneMember { .. }
                | Guard::MinMembers { .. }
                | Guard::HasKey { .. }
                | Guard::NotHasKey { .. }
                | Guard::ContainsEquals { .. }
                | Guard::ContainsMemberEquals { .. }
                | Guard::ContainsTruthyMember { .. },
            ) => {}
        }
    }

    /// Projects a predicate stack into a deduplicated guard conjunction.
    #[must_use]
    pub fn contract_guard_stack(predicates: &[Self]) -> Vec<Guard> {
        let mut guards = Vec::new();
        for predicate in predicates {
            if let Some(projected) = predicate.contract_guards() {
                for guard in projected {
                    if !guards.contains(&guard) {
                        guards.push(guard);
                    }
                }
            }
        }
        guards
    }

    /// Rewrites every values path carried by this formula.
    #[must_use]
    pub fn map_value_paths<F>(self, map: &mut F) -> Self
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        match self.kind() {
            PredicateKind::True => Self::True,
            PredicateKind::False => Self::False,
            PredicateKind::Approximate {
                marker,
                paths,
                role,
                sound_subset,
            } => Self::Approximate(
                marker.to_owned(),
                paths.iter().cloned().map(&mut *map).collect(),
                role,
                sound_subset.map(|predicate| Box::new(predicate.clone().map_value_paths(map))),
            ),
            PredicateKind::Guard(guard) => Self::Guard(guard.clone().map_value_paths(map)),
            PredicateKind::Not(inner) => Self::Not(Box::new(inner.clone().map_value_paths(map))),
            PredicateKind::And(predicates) => Self::And(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect(),
            ),
            PredicateKind::Or(predicates) => Self::Or(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect(),
            ),
        }
    }

    fn truthy_values_path(path: ValuesPath) -> Self {
        Self::Guard(Guard::Truthy { path })
    }
}

fn flatten_contract_guards(predicate: &Predicate, negated: bool) -> Option<Vec<Guard>> {
    match (predicate.kind(), negated) {
        (PredicateKind::True, false) => Some(Vec::new()),
        (PredicateKind::True | PredicateKind::False | PredicateKind::Approximate { .. }, true)
        | (PredicateKind::False | PredicateKind::Approximate { .. }, false) => None,
        (PredicateKind::Guard(guard), false) => Some(vec![guard.clone()]),
        (PredicateKind::Guard(Guard::Or { paths }), true) => Some(
            paths
                .iter()
                .map(|path| Guard::Not { path: path.clone() })
                .collect(),
        ),
        (PredicateKind::Guard(guard), true) => negated_guard(guard).map(|guard| vec![guard]),
        (PredicateKind::Not(inner), polarity) => flatten_contract_guards(inner, !polarity),
        (PredicateKind::And(predicates), false) | (PredicateKind::Or(predicates), true) => {
            predicates
                .iter()
                .map(|predicate| flatten_contract_guards(predicate, negated))
                .collect::<Option<Vec<_>>>()
                .map(|guards| guards.into_iter().flatten().collect())
        }
        (PredicateKind::Or(predicates), false) | (PredicateKind::And(predicates), true) => {
            predicates
                .iter()
                .map(|predicate| flatten_contract_guards(predicate, negated))
                .collect::<Option<Vec<_>>>()
                .map(alternatives_to_guards)
        }
    }
}

fn negated_guard(guard: &Guard) -> Option<Guard> {
    match guard {
        Guard::Truthy { path } | Guard::With { path } => Some(Guard::Not { path: path.clone() }),
        Guard::Not { path } => Some(Guard::Truthy { path: path.clone() }),
        Guard::Eq { path, value } => Some(Guard::NotEq {
            path: path.clone(),
            value: value.clone(),
        }),
        Guard::NotEq { path, value } => Some(Guard::Eq {
            path: path.clone(),
            value: value.clone(),
        }),
        Guard::TypeIs { path, schema_type } => Some(Guard::NotTypeIs {
            path: path.clone(),
            schema_type: schema_type.clone(),
        }),
        Guard::NotTypeIs { path, schema_type } => Some(Guard::TypeIs {
            path: path.clone(),
            schema_type: schema_type.clone(),
        }),
        Guard::HasKey { path, key } => Some(Guard::NotHasKey {
            path: path.clone(),
            key: key.clone(),
        }),
        Guard::NotHasKey { path, key } => Some(Guard::HasKey {
            path: path.clone(),
            key: key.clone(),
        }),
        Guard::Absent { .. }
        | Guard::MatchesPattern { .. }
        | Guard::NotMatchesPattern { .. }
        | Guard::RangeKeyPrefix { .. }
        | Guard::RangeKeyEquals { .. }
        | Guard::RangeKeyMatches { .. }
        | Guard::Or { .. }
        | Guard::AnyOf { .. }
        | Guard::Range { .. }
        | Guard::Default { .. }
        | Guard::IntGt { .. }
        | Guard::IntLt { .. }
        | Guard::AtMostOneMember { .. }
        | Guard::MinMembers { .. }
        | Guard::ContainsEquals { .. }
        | Guard::ContainsMemberEquals { .. }
        | Guard::ContainsTruthyMember { .. } => None,
    }
}

/// Normalize a disjunction of guard conjunctions into guard form: a single
/// alternative collapses to its conjunction, all-truthy alternatives become
/// the flat [`Guard::Or`], anything else the general [`Guard::AnyOf`].
fn alternatives_to_guards(mut alternatives: Vec<Vec<Guard>>) -> Vec<Guard> {
    for alternative in &mut alternatives {
        alternative.sort();
        alternative.dedup();
    }
    alternatives.sort();
    alternatives.dedup();

    if alternatives.len() == 1 {
        return alternatives.pop().unwrap_or_default();
    }

    if let Some(paths) = truthy_or_paths(&alternatives) {
        return vec![Guard::Or { paths }];
    }

    vec![Guard::AnyOf { alternatives }]
}

fn truthy_or_paths(alternatives: &[Vec<Guard>]) -> Option<Vec<ValuesPath>> {
    alternatives
        .iter()
        .map(|alternative| match alternative.as_slice() {
            [Guard::Truthy { path }] => Some(path.clone()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/predicate.rs"]
mod tests;
