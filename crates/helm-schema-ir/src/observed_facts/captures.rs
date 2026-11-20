use std::collections::BTreeSet;

use helm_schema_core::ValuesPath;

/// One captured `fail` call: the predicate conjunction reaching it. Raw
/// predicates, not [`helm_schema_core::GuardDnf`]: the DNF conversion drops
/// conjuncts it cannot represent, which is safe for row conditions (wider
/// arms) but unsound for fail NEGATION. Enclosing conditions whose lowering
/// was APPROXIMATE (truthy fallbacks, dropped conjuncts) appear in the
/// conjunction as [`helm_schema_core::Predicate::Approximate`] conjuncts,
/// so the negation can abstain instead of manufacturing requirements the
/// chart never stated.
#[derive(Clone, Debug)]
pub(crate) struct FailCapture {
    pub(crate) conjunction: helm_schema_core::Conjunction,
    /// The range facts active at the capture site. Input identity says which
    /// path the header actually iterates; member identity says which path
    /// supplies the values-backed members of a possibly derived iterable.
    pub(crate) ranged: crate::range_modes::RangeModes,
    pub(crate) kind: CaptureKind,
}

impl PartialEq for FailCapture {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for FailCapture {}

impl PartialOrd for FailCapture {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FailCapture {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.conjunction
            .cmp(&other.conjunction)
            .then_with(|| self.ranged.cmp(&other.ranged))
            .then_with(|| self.kind.cmp(&other.kind))
    }
}

/// How a [`FailCapture`]'s conjunction lowers into schema requirements.
/// The variants select mutually exclusive lowering paths in the signal
/// builder; the payloads exist only for their variant's lane.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CaptureKind {
    /// A direct `fail`-style capture: the failing TEST conjunct is negated
    /// wherever the outer guards hold.
    #[default]
    Fail,
    /// A scalar path must have the named JSON Schema type whenever the
    /// capture's execution predicates hold. `null_aborts` distinguishes a
    /// strict Go parameter from a selected input whose null arm never reaches
    /// that parameter.
    ValueType {
        path: ValuesPath,
        schema_type: String,
        null_aborts: bool,
    },
    /// Rendering ABORTS wherever the capture's execution predicates hold and
    /// this path is absent: a nil-strict string consumer reads it (helm
    /// answers `wrong type for value; expected string; got interface {}`
    /// for `tpl`, `b64enc`, `trim`, `nindent`, … on a nil operand).
    /// Lowers to a document-level terminal clause, which — unlike a parent
    /// member requirement — reaches a top-level path and carries the
    /// `Absent` guard's ownership semantics.
    AbsenceAborts { path: ValuesPath },
    /// A member-access capture (`[outer…, ¬object(P)]` from a field access
    /// through `P`): the signal builder folds these per path into one
    /// bypass-proof arm instead of lowering each as its own implication.
    MemberAccess {
        /// Raw input kinds converted to an object by a proven earlier
        /// mutation on every execution path reaching this member access.
        handled_kinds: BTreeSet<String>,
    },
}

impl FailCapture {
    /// Whether any enclosing condition's lowering was approximate: the
    /// negation-based lowering must abstain for the whole capture.
    pub(crate) fn contains_approximation(&self) -> bool {
        self.conjunction
            .iter()
            .any(helm_schema_core::Predicate::contains_approximation)
    }
}

impl CaptureKind {
    pub(crate) fn sole_value_path(&self) -> Option<&ValuesPath> {
        match self {
            Self::Fail | Self::MemberAccess { .. } => None,
            Self::ValueType { path, .. } | Self::AbsenceAborts { path } => Some(path),
        }
    }

    /// Rewrite every values path the kind payload carries (dependency
    /// namespacing rebases captures under the subchart's key exactly like
    /// the conjunction's predicate paths).
    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(ValuesPath) -> ValuesPath,
    {
        match self {
            Self::Fail | Self::MemberAccess { .. } => {}
            Self::ValueType { path, .. } | Self::AbsenceAborts { path } => {
                *path = map(path.clone());
            }
        }
    }
}
