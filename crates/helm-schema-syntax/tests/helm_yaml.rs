//! The reader against Helm's resource decoding: k8s.io/apimachinery's
//! `YAMLReader` frames the documents and `sigs.k8s.io/yaml.YAMLToJSON`
//! decodes each one.
//!
//! The fixture records Go 1.27.3, apimachinery v0.36.1, `sigs.k8s.io/yaml`
//! v1.6.0 and `go.yaml.in/yaml/v2` v2.4.3, including 65 decodes per document
//! to expose nondeterministic map-key collisions.
//! Reader classes pin every intentional widening and distinguish unsupported
//! syntax from a known Go rejection.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_syntax::{
    HelmYamlDocument, HelmYamlError, HelmYamlKey, helm_yaml_documents, helm_yaml_key,
};
use indoc::indoc;
use serde::Deserialize;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Expected {
    Exact,
    Unknown,
    Rejected,
    Unsupported,
}

#[derive(Deserialize)]
struct GoOutcome {
    input: String,
    json: Option<String>,
    error: Option<String>,
    reader: Option<Expected>,
    documents: Option<Vec<GoDocument>>,
    review: bool,
}

#[derive(Deserialize)]
struct GoDocument {
    #[serde(default)]
    json: Value,
    error: Option<String>,
    reader: Expected,
}

/// Go's JSON encoding writes integral floats in integer form.
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
            HelmYamlDocument::Failed(err) => HelmYamlDocument::Failed(err),
        })
        .collect()
}

fn expected(document: GoDocument) -> eyre::Result<HelmYamlDocument> {
    let GoDocument {
        json,
        error,
        reader,
    } = document;
    Ok(match reader {
        Expected::Exact => {
            sim_assert_eq!(have: error, want: None);
            HelmYamlDocument::Decoded {
                value: integral(json),
                unknown: Vec::new(),
            }
        }
        Expected::Unknown => {
            // Go either decodes a finite float key or produces colliding-key alternatives.
            assert!(
                error
                    .as_deref()
                    .is_none_or(|error| error == "nondeterministic")
            );
            HelmYamlDocument::Decoded {
                value: Value::Null,
                unknown: vec![String::new()],
            }
        }
        Expected::Rejected => {
            let error = error.ok_or_eyre("Go must reject a reader-rejected document")?;
            assert_ne!(error, "nondeterministic");
            HelmYamlDocument::Failed(HelmYamlError::Rejected)
        }
        Expected::Unsupported => HelmYamlDocument::Failed(HelmYamlError::Unsupported),
    })
}

fn check_outcomes(review_only: bool) -> eyre::Result<()> {
    let outcomes: Vec<GoOutcome> =
        serde_json::from_str(include_str!("fixtures/helm_yaml_go_witness.json"))?;
    let mut checked = 0;
    for outcome in outcomes {
        let GoOutcome {
            input,
            json,
            error,
            reader,
            documents,
            review,
        } = outcome;
        if review_only && !review {
            continue;
        }
        let documents = if let Some(documents) = documents {
            documents
        } else {
            let reader = reader.ok_or_eyre("missing reader class")?;
            // Unsupported nesting does not need the oracle value deserialized.
            let json = if matches!(reader, Expected::Exact) {
                json.map(|json| serde_json::from_str(&json))
                    .transpose()?
                    .unwrap_or_default()
            } else {
                Value::Null
            };
            vec![GoDocument {
                json,
                error,
                reader,
            }]
        };
        let want = documents
            .into_iter()
            .map(expected)
            .collect::<eyre::Result<Vec<_>>>()?;
        sim_assert_eq!(
            have: (input.as_str(), normalized(helm_yaml_documents(&input))),
            want: (input.as_str(), want)
        );
        checked += 1;
    }
    sim_assert_eq!(have: checked, want: if review_only { 20 } else { 226 });
    Ok(())
}

#[test]
fn documents_decode_as_go_yaml_to_json() -> eyre::Result<()> {
    check_outcomes(false)
}

#[test]
fn review_syntax_probes_match_go() -> eyre::Result<()> {
    check_outcomes(true)
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
            HelmYamlDocument::Decoded { value: json!({"a": true}), unknown: Vec::new() },
            HelmYamlDocument::Failed(HelmYamlError::Rejected),
            HelmYamlDocument::Decoded { value: json!({"c": [10, 31]}), unknown: Vec::new() },
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
            value: json!({"a/b": [{"x": 1}, null], "c~d": {"e": "f"}}),
            unknown: vec!["/a~1b/1".to_string()],
        }]
    );
}

