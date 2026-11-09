//! A compact, deterministic text rendering of an evaluated document for
//! golden tests: one node per line, arms prefixed with their condition,
//! reads listed after the tree.
//!
//! The view is normalized so that it states placement structure, not one
//! interpreter's representation of it, and loses nothing:
//! - the document's validity domain (the kind and spelling tests of every
//!   rendered value) prints once, as `valid:`, and every arm prints its
//!   condition relative to that domain and to its ancestors' conditions;
//! - a whole-value splice's Go-format arms (scalar spelling, `map[…]`,
//!   `[…]`) fold into one `splice … scalar` only when they differ solely in
//!   that value's language, so their disjunction covers the same control;
//! - sibling mapping layouts that together cover their context merge into one
//!   mapping whose entries keep their arm conditions;
//! - reads print as a sorted set, each under its execution guards.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::{Guard, GuardValue, ValueKind};
use helm_schema_core::{Predicate, PredicateKind, ValuesPath};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, Sequence, Splice, StringPart,
};

/// Render an evaluated document as a deterministic dump.
#[must_use]
pub(crate) fn dump_document(document: &crate::tests::support::EvaluatedDocument) -> String {
    let document = &document.inspection;
    let mut out = String::new();
    let mut language = BTreeSet::new();
    collect_rendered(&document.root, &mut language);
    let folded = fold_tree(&document.root, &language);
    let mut domain = Vec::new();
    validity_domain(&folded, &language, &mut domain);
    let valid = Predicate::all(domain).normalize_boolean();
    if valid != Predicate::True {
        let _ = writeln!(out, "valid: {}", fmt_condition(&valid));
    }
    dump_guarded(&normalize_guarded(&folded, &valid), &valid, 0, &mut out);
    let reads = document
        .reads
        .iter()
        .map(|read| {
            format!(
                "  {} [{}]",
                read.values_path.encode(),
                read.condition
                    .guard_conjunctions()
                    .iter()
                    .flatten()
                    .map(fmt_guard)
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
        .collect::<BTreeSet<_>>();
    if !reads.is_empty() {
        let _ = writeln!(out, "reads:");
        for read in reads {
            let _ = writeln!(out, "{read}");
        }
    }
    out
}

/// Whether a guard tests the kind or spelling of `path`'s rendered value.
fn value_language_atom(guard: &Guard, path: &ValuesPath) -> bool {
    match guard {
        Guard::TypeIs {
            path: tested,
            schema_type: _,
        }
        | Guard::NotTypeIs {
            path: tested,
            schema_type: _,
        }
        | Guard::MatchesPattern {
            path: tested,
            pattern: _,
            templated: _,
        }
        | Guard::NotMatchesPattern {
            path: tested,
            pattern: _,
        }
        | Guard::MatchesValue {
            path: tested,
            pattern: _,
        }
        | Guard::CollectionPattern {
            path: tested,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | Guard::Absent { path: tested } => tested == path,
        // Boolean, nil and empty spellings: plain-scalar spelling and `required` abort tests.
        Guard::Eq {
            path: tested,
            value,
        }
        | Guard::NotEq {
            path: tested,
            value,
        } => {
            tested == path
                && match value {
                    GuardValue::Bool(_) | GuardValue::Null => true,
                    GuardValue::String(text) => text.is_empty(),
                    GuardValue::Int(_) | GuardValue::Float(_) => false,
                }
        }
        Guard::Truthy { path: _ }
        | Guard::Not { path: _ }
        | Guard::RangeKeyPrefix { path: _, prefix: _ }
        | Guard::RangeKeyEquals { path: _, key: _ }
        | Guard::RangeKeyMatches {
            path: _,
            pattern: _,
        }
        | Guard::Or { paths: _ }
        | Guard::AnyOf { alternatives: _ }
        | Guard::Range { path: _ }
        | Guard::With { path: _ }
        | Guard::Default { path: _ }
        | Guard::IntGt { path: _, bound: _ }
        | Guard::IntLt { path: _, bound: _ }
        | Guard::AtMostOneMember { path: _ }
        | Guard::MinMembers { path: _, bound: _ }
        | Guard::HasKey { path: _, key: _ }
        | Guard::NotHasKey { path: _, key: _ }
        | Guard::ContainsEquals { path: _, value: _ }
        | Guard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | Guard::ContainsTruthyMember { path: _, member: _ } => false,
    }
}

/// Whether every atom of `predicate` tests the kind or spelling of a rendered value.
fn pure_language(predicate: &Predicate, language: &BTreeSet<ValuesPath>) -> bool {
    match predicate.kind() {
        PredicateKind::Guard(guard) => language.iter().any(|path| value_language_atom(guard, path)),
        PredicateKind::Not(inner) => pure_language(inner, language),
        PredicateKind::And(parts) | PredicateKind::Or(parts) => {
            parts.iter().all(|part| pure_language(part, language))
        }
        PredicateKind::True | PredicateKind::False | PredicateKind::Approximate { .. } => false,
    }
}

/// Splits a condition into its control conjuncts and its value-language conjuncts.
fn split_language(
    condition: &Predicate,
    language: &BTreeSet<ValuesPath>,
) -> (Predicate, Predicate) {
    let parts = match condition.kind() {
        PredicateKind::And(parts) => parts.to_vec(),
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Guard(_)
        | PredicateKind::Not(_)
        | PredicateKind::Or(_)
        | PredicateKind::Approximate { .. } => vec![condition.clone()],
    };
    let (pure, control): (Vec<_>, Vec<_>) = parts
        .into_iter()
        .partition(|part| pure_language(part, language));
    (Predicate::all(control), Predicate::all(pure))
}

/// Collects, for every arm, "its control implies its value-language conjuncts": together they
/// are the document's validity domain, printed once instead of on every arm.
fn validity_domain(
    guarded: &Guarded<AbstractFragment>,
    language: &BTreeSet<ValuesPath>,
    domain: &mut Vec<Predicate>,
) {
    for (condition, fragment) in &guarded.arms {
        let (control, pure) = split_language(condition, language);
        if pure != Predicate::True {
            domain.push(Predicate::Or(vec![control.negated(), pure]));
        }
        match fragment {
            AbstractFragment::Mapping(mapping) => {
                for entry in &mapping.entries {
                    validity_domain(&entry.value, language, domain);
                }
            }
            AbstractFragment::Sequence(sequence) => {
                for item in &sequence.items {
                    validity_domain(item, language, domain);
                }
            }
            AbstractFragment::Scalar(_) | AbstractFragment::Splice(_) => {}
        }
    }
}

/// Folds Go-format arms throughout the tree.
fn fold_tree(
    guarded: &Guarded<AbstractFragment>,
    language: &BTreeSet<ValuesPath>,
) -> Guarded<AbstractFragment> {
    Guarded {
        arms: fold_go_format_arms(guarded, language)
            .into_iter()
            .map(|(condition, fragment)| {
                let fragment = match fragment {
                    AbstractFragment::Mapping(mapping) => AbstractFragment::Mapping(Mapping {
                        entries: mapping
                            .entries
                            .into_iter()
                            .map(|entry| super::domain::MappingEntry {
                                value: fold_tree(&entry.value, language),
                                key: entry.key,
                            })
                            .collect(),
                    }),
                    AbstractFragment::Sequence(sequence) => AbstractFragment::Sequence(Sequence {
                        items: sequence
                            .items
                            .iter()
                            .map(|item| fold_tree(item, language))
                            .collect(),
                    }),
                    AbstractFragment::Scalar(_) | AbstractFragment::Splice(_) => fragment,
                };
                (condition, fragment)
            })
            .collect(),
    }
}

/// Every path the document renders: their kind and spelling tests guard the whole document's
/// validity, so they recur on every arm, not only on the arm that renders them.
fn collect_rendered(guarded: &Guarded<AbstractFragment>, paths: &mut BTreeSet<ValuesPath>) {
    for (_, fragment) in &guarded.arms {
        paths.extend(rendered_paths(fragment));
        match fragment {
            AbstractFragment::Mapping(mapping) => {
                for entry in &mapping.entries {
                    collect_rendered(&entry.value, paths);
                }
            }
            AbstractFragment::Sequence(sequence) => {
                for item in &sequence.items {
                    collect_rendered(item, paths);
                }
            }
            AbstractFragment::Scalar(_) | AbstractFragment::Splice(_) => {}
        }
    }
}

/// The paths a fragment renders at its own position (not inside nested containers).
fn rendered_paths(fragment: &AbstractFragment) -> BTreeSet<ValuesPath> {
    match fragment {
        AbstractFragment::Splice(splice) => BTreeSet::from([splice.values_path.clone()]),
        AbstractFragment::Scalar(scalar) => scalar
            .parts
            .iter()
            .filter_map(|part| match part {
                StringPart::Splice(splice) => Some(splice.values_path.clone()),
                StringPart::Text(_) | StringPart::Taint(_) => None,
            })
            .collect(),
        AbstractFragment::Mapping(_) | AbstractFragment::Sequence(_) => BTreeSet::new(),
    }
}

/// The path of a whole-value splice arm: a bare splice or a scalar of exactly one splice.
fn whole_splice(fragment: &AbstractFragment) -> Option<&Splice> {
    match fragment {
        AbstractFragment::Splice(splice) => Some(splice),
        AbstractFragment::Scalar(AbstractString { parts, suppressed }) => match parts.as_slice() {
            [StringPart::Splice(splice)] if !suppressed && splice.kind == ValueKind::Scalar => {
                Some(splice)
            }
            _ => None,
        },
        AbstractFragment::Mapping(_) | AbstractFragment::Sequence(_) => None,
    }
}

/// Folds the Go-format arms (`map[…]`, `[…]`) of one whole-value splice into its scalar arm.
fn fold_go_format_arms(
    guarded: &Guarded<AbstractFragment>,
    language: &BTreeSet<ValuesPath>,
) -> Vec<(Predicate, AbstractFragment)> {
    let mut arms: Vec<(Predicate, AbstractFragment)> = Vec::new();
    let mut folded = vec![false; guarded.arms.len()];
    for (index, (condition, fragment)) in guarded.arms.iter().enumerate() {
        if folded.get(index).copied().unwrap_or(true) {
            continue;
        }
        let scalar = whole_splice(fragment).filter(|splice| splice.kind == ValueKind::Scalar);
        let Some(scalar) = scalar else {
            arms.push((condition.clone(), fragment.clone()));
            continue;
        };
        let mut conditions = vec![condition.clone()];
        let (control, _) = split_language(condition, language);
        for (other, (other_condition, other_fragment)) in guarded.arms.iter().enumerate() {
            if let Some(splice) = whole_splice(other_fragment)
                && splice.values_path == scalar.values_path
                && matches!(
                    splice.kind,
                    ValueKind::GoFormattedMap | ValueKind::GoFormattedSequence
                )
                && split_language(other_condition, language).0 == control
                && let Some(mark) = folded.get_mut(other)
                && !*mark
            {
                *mark = true;
                conditions.push(other_condition.clone());
            }
        }
        let condition = if conditions.len() == 1 {
            conditions.swap_remove(0)
        } else {
            Predicate::Or(conditions).normalize_boolean()
        };
        arms.push((condition, AbstractFragment::Splice(scalar.clone())));
    }
    arms
}

fn normalize_guarded(
    guarded: &Guarded<AbstractFragment>,
    context: &Predicate,
) -> Guarded<AbstractFragment> {
    let mut normalized: Vec<(Predicate, AbstractFragment)> = guarded
        .arms
        .iter()
        .map(|(condition, fragment)| {
            let inner = Predicate::all(vec![context.clone(), condition.clone()]);
            (condition.clone(), normalize_fragment(fragment, &inner))
        })
        .filter(|(condition, _)| *condition != Predicate::False)
        .collect();
    // Mapping layouts that together cover every state of their context share one lexical shell:
    // merge them into one mapping whose entries keep their arm conditions.
    let mapping_conditions = normalized
        .iter()
        .filter(|(_, fragment)| matches!(fragment, AbstractFragment::Mapping(_)))
        .map(|(condition, _)| condition.clone())
        .collect::<Vec<_>>();
    if mapping_conditions.len() > 1 && context.exactly_implies(&Predicate::Or(mapping_conditions)) {
        let mut merged = Mapping::default();
        let mut conditions = Vec::new();
        let mut first = None;
        let mut rest = Vec::new();
        for (index, (condition, fragment)) in normalized.into_iter().enumerate() {
            let AbstractFragment::Mapping(mapping) = fragment else {
                rest.push((condition, fragment));
                continue;
            };
            first.get_or_insert(index);
            for entry in mapping.entries {
                let mut value = entry.value;
                for (arm_condition, _) in &mut value.arms {
                    *arm_condition = Predicate::all(vec![condition.clone(), arm_condition.clone()])
                        .normalize_boolean();
                }
                if let Some(existing) = merged
                    .entries
                    .iter_mut()
                    .find(|known| known.key == entry.key)
                {
                    existing.value.extend(value);
                } else {
                    merged.entries.push(super::domain::MappingEntry {
                        key: entry.key,
                        value,
                    });
                }
            }
            conditions.push(condition);
        }
        for entry in &mut merged.entries {
            entry.value = normalize_guarded(&entry.value, context);
        }
        let mut result = rest;
        result.insert(
            first.unwrap_or_default().min(result.len()),
            (
                Predicate::Or(conditions).normalize_boolean(),
                AbstractFragment::Mapping(merged),
            ),
        );
        normalized = result;
    }
    // Arms that render the same fragment are one arm under the disjunction of their conditions.
    let mut arms: Vec<(Predicate, AbstractFragment)> = Vec::new();
    for (condition, fragment) in normalized {
        if let Some((known, _)) = arms.iter_mut().find(|(_, known)| *known == fragment) {
            *known = Predicate::Or(vec![known.clone(), condition]).normalize_boolean();
        } else {
            arms.push((condition, fragment));
        }
    }
    Guarded { arms }
}

fn normalize_fragment(fragment: &AbstractFragment, context: &Predicate) -> AbstractFragment {
    match fragment {
        AbstractFragment::Mapping(mapping) => AbstractFragment::Mapping(Mapping {
            entries: mapping
                .entries
                .iter()
                .map(|entry| super::domain::MappingEntry {
                    key: entry.key.clone(),
                    value: normalize_guarded(&entry.value, context),
                })
                .collect(),
        }),
        AbstractFragment::Sequence(sequence) => AbstractFragment::Sequence(Sequence {
            items: sequence
                .items
                .iter()
                .map(|item| normalize_guarded(item, context))
                .collect(),
        }),
        AbstractFragment::Scalar(_) | AbstractFragment::Splice(_) => fragment.clone(),
    }
}

/// `condition` relative to its ancestors' `context`: the conjuncts the context already implies drop out.
fn relative(condition: &Predicate, context: &Predicate) -> Predicate {
    if context.exactly_implies(condition) {
        return Predicate::True;
    }
    match condition.kind() {
        PredicateKind::And(parts) => {
            let kept = parts
                .iter()
                .filter(|part| !context.exactly_implies(part))
                .cloned()
                .collect::<Vec<_>>();
            if kept.len() == 1 {
                kept.into_iter().next().unwrap_or(Predicate::True)
            } else {
                Predicate::And(kept)
            }
        }
        PredicateKind::True
        | PredicateKind::False
        | PredicateKind::Guard(_)
        | PredicateKind::Not(_)
        | PredicateKind::Or(_)
        | PredicateKind::Approximate { .. } => condition.clone(),
    }
}

fn dump_guarded(
    guarded: &Guarded<AbstractFragment>,
    context: &Predicate,
    depth: usize,
    out: &mut String,
) {
    let pad = "  ".repeat(depth);
    for (condition, node) in &guarded.arms {
        let _ = writeln!(
            out,
            "{pad}when {}:",
            fmt_condition(&relative(condition, context))
        );
        let inner = Predicate::all(vec![context.clone(), condition.clone()]);
        dump_node(node, &inner, depth + 1, out);
    }
}

fn dump_node(node: &AbstractFragment, context: &Predicate, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    match node {
        AbstractFragment::Mapping(mapping) => {
            let _ = writeln!(out, "{pad}mapping:");
            for entry in &mapping.entries {
                match &entry.key {
                    EntryKey::Literal(key) => {
                        let _ = writeln!(out, "{pad}  key {key:?}:");
                    }
                    EntryKey::Dynamic(key) => {
                        let _ = writeln!(out, "{pad}  key dynamic {}:", fmt_string(key));
                    }
                }
                dump_guarded(&entry.value, context, depth + 2, out);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            let _ = writeln!(out, "{pad}sequence:");
            for item in &sequence.items {
                let _ = writeln!(out, "{pad}  item:");
                dump_guarded(item, context, depth + 2, out);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            let suppressed = if scalar.suppressed { " suppressed" } else { "" };
            let _ = writeln!(out, "{pad}scalar{suppressed} {}", fmt_string(scalar));
        }
        AbstractFragment::Splice(splice) => {
            let _ = writeln!(out, "{pad}{}", fmt_splice(splice));
        }
    }
}

fn fmt_string(string: &AbstractString) -> String {
    let parts: Vec<String> = string
        .parts
        .iter()
        .map(|part| match part {
            StringPart::Text(alternatives) => {
                let rendered: Vec<String> = alternatives
                    .iter()
                    .map(|text| format!("{text:?}"))
                    .collect();
                format!("text{{{}}}", rendered.join("|"))
            }
            StringPart::Splice(splice) => fmt_splice(splice),
            StringPart::Taint(taint) => {
                let rendered: Vec<String> = taint
                    .paths
                    .iter()
                    .map(helm_schema_core::ValuesPath::encode)
                    .collect();
                format!("taint{{{}}}", rendered.join(", "))
            }
        })
        .collect();
    format!("[{}]", parts.join(" "))
}

fn fmt_splice(splice: &Splice) -> String {
    let kind = match splice.kind {
        ValueKind::Scalar => "scalar",
        ValueKind::PartialScalar => "partial",
        ValueKind::Fragment => "fragment",
        ValueKind::Serialized => "serialized",
        ValueKind::YamlSerialized => "yaml-serialized",
        ValueKind::TemplatedYamlSerialized => "templated-yaml-serialized",
        ValueKind::WidenedDependency => "widened-dependency",
        ValueKind::WidenedRegion => "widened-region",
        ValueKind::GoFormattedMap => "go-formatted-map",
        ValueKind::GoFormattedSequence => "go-formatted-sequence",
    };
    format!("splice {} {kind}", splice.values_path.encode())
}

fn fmt_condition(condition: &Predicate) -> String {
    match condition.kind() {
        helm_schema_core::PredicateKind::True => "always".to_string(),
        helm_schema_core::PredicateKind::False => "never".to_string(),
        helm_schema_core::PredicateKind::Approximate { paths, .. } => {
            format!(
                "approximate({})",
                paths
                    .iter()
                    .map(helm_schema_core::ValuesPath::encode)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        helm_schema_core::PredicateKind::Guard(guard) => fmt_guard(guard),
        helm_schema_core::PredicateKind::Not(inner) => format!("!({})", fmt_condition(inner)),
        helm_schema_core::PredicateKind::And(parts) => {
            let rendered: Vec<String> = parts.iter().map(fmt_condition).collect();
            format!("({})", rendered.join(" && "))
        }
        helm_schema_core::PredicateKind::Or(parts) => {
            let rendered: Vec<String> = parts.iter().map(fmt_condition).collect();
            format!("({})", rendered.join(" || "))
        }
    }
}

fn fmt_guard(guard: &Guard) -> String {
    match guard {
        Guard::Truthy { path } => format!("truthy({})", path.encode()),
        Guard::Not { path } => format!("not({})", path.encode()),
        Guard::Eq { path, value } => format!("eq({} == {value})", path.encode()),
        Guard::NotEq { path, value } => format!("ne({} != {value})", path.encode()),
        Guard::Absent { path } => format!("absent({})", path.encode()),
        Guard::MatchesPattern {
            path,
            pattern,
            templated,
        } => {
            let suffix = if *templated { " templated" } else { "" };
            format!("matches({} ~ {pattern}{suffix})", path.encode())
        }
        Guard::MatchesValue { path, pattern } => {
            format!("matchesValue({}: {pattern:?})", path.encode())
        }
        Guard::CollectionPattern {
            path,
            domain,
            quantifier,
            pattern,
        } => {
            format!(
                "collectionPattern({}: {domain:?} {quantifier:?} ~ {pattern})",
                path.encode()
            )
        }
        Guard::NotMatchesPattern { path, pattern } => {
            format!("notMatches({} ~ {pattern})", path.encode())
        }
        Guard::RangeKeyPrefix { path, prefix } => {
            format!("rangeKeyPrefix({}: {prefix})", path.encode())
        }
        Guard::RangeKeyMatches { path, pattern } => {
            format!("rangeKeyMatches({} ~ {pattern})", path.encode())
        }
        Guard::AtMostOneMember { path } => format!("atMostOneMember({})", path.encode()),
        Guard::MinMembers { path, bound } => {
            format!("minMembers({} >= {bound})", path.encode())
        }
        Guard::HasKey { path, key } => format!("hasKey({}: {key})", path.encode()),
        Guard::NotHasKey { path, key } => format!("notHasKey({}: {key})", path.encode()),
        Guard::ContainsEquals { path, value } => {
            format!("containsEquals({} ∋ {value})", path.encode())
        }
        Guard::ContainsMemberEquals {
            path,
            member,
            value,
        } => format!(
            "containsMemberEquals({}.*.{member} == {value})",
            path.encode()
        ),
        Guard::ContainsTruthyMember { path, member } => {
            format!("containsTruthyMember({}.*.{member})", path.encode())
        }
        Guard::Or { paths } => format!(
            "or({})",
            paths
                .iter()
                .map(helm_schema_core::ValuesPath::encode)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Guard::AnyOf { alternatives } => {
            let rendered: Vec<String> = alternatives
                .iter()
                .map(|alternative| {
                    let guards: Vec<String> = alternative.iter().map(fmt_guard).collect();
                    format!("[{}]", guards.join(", "))
                })
                .collect();
            format!("anyOf({})", rendered.join(" | "))
        }
        Guard::Range { path } => format!("range({})", path.encode()),
        Guard::With { path } => format!("with({})", path.encode()),
        Guard::Default { path } => format!("default({})", path.encode()),
        Guard::TypeIs { path, schema_type } => {
            format!("typeIs({}: {schema_type})", path.encode())
        }
        Guard::NotTypeIs { path, schema_type } => {
            format!("notTypeIs({}: {schema_type})", path.encode())
        }
        Guard::IntGt { path, bound } => format!("intGt({} > {bound})", path.encode()),
        Guard::IntLt { path, bound } => format!("intLt({} < {bound})", path.encode()),
        Guard::RangeKeyEquals { path, key } => {
            format!("rangeKeyEquals({}[{key}])", path.encode())
        }
    }
}
