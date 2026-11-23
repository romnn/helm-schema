//! Resident pinned-Go oracle and canonical comparison helpers.

use base64::Engine as _;
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_goyaml::{Error, Event, EventKind, ScalarStyle, Target, read_document, to_json};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

pub(super) const STRUCTURAL: &[u8] = b" \n-?:,[]{}#|>'\"\\";

pub(super) const WIDE: &[u8] = b" \n-?:,[]{}#|>'\"\\\t\r0123456789&*!%@.~`anux";

pub(super) fn encoded(input: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(input)
}

pub(super) fn error_summary(error: &Error) -> Value {
    json!({"kind":error.kind(),"line":error.line()})
}

pub(super) fn oracle_error(value: &Value) -> Value {
    if value.is_null() {
        Value::Null
    } else {
        json!({"kind":value["kind"],"line":value["line"]})
    }
}

pub(super) fn event_json(event: &Event) -> Value {
    let Event { kind, start, end } = event;
    let mut json = serde_json::Map::new();
    json.insert("start".to_owned(), json!(start));
    json.insert("end".to_owned(), json!(end));
    let (name, implicit) = match kind {
        EventKind::StreamStart => ("stream_start", false),
        EventKind::StreamEnd => ("stream_end", false),
        EventKind::DocumentStart { implicit } => ("document_start", *implicit),
        EventKind::DocumentEnd { implicit } => ("document_end", *implicit),
        EventKind::Alias(anchor) => {
            json.insert("anchor".to_owned(), json!(anchor));
            ("alias", false)
        }
        EventKind::Scalar {
            anchor,
            tag,
            value,
            style,
            implicit,
        } => {
            if !anchor.is_empty() {
                json.insert("anchor".to_owned(), json!(anchor));
            }
            if !tag.is_empty() {
                json.insert("tag".to_owned(), json!(tag));
            }
            if !value.is_empty() {
                json.insert("value".to_owned(), json!(value));
            }
            json.insert(
                "style".to_owned(),
                json!(match style {
                    ScalarStyle::Plain => "plain",
                    ScalarStyle::SingleQuoted => "single_quoted",
                    ScalarStyle::DoubleQuoted => "double_quoted",
                    ScalarStyle::Literal => "literal",
                    ScalarStyle::Folded => "folded",
                }),
            );
            ("scalar", *implicit)
        }
        EventKind::SequenceStart {
            anchor,
            tag,
            implicit,
        }
        | EventKind::MappingStart {
            anchor,
            tag,
            implicit,
        } => {
            if !anchor.is_empty() {
                json.insert("anchor".to_owned(), json!(anchor));
            }
            if !tag.is_empty() {
                json.insert("tag".to_owned(), json!(tag));
            }
            (
                if matches!(kind, EventKind::SequenceStart { .. }) {
                    "sequence_start"
                } else {
                    "mapping_start"
                },
                *implicit,
            )
        }
        EventKind::SequenceEnd => ("sequence_end", false),
        EventKind::MappingEnd => ("mapping_end", false),
    };
    json.insert("kind".to_owned(), json!(name));
    json.insert("implicit".to_owned(), json!(implicit));
    Value::Object(json)
}

pub(super) fn compare(input: &[u8], oracle: &Value, all_targets: bool) -> eyre::Result<()> {
    let document = read_document(input);
    let result = document.as_ref().map(|document| &document.value);
    let ours_error = document.as_ref().err().map_or(Value::Null, error_summary);
    let go_v2_error = oracle
        .pointer("/v2/error")
        .ok_or_eyre("missing oracle v2 error")?;
    let probe = oracle.get("probe").ok_or_eyre("missing oracle probe")?;
    let go_error = oracle_error(go_v2_error);
    if ours_error != go_error {
        eyre::bail!(
            "input {:?}: port error {ours_error}; Go {go_error}; port {result:?}; Go {oracle}",
            String::from_utf8_lossy(input)
        );
    }
    if let Ok(value) = result {
        let tagged = value.tagged();
        if tagged != *oracle.pointer("/v2/value").ok_or_eyre("missing v2 value")? {
            eyre::bail!(
                "input {:?}: port tree {tagged}; Go {}",
                String::from_utf8_lossy(input),
                *oracle.pointer("/v2/value").ok_or_eyre("missing v2 value")?
            );
        }
        let document = document.as_ref().map_err(Clone::clone)?;
        let events = &document.events;
        let expected_events = probe.get("events").ok_or_eyre("missing probe events")?;
        if !expected_events.is_null() {
            let actual: Vec<_> = events.iter().map(event_json).collect();
            if json!(actual) != *expected_events {
                eyre::bail!(
                    "input {:?}: port events {}; Go {expected_events}",
                    String::from_utf8_lossy(input),
                    json!(actual)
                );
            }
        }
        sim_assert_eq!(have: json!(document.end), want: probe.get("end").ok_or_eyre("missing probe end")?.clone());
        let targets: &[(Target, &str)] = if all_targets {
            &[
                (Target::Generic, "sigs"),
                (Target::Map, "map"),
                (Target::Array, "array"),
                (Target::Head, "head"),
            ]
        } else {
            &[(Target::Generic, "sigs")]
        };
        for (target, name) in targets {
            let actual = to_json(value, *target);
            let expected = &oracle[*name];
            if actual.is_ok() != expected["error"].is_null() {
                eyre::bail!(
                    "input {:?}, target {target:?}: port {actual:?}, Go {expected}",
                    String::from_utf8_lossy(input)
                );
            }
            if let Ok(actual) = actual
                && !json_equal(&actual, &expected["value"])
                && !(matches!(target, Target::Generic | Target::Map | Target::Array)
                    && has_coercion_collision(value)
                    && coercion_matches(value, &actual)
                    && coercion_matches(value, &expected["value"]))
            {
                eyre::bail!(
                    "input {:?}, target {target:?}: port {actual}, Go {}",
                    String::from_utf8_lossy(input),
                    expected["value"]
                );
            }
        }
    }
    Ok(())
}

