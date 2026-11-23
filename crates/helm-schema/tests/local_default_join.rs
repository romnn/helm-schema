//! Preserve default provenance across assigned and untouched control exits.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, GenerateOptions, SchemaProfile};
use helm_schema::output::{EmitRequest, OutputPipelineOptions, ReferencePolicy};
use helm_schema::provider::ProviderOptions;
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const RANGE: &str = r#"{{ range $x := list .Values.item }}{{ if $.Values.flag }}{{ $x = default "d" $.Values.other }}{{ end }}READ{{ end }}"#;
const WITHOUT_RANGE: &str = r#"{{ $x := .Values.item }}{{ if $.Values.flag }}{{ $x = default "d" $.Values.other }}{{ end }}READ"#;
const NESTED_RANGE: &str = r#"{{ range $x := list .Values.item }}{{ if $.Values.flag }}{{ if $.Values.nested }}{{ $x = default "d" $.Values.other }}{{ end }}{{ end }}READ{{ end }}"#;

fn schema_for(template_path: &str, template: &str) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    for (path, source) in [
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: default-join
                version: 0.1.0
            "},
        ),
        (
            "values.yaml",
            indoc! {"
                item: null
                other: null
                flag: false
                nested: false
            "},
        ),
        (template_path, template),
    ] {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    let session = AnalysisSession::new(GenerateOptions {
        chart_source: chart_dir.into(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::Full.into(),
        authoring: AuthoringPolicy {
            declared_types: DeclaredTypes::Annotate,
            ..Default::default()
        },
        provider: ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            ..Default::default()
        },
    });
    Ok(session.emit(EmitRequest {
        reference_policy: ReferencePolicy::SelfContained,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: helm_schema::output::DefinitionNames::Source,
        },
    })?)
}

fn expected_schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "flag": {},
            "global": {},
            "item": {},
            "nested": {},
            "other": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "assumptions": {"release-namespace": {"max-bytes": 63, "min-bytes": 1, "pattern": "^(?:[a-z0-9]([-a-z0-9]*[a-z0-9])?)$"}},
            "authoring": {"declared-types": "annotate", "root": "closed"},
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    })
}

/// `{"$ref": …}` to the generated definition `name`.
fn definition(name: &str) -> Value {
    json!({"$ref": format!("#/definitions/{}", name.replace('/', "~1"))})
}

/// The document holds `path`, and its value satisfies `schema`.
fn member(path: &str, schema: &Value) -> Value {
    json!({"properties": {path: schema}, "required": [path], "type": "object"})
}

/// The document lacks `path`.
fn missing(path: &str) -> Value {
    json!({"not": member(path, &json!({}))})
}

/// `path` holds a mapping with at least one member, in either conjunct order.
fn nonempty_map(path: &str, bound_first: bool) -> Value {
    let bound = member(path, &json!({"minProperties": 1, "type": "object"}));
    let map = member(path, &json!({"type": "object"}));
    json!({"allOf": if bound_first { [bound, map] } else { [map, bound] }})
}

/// One way a payload can decode as Helm's manifest root mapping.
#[derive(Clone, Copy)]
enum RootDecoding {
    NullType,
    Missing,
    Null,
    Map,
}

const MAP_SECOND: [RootDecoding; 4] = [
    RootDecoding::NullType,
    RootDecoding::Map,
    RootDecoding::Missing,
    RootDecoding::Null,
];
const MAP_LAST: [RootDecoding; 4] = [
    RootDecoding::NullType,
    RootDecoding::Missing,
    RootDecoding::Null,
    RootDecoding::Map,
];

/// Helm decodes the manifest root as a mapping: `path` is null, missing, or a mapping, in the
/// generator's alternative order.
fn decodes_as_root(path: &str, order: [RootDecoding; 4]) -> Value {
    let alternatives = order
        .iter()
        .map(|alternative| match alternative {
            RootDecoding::NullType => member(path, &json!({"type": "null"})),
            RootDecoding::Missing => missing(path),
            RootDecoding::Null => member(path, &json!({"enum": [null]})),
            RootDecoding::Map => member(path, &json!({"type": "object"})),
        })
        .collect::<Vec<_>>();
    json!({"anyOf": alternatives})
}

