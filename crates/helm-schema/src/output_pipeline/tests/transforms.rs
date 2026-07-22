use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use super::GENERATED_SCHEMA_MARKER_KEY;
use crate::output_pipeline::{
    OutputPipelineOptions, PolicyInputs, ReferenceMode, apply_schema_output_pipeline,
};

#[test]
fn reference_mode_preserves_refs_when_requested() {
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "fromRef": {
                "$ref": "./shared.json#/definitions/stringValue"
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(
        schema,
        PolicyInputs::default(),
        &[],
        std::path::Path::new("/does/not/matter"),
        OutputPipelineOptions {
            reference_mode: ReferenceMode::PreserveRefs,
            strip_descriptions: false,
            minimize: false,
        },
    )
    .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .pointer("/properties/fromRef/$ref")
            .and_then(Value::as_str),
        want: Some("./shared.json#/definitions/stringValue"),
        "reference-preserving output mode should not dereference refs"
    );
}

#[test]
fn self_contained_reference_mode_preserves_prepared_internal_refs() {
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {
            "stringValue": {
                "type": "string"
            }
        },
        "properties": {
            "fromRef": {
                "$ref": "#/$defs/stringValue"
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(
        schema,
        PolicyInputs::default(),
        &[],
        std::path::Path::new("/does/not/matter"),
        OutputPipelineOptions {
            reference_mode: ReferenceMode::SelfContained,
            strip_descriptions: false,
            minimize: false,
        },
    )
    .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .pointer("/properties/fromRef/$ref")
            .and_then(Value::as_str),
        want: Some("#/$defs/stringValue"),
        "self-contained final output should keep prepared internal refs"
    );
    sim_assert_eq!(
        have: output
            .pointer("/$defs/stringValue/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "prepared definitions should remain available under $defs"
    );
}

#[test]
fn self_contained_reference_mode_rejects_unprepared_external_refs() {
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "properties": {
            "fromRef": {
                "$ref": "./shared.json#/definitions/stringValue"
            }
        },
        "type": "object"
    });

    let err = apply_schema_output_pipeline(
        schema,
        PolicyInputs::default(),
        &[],
        std::path::Path::new("/does/not/matter"),
        OutputPipelineOptions {
            reference_mode: ReferenceMode::SelfContained,
            strip_descriptions: false,
            minimize: false,
        },
    )
    .expect_err("unprepared external ref should fail final output transform");

    assert!(
        err.to_string()
            .contains("external $ref remained after input preparation"),
        "unexpected error: {err}"
    );
}

#[test]
fn fully_inlined_export_reference_mode_inlines_prepared_internal_refs() {
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {
            "stringValue": {
                "type": "string"
            }
        },
        "properties": {
            "fromRef": {
                "$ref": "#/$defs/stringValue"
            }
        },
        "type": "object"
    });

    let output = apply_schema_output_pipeline(
        schema,
        PolicyInputs::default(),
        &[],
        std::path::Path::new("/does/not/matter"),
        OutputPipelineOptions {
            reference_mode: ReferenceMode::FullyInlinedExport,
            strip_descriptions: false,
            minimize: false,
        },
    )
    .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .pointer("/properties/fromRef/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "fully inlined export mode should inline prepared internal refs"
    );
    assert!(output.pointer("/properties/fromRef/$ref").is_none());
}

#[test]
fn output_pipeline_marks_final_schema_as_generated() {
    let schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object"
    });

    let output = apply_schema_output_pipeline(
        schema,
        PolicyInputs::default(),
        &[],
        std::path::Path::new("/does/not/matter"),
        OutputPipelineOptions {
            reference_mode: ReferenceMode::PreserveRefs,
            strip_descriptions: false,
            minimize: false,
        },
    )
    .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .get(GENERATED_SCHEMA_MARKER_KEY)
            .and_then(Value::as_bool),
        want: Some(true)
    );
}