pub(super) fn words(
    alphabet: &[u8],
    max: usize,
    mut visit: impl FnMut(&[u8]) -> eyre::Result<()>,
) -> eyre::Result<u64> {
    let mut count = 0;
    for length in 0..=max {
        let mut digits = vec![0; length];
        loop {
            let input: Vec<_> = digits
                .iter()
                .map(|index| {
                    alphabet
                        .get(*index)
                        .copied()
                        .ok_or_eyre("invalid exhaustive alphabet index")
                })
                .collect::<eyre::Result<_>>()?;
            visit(&input)?;
            count += 1;
            let mut carry = true;
            for digit in digits.iter_mut().rev() {
                *digit += 1;
                if *digit < alphabet.len() {
                    carry = false;
                    break;
                }
                *digit = 0;
            }
            if carry {
                break;
            }
        }
    }
    Ok(count)
}

fn json_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => {
            left.as_f64().map(f64::to_bits) == right.as_f64().map(f64::to_bits)
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| json_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right.get(key).is_some_and(|right| json_equal(value, right))
                })
        }
        _ => left == right,
    }
}

// Go ranges an unordered map before assigning keys into its string-keyed map.
// Distinct YAML keys can therefore select different values for one coerced key.
struct CoercedKey {
    raw: Vec<u8>,
    rendered: String,
}

struct CoercionGroup<'a> {
    rendered: String,
    choices: Vec<&'a helm_schema_goyaml::Value>,
}

fn key_identity(key: &helm_schema_goyaml::Value) -> Option<CoercedKey> {
    use helm_schema_goyaml::{Entry, Value as YamlValue};
    let singleton = YamlValue::Mapping(vec![Entry {
        key: key.clone(),
        value: YamlValue::Null,
        authored: false,
    }]);
    let json = to_json(&singleton, Target::Generic).ok()?;
    let rendered = json.as_object()?.keys().next()?.clone();
    let raw = match key {
        YamlValue::String(text) => text.as_bytes().to_vec(),
        YamlValue::RawString(bytes) => bytes.clone(),
        YamlValue::Bool(_) | YamlValue::Int(_) | YamlValue::Float(_) => {
            rendered.as_bytes().to_vec()
        }
        YamlValue::Null | YamlValue::Uint(_) | YamlValue::Sequence(_) | YamlValue::Mapping(_) => {
            return None;
        }
    };
    Some(CoercedKey { raw, rendered })
}

fn has_coercion_collision(source: &helm_schema_goyaml::Value) -> bool {
    use helm_schema_goyaml::Value as YamlValue;
    match source {
        YamlValue::Mapping(entries) => {
            let mut keys = std::collections::BTreeSet::new();
            entries.iter().any(|entry| {
                key_identity(&entry.key)
                    .is_some_and(|CoercedKey { raw, rendered: _ }| !keys.insert(raw))
                    || has_coercion_collision(&entry.value)
            })
        }
        YamlValue::Sequence(items) => items.iter().any(has_coercion_collision),
        YamlValue::Null
        | YamlValue::Bool(_)
        | YamlValue::Int(_)
        | YamlValue::Uint(_)
        | YamlValue::Float(_)
        | YamlValue::String(_)
        | YamlValue::RawString(_) => false,
    }
}

fn coercion_matches(source: &helm_schema_goyaml::Value, candidate: &Value) -> bool {
    use helm_schema_goyaml::Value as YamlValue;
    match source {
        YamlValue::Mapping(entries) => {
            let Some(candidate) = candidate.as_object() else {
                return false;
            };
            let mut raw_groups = std::collections::BTreeMap::new();
            for entry in entries {
                let Some(CoercedKey { raw, rendered }) = key_identity(&entry.key) else {
                    return false;
                };
                let group = raw_groups.entry(raw).or_insert_with(|| CoercionGroup {
                    rendered,
                    choices: Vec::new(),
                });
                group.choices.push(&entry.value);
            }
            let mut groups = std::collections::BTreeMap::new();
            for (_, CoercionGroup { rendered, choices }) in raw_groups {
                groups.insert(rendered, choices);
            }
            candidate.len() == groups.len()
                && groups.iter().all(|(key, choices)| {
                    candidate.get(key).is_some_and(|candidate| {
                        choices
                            .iter()
                            .any(|source| coercion_matches(source, candidate))
                    })
                })
        }
        YamlValue::Sequence(items) => candidate.as_array().is_some_and(|candidate| {
            items.len() == candidate.len()
                && items
                    .iter()
                    .zip(candidate)
                    .all(|(source, candidate)| coercion_matches(source, candidate))
        }),
        YamlValue::Null
        | YamlValue::Bool(_)
        | YamlValue::Int(_)
        | YamlValue::Uint(_)
        | YamlValue::Float(_)
        | YamlValue::String(_)
        | YamlValue::RawString(_) => {
            to_json(source, Target::Generic).is_ok_and(|value| json_equal(&value, candidate))
        }
    }
}

mod control;
mod reader_oracle;
