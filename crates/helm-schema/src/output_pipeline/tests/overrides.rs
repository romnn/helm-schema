use std::fs;
use std::path::PathBuf;
use test_util::prelude::sim_assert_eq;

use serde_json::Value;

use crate::output_pipeline::{
    OutputPipelineOptions, PolicyInputOptions, ReferenceMode, apply_schema_output_pipeline,
    load_policy_inputs,
};

fn test_temp_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "helm-schema-output-pipeline-{name}-{}",
        std::process::id()
    ))
}

fn policy_options(reference_mode: ReferenceMode) -> PolicyInputOptions {
    PolicyInputOptions {
        reference_mode,
        fetch_policy: crate::fetch_policy::FetchPolicy::new(true, false),
        load_budget: crate::load_budget::LoadBudget::default(),
    }
}

fn output_options(reference_mode: ReferenceMode) -> OutputPipelineOptions {
    OutputPipelineOptions {
        reference_mode,
        strip_descriptions: false,
        minimize: false,
    }
}

#[test]
fn prepared_override_schemas_bundle_refs_before_merge() {
    let temp_dir = test_temp_dir("prepared-overrides");
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    fs::write(
        temp_dir.join("shared.json"),
        r#"{
            "definitions": {
                "cloud": {
                    "enum": [null, "azure", "minikube"]
                }
            }
        }"#,
    )
    .expect("write shared schema");
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        r#"{
            "properties": {
                "cloud": {
                    "$ref": "./shared.json#/definitions/cloud"
                }
            }
        }"#,
    )
    .expect("write override schema");

    let policy_options = policy_options(ReferenceMode::SelfContained);
    let output_options = output_options(ReferenceMode::SelfContained);
    let policy_inputs =
        load_policy_inputs(&[override_path], &policy_options).expect("load overrides");
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "cloud": {
                "type": ["boolean", "string"]
            }
        },
        "type": "object"
    });

    let output =
        apply_schema_output_pipeline(schema, policy_inputs, &[], &temp_dir, &output_options)
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
        r#"{
            "definitions": {
                "cloud": {
                    "enum": [null, "azure", "minikube"]
                }
            }
        }"#,
    )
    .expect("write shared schema");
    let override_path = temp_dir.join("override.json");
    fs::write(
        &override_path,
        r#"{
            "properties": {
                "cloud": {
                    "$ref": "./shared.json#/definitions/cloud"
                }
            }
        }"#,
    )
    .expect("write override schema");

    let policy_options = policy_options(ReferenceMode::FullyInlinedExport);
    let output_options = output_options(ReferenceMode::FullyInlinedExport);
    let policy_inputs =
        load_policy_inputs(&[override_path], &policy_options).expect("load overrides");
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "cloud": {
                "type": ["boolean", "string"]
            }
        },
        "type": "object"
    });

    let output =
        apply_schema_output_pipeline(schema, policy_inputs, &[], &temp_dir, &output_options)
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
        r#"{
            "properties": {
                "cloud": {
                    "$ref": "./shared.json#/definitions/cloud"
                }
            }
        }"#,
    )
    .expect("write override schema");

    let policy_options = policy_options(ReferenceMode::PreserveRefs);
    let output_options = output_options(ReferenceMode::PreserveRefs);
    let policy_inputs =
        load_policy_inputs(&[override_path], &policy_options).expect("load overrides");
    let schema = serde_json::json!({
        "properties": {
            "cloud": {
                "type": "string"
            }
        },
        "type": "object"
    });

    let output =
        apply_schema_output_pipeline(schema, policy_inputs, &[], &temp_dir, &output_options)
            .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .pointer("/properties/cloud/$ref")
            .and_then(Value::as_str),
        want: Some("./shared.json#/definitions/cloud"),
    );

    fs::remove_dir_all(&temp_dir).expect("remove temp dir");
}
