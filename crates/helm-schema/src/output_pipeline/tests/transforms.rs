use color_eyre::eyre;
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

use crate::output_pipeline::{
    EmitRequest, FinalOutputPolicy, OutputPipelineOptions, PreparedEmitRequest, ReferencePolicy,
    apply_schema_output_pipeline,
};

fn output_policy() -> FinalOutputPolicy {
    FinalOutputPolicy::for_profile(crate::generation::SchemaProfile::Full, false)
}

fn request(reference_policy: ReferencePolicy) -> PreparedEmitRequest {
    PreparedEmitRequest::empty(EmitRequest {
        reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: false,
        },
    })
}

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
        request(ReferencePolicy::PreserveRefs),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
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
        request(ReferencePolicy::SelfContained),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
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
fn fully_inlined_output_prunes_generator_definitions_orphaned_by_transport() -> eyre::Result<()> {
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
        request(ReferencePolicy::FullyInlinedExport),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
    )?;

    sim_assert_eq!(have: output.get("$defs"), want: None);
    sim_assert_eq!(
        have: output.pointer("/properties/fromRef/type").and_then(Value::as_str),
        want: Some("string")
    );
    Ok(())
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
        request(ReferencePolicy::SelfContained),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
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
        request(ReferencePolicy::FullyInlinedExport),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
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
        request(ReferencePolicy::PreserveRefs),
        std::path::Path::new("/does/not/matter"),
        output_policy(),
    )
    .expect("apply output pipeline");

    sim_assert_eq!(
        have: output
            .get("x-helm-schema-generated")
            .and_then(Value::as_bool),
        want: Some(true)
    );
}
