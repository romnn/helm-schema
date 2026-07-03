//! The `Guarded<AbstractFragment>` domain: an abstract rendered document.
//!
//! A fragment is the shape a template *renders*, with template control flow
//! preserved as guard trees instead of being flattened into per-row guard
//! vectors. Guards live on branch nodes: every alternative of a
//! [`Guarded`] value carries the [`PathCondition`] under which that arm
//! materializes, and root-to-leaf condition chains replace the current
//! pipeline's ambient guard stacks.

use std::collections::BTreeSet;

use crate::{ContractProvenance, ValueKind};
use helm_schema_core::Predicate;

/// The condition under which a guarded arm materializes.
///
/// This is exactly the existing typed predicate lattice; the fragment domain
/// deliberately introduces no parallel guard representation.
pub type PathCondition = Predicate;

/// A guarded alternative set: each arm materializes when its condition
/// holds. An arm with [`Predicate::True`] is unconditional; several arms are
/// candidates preserved side by side (branch alternatives, merge
/// contributions, and ambiguous evaluations all land here — arms are
/// *contribution candidates*, and mutual exclusivity is expressed only by
/// contradictory conditions).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guarded<T> {
    /// The guarded arms in document order.
    pub arms: Vec<(PathCondition, T)>,
}

impl<T> Default for Guarded<T> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<T> Guarded<T> {
    /// A guarded value with no arms (nothing renders).
    #[must_use]
    pub fn empty() -> Self {
        Self { arms: Vec::new() }
    }

    /// A single unconditional arm.
    #[must_use]
    pub fn unconditional(node: T) -> Self {
        Self {
            arms: vec![(Predicate::True, node)],
        }
    }

    /// A single arm under `condition`.
    #[must_use]
    pub fn conditional(condition: PathCondition, node: T) -> Self {
        Self {
            arms: vec![(condition, node)],
        }
    }

    /// Whether this guarded value has no arms.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arms.is_empty()
    }

    /// Conjoin `condition` onto every arm (used when a control region
    /// dissolves into the surrounding container: each contribution keeps its
    /// own arm conditions and gains the region branch condition).
    pub fn guard_all(&mut self, condition: &PathCondition) {
        if condition.is_trivial() && *condition == Predicate::True {
            return;
        }
        for (arm_condition, _) in &mut self.arms {
            *arm_condition = and_conditions(condition.clone(), arm_condition.clone());
        }
    }

    /// Append all arms of `other`.
    pub fn extend(&mut self, other: Self) {
        self.arms.extend(other.arms);
    }
}

/// Conjoin two conditions, flattening nested `And`s and treating
/// [`Predicate::True`] as identity. Operand order is preserved (outer
/// condition first) so lowered guard stacks read root-to-leaf.
#[must_use]
pub fn and_conditions(outer: PathCondition, inner: PathCondition) -> PathCondition {
    let mut parts = Vec::new();
    for condition in [outer, inner] {
        match condition {
            Predicate::True => {}
            Predicate::And(inner_parts) => {
                for part in inner_parts {
                    if !parts.contains(&part) {
                        parts.push(part);
                    }
                }
            }
            other => {
                if !parts.contains(&other) {
                    parts.push(other);
                }
            }
        }
    }
    Predicate::all(parts)
}

/// One node of the abstract rendered document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbstractFragment {
    /// A YAML mapping.
    Mapping(Mapping),
    /// A YAML sequence.
    Sequence(Sequence),
    /// A scalar value, possibly partial (literal text interleaved with
    /// template holes).
    Scalar(AbstractString),
    /// A `.Values` path rendered whole at this position.
    Splice(Splice),
    /// A position whose rendered content is unknown; `taint` conservatively
    /// names the `.Values` paths that flowed into it.
    Opaque(Opaque),
}

/// A YAML mapping: entries in document order, one entry per key (repeated
/// keys across branches merge their guarded value arms).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mapping {
    /// The mapping entries in first-seen document order.
    pub entries: Vec<MappingEntry>,
}

/// One mapping entry: the key and its guarded value alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingEntry {
    /// The entry key.
    pub key: EntryKey,
    /// The guarded value alternatives for this key.
    pub value: Guarded<AbstractFragment>,
}

/// A mapping key: literal text, or a templated key whose text is only
/// partially known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKey {
    /// A plain literal key (unquoted).
    Literal(String),
    /// A templated key. Projections attribute the entry's value at the
    /// *parent* path (no invented segment) and surface the key's splices as
    /// pathless scalar uses, mirroring the line model's refusal to guess a
    /// segment for templated keys.
    Dynamic(AbstractString),
}

/// A YAML sequence: guarded items in document order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sequence {
    /// The sequence items in document order.
    pub items: Vec<Guarded<AbstractFragment>>,
}

/// A scalar modeled as ordered parts: literal text runs, whole-value
/// splices, and opaque taint. Within one arm the parts list is the ordered
/// *set of contributions* to the rendered text: alternation inside a single
/// hole (a `Choice` value) degrades to multiple parts, which projections
/// treat identically (every contribution attributes at the scalar's
/// position).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbstractString {
    /// The scalar parts in render order.
    pub parts: Vec<StringPart>,
    /// The scalar is a render-suppressed text blob (a block scalar body):
    /// contained splices influence its text but are not sink-typed at the
    /// scalar's document position, so projections keep them pathless.
    pub suppressed: bool,
}

impl AbstractString {
    /// A scalar consisting of one literal text run.
    #[must_use]
    pub fn literal(text: impl Into<String>) -> Self {
        Self {
            parts: vec![StringPart::Text([text.into()].into_iter().collect())],
            suppressed: false,
        }
    }
}

/// One part of an [`AbstractString`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringPart {
    /// Literal text alternatives. A singleton set is plain source text;
    /// larger sets arise when a hole statically evaluates to a finite string
    /// set (branch literals, `printf` over string sets, …).
    Text(BTreeSet<String>),
    /// A `.Values` path rendered into the scalar.
    Splice(Splice),
    /// Unknown rendered text conservatively attributed to these `.Values`
    /// paths.
    Taint(BTreeSet<String>),
}

/// A `.Values` path rendered at a fragment position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Splice {
    /// The dotted `.Values` path (never empty).
    pub values_path: String,
    /// Whether the path renders a whole scalar, part of a scalar, or a YAML
    /// fragment at this position.
    pub kind: ValueKind,
    /// Render-site semantics carried with the splice.
    pub meta: SpliceMeta,
}

impl Splice {
    /// A plain scalar splice with default meta.
    #[must_use]
    pub fn scalar(values_path: impl Into<String>) -> Self {
        Self {
            values_path: values_path.into(),
            kind: ValueKind::Scalar,
            meta: SpliceMeta::default(),
        }
    }
}

/// Splice metadata: defaultedness, encoding, and source provenance.
///
/// Deliberately *no* predicates here — helper-internal branch conditions
/// lower into [`Guarded`] arms so the guard structure stays in the tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpliceMeta {
    /// The render site substitutes a fallback when the path is empty/nil
    /// (`default`, chart-level `set … default` mutations).
    pub defaulted: bool,
    /// The rendered text is an encoded transform of the value (`b64enc`),
    /// so the sink schema does not constrain the value's shape.
    pub encoded: bool,
    /// Helper-body source sites this splice was derived through.
    pub provenance: Vec<ContractProvenance>,
}

/// An opaque fragment position: content unknown, influence preserved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opaque {
    /// The `.Values` paths that flowed into the unknown content.
    pub taint: BTreeSet<String>,
}
