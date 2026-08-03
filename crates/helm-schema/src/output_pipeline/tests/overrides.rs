use indoc::indoc;
use std::fs;
use std::path::PathBuf;
use test_util::prelude::sim_assert_eq;

use color_eyre::eyre;
use serde_json::Value;

use crate::output_pipeline::{
    EmitRequest, FinalOutputPolicy, OutputPipelineOptions, PolicyInputOptions, PreparedEmitRequest,
    ReferencePolicy, apply_schema_output_pipeline, prepare_emit_request,
};

fn test_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "helm-schema-output-pipeline-{name}-{}",
        std::process::id()
    ))
}

fn policy_options() -> PolicyInputOptions {
    PolicyInputOptions {
        fetch_policy: crate::fetch_policy::FetchPolicy::new(true, false),
        load_budget: crate::load_budget::LoadBudget::default(),
    }
}

fn request(reference_policy: ReferencePolicy) -> EmitRequest {
    EmitRequest {
        reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: false,
        },
    }
}

fn prepare(
    paths: &[PathBuf],
    reference_policy: ReferencePolicy,
) -> crate::error::EngineResult<PreparedEmitRequest> {
    prepare_emit_request(paths, &policy_options(), request(reference_policy))
}

fn output_policy() -> FinalOutputPolicy {
    FinalOutputPolicy::for_profile(crate::generation::SchemaProfile::Full, false)
}

