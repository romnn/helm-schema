//! Generation-decision reports for one values path.
//!
//! A report exports the typed decision records one emitter run captured for a
//! path. It is an export boundary, not a stored second model: guard records
//! referenced by position are read from the contract signals that own them.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use helm_schema_core::{ConditionalGuard, ContractPathSchemaEvidence, ValuesPath};
use serde::Serialize;
use serde_json::Value;

pub use helm_schema_gen::{
    BaseOwner, BaseOwnerDecision, BaseOwnerRule, ChannelAdjustment, ChannelDisposition,
    ConditionalBaseEffect, ContainmentCheck, ContainmentDecision, ContainmentShortCircuit,
    DropReason, EmissionOrigin, FalsyEscapeReason, GenerationDecisions, ImplicationRef,
    IndependentChannels, IndependentQualification, JsonSchemaType, MergeBase, NullAdmissionReason,
    OverlayResolution, PathGenerationDecision, PathResolution, PolicyEvaluation, PolicyRule,
    QualifiedContract,
};

/// Version of the generation-decision report layout.
pub const REPORT_FORMAT_VERSION: u32 = 1;

/// Output format of a generation-decision report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainFormat {
    /// Versioned JSON with sorted keys, two-space indentation and one final
    /// newline.
    Json,
    /// The same records as an indented text tree.
    Text,
}

#[derive(Serialize)]
struct GenerationReport<'a> {
    format_version: u32,
    scope: &'static str,
    path: &'a ValuesPath,
    status: ReportStatus,
    coverage: Coverage,
    generation: Option<&'a PathGenerationDecision>,
    references: GuardReferences<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ReportStatus {
    Observed,
    NotObserved,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum PhaseCoverage {
    Recorded,
    NotRecorded,
}

/// Which explanation phases this report layout records.
#[derive(Serialize)]
struct Coverage {
    generation: PhaseCoverage,
    sources: PhaseCoverage,
    inputs: PhaseCoverage,
    interpreter: PhaseCoverage,
    signals: PhaseCoverage,
    providers: PhaseCoverage,
    emission: PhaseCoverage,
}

/// Guards of the overlays and requirement implications the decisions
/// reference by position, in position order. Backprojected implications are
/// synthesized during lowering and keep no guard record to reference.
#[derive(Serialize)]
struct GuardReferences<'a> {
    overlays: Vec<GuardReference<'a>>,
    requirement_implications: Vec<GuardReference<'a>>,
}

/// The guards of one referenced record, by its position in the owning signal
/// vector.
#[derive(Serialize)]
struct GuardReference<'a> {
    index: usize,
    guards: &'a [ConditionalGuard],
}

fn guard_references(references: BTreeMap<usize, &[ConditionalGuard]>) -> Vec<GuardReference<'_>> {
    references
        .into_iter()
        .map(|(index, guards)| GuardReference { index, guards })
        .collect()
}

pub(crate) fn generation_report(
    path: &ValuesPath,
    decision: Option<&PathGenerationDecision>,
    evidence: Option<&ContractPathSchemaEvidence>,
    format: ExplainFormat,
) -> Result<String, serde_json::Error> {
    let mut overlays = BTreeMap::new();
    let mut requirement_implications = BTreeMap::new();
    if let (Some(decision), Some(evidence)) = (decision, evidence) {
        for overlay in &decision.overlays {
            if let Some(source) = evidence.conditional_overlays.get(overlay.overlay) {
                overlays.insert(overlay.overlay, source.guards.as_slice());
            }
        }
        for containment in &decision.containment_checks {
            let implication = containment.implication;
            if implication.origin == EmissionOrigin::RequirementImplication
                && let Some(source) = evidence.requirement_implications.get(implication.index)
            {
                requirement_implications.insert(implication.index, source.outer_guards.as_slice());
            }
        }
    }
    let report = GenerationReport {
        format_version: REPORT_FORMAT_VERSION,
        scope: "generation_decisions",
        path,
        status: if decision.is_some() {
            ReportStatus::Observed
        } else {
            ReportStatus::NotObserved
        },
        coverage: Coverage {
            generation: PhaseCoverage::Recorded,
            sources: PhaseCoverage::NotRecorded,
            inputs: PhaseCoverage::NotRecorded,
            interpreter: PhaseCoverage::NotRecorded,
            signals: PhaseCoverage::NotRecorded,
            providers: PhaseCoverage::NotRecorded,
            emission: PhaseCoverage::NotRecorded,
        },
        generation: decision,
        references: GuardReferences {
            overlays: guard_references(overlays),
            requirement_implications: guard_references(requirement_implications),
        },
    };
    // Serializing through `Value` sorts every object's keys.
    let report = serde_json::to_value(&report)?;
    Ok(match format {
        ExplainFormat::Json => serde_json::to_string_pretty(&report)? + "\n",
        ExplainFormat::Text => render_text(&report),
    })
}

fn render_text(report: &Value) -> String {
    let mut text = String::new();
    if let Value::Object(fields) = report {
        for (key, value) in fields {
            write_text_node(&mut text, 0, key, value);
        }
    }
    text
}

/// Writes a record whose members are all scalars on one line, and any other
/// record as a label followed by its indented members.
fn write_text_node(text: &mut String, depth: usize, label: &str, value: &Value) {
    let indent = "  ".repeat(depth);
    if let Some(inline) = inline_text(value) {
        let _ = writeln!(text, "{indent}{label}: {inline}");
        return;
    }
    let _ = writeln!(text, "{indent}{label}:");
    match value {
        Value::Object(fields) => {
            for (key, child) in fields {
                write_text_node(text, depth + 1, key, child);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                write_text_node(text, depth + 1, &format!("[{index}]"), item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn inline_text(value: &Value) -> Option<String> {
    match value {
        Value::Object(fields) if fields.is_empty() => Some("{}".to_string()),
        Value::Object(fields) => {
            let members = fields
                .iter()
                .map(|(key, member)| {
                    scalar_list_text(member).map(|member| format!("{key}={member}"))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(members.join(" "))
        }
        _ => scalar_list_text(value),
    }
}

fn scalar_list_text(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) => {
            let items = items.iter().map(scalar_text).collect::<Option<Vec<_>>>()?;
            Some(format!("[{}]", items.join(", ")))
        }
        _ => scalar_text(value),
    }
}

/// A scalar as text. A string stays bare only when it is a plain word that
/// no other scalar prints as; any other string is a quoted, escaped JSON
/// string, so `"false"` never reads as `false` and data cannot break the
/// tree's lines.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if is_bare_word(text) => Some(text.clone()),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            Some(value.to_string())
        }
        Value::Array(_) | Value::Object(_) => None,
    }
}

fn is_bare_word(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-/*".contains(character))
        && serde_json::from_str::<Value>(text).is_err()
}