/// The definitions the default-join clauses share, by their generated names.
///
/// - `t`: Helm truthiness (`true`, a non-zero number, a non-empty string, list or map).
/// - `helm-marker-free`: no `<no value>` in the serialized text, which Helm deletes after
///   rendering; only such a payload reaches the manifest unchanged.
/// - `<path>+<path>:helm-marker-free+<path>=null`: the payload of `path` reaches the manifest
///   unchanged (missing and null serialize as `null`).
fn shared_definitions() -> serde_json::Map<String, Value> {
    let unchanged = |path: &str| {
        json!({"anyOf": [
            member(path, &definition("helm-marker-free")),
            missing(path),
            member(path, &json!({"enum": [null]}))
        ]})
    };
    let mut definitions = serde_json::Map::new();
    for (name, schema) in [
        (
            "t",
            json!({"anyOf": [
                {"minProperties": 1, "type": "object"},
                {"not": {"const": 0}, "type": "number"},
                {"minLength": 1, "type": "string"},
                {"minItems": 1, "type": "array"},
                {"const": true}
            ]}),
        ),
        (
            "helm-marker-free",
            json!({"anyOf": [
                {
                    "additionalProperties": definition("helm-marker-free"),
                    "propertyNames": {"not": {"pattern": "<no value>"}},
                    "type": "object"
                },
                {"type": ["null", "boolean", "number"]},
                {"items": definition("helm-marker-free"), "type": "array"},
                {"not": {"pattern": "<no value>"}, "type": "string"}
            ]}),
        ),
        ("constrains/flag:t@2", member("flag", &definition("t"))),
        (
            "constrains/flag:t",
            json!({"not": definition("constrains/flag:t@2")}),
        ),
        ("constrains/other:t", member("other", &definition("t"))),
        (
            "constrains/other+other:helm-marker-free+other=null",
            unchanged("other"),
        ),
        (
            "constrains/item+item:helm-marker-free+item=null",
            unchanged("item"),
        ),
    ] {
        definitions.insert(name.to_owned(), schema);
    }
    definitions
}

/// `if <any abort arm> then false`, with the definitions its arms use.
fn abort_schema(
    arms: &[Value],
    definitions: serde_json::Map<String, Value>,
) -> eyre::Result<Value> {
    let mut schema = expected_schema();
    let object = schema
        .as_object_mut()
        .ok_or_eyre("expected schema must be an object")?;
    object.insert(
        "allOf".into(),
        json!([{"if": {"anyOf": arms}, "then": false}]),
    );
    object.insert("definitions".into(), Value::Object(definitions));
    Ok(schema)
}

/// The schema of `RANGE` and `WITHOUT_RANGE`: `toYaml $x` lands at the `ConfigMap` document
/// root after its mapping, so Helm aborts unless the payload is a non-empty mapping or `<no
/// value>` deletion changes it. `$x` is `other` when `flag` and `other` are truthy, the
/// constant `d` when `flag` is truthy and `other` falsy, and `item` otherwise (Helm 4.2.3,
/// `transfer/abortfa-receipts/local_default_join/<test>/verdicts.tsv`: 23 documents).
fn expected_default_join_schema() -> eyre::Result<Value> {
    abort_schema(
        &[
            // `default "d"` selects the bare scalar `d`.
            json!({"allOf": [
                {"not": definition("constrains/other:t")},
                definition("constrains/flag:t@2")
            ]}),
            // `other` splices in place of mapping members, so it must be a non-empty map.
            json!({"allOf": [
                definition("constrains/other+other:helm-marker-free+other=null"),
                {"not": nonempty_map("other", false)},
                definition("constrains/flag:t@2"),
                definition("constrains/other:t")
            ]}),
            // The manifest root decodes as a mapping.
            json!({"allOf": [
                definition("constrains/other+other:helm-marker-free+other=null"),
                {"not": decodes_as_root("other", MAP_SECOND)},
                definition("constrains/flag:t@2"),
                definition("constrains/other:t")
            ]}),
            json!({"allOf": [
                definition("constrains/flag:t"),
                {"not": nonempty_map("item", true)},
                definition("constrains/item+item:helm-marker-free+item=null")
            ]}),
            json!({"allOf": [
                definition("constrains/flag:t"),
                definition("constrains/item+item:helm-marker-free+item=null"),
                {"not": decodes_as_root("item", MAP_LAST)}
            ]}),
        ],
        shared_definitions(),
    )
}