#[test]
fn prepared_override_schemas_bundle_refs_before_merge() {
    let temp_dir = test_temp_dir("prepared-overrides");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    fs::write(
        temp_dir.join("shared.json"),
        indoc! {r#"
            {
                "definitions": {
                    "cloud": {
                        "enum": [null, "azure", "minikube"]
                    }
                }
            }"#},
    )
    .expect("write shared schema");
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        indoc! {r#"
            {
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#},
    )
    .expect("write override schema");

    let prepared =
        prepare(&[override_path], ReferencePolicy::SelfContained).expect("load overrides");
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "cloud": {
                "type": ["boolean", "string"]
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(schema, prepared, &temp_dir, output_policy())
        .expect("apply output pipeline");

    let cloud = output.pointer("/properties/cloud").expect("cloud schema");
    sim_assert_eq!(
        have: cloud,
        want: &serde_json::json!({
            "$ref": "#/$defs/schema1"
        }),
        "prepared override refs should replace inferred constraints with bundled refs"
    );
    sim_assert_eq!(
        have: output.pointer("/$defs/schema1"),
        want: Some(&serde_json::json!({
            "enum": [null, "azure", "minikube"]
        })),
        "prepared override refs should carry resolved content under $defs"
    );

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}

#[test]
fn fully_inlined_export_override_refs_resolve_before_merge() {
    let temp_dir = test_temp_dir("prepared-overrides-inline");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    fs::write(
        temp_dir.join("shared.json"),
        indoc! {r#"
            {
                "definitions": {
                    "cloud": {
                        "enum": [null, "azure", "minikube"]
                    }
                }
            }"#},
    )
    .expect("write shared schema");
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        indoc! {r#"
            {
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#},
    )
    .expect("write override schema");

    let prepared =
        prepare(&[override_path], ReferencePolicy::FullyInlinedExport).expect("load overrides");
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "cloud": {
                "type": ["boolean", "string"]
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(schema, prepared, &temp_dir, output_policy())
        .expect("apply output pipeline");

    let cloud = output.pointer("/properties/cloud").expect("cloud schema");
    sim_assert_eq!(
        have: cloud,
        want: &serde_json::json!({
            "enum": [null, "azure", "minikube"]
        }),
        "fully inlined export refs should replace inferred constraints after dereferencing"
    );

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}

#[test]
fn override_refs_are_preserved_when_reference_mode_preserves_refs() {
    let temp_dir = test_temp_dir("prepared-overrides-keep-refs");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        indoc! {r#"
            {
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#},
    )
    .expect("write override schema");

    let prepared =
        prepare(&[override_path], ReferencePolicy::PreserveRefs).expect("load overrides");
    let schema = serde_json::json!({
        "properties": {
            "cloud": {
                "type": "string"
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(schema, prepared, &temp_dir, output_policy())
        .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .pointer("/properties/cloud/$ref")
            .and_then(Value::as_str),
        want: Some("./shared.json#/definitions/cloud"),
    );

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}

#[test]
fn override_loader_rejects_non_schema_roots() -> eyre::Result<()> {
    let temp_dir = test_temp_dir("invalid-root");
    fs::create_dir_all(&temp_dir)?;
    for (index, root) in [
        serde_json::json!(null),
        serde_json::json!(3),
        serde_json::json!("schema"),
        serde_json::json!([]),
    ]
    .into_iter()
    .enumerate()
    {
        let path = temp_dir.join(format!("override-{index}.json"));
        fs::write(&path, serde_json::to_vec(&root)?)?;
        let result = prepare(&[path], ReferencePolicy::PreserveRefs);
        sim_assert_eq!(have: result.is_err(), want: true);
    }
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[test]
fn prepared_override_identity_includes_replacement_intent() -> eyre::Result<()> {
    let temp_dir = test_temp_dir("override-identity");
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("shared.json"),
        r#"{"enum":[null,"azure","minikube"]}"#,
    )?;
    let referenced_path = temp_dir.join("referenced.json");
    fs::write(
        &referenced_path,
        r#"{"properties":{"cloud":{"$ref":"./shared.json"}}}"#,
    )?;
    let inline_path = temp_dir.join("inline.json");
    fs::write(
        &inline_path,
        r#"{"properties":{"cloud":{"enum":[null,"azure","minikube"]}}}"#,
    )?;
    let referenced = prepare(&[referenced_path], ReferencePolicy::FullyInlinedExport)?;
    let inline = prepare(&[inline_path], ReferencePolicy::FullyInlinedExport)?;

    sim_assert_eq!(
        have: referenced.identity().digest == inline.identity().digest,
        want: false
    );

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[test]
fn caller_authored_ref_replace_keys_do_not_collide_with_merge_intent() -> eyre::Result<()> {
    let temp_dir = test_temp_dir("caller-ref-replace");
    fs::create_dir_all(&temp_dir)?;
    fs::write(
        temp_dir.join("shared.json"),
        r#"{"enum":["azure","minikube"]}"#,
    )?;
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        indoc! {r#"
            {
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json",
                        "$ref-replace": "caller ref value"
                    }
                },
                "x-caller": {
                    "$ref-replace": "caller non-ref value"
                }
            }
        "#},
    )?;
    let base = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"cloud": {"type": "string"}}
    });

    for reference_mode in [
        ReferencePolicy::SelfContained,
        ReferencePolicy::FullyInlinedExport,
        ReferencePolicy::PreserveRefs,
    ] {
        let prepared = prepare(std::slice::from_ref(&override_path), reference_mode)?;
        let output =
            apply_schema_output_pipeline(base.clone(), prepared, &temp_dir, output_policy())?;
        sim_assert_eq!(
            have: output.pointer("/x-caller/$ref-replace"),
            want: Some(&serde_json::json!("caller non-ref value"))
        );
        let ref_location_value = output.pointer("/properties/cloud/$ref-replace");
        let expected_annotation = match reference_mode {
            ReferencePolicy::SelfContained => "bundled",
            ReferencePolicy::FullyInlinedExport => "fully-inlined",
            ReferencePolicy::PreserveRefs => "preserved",
        };
        sim_assert_eq!(
            have: output
                .pointer("/x-helm-schema-policy/modifiers/reference-mode")
                .and_then(Value::as_str),
            want: Some(expected_annotation),
            "the request's reference policy must govern preparation and annotation"
        );
        if reference_mode == ReferencePolicy::SelfContained {
            sim_assert_eq!(have: ref_location_value, want: None);
        } else {
            sim_assert_eq!(
                have: ref_location_value,
                want: Some(&serde_json::json!("caller ref value"))
            );
        }
    }

    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

#[test]
fn final_override_replacement_prunes_the_orphaned_generator_definition() -> eyre::Result<()> {
    let temp_dir = test_temp_dir("orphaned-generated-definition");
    fs::create_dir_all(&temp_dir)?;
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        r#"{"properties":{"value":{"anyOf":[{"type":"integer"}]}}}"#,
    )?;
    let prepared = prepare(&[override_path], ReferencePolicy::SelfContained)?;
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {
            "generated": { "type": "string" },
        },
        "properties": {
            "value": { "$ref": "#/$defs/generated" },
        },
        "type": "object",
    });

    let output = apply_schema_output_pipeline(schema, prepared, &temp_dir, output_policy())?;

    sim_assert_eq!(have: output.get("$defs"), want: None);
    sim_assert_eq!(
        have: output.pointer("/properties/value"),
        want: Some(&serde_json::json!({ "anyOf": [{ "type": "integer" }] }))
    );
    fs::remove_dir_all(&temp_dir)?;
    Ok(())
}
