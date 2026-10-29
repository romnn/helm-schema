//! Direct controls for value-algebra execution and Helm YAML boundaries.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::Predicate;
use indoc::{formatdoc, indoc};
use serde::Deserialize as _;
use test_util::prelude::sim_assert_eq;

use crate::analysis_db::IrAnalysisDb;
use crate::value::tests::execute_document;
use crate::{SymbolicTemplate, value::execute_documents};

#[derive(Debug, PartialEq, Eq)]
struct Rendered {
    abort: Predicate,
    text: Vec<RenderedText>,
}

#[derive(Debug, PartialEq, Eq)]
struct RenderedText {
    condition: Predicate,
    text: Option<String>,
}

fn run(source: &str) -> Rendered {
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let execution = execute_document(source, Some("templates/cm.yaml"), &db);
    Rendered {
        abort: execution.abort_predicate(),
        text: execution
            .text()
            .map(|(condition, text)| RenderedText {
                condition,
                text: text.exact_text(),
            })
            .collect(),
    }
}

fn normal(text: &str) -> Rendered {
    Rendered {
        abort: Predicate::False,
        text: vec![RenderedText {
            condition: Predicate::True,
            text: Some(text.to_owned()),
        }],
    }
}

/// Why Helm 4.2.3's `SimpleHead` decoding of a rendered document is not
/// proven to succeed: `apiVersion` and `kind` must be unambiguous strings,
/// `metadata` a mapping whose `name` is a string and whose `annotations`
/// map strings to strings, and no hook annotation may steer the render.
/// Case-folded field aliases and non-finite numbers remain unverified.
/// An empty document is skipped by Helm. `None` when it provably decodes.
fn helm_header_problem(document: &serde_yaml::Value) -> Option<String> {
    if !yaml_numbers_finite(document) || serde_json::to_value(document).is_err() {
        return Some("the rendered document is outside Helm's JSON loader domain".into());
    }
    let text = |value: Option<&serde_yaml::Value>, optional: bool| match value {
        None | Some(serde_yaml::Value::Null) => optional,
        Some(serde_yaml::Value::String(text)) => !text.is_empty() && !yaml_one_one_ambiguous(text),
        Some(_) => false,
    };
    let header = match document {
        serde_yaml::Value::Null => return None,
        serde_yaml::Value::Mapping(header) => header,
        _ => return Some("a document Helm's manifest head cannot decode".into()),
    };
    if !header_keys_supported(header, &["apiVersion", "kind", "metadata"]) {
        return Some("manifest header field aliases are unverified".into());
    }
    if !text(header.get("apiVersion"), false) || !text(header.get("kind"), false) {
        return Some("apiVersion and kind are not proven strings".into());
    }
    let metadata = match header.get("metadata") {
        None | Some(serde_yaml::Value::Null) => return None,
        Some(serde_yaml::Value::Mapping(metadata)) => metadata,
        Some(_) => return Some("metadata is not a mapping".into()),
    };
    if !header_keys_supported(metadata, &["name", "annotations"]) {
        return Some("metadata field aliases are unverified".into());
    }
    if !text(metadata.get("name"), true) {
        return Some("metadata.name is not a proven string".into());
    }
    match metadata.get("annotations") {
        None | Some(serde_yaml::Value::Null) => None,
        Some(serde_yaml::Value::Mapping(annotations)) => {
            let proven = annotations.iter().all(|(key, value)| {
                matches!(key, serde_yaml::Value::String(key) if !key.starts_with("helm.sh/hook"))
                    && text(Some(value), true)
            });
            (!proven).then(|| "metadata.annotations are not proven string pairs".into())
        }
        Some(_) => Some("metadata.annotations is not a mapping".into()),
    }
}

/// Go folds struct field names, including Unicode aliases of ASCII letters.
/// Only ASCII string keys without aliases of the supported fields are proven.
fn header_keys_supported(mapping: &serde_yaml::Mapping, fields: &[&str]) -> bool {
    mapping.keys().all(|key| {
        let serde_yaml::Value::String(key) = key else {
            return false;
        };
        key.is_ascii()
            && !fields
                .iter()
                .any(|field| key != field && key.eq_ignore_ascii_case(field))
    })
}

/// Rejects non-finite numbers before JSON serialization can replace them with null.
fn yaml_numbers_finite(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null | serde_yaml::Value::Bool(_) | serde_yaml::Value::String(_) => true,
        serde_yaml::Value::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        serde_yaml::Value::Sequence(values) => values.iter().all(yaml_numbers_finite),
        serde_yaml::Value::Mapping(values) => values
            .iter()
            .all(|(key, value)| yaml_numbers_finite(key) && yaml_numbers_finite(value)),
        serde_yaml::Value::Tagged(tagged) => yaml_numbers_finite(&tagged.value),
    }
}