/// The schema of `NESTED_RANGE`: as [`expected_default_join_schema`], with `$x` reassigned only
/// when `nested` is truthy too. The `item` arms recur under both `flag` polarities, so their
/// two `item` facts become shared definitions.
fn expected_nested_default_join_schema() -> eyre::Result<Value> {
    let mut definitions = shared_definitions();
    for (name, schema) in [
        ("constrains/nested:t@2", member("nested", &definition("t"))),
        (
            "constrains/nested:t",
            json!({"not": definition("constrains/nested:t@2")}),
        ),
        (
            "constrains/item",
            json!({"not": nonempty_map("item", true)}),
        ),
        (
            "constrains/item+item=null",
            json!({"not": decodes_as_root("item", MAP_LAST)}),
        ),
    ] {
        definitions.insert(name.to_owned(), schema);
    }
    abort_schema(
        &[
            json!({"allOf": [
                {"not": definition("constrains/other:t")},
                definition("constrains/nested:t@2"),
                definition("constrains/flag:t@2")
            ]}),
            json!({"allOf": [
                definition("constrains/other+other:helm-marker-free+other=null"),
                {"not": nonempty_map("other", false)},
                definition("constrains/nested:t@2"),
                definition("constrains/flag:t@2"),
                definition("constrains/other:t")
            ]}),
            json!({"allOf": [
                definition("constrains/other+other:helm-marker-free+other=null"),
                {"not": decodes_as_root("other", MAP_SECOND)},
                definition("constrains/nested:t@2"),
                definition("constrains/flag:t@2"),
                definition("constrains/other:t")
            ]}),
            json!({"allOf": [
                definition("constrains/item+item:helm-marker-free+item=null"),
                definition("constrains/item+item=null"),
                definition("constrains/flag:t")
            ]}),
            json!({"allOf": [
                definition("constrains/item"),
                definition("constrains/item+item:helm-marker-free+item=null"),
                definition("constrains/flag:t")
            ]}),
            json!({"allOf": [
                definition("constrains/nested:t"),
                definition("constrains/item"),
                definition("constrains/item+item:helm-marker-free+item=null"),
                definition("constrains/flag:t@2")
            ]}),
            json!({"allOf": [
                definition("constrains/nested:t"),
                definition("constrains/item+item:helm-marker-free+item=null"),
                definition("constrains/item+item=null"),
                definition("constrains/flag:t@2")
            ]}),
        ],
        definitions,
    )
}

fn serialized_schema_for(program: &str) -> eyre::Result<Value> {
    let body = program.replace("READ", "{{ $x | toYaml }}");
    schema_for(
        "templates/probe.yaml",
        &formatdoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
        {body}
    "},
    )
}

#[test]
fn bare_reads_preserve_the_complete_schema_across_join_shapes() -> eyre::Result<()> {
    for program in [RANGE, WITHOUT_RANGE, NESTED_RANGE] {
        // Helm leaves NOTES output unparsed, isolating the read and provenance law.
        let schema = schema_for("templates/NOTES.txt", &program.replace("READ", "{{ $x }}"))?;
        sim_assert_eq!(have: schema, want: expected_schema(), "{program}");
    }
    Ok(())
}

#[test]
fn range_default_fact_is_absent_after_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(RANGE)?;
    sim_assert_eq!(have: schema, want: expected_default_join_schema()?);
    Ok(())
}

#[test]
fn declared_local_default_fact_unions_across_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(WITHOUT_RANGE)?;
    sim_assert_eq!(have: schema, want: expected_default_join_schema()?);
    Ok(())
}

#[test]
fn nested_range_default_fact_is_absent_after_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(NESTED_RANGE)?;
    sim_assert_eq!(have: schema, want: expected_nested_default_join_schema()?);
    Ok(())
}
