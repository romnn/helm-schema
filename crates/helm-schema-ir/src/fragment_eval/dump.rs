//! A compact, deterministic text rendering of an evaluated document for
//! golden tests: one node per line, arms prefixed with their condition,
//! reads listed after the tree.

use std::fmt::Write as _;

use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::domain::{AbstractFragment, AbstractString, EntryKey, Guarded, Splice, StringPart};
use super::eval::EvaluatedDocument;

/// Render an evaluated document as a deterministic dump.
#[must_use]
pub fn dump_document(document: &EvaluatedDocument) -> String {
    let mut out = String::new();
    dump_guarded(&document.root, 0, &mut out);
    if !document.reads.is_empty() {
        let _ = writeln!(out, "reads:");
        for read in &document.reads {
            let _ = writeln!(
                out,
                "  {} [{}]",
                read.values_path,
                read.condition
                    .guard_conjunctions()
                    .iter()
                    .flatten()
                    .map(fmt_guard)
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    out
}

fn dump_guarded(guarded: &Guarded<AbstractFragment>, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    for (condition, node) in &guarded.arms {
        let _ = writeln!(out, "{pad}when {}:", fmt_condition(condition));
        dump_node(node, depth + 1, out);
    }
}

fn dump_node(node: &AbstractFragment, depth: usize, out: &mut String) {
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
                dump_guarded(&entry.value, depth + 2, out);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            let _ = writeln!(out, "{pad}sequence:");
            for item in &sequence.items {
                let _ = writeln!(out, "{pad}  item:");
                dump_guarded(item, depth + 2, out);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            let suppressed = if scalar.suppressed { " suppressed" } else { "" };
            let _ = writeln!(out, "{pad}scalar{suppressed} {}", fmt_string(scalar));
        }
        AbstractFragment::Splice(splice) => {
            let _ = writeln!(out, "{pad}{}", fmt_splice(splice));
        }
        AbstractFragment::Opaque(opaque) => {
            if opaque.taint.is_empty() {
                let _ = writeln!(out, "{pad}opaque");
            } else {
                let taint: Vec<&str> = opaque.taint.iter().map(String::as_str).collect();
                let _ = writeln!(out, "{pad}opaque taint={{{}}}", taint.join(", "));
            }
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
                let rendered: Vec<&str> = taint.paths.iter().map(String::as_str).collect();
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
    };
    let mut rendered = format!("splice {} {kind}", splice.values_path);
    if splice.meta.defaulted {
        rendered.push_str(" defaulted");
    }
    if splice.meta.encoded {
        rendered.push_str(" encoded");
    }
    if splice.meta.range_key {
        rendered.push_str(" range-key");
    }
    rendered
}

fn fmt_condition(condition: &Predicate) -> String {
    match condition {
        Predicate::True => "always".to_string(),
        Predicate::False => "never".to_string(),
        Predicate::Approximate { paths, .. } => {
            format!(
                "approximate({})",
                paths.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        }
        Predicate::Guard(guard) => fmt_guard(guard),
        Predicate::Not(inner) => format!("!({})", fmt_condition(inner)),
        Predicate::And(parts) => {
            let rendered: Vec<String> = parts.iter().map(fmt_condition).collect();
            format!("({})", rendered.join(" && "))
        }
        Predicate::Or(parts) => {
            let rendered: Vec<String> = parts.iter().map(fmt_condition).collect();
            format!("({})", rendered.join(" || "))
        }
    }
}

fn fmt_guard(guard: &Guard) -> String {
    match guard {
        Guard::Truthy { path } => format!("truthy({path})"),
        Guard::Not { path } => format!("not({path})"),
        Guard::Eq { path, value } => format!("eq({path} == {value})"),
        Guard::NotEq { path, value } => format!("ne({path} != {value})"),
        Guard::Absent { path } => format!("absent({path})"),
        Guard::MatchesPattern {
            path,
            pattern,
            templated,
        } => {
            let suffix = if *templated { " templated" } else { "" };
            format!("matches({path} ~ {pattern}{suffix})")
        }
        Guard::RangeKeyPrefix { path, prefix } => {
            format!("rangeKeyPrefix({path}: {prefix})")
        }
        Guard::RangeKeyMatches { path, pattern } => {
            format!("rangeKeyMatches({path} ~ {pattern})")
        }
        Guard::Or { paths } => format!("or({})", paths.join(", ")),
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
        Guard::Range { path } => format!("range({path})"),
        Guard::With { path } => format!("with({path})"),
        Guard::Default { path } => format!("default({path})"),
        Guard::TypeIs { path, schema_type } => format!("typeIs({path}: {schema_type})"),
        Guard::NotTypeIs { path, schema_type } => format!("notTypeIs({path}: {schema_type})"),
        Guard::IntGt { path, bound } => format!("intGt({path} > {bound})"),
        Guard::IntLt { path, bound } => format!("intLt({path} < {bound})"),
        Guard::RangeKeyEquals { path, key } => format!("rangeKeyEquals({path}[{key}])"),
    }
}
