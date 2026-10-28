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
            .then_with(|| match (&self.kind, &other.kind) {
                (
                    CaptureKind::StringRequirement {
                        path: left_path,
                        route: left_route,
                        selection: left_selection,
                    },
                    CaptureKind::StringRequirement {
                        path: right_path,
                        route: right_route,
                        selection: right_selection,
                    },
                ) => left_path
                    .cmp(right_path)
                    .then_with(|| left_route.cmp(right_route))
                    .then_with(|| left_selection.cmp(right_selection)),
                _ => self.kind.cmp(&other.kind),
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StringRequirementRoute {
    /// A direct consumer whose row already owns its raw string contract.
    Direct,
    /// A direct consumer lowered under ambient execution predicates.
    Scoped,
    /// One selected raw input arm of a composed expression.
    Selected,
    /// A certified serializer rendered the raw value before this consumer.
    /// The consumer constrains that text, not the original value's kind.
    Serialized,
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
    /// Collections whose range key reaches a strict string consumer.
    RangeKeyStrings { paths: BTreeSet<ValuesPath> },
    /// Every member of the named collection paths reaches a strict runtime
    /// consumer with the given JSON kind; a pattern additionally binds each
    /// member to a parser's lexical domain (genSignedCert's ip list).
    CollectionItems {
        paths: BTreeSet<ValuesPath>,
        schema_type: String,
        pattern: Option<String>,
    },
    /// A literal zero-based `index` executes on this source path.
    IndexAccess { path: ValuesPath, index: usize },
    /// A literal index executes on a list produced by splitting source text.
    SplitIndexAccess {
        paths: BTreeSet<ValuesPath>,
        separator: String,
        index: usize,
        total_text_preimage: bool,
    },
    /// A scalar path must have the named JSON Schema type whenever the
    /// capture's execution predicates hold. `null_aborts` distinguishes a
    /// strict Go parameter from a selected input whose null arm never reaches
    /// that parameter.
    ValueType {
        path: ValuesPath,
        schema_type: String,
        null_aborts: bool,
    },
    /// A strict string consumer with its exact selection conjunction.
    /// `route` distinguishes direct raw consumption, selected raw
    /// consumption, and consumption after a certified serializer.
    StringRequirement {
        path: ValuesPath,
        route: StringRequirementRoute,
        selection: helm_schema_core::Conjunction,
    },
    /// A range header iterates this values path itself, or a wildcard path
    /// identifies the values-backed member alternative supplied to a
    /// derived iterable. The header establishes that input's iterable
    /// domain independently of whether its body renders any rows.
    RangeInput {
        path: ValuesPath,
        destructured: bool,
        json_decoded: bool,
    },
    /// A range header iterates the first-truthy selection of the ordered
    /// identity candidates in `chain`, and `path` is the candidate this
    /// capture claims: it must be iterable exactly where the conjunction's
    /// selection conjuncts pick it. `chain` lets the lowering strip the
    /// caller's conjunctive with-marker stamp for those paths — the
    /// header's disjunctive condition was approximated by per-path `With`
    /// markers, which the exact selection conjuncts refine (a `¬truthy`
    /// prior would otherwise contradict its own marker and the claim could
    /// never fire).
    RangeSelection {
        path: ValuesPath,
        chain: Vec<ValuesPath>,
        allow_integer: bool,
    },
    /// A `dig` SUBJECT step: whenever the capture's execution predicates
    /// hold, the path must be an object even when explicitly null — Sprig
    /// type-asserts the dict before any nil handling, so a null aborts
    /// while absence stays open (the conjunction carries the strict
    /// presence guard).
    DigSubject { path: ValuesPath },
    /// A `dig` SUBJECT must additionally be PRESENT: the type assertion
    /// runs before any missing-key handling, so an absent subject reads as
    /// nil and aborts exactly like an explicit null (loki's null-deleted
    /// `storage_config`). Lowers to a `HasMember` presence requirement on
    /// the parent path.
    RequiredPresence { path: ValuesPath },
    /// Rendering ABORTS wherever the capture's execution predicates hold and
    /// this path is absent: a nil-strict string consumer reads it (helm
    /// answers `wrong type for value; expected string; got interface {}`
    /// for `tpl`, `b64enc`, `trim`, `nindent`, … on a nil operand).
    /// Lowers to a document-level terminal clause, which — unlike a parent
    /// member requirement — reaches a top-level path and carries the
    /// `Absent` guard's ownership semantics.
    AbsenceAborts { path: ValuesPath },
    /// A comparison operand must have the named JSON Schema type when
    /// PRESENT and non-null; `eq`/`ne` compare `nil` against anything.
    ComparableKind {
        path: ValuesPath,
        schema_type: String,
    },
    /// A string path must match the pattern whenever the capture's execution
    /// predicates hold.
    ValuePattern {
        path: ValuesPath,
        pattern: String,
        templated: bool,
    },
    /// A `%s` substitution opens a plain token, so the raw operand must
    /// format as either string text or a structurally safe mapping.
    PrintfStringOperand { path: ValuesPath },
    /// A raw splice inside an UNQUOTED scalar: the path's own text must keep
    /// the plain token intact (`: `, ` #`, and line breaks end it).
    PlainSlotText {
        path: ValuesPath,
        token_initial: bool,
        templated: bool,
    },
    /// Collections whose range KEY renders raw into an unquoted slot — the
    /// mapping key it becomes (`{{ $key }}:`) or a plain value slot it fills
    /// (`name: {{ $key }}`). The claim binds the collection's KEYS, so it
    /// lowers onto `propertyNames` rather than a member.
    RangeKeyPlainSlot { paths: BTreeSet<ValuesPath> },
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

    pub(crate) fn requirement_is_implied_by(
        &self,
        condition: &helm_schema_core::Predicate,
    ) -> bool {
        let (path, required_type) = match &self.kind {
            CaptureKind::StringRequirement { path, .. } => (path, Some("string")),
            CaptureKind::ValueType {
                path, schema_type, ..
            } => (path, Some(schema_type.as_str())),
            CaptureKind::AbsenceAborts { path } => (path, None),
            _ => return false,
        };
        condition.contract_guards().is_some_and(|guards| {
            guards.iter().any(|guard| {
                matches!(
                    guard,
                    crate::Guard::TypeIs {
                        path: guarded,
                        schema_type,
                    } if *guarded == *path
                        && required_type.is_none_or(|required| schema_type == required)
                )
            })
        })
    }
}

impl CaptureKind {
    pub(crate) fn sole_value_path(&self) -> Option<&ValuesPath> {
        match self {
            Self::Fail | Self::MemberAccess { .. } => None,
            Self::RangeKeyStrings { paths }
            | Self::RangeKeyPlainSlot { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                let mut paths = paths.iter();
                match (paths.next(), paths.next()) {
                    (Some(path), None) => Some(path),
                    _ => None,
                }
            }
            Self::IndexAccess { path, .. }
            | Self::ValueType { path, .. }
            | Self::StringRequirement { path, .. }
            | Self::RangeInput { path, .. }
            | Self::RangeSelection { path, .. }
            | Self::DigSubject { path }
            | Self::RequiredPresence { path }
            | Self::AbsenceAborts { path }
            | Self::ComparableKind { path, .. }
            | Self::ValuePattern { path, .. }
            | Self::PrintfStringOperand { path }
            | Self::PlainSlotText { path, .. } => Some(path),
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
            Self::RangeKeyStrings { paths }
            | Self::RangeKeyPlainSlot { paths }
            | Self::CollectionItems { paths, .. }
            | Self::SplitIndexAccess { paths, .. } => {
                *paths = paths.iter().cloned().map(&mut *map).collect();
            }
            Self::IndexAccess { path, .. }
            | Self::ValueType { path, .. }
            | Self::RangeInput { path, .. }
            | Self::DigSubject { path }
            | Self::RequiredPresence { path }
            | Self::AbsenceAborts { path }
            | Self::ComparableKind { path, .. }
            | Self::ValuePattern { path, .. }
            | Self::PrintfStringOperand { path }
            | Self::PlainSlotText { path, .. } => {
                *path = map(path.clone());
            }
            Self::StringRequirement {
                path, selection, ..
            } => {
                *path = map(path.clone());
                *selection = selection
                    .iter()
                    .cloned()
                    .map(|predicate| predicate.map_value_paths(map))
                    .collect();
            }
            Self::RangeSelection { path, chain, .. } => {
                *path = map(path.clone());
                *chain = chain.iter().cloned().map(&mut *map).collect();
            }
        }
    }
}
