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

fn schema_for(template: &str) -> eyre::Result<Value> {
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
        ("templates/probe.yaml", template),
    ] {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir,
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

fn expected_schema(nonnullable_paths: &[&str]) -> eyre::Result<Value> {
    let mut schema = json!({
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
    });
    if !nonnullable_paths.is_empty() {
        let clauses: Vec<Value> = nonnullable_paths
            .iter()
            .map(|path| {
                json!({
                    "if": {
                        "anyOf": [
                            {"not": {
                                "properties": {(*path): {}},
                                "required": [path],
                                "type": "object"
                            }},
                            {
                                "properties": {(*path): {"enum": [null]}},
                                "required": [path],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                })
            })
            .collect();
        schema
            .as_object_mut()
            .ok_or_eyre("expected schema must be an object")?
            .insert("allOf".into(), clauses.into());
    }
    Ok(schema)
}

fn serialized_schema_for(program: &str) -> eyre::Result<Value> {
    let body = program.replace("READ", "{{ $x | toYaml }}");
    schema_for(&formatdoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
        {body}
    "})
}

#[test]
fn bare_reads_preserve_the_complete_schema_across_join_shapes() -> eyre::Result<()> {
    for program in [RANGE, WITHOUT_RANGE, NESTED_RANGE] {
        let schema = schema_for(&program.replace("READ", "{{ $x }}"))?;
        sim_assert_eq!(have: schema, want: expected_schema(&[])?, "{program}");
    }
    Ok(())
}

#[test]
fn range_default_fact_is_absent_after_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(RANGE)?;
    sim_assert_eq!(have: schema, want: expected_schema(&["item", "other"])?);
    Ok(())
}

#[test]
fn declared_local_default_fact_unions_across_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(WITHOUT_RANGE)?;
    sim_assert_eq!(have: schema, want: expected_schema(&["item"])?);
    Ok(())
}

#[test]
fn nested_range_default_fact_is_absent_after_an_untouched_exit() -> eyre::Result<()> {
    let schema = serialized_schema_for(NESTED_RANGE)?;
    sim_assert_eq!(have: schema, want: expected_schema(&["item", "other"])?);
    Ok(())
}