/// Whether the plain spelling `text` may decode to something other than
/// this string in YAML 1.1.
fn yaml_one_one_ambiguous(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "y" | "yes" | "n" | "no" | "on" | "off" | "true" | "false" | "null" | "~"
    ) || text
        .chars()
        .all(|character| character.is_ascii_digit() || "+-.:_eEoOxX".contains(character))
}

/// Literal output retains the complete document, including missing-field evidence.
#[test]
fn literal_output_control_is_exact() {
    let template = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: literal
        data:
          greeting: "hello world"
          count: 3
    "#};
    sim_assert_eq!(have: run(template), want: normal(template));
}

/// An executed fail aborts and publishes no completed output.
#[test]
fn direct_abort_control_is_exact() {
    sim_assert_eq!(have: run("{{ fail \"boom\" }}\n"), want: Rendered {
        abort: Predicate::True,
        text: Vec::new(),
    });
    let files = [
        SymbolicTemplate {
            source: "{{ fail \"boom\" }}\n",
            source_path: "templates/fail.yaml",
        },
        SymbolicTemplate {
            source: indoc! {"
                apiVersion: v1
                kind: ConfigMap
                data:
                  a: b
            "},
            source_path: "templates/cm.yaml",
        },
    ];
    let db = IrAnalysisDb::new(&DefineIndex::new());
    let rendered: Vec<_> = execute_documents(&files, &db)
        .iter()
        .map(|execution| Rendered {
            abort: execution.abort_predicate(),
            text: execution
                .text()
                .map(|(condition, text)| RenderedText {
                    condition,
                    text: text.exact_text(),
                })
                .collect(),
        })
        .collect();
    // The later file never executes, so it publishes neither text nor its own abort.
    sim_assert_eq!(have: rendered, want: vec![
        Rendered { abort: Predicate::True, text: Vec::new() },
        Rendered { abort: Predicate::False, text: Vec::new() },
    ]);
}

/// A values-guarded fail preserves the exact abort and surviving-output predicates.
#[test]
fn guarded_abort_preserves_alternatives() {
    sim_assert_eq!(have: run("{{ if .Values.x }}{{ fail \"boom\" }}{{ end }}\n"), want: Rendered {
        abort: Predicate::truthy_path("x"),
        text: vec![RenderedText { condition: Predicate::truthy_path("x").negated(), text: Some("\n".to_owned()) }],
    });
}

/// Serialization freezes the dictionary before a later mutation.
#[test]
fn snapshot_control_is_exact() {
    let template = indoc! {r#"
        {{- $d := dict "x" "old" -}}
        {{- $saved := toJson $d -}}
        {{- $_ := set $d "x" "new" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: snapshot
        data:
          snapshot: {{ $saved | quote }}
    "#};
    sim_assert_eq!(have: run(template), want: normal(indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: snapshot
        data:
          snapshot: "{\"x\":\"old\"}"
    "#}));
}

/// A dictionary alias sees the later write in its complete rendered document.
#[test]
fn alias_control_is_exact() {
    let template = indoc! {r#"
        {{- $a := dict "x" "old" -}}
        {{- $b := $a -}}
        {{- $_ := set $a "x" "new" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: alias
        data:
          visible: {{ eq (get $b "x") "new" }}
    "#};
    sim_assert_eq!(have: run(template), want: normal(indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: alias
        data:
          visible: true
    "}));
}

/// A static Boolean remains verbatim output.
#[test]
fn static_alias_boolean_is_exact() {
    let template = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        data:
          visible: true
    "};
    sim_assert_eq!(have: run(template), want: normal(template));
}

/// Execution preserves YAML spelling; the Helm decoder owns YAML 1.1 resolution.
#[test]
fn yaml_one_one_scalars_preserve_text() {
    for (scalar, value) in [
        ("yes", serde_json::json!(true)),
        ("on", serde_json::json!(true)),
        ("012", serde_json::json!(10)),
        ("1:30", serde_json::json!("1:30")),
        ("True", serde_json::json!(true)),
    ] {
        let template = format!("apiVersion: v1\nkind: ConfigMap\ndata:\n  v: {scalar}\n");
        sim_assert_eq!(have: run(&template), want: normal(&template));
        sim_assert_eq!(have: helm_schema_syntax::helm_yaml_documents(&template), want: vec![
            helm_schema_syntax::HelmYamlDocument::Decoded {
                value: serde_json::json!({"apiVersion": "v1", "kind": "ConfigMap", "data": {"v": value}}),
                unknown: Vec::new(),
            }
        ]);
    }
}

/// Repeated execution has the same full abort and output result.
#[test]
fn execution_answers_are_deterministic() {
    let source = "{{ fail \"boom\" }}\n";
    let expected = Rendered {
        abort: Predicate::True,
        text: Vec::new(),
    };
    sim_assert_eq!(have: run(source), want: expected);
    sim_assert_eq!(have: run(source), want: expected);
}

/// Unexecuted fails retain their complete runtime output and abort predicate.
#[test]
fn unexecuted_fails_are_not_claimed_aborts() {
    let sources = [
        "{{ range list }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ range $k, $v := dict }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ with \"\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if false }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if eq 1 2 }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ define \"never\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Release.IsUpgrade }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Capabilities.APIVersions.Has \"x/v1\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if lookup \"v1\" \"Secret\" \"default\" \"x\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ $x := false }}{{ if $x }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if (empty (list 1)) }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if not (hasKey (dict \"a\" 1) \"a\") }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if ne .Release.Name \"p\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if ne .Chart.Name \"c\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if semverCompare \"<1.0.0\" .Capabilities.KubeVersion.Version }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if .Files.Get \"x\" }}{{ fail \"boom\" }}{{ end }}\n",
        "{{ if (include \"missing.helper\" .) }}{{ fail \"boom\" }}{{ end }}\n",
    ];
    for source in sources {
        sim_assert_eq!(have: run(source), want: normal("\n"), "{source}");
    }
}

/// Static text remains exact even when Helm's subsequent YAML stage cannot be certified.
fn assert_helm_yaml_gap(template: &str, expected: &str) -> eyre::Result<()> {
    sim_assert_eq!(have: run(template), want: normal(template));
    let document = serde_yaml::Value::deserialize(
        serde_yaml::Deserializer::from_str(template)
            .next()
            .ok_or_eyre("missing document")?,
    )?;
    sim_assert_eq!(have: helm_header_problem(&document), want: Some(expected.to_owned()));
    Ok(())
}

/// Static documents can execute exactly while Helm's manifest head rejects their shape.
#[test]
fn normal_needs_valid_manifest_heads() -> eyre::Result<()> {
    for (template, problem) in [
        (
            indoc! {"
            - a
            - b
        "},
            "a document Helm's manifest head cannot decode",
        ),
        (
            "just text\n",
            "a document Helm's manifest head cannot decode",
        ),
        (
            indoc! {"
            apiVersion: v1
            kind: ConfigMap
            metadata: []
        "},
            "metadata is not a mapping",
        ),
    ] {
        assert_helm_yaml_gap(template, problem)?;
    }
    Ok(())
}

/// Go's folded struct lookup can reach typed fields through aliases Rust does not look up.
#[test]
fn case_folded_headers_do_not_prove_normal() -> eyre::Result<()> {
    for (alias, problem) in [
        (
            "Metadata: []",
            "manifest header field aliases are unverified",
        ),
        (
            "APIVERSION: []",
            "manifest header field aliases are unverified",
        ),
        ("KIND: []", "manifest header field aliases are unverified"),
        (
            "\u{212a}ind: []",
            "manifest header field aliases are unverified",
        ),
        (
            indoc! {"
            metadata:
              Name: []
        "},
            "metadata field aliases are unverified",
        ),
        (
            indoc! {"
            metadata:
              Annotations: []
        "},
            "metadata field aliases are unverified",
        ),
        (
            indoc! {"
            metadata:
              annotationſ: []
        "},
            "metadata field aliases are unverified",
        ),
    ] {
        assert_helm_yaml_gap(
            &formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            {alias}
        "},
            problem,
        )?;
    }
    Ok(())
}

/// Non-finite values are outside Helm's JSON domain, including nested mappings and sequences.
#[test]
fn non_finite_yaml_does_not_prove_normal() -> eyre::Result<()> {
    for scalar in [".nan", ".inf", "-.inf"] {
        for body in [
            format!("data: {scalar}\n"),
            formatdoc! {"
                data:
                  nested:
                    - values: [{scalar}]
            "},
        ] {
            assert_helm_yaml_gap(
                &formatdoc! {"
                apiVersion: v1
                kind: ConfigMap
                {body}
            "},
                "the rendered document is outside Helm's JSON loader domain",
            )?;
        }
    }
    // Finite numbers and quoted spellings remain valid loader inputs.
    let finite = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          nested: [1.5, 0, -1, ".nan", ".inf"]
    "#};
    sim_assert_eq!(have: run(finite), want: normal(finite));
    sim_assert_eq!(have: helm_header_problem(&serde_yaml::from_str(finite)?), want: None);
    Ok(())
}
