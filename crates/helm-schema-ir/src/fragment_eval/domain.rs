//! The `Guarded<AbstractFragment>` domain: an abstract rendered document.
//!
//! A fragment is the shape a template *renders*, with template control flow
//! preserved as guard trees instead of being flattened into per-row guard
//! vectors. Guards live on branch nodes: every alternative of a
//! [`Guarded`] value carries the [`PathCondition`] under which that arm
//! materializes, and root-to-leaf condition chains replace the current
//! pipeline's ambient guard stacks.

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::{ContractProvenance, ResourceRef, ValueKind};
use helm_schema_core::Predicate;

/// Render-site facts resolved at evaluation time: the manifest resource
/// whose span contains the site, that resource span's path prefix (List
/// envelope items rebase their emitted paths below `items[*]`), and the
/// site's source provenance. Shared by every row one hole produces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiteFacts {
    /// The resource containing this site, when its document declares one.
    pub resource: Option<ResourceRef>,
    /// Path segments the containing resource span strips from emitted paths.
    pub path_prefix: Vec<String>,
    /// The site's own source span, when the template has a source path.
    pub provenance: Option<ContractProvenance>,
}

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
    /// structural dynamic-member path without guessing its rendered key; the
    /// key's own splices are recorded as pathless reads at the eval site,
    /// where the ambient range/branch predicates are still known.
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
    Taint(TaintPart),
}

/// Unknown rendered text inside a scalar with its influencing paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaintPart {
    /// The `.Values` paths that flowed into the unknown text.
    pub paths: BTreeSet<String>,
    /// Exact structure serialized into this text, retained so a later
    /// `fromJson` can recover the value shape across a helper boundary.
    pub(crate) structured_value: Option<crate::abstract_value::AbstractValue>,
    /// Whether `structured_value` was serialized as JSON at this boundary.
    pub(crate) json_serialized: bool,
    /// The render site the taint was observed at.
    pub site: Option<Rc<SiteFacts>>,
    /// Helper-body source sites the taint was derived through (spliced
    /// summary content keeps its body sites here).
    pub provenance: Vec<ContractProvenance>,
}

impl TaintPart {
    /// Taint with no resolved site (stamped later by the interpreter).
    #[must_use]
    pub fn new(paths: BTreeSet<String>) -> Self {
        Self {
            paths,
            structured_value: None,
            json_serialized: false,
            site: None,
            provenance: Vec::new(),
        }
    }

    pub(crate) fn from_json_serialized(value: crate::abstract_value::AbstractValue) -> Self {
        Self {
            paths: value.fragment_rendered_paths(),
            structured_value: Some(value),
            json_serialized: true,
            site: None,
            provenance: Vec::new(),
        }
    }
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
    /// The rendered text is a total stringification of the value (`quote`,
    /// `toString`, `join`): any input type renders, so the sink neither
    /// constrains nor reveals the input shape.
    pub shape_erased: bool,
    /// The rendered fragment is the result of `toYaml`: every input kind can
    /// be serialized, but its placement can still require sequence shape.
    pub yaml_serialized: bool,
    /// A string-consuming transform (`trunc`, `b64enc`, a dynamic `printf`
    /// format) shaped the rendered text: rendering fails for non-string
    /// values, so this splice's row binds a string contract under its own
    /// conditions.
    pub string_contract: bool,
    /// The splice renders JSON text whose decoded value preserves this input identity.
    pub json_serialized: bool,
    /// The splice's runtime identity was recovered through JSON decoding.
    pub json_decoded: bool,
    /// Helper-body source sites this splice was derived through.
    pub provenance: Vec<ContractProvenance>,
    /// The render site the splice materializes at.
    pub site: Option<Rc<SiteFacts>>,
}

/// An opaque fragment position: content unknown, influence preserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opaque {
    /// The `.Values` paths that flowed into the unknown content.
    pub taint: BTreeSet<String>,
    /// The hole kind the opaque content renders as (scalar holes taint as
    /// scalars, fragment holes as fragments).
    pub kind: ValueKind,
    /// The render site the opaque content was observed at.
    pub site: Option<Rc<SiteFacts>>,
    /// Helper-body source sites the content was derived through (spliced
    /// summary content keeps its body sites here).
    pub provenance: Vec<ContractProvenance>,
}

impl Default for Opaque {
    fn default() -> Self {
        Self {
            taint: BTreeSet::new(),
            kind: ValueKind::Scalar,
            site: None,
            provenance: Vec::new(),
        }
    }
}

/// Stamp `site` onto every row-producing node below `guarded` that has no
/// site yet (nested-file content arrives already stamped by its own
/// interpreter and keeps its facts).
pub fn stamp_fragment_sites(guarded: &mut Guarded<AbstractFragment>, site: &Option<Rc<SiteFacts>>) {
    if site.is_none() {
        return;
    }
    for (_, node) in &mut guarded.arms {
        stamp_node_sites(node, site);
    }
}

fn stamp_node_sites(node: &mut AbstractFragment, site: &Option<Rc<SiteFacts>>) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mut mapping.entries {
                stamp_fragment_sites(&mut entry.value, site);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            for item in &mut sequence.items {
                stamp_fragment_sites(item, site);
            }
        }
        AbstractFragment::Scalar(scalar) => stamp_part_sites(&mut scalar.parts, site),
        AbstractFragment::Splice(splice) => {
            if splice.meta.site.is_none() {
                splice.meta.site = site.clone();
            }
        }
        AbstractFragment::Opaque(opaque) => {
            if opaque.site.is_none() {
                opaque.site = site.clone();
            }
        }
    }
}

/// Stamp `site` onto every splice and taint part that has no site yet.
pub fn stamp_part_sites(parts: &mut [StringPart], site: &Option<Rc<SiteFacts>>) {
    if site.is_none() {
        return;
    }
    for part in parts {
        match part {
            StringPart::Text(_) => {}
            StringPart::Splice(splice) => {
                if splice.meta.site.is_none() {
                    splice.meta.site = site.clone();
                }
            }
            StringPart::Taint(taint) => {
                if taint.site.is_none() {
                    taint.site = site.clone();
                }
            }
        }
    }
}
