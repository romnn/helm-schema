use color_eyre::eyre;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use crate::generation::SchemaProfile;
use crate::output_pipeline::{
    EmitRequest, FinalOutputPolicy, OutputPipelineOptions, PreparedEmitRequest, ReferencePolicy,
    apply_schema_output_pipeline,
};

fn request(reference_policy: ReferencePolicy) -> PreparedEmitRequest {
    PreparedEmitRequest::empty(EmitRequest {
        reference_policy,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: false,
        },
    })
}

fn emit(
    schema: Value,
    profile: SchemaProfile,
    reference_policy: ReferencePolicy,
) -> eyre::Result<Value> {
    Ok(apply_schema_output_pipeline(
        schema,
        request(reference_policy),
        std::path::Path::new("/does/not/matter"),
        FinalOutputPolicy::for_profile(profile, false),
    )?)
}

#[test]
fn final_policy_annotation_is_deterministic_and_overwrites_caller_key() -> eyre::Result<()> {
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "x-helm-schema-generated": false,
        "x-helm-schema-policy": "caller value"
    });

    let first = emit(
        schema.clone(),
        SchemaProfile::Lean,
        ReferencePolicy::PreserveRefs,
    )?;
    let second = emit(schema, SchemaProfile::Lean, ReferencePolicy::PreserveRefs)?;

    sim_assert_eq!(have: &first, want: &second);
    sim_assert_eq!(
        have: first["x-helm-schema-generated"].clone(),
        want: json!(true)
    );
    sim_assert_eq!(
        have: first["x-helm-schema-policy"]["requested-profile"].clone(),
        want: json!("lean")
    );
    sim_assert_eq!(
        have: first["x-helm-schema-policy"]["resolved"].clone(),
        want: json!({
            "kind-partitions": false,
            "local-conditionals": true,
            "root-anchored-conditionals": false,
            "terminal-clauses": false
        })
    );
    Ok(())
}

#[test]
fn boolean_roots_are_wrapped_without_changing_acceptance() -> eyre::Result<()> {
    for boolean in [false, true] {
        let output = emit(
            Value::Bool(boolean),
            SchemaProfile::Full,
            ReferencePolicy::PreserveRefs,
        )?;
        sim_assert_eq!(
            have: output["$schema"].clone(),
            want: json!("http://json-schema.org/draft-07/schema#")
        );
        sim_assert_eq!(have: output["allOf"].clone(), want: json!([boolean]));
        let validator = jsonschema::validator_for(&output)?;
        sim_assert_eq!(have: validator.is_valid(&json!({})), want: boolean);
    }
    Ok(())
}

#[test]
fn non_schema_roots_cannot_escape_annotation() {
    for root in [json!(null), json!(3), json!("schema"), json!([])] {
        let result = apply_schema_output_pipeline(
            root,
            request(ReferencePolicy::PreserveRefs),
            std::path::Path::new("/does/not/matter"),
            FinalOutputPolicy::for_profile(SchemaProfile::Full, false),
        );
        sim_assert_eq!(have: result.is_err(), want: true);
    }
}

#[test]
fn reference_modifier_changes_the_policy_fingerprint() -> eyre::Result<()> {
    let schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object"
    });
    let bundled = emit(
        schema.clone(),
        SchemaProfile::Full,
        ReferencePolicy::SelfContained,
    )?;
    let preserved = emit(schema, SchemaProfile::Full, ReferencePolicy::PreserveRefs)?;

    let bundled_fingerprint = bundled
        .pointer("/x-helm-schema-policy/policy-fingerprint")
        .and_then(Value::as_str);
    let preserved_fingerprint = preserved
        .pointer("/x-helm-schema-policy/policy-fingerprint")
        .and_then(Value::as_str);
    sim_assert_eq!(
        have: bundled_fingerprint == preserved_fingerprint,
        want: false
    );
    Ok(())
}
