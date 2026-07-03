//! Lowering from the expression value lattice ([`AbstractValue`]) into
//! fragment nodes.
//!
//! The lowering rules, and why:
//!
//! - `ValuesPath` becomes a [`Splice`]; the render site's kind (scalar vs
//!   fragment) comes from the hole context, and defaulted/encoded meta comes
//!   from the expression effects at the hole.
//! - `OutputPath` (a helper-projected rendering) becomes one guarded arm per
//!   recorded helper predicate branch, each holding a [`Splice`] that keeps
//!   the helper meta's defaultedness and provenance. Helper-internal branch
//!   conditions therefore live in the *tree* (as [`PathCondition`]s), not in
//!   splice meta.
//! - `StringSet` becomes a [`Scalar`](AbstractFragment::Scalar) whose single
//!   text part carries the full alternative set (merged scalar
//!   alternatives).
//! - `Dict` / `List` become structure ([`Mapping`] / [`Sequence`]); child
//!   values lower recursively with structure-derived kinds.
//! - `Overlay` (dict entries over a fallback) lowers to sibling arms: the
//!   entry mapping and the fallback are both contribution candidates. This
//!   degrades merge semantics to arm alternation, which is projection-
//!   equivalent (every arm is walked) and keeps the domain free of a
//!   dedicated overlay node.
//! - `Choice` lowers to guarded arms; all plain string members merge into
//!   one text arm first so finite scalar alternatives stay a single scalar.
//! - `Widened` (dataflow through an unknown call) becomes [`Opaque`] taint:
//!   the content is unknown but the influencing paths attribute
//!   conservatively at the position.
//! - `Top` / `Unknown` / `RootContext` become empty-taint [`Opaque`] nodes:
//!   unknown content with no attributable influence.
//!
//! For partial scalars ([`lower_value_scalar_arms`]) the same rules apply
//! part-wise: alternation inside one hole degrades to a *contribution set*
//! of parts on one arm (matching how the current pipeline attributes every
//! path of a hole at the slot), except that helper predicate branches still
//! split into guarded arms so their conditions survive in the tree.

use std::collections::BTreeSet;

use crate::ValueKind;
use crate::abstract_value::{AbstractValue, path_is_encoded};
use crate::helper_summary::HelperOutputMeta;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Opaque,
    PathCondition, Sequence, Splice, SpliceMeta, StringPart,
};

/// Bound on guarded-arm fan-out when lowering scalar alternatives. Beyond
/// the cap the alternatives collapse into one unconditional contribution-set
/// arm (conditions dropped, meta kept) so pathological templates stay
/// bounded.
pub(crate) const MAX_SCALAR_ARMS: usize = 8;

/// Effect-derived context under which a hole's value lowers: which paths the
/// expression defaulted or encoded, and which paths carry chart-level `set …
/// default` normalization.
pub(crate) struct LowerScope<'a> {
    pub(crate) defaulted_paths: &'a BTreeSet<String>,
    pub(crate) encoded_paths: &'a BTreeSet<String>,
    pub(crate) chart_value_defaults: &'a BTreeSet<String>,
}

impl LowerScope<'_> {
    pub(super) fn splice(
        &self,
        path: &str,
        kind: ValueKind,
        helper_meta: Option<&HelperOutputMeta>,
    ) -> Splice {
        let defaulted = helper_meta.is_some_and(|meta| meta.defaulted)
            || self.defaulted_paths.contains(path)
            || self.chart_value_defaults.contains(path);
        Splice {
            values_path: path.to_string(),
            kind,
            meta: SpliceMeta {
                defaulted,
                encoded: path_is_encoded(path, self.encoded_paths),
                provenance: helper_meta
                    .map(|meta| meta.provenance.clone())
                    .unwrap_or_default(),
            },
        }
    }
}

/// The helper meta's predicate branches as arm conditions (one unconditional
/// arm when the meta records none).
fn helper_meta_conditions(meta: &HelperOutputMeta) -> Vec<PathCondition> {
    if meta.predicates.is_empty() {
        return vec![Predicate::True];
    }
    meta.predicates
        .iter()
        .map(|branch| Predicate::all(branch.iter().cloned().collect()))
        .collect()
}