#[test]
fn keys_read_as_helm_reads_them() {
    // A block scalar key ends without a line break.
    let block = indoc! {"
        |
         x"};
    let keys = [
        "on", "N", "010", "+1", "'yes'", "\"<<\"", "<<", "1.5", "~", "!!str x", "[a]", block,
        "[!a,]",
    ];
    sim_assert_eq!(
        have: keys.map(helm_yaml_key),
        want: [
            HelmYamlKey::Key("true".to_string()), HelmYamlKey::Key("false".to_string()),
            HelmYamlKey::Key("8".to_string()), HelmYamlKey::Key("1".to_string()),
            HelmYamlKey::Key("yes".to_string()), HelmYamlKey::Key("<<".to_string()),
            HelmYamlKey::Merge, HelmYamlKey::Unknown, HelmYamlKey::Unknown,
            HelmYamlKey::Key("x".to_string()), HelmYamlKey::Unknown,
            HelmYamlKey::Key("x\n".to_string()), HelmYamlKey::Unknown,
        ]
    );
}

#[test]
fn nesting_beyond_the_reader_bound_is_unsupported() {
    let deep = format!("{}{}", "[".repeat(129), "]".repeat(129));
    let shallow = format!("{}{}", "[".repeat(128), "]".repeat(128));
    sim_assert_eq!(have: helm_yaml_documents(&deep), want: vec![HelmYamlDocument::Failed(HelmYamlError::Unsupported)]);
    assert!(matches!(
        helm_yaml_documents(&shallow).as_slice(),
        [HelmYamlDocument::Decoded { .. }]
    ));
}

#[test]
fn explicit_tags_validate_before_duplicate_values_are_overwritten() {
    let invalid = [
        indoc! {"
            a: !!int nope
            a: 1
        "},
        indoc! {"
            a: !!bool 1
            a: 1
        "},
        indoc! {"
            a: !!null yes
            a: 1
        "},
        indoc! {"
            a: !!float yes
            a: 1
        "},
        indoc! {"
            a: !!binary invalid
            a: 1
        "},
        indoc! {"
            a: !!timestamp 2001-02-29
            a: 1
        "},
    ];
    for source in invalid {
        sim_assert_eq!(have: helm_yaml_documents(source), want: vec![HelmYamlDocument::Failed(HelmYamlError::Rejected)]);
    }
    let valid = indoc! {"
        string: !!str on
        boolean: !!bool yes
        integer: !!int '012'
        float: !!float 12
        'null': !!null ~
        binary: !!binary Zh==
        timestamp: !!timestamp 2000-02-29
    "};
    sim_assert_eq!(
        have: normalized(helm_yaml_documents(valid)),
        want: vec![HelmYamlDocument::Decoded {
            value: json!({"string": "on", "boolean": true, "integer": 10, "float": 12, "null": null, "binary": "f", "timestamp": "2000-02-29"}),
            unknown: Vec::new(),
        }]
    );
}

#[test]
fn flow_plain_question_marks_are_unsupported() {
    // Go ends a plain flow scalar at `?`; libyaml-safer can keep it in the text.
    for source in [
        "v: [a?b]\n",
        "v: {a?b: c}\n",
        "v: [a? b]\n",
        "v: [a?]\n",
        "v: {a: b?c}\n",
    ] {
        sim_assert_eq!(
            have: helm_yaml_documents(source),
            want: vec![HelmYamlDocument::Failed(HelmYamlError::Unsupported)]
        );
    }
    for source in ["v: a?b\n", "v: ['a?b']\n"] {
        let value = if source.contains('[') {
            json!({"v": ["a?b"]})
        } else {
            json!({"v": "a?b"})
        };
        sim_assert_eq!(
            have: helm_yaml_documents(source),
            want: vec![HelmYamlDocument::Decoded { value, unknown: Vec::new() }]
        );
    }
}

#[test]
fn ambiguous_keys_do_not_claim_a_deterministic_go_rejection() {
    // Go can overwrite the non-finite value while stringifying colliding keys.
    let source = indoc! {r#"
        1: .inf
        "1": finite
    "#};
    sim_assert_eq!(
        have: helm_yaml_documents(source),
        want: vec![HelmYamlDocument::Failed(HelmYamlError::Unsupported)]
    );
}

/// Tag lookalikes ending at `,` cost one extra scan, not one per lookalike.
#[test]
fn tag_lookalikes_before_commas_decode_in_linear_time() {
    let count = 50_000;
    let text = "!a, ".repeat(count);
    let quoted = format!("a: \"{text}\"\n");
    let commented = format!("a: 1 # {text}\nb: [x, # {text}\n  z]\n");
    let started = std::time::Instant::now();
    sim_assert_eq!(
        have: helm_yaml_documents(&quoted),
        want: vec![HelmYamlDocument::Decoded { value: json!({"a": text}), unknown: Vec::new() }]
    );
    sim_assert_eq!(
        have: helm_yaml_documents(&commented),
        want: vec![HelmYamlDocument::Decoded { value: json!({"a": 1, "b": ["x", "z"]}), unknown: Vec::new() }]
    );
    // Single-quoted flow items need a scan per lookalike; past a linear budget they are declined.
    let items = format!("[{}]\n", "'!a', ".repeat(count));
    sim_assert_eq!(
        have: helm_yaml_documents(&items),
        want: vec![HelmYamlDocument::Failed(HelmYamlError::Unsupported)]
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[test]
fn tag_lookalikes_inside_flow_scalars_decode_exactly() {
    for (source, value) in [
        ("['!a', b]\n", json!(["!a", "b"])),
        ("[x !a, b]\n", json!(["x !a", "b"])),
        (
            "{k: '!a', l: \"!b, c\"}\n",
            json!({"k": "!a", "l": "!b, c"}),
        ),
    ] {
        sim_assert_eq!(
            have: helm_yaml_documents(source),
            want: vec![HelmYamlDocument::Decoded { value, unknown: Vec::new() }]
        );
    }
}
