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
    PathCondition, Sequence, Splice, SpliceMeta, StringPart, TaintPart,
};

/// Bound on *correlated* guarded-arm fan-out (cross-segment products) when
/// lowering scalar alternatives. Beyond the cap the product degrades to
/// per-arm contributions that keep their own conditions (correlation is
/// dropped, conditions are not).
pub(crate) const MAX_SCALAR_ARMS: usize = 8;

/// Hard bound on total guarded arms per scalar hole. Beyond it the
/// alternatives collapse into one unconditional contribution-set arm
/// (conditions dropped, meta kept) so pathological templates stay bounded.
pub(crate) const MAX_SCALAR_ARM_FANOUT: usize = 64;

/// Effect-derived context under which a hole's value lowers: which paths the
/// expression defaulted or encoded, which paths carry chart-level `set …
/// default` normalization, and the per-path binding-time helper meta of
/// locals the expression read.
pub(crate) struct LowerScope<'a> {
    pub(crate) defaulted_paths: &'a BTreeSet<String>,
    pub(crate) encoded_paths: &'a BTreeSet<String>,
    pub(crate) chart_value_defaults: &'a BTreeSet<String>,
    pub(crate) local_output_meta: &'a std::collections::BTreeMap<String, HelperOutputMeta>,
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
                site: None,
            },
        }
    }

    /// The guarded splice arms for one rendered values path. A path that
    /// flowed through a local keeps the local's binding-time helper branch
    /// conditions (recorded in the hole's `local_output_meta`), splitting
    /// into per-branch arms exactly like a directly rendered helper value —
    /// transfer functions like `printf` collapse the value shape but the
    /// recorded meta keeps the per-path facts. Everything else lowers as one
    /// unconditional arm.
    pub(super) fn path_splice_arms(
        &self,
        path: &str,
        kind: ValueKind,
    ) -> Vec<(PathCondition, Splice)> {
        match self.local_output_meta.get(path) {
            Some(meta) if !meta.predicates.is_empty() => helper_meta_conditions(meta)
                .into_iter()
                .map(|condition| (condition, self.splice(path, kind, Some(meta))))
                .collect(),
            _ => vec![(Predicate::True, self.splice(path, kind, None))],
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
                let mut out = Guarded::empty();
                for (condition, splice) in scope.path_splice_arms(path, kind) {
                    out.arms.push((condition, AbstractFragment::Splice(splice)));
                }
                out
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
        AbstractValue::Widened(paths) => {
            // A widened transform still attributes exactly: paths whose
            // branch conditions are recorded (helper rows collapsed by
            // transfer functions like `printf … | trunc`) keep them as
            // guarded arms; the rest stay conservative taint.
            let mut out = Guarded::empty();
            let mut taint = BTreeSet::new();
            for path in paths {
                match scope.local_output_meta.get(path) {
                    Some(meta) if !meta.predicates.is_empty() || meta.defaulted => {
                        for (condition, splice) in scope.path_splice_arms(path, kind) {
                            out.arms.push((condition, AbstractFragment::Splice(splice)));
                        }
                    }
                    _ => {
                        taint.insert(path.clone());
                    }
                }
            }
            if !taint.is_empty() {
                out.arms.push((
                    Predicate::True,
                    AbstractFragment::Opaque(Opaque {
                        taint,
                        kind,
                        site: None,
                    }),
                ));
            }
            out
        }
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
/// branches split into arms so their conditions stay in the tree. `kind` is
/// the hole's own render kind: fragment-rendering holes (`toYaml …` inside a
/// block scalar) keep fragment evidence even though they sit in scalar text.
pub(crate) fn lower_value_scalar_arms(
    value: &AbstractValue,
    kind: ValueKind,
    scope: &LowerScope<'_>,
) -> Vec<(PathCondition, Vec<StringPart>)> {
    match value {
        AbstractValue::Top | AbstractValue::Unknown | AbstractValue::RootContext => Vec::new(),
        AbstractValue::ValuesPath(path) => {
            if path.is_empty() {
                Vec::new()
            } else {
                scope
                    .path_splice_arms(path, kind)
                    .into_iter()
                    .map(|(condition, splice)| (condition, vec![StringPart::Splice(splice)]))
                    .collect()
            }
        }
        AbstractValue::OutputPath(path, meta) => helper_meta_conditions(meta)
            .into_iter()
            .map(|condition| {
                (
                    condition,
                    vec![StringPart::Splice(scope.splice(path, kind, Some(meta)))],
                )
            })
            .collect(),
        AbstractValue::StringSet(strings) => {
            vec![(Predicate::True, vec![StringPart::Text(strings.clone())])]
        }
        AbstractValue::Dict(_) | AbstractValue::List(_) | AbstractValue::Overlay { .. } => {
            let taint = value.fragment_rendered_paths();
            vec![(
                Predicate::True,
                vec![StringPart::Taint(TaintPart::new(taint))],
            )]
        }
        AbstractValue::Choice(choices) => {
            let mut base_parts = Vec::new();
            let mut conditional_arms = Vec::new();
            for choice in choices {
                for (condition, parts) in lower_value_scalar_arms(choice, kind, scope) {
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
            if arms.len() > MAX_SCALAR_ARM_FANOUT {
                let parts = arms.into_iter().flat_map(|(_, parts)| parts).collect();
                return vec![(Predicate::True, parts)];
            }
            arms
        }
        AbstractValue::Widened(paths) => {
            let mut arms = Vec::new();
            let mut taint = BTreeSet::new();
            for path in paths {
                match scope.local_output_meta.get(path) {
                    Some(meta) if !meta.predicates.is_empty() || meta.defaulted => {
                        for (condition, splice) in scope.path_splice_arms(path, kind) {
                            arms.push((condition, vec![StringPart::Splice(splice)]));
                        }
                    }
                    _ => {
                        taint.insert(path.clone());
                    }
                }
            }
            if !taint.is_empty() {
                arms.push((
                    Predicate::True,
                    vec![StringPart::Taint(TaintPart::new(taint))],
                ));
            }
            arms
        }
    }
}