/// Lower a hole value that stands as an entire fragment position (an entry
/// value, a sequence item, or a standalone output line).
pub(crate) fn lower_value(
    value: &AbstractValue,
    kind: ValueKind,
    scope: &LowerScope<'_>,
) -> Guarded<AbstractFragment> {
    match value {
        AbstractValue::Top | AbstractValue::Unknown | AbstractValue::RootContext => {
            Guarded::unconditional(AbstractFragment::Opaque(Opaque::default()))
        }
        AbstractValue::ValuesPath(path) => {
            if path.is_empty() {
                Guarded::unconditional(AbstractFragment::Opaque(Opaque::default()))
            } else {
                Guarded::unconditional(AbstractFragment::Splice(scope.splice(path, kind, None)))
            }
        }
        AbstractValue::OutputPath(path, meta) => {
            let mut out = Guarded::empty();
            for condition in helper_meta_conditions(meta) {
                out.arms.push((
                    condition,
                    AbstractFragment::Splice(scope.splice(path, kind, Some(meta))),
                ));
            }
            out
        }
        AbstractValue::StringSet(strings) => {
            Guarded::unconditional(AbstractFragment::Scalar(AbstractString {
                parts: vec![StringPart::Text(strings.clone())],
                suppressed: false,
            }))
        }
        AbstractValue::Dict(entries) => {
            Guarded::unconditional(AbstractFragment::Mapping(lower_entries(entries, scope)))
        }
        AbstractValue::List(items) => {
            let items = items
                .iter()
                .map(|item| lower_value(item, structure_child_kind(item), scope))
                .collect();
            Guarded::unconditional(AbstractFragment::Sequence(Sequence { items }))
        }
        AbstractValue::Overlay { entries, fallback } => {
            let mut out =
                Guarded::unconditional(AbstractFragment::Mapping(lower_entries(entries, scope)));
            out.extend(lower_value(fallback, kind, scope));
            out
        }
        AbstractValue::Choice(choices) => {
            let mut strings = BTreeSet::new();
            let mut out = Guarded::empty();
            for choice in choices {
                if let AbstractValue::StringSet(members) = choice {
                    strings.extend(members.iter().cloned());
                } else {
                    out.extend(lower_value(choice, kind, scope));
                }
            }
            if !strings.is_empty() {
                out.arms.push((
                    Predicate::True,
                    AbstractFragment::Scalar(AbstractString {
                        parts: vec![StringPart::Text(strings)],
                        suppressed: false,
                    }),
                ));
            }
            out
        }
        AbstractValue::Widened(paths) => Guarded::unconditional(AbstractFragment::Opaque(Opaque {
            taint: paths.clone(),
        })),
    }
}

fn lower_entries(
    entries: &std::collections::BTreeMap<String, AbstractValue>,
    scope: &LowerScope<'_>,
) -> Mapping {
    Mapping {
        entries: entries
            .iter()
            .map(|(key, value)| MappingEntry {
                key: EntryKey::Literal(key.clone()),
                value: lower_value(value, structure_child_kind(value), scope),
            })
            .collect(),
    }
}

fn structure_child_kind(value: &AbstractValue) -> ValueKind {
    match value {
        AbstractValue::Dict(_) | AbstractValue::List(_) | AbstractValue::Overlay { .. } => {
            ValueKind::Fragment
        }
        AbstractValue::Choice(choices)
            if choices.iter().any(|choice| {
                matches!(
                    choice,
                    AbstractValue::Dict(_) | AbstractValue::List(_) | AbstractValue::Overlay { .. }
                )
            }) =>
        {
            ValueKind::Fragment
        }
        _ => ValueKind::Scalar,
    }
}

/// Lower a hole value rendered *inside* a partial scalar: guarded arms of
/// part lists. One hole usually yields a single arm; helper predicate
/// branches split into arms so their conditions stay in the tree.
pub(crate) fn lower_value_scalar_arms(
    value: &AbstractValue,
    scope: &LowerScope<'_>,
) -> Vec<(PathCondition, Vec<StringPart>)> {
    match value {
        AbstractValue::Top | AbstractValue::Unknown | AbstractValue::RootContext => Vec::new(),
        AbstractValue::ValuesPath(path) => {
            if path.is_empty() {
                Vec::new()
            } else {
                vec![(
                    Predicate::True,
                    vec![StringPart::Splice(scope.splice(
                        path,
                        ValueKind::PartialScalar,
                        None,
                    ))],
                )]
            }
        }
        AbstractValue::OutputPath(path, meta) => helper_meta_conditions(meta)
            .into_iter()
            .map(|condition| {
                (
                    condition,
                    vec![StringPart::Splice(scope.splice(
                        path,
                        ValueKind::PartialScalar,
                        Some(meta),
                    ))],
                )
            })
            .collect(),
        AbstractValue::StringSet(strings) => {
            vec![(Predicate::True, vec![StringPart::Text(strings.clone())])]
        }
        AbstractValue::Dict(_) | AbstractValue::List(_) | AbstractValue::Overlay { .. } => {
            let taint = value.fragment_rendered_paths();
            vec![(Predicate::True, vec![StringPart::Taint(taint)])]
        }
        AbstractValue::Choice(choices) => {
            let mut base_parts = Vec::new();
            let mut conditional_arms = Vec::new();
            for choice in choices {
                for (condition, parts) in lower_value_scalar_arms(choice, scope) {
                    if condition == Predicate::True {
                        base_parts.extend(parts);
                    } else {
                        conditional_arms.push((condition, parts));
                    }
                }
            }
            let mut arms = Vec::new();
            if !base_parts.is_empty() || conditional_arms.is_empty() {
                arms.push((Predicate::True, base_parts));
            }
            arms.extend(conditional_arms);
            if arms.len() > MAX_SCALAR_ARMS {
                let parts = arms.into_iter().flat_map(|(_, parts)| parts).collect();
                return vec![(Predicate::True, parts)];
            }
            arms
        }
        AbstractValue::Widened(paths) => {
            vec![(Predicate::True, vec![StringPart::Taint(paths.clone())])]
        }
    }
}
