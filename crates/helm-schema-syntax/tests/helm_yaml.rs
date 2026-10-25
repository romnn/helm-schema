//! `helm_yaml_documents` against `sigs.k8s.io/yaml.YAMLToJSON`, the decoder
//! Helm's resource builder uses.
//!
//! `fixtures/helm_yaml_go_witness.json` holds each input with Go's JSON or
//! error, produced by Go 1.27.3 with `sigs.k8s.io/yaml` v1.6.0 over
//! `go.yaml.in/yaml/v2` v2.4.3; an output that changed across 64 repeated
//! decodes is recorded as the error `nondeterministic`.

use color_eyre::eyre;
use helm_schema_syntax::{HelmYamlDocument, HelmYamlKey, helm_yaml_documents, helm_yaml_key};
use indoc::indoc;
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Deserialize)]
struct GoOutcome {
    input: String,
    json: Option<String>,
    error: Option<String>,
}

/// Inputs Go decodes that this reader reports as unknown, with the value
/// and unknown pointers it reports instead.
fn widened(input: &str) -> Option<(Value, Vec<&'static str>)> {
    Some(match input {
        // An explicitly tagged scalar.
        "v: !!str yes\n" | "v: !!bool yes\n" | "v: ! yes\n" | "v: !foo bar\n" => {
            (json!({"v": null}), vec!["/v"])
        }
        "a: !!binary aGVsbG8=\n"
        | "a: !!merge <<\n"
        | "a: !!int '12'\n"
        | "a: !!float 1\n"
        | "a: !!timestamp 2001-12-14\n" => (json!({"a": null}), vec!["/a"]),
        // A key with a non-specific tag, a float key, and colliding
        // stringified keys.
        "! <<: {a: 1}\n"
        | "1.5: v\n"
        | "1e2: v\n"
        | "1.0: v\n"
        | ".inf: v\n"
        | "1: a\n\"1\": b\n"
        | "true: a\n\"true\": b\n"
        | "1.0: a\n1: b\n" => (Value::Null, vec![""]),
        _ => return None,
    })
}

/// `value` with each integral float in the 64-bit integer range as an
/// integer, the form Go's JSON encoding gives it.
fn integral(value: Value) -> Value {
    match value {
        Value::Number(number) => match number.as_f64() {
            Some(float) if number.is_f64() && float.fract() == 0.0 && float.abs() < 9.2e18 => {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the float is integral and within the i64 range"
                )]
                let integer = float as i64;
                Value::from(integer)
            }
            _ => Value::Number(number),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(integral).collect()),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (key, integral(value)))
                .collect(),
        ),
        value @ (Value::Null | Value::Bool(_) | Value::String(_)) => value,
    }
}

fn normalized(documents: Vec<HelmYamlDocument>) -> Vec<HelmYamlDocument> {
    documents
        .into_iter()
        .map(|document| match document {
            HelmYamlDocument::Decoded { value, unknown } => HelmYamlDocument::Decoded {
                value: integral(value),
                unknown,
            },
            HelmYamlDocument::Refused => HelmYamlDocument::Refused,
        })
        .collect()
}

#[test]
fn documents_decode_as_go_yaml_to_json() -> eyre::Result<()> {
    let outcomes: Vec<GoOutcome> =
        serde_json::from_str(include_str!("fixtures/helm_yaml_go_witness.json"))?;
    for outcome in outcomes {
        let want = match (widened(&outcome.input), &outcome.json, &outcome.error) {
            (Some((value, unknown)), _, _) => HelmYamlDocument::Decoded {
                value,
                unknown: unknown.into_iter().map(str::to_string).collect(),
            },
            (None, Some(json), None) => HelmYamlDocument::Decoded {
                value: integral(serde_json::from_str(json)?),
                unknown: Vec::new(),
            },
            (None, None, Some(_)) => HelmYamlDocument::Refused,
            (None, _, _) => eyre::bail!("malformed outcome for {:?}", outcome.input),
        };
        sim_assert_eq!(
            have: (outcome.input.as_str(), normalized(helm_yaml_documents(&outcome.input))),
            want: (outcome.input.as_str(), vec![want])
        );
    }
    Ok(())
}

#[test]
fn every_document_of_a_stream_decodes_separately() {
    let source = indoc! {"
        a: &x on
        ---
        b: *x
        ---
        c: [012, 0x1f]
    "};

    sim_assert_eq!(
        have: helm_yaml_documents(source),
        want: vec![
            HelmYamlDocument::Decoded {
                value: json!({"a": true}),
                unknown: Vec::new(),
            },
            HelmYamlDocument::Refused,
            HelmYamlDocument::Decoded {
                value: json!({"c": [10, 31]}),
                unknown: Vec::new(),
            },
        ]
    );
}

#[test]
fn unknown_values_are_reported_at_their_pointers() {
    let source = indoc! {"
        a/b:
          - x: !!int '1'
          - {1.5: y}
        c~d: {e: !!str f}
    "};

    sim_assert_eq!(
        have: helm_yaml_documents(source),
        want: vec![HelmYamlDocument::Decoded {
            value: json!({"a/b": [{"x": null}, null], "c~d": {"e": null}}),
            unknown: vec!["/a~1b/0/x".to_string(), "/a~1b/1".to_string(), "/c~0d/e".to_string()],
        }]
    );
}

#[test]
fn keys_read_as_helm_reads_them() {
    let keys = [
        "on", "N", "010", "+1", "'yes'", "\"<<\"", "<<", "1.5", "~", "!!str x", "[a]",
    ];

    sim_assert_eq!(
        have: keys.map(helm_yaml_key),
        want: [
            HelmYamlKey::Key("true".to_string()),
            HelmYamlKey::Key("false".to_string()),
            HelmYamlKey::Key("8".to_string()),
            HelmYamlKey::Key("1".to_string()),
            HelmYamlKey::Key("yes".to_string()),
            HelmYamlKey::Key("<<".to_string()),
            HelmYamlKey::Merge,
            HelmYamlKey::Unknown,
            HelmYamlKey::Unknown,
            HelmYamlKey::Unknown,
            HelmYamlKey::Unknown,
        ]
    );
}

#[test]
fn nesting_beyond_the_depth_limit_is_refused() {
    let deep = format!("{}{}", "[".repeat(129), "]".repeat(129));
    let shallow = format!("{}{}", "[".repeat(128), "]".repeat(128));

    sim_assert_eq!(have: helm_yaml_documents(&deep), want: vec![HelmYamlDocument::Refused]);
    sim_assert_eq!(
        have: matches!(
            helm_yaml_documents(&shallow).as_slice(),
            [HelmYamlDocument::Decoded { .. }]
        ),
        want: true
    );
}
