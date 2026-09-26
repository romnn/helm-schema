//! Semantic assertions for the falco chart: the falcosidekick `rolearn`
//! branch pair. The quote branch (`useirsa=true`, service-account
//! annotation) renders any value, while the b64enc branch (`useirsa=false`,
//! secret data) fails rendering for non-strings — even though the quote
//! branch sits behind the compound guard
//! `or .Values.config.azure.workloadIdentityClientID (and .Values.config.aws.useirsa .Values.config.aws.rolearn)`.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre;
use serde_json::Value;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn falco_rolearn_contract_is_branch_scoped() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("falco")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Cases compose over the chart defaults: helm validates the coalesced
    // document, and the chart navigates hosts these overrides do not touch.
    let instance = |rolearn: serde_json::Value, useirsa: bool| {
        chart_instances::with_override(
            "falco",
            serde_json::json!({
                "falcosidekick": {
                    "enabled": true,
                    "config": { "aws": { "rolearn": rolearn, "useirsa": useirsa } }
                }
            }),
        )
        .expect("compose falco instance")
    };

    assert!(
        validator.is_valid(&instance(serde_json::json!({ "bad": true }), true)),
        "the quoted annotation renders a map when useirsa=true"
    );
    assert!(
        !validator.is_valid(&instance(serde_json::json!({ "bad": true }), false)),
        "the b64enc secret branch fails rendering for a map when useirsa=false"
    );
    for useirsa in [true, false] {
        assert!(
            validator.is_valid(&instance(
                serde_json::json!("arn:aws:iam::1:role/x"),
                useirsa
            )),
            "strings render in both states (useirsa={useirsa})"
        );
    }
    Ok(())
}

/// Helm's `CoalesceTables(overrides, values)` as `helm lint -f` applies it
/// (Helm v4.2.3 `pkg/chart/v2/lint/rules/values.go:62-68`): the overrides
/// win and tables merge.
fn coalesce_tables(overrides: Value, values: Value) -> Value {
    match (overrides, values) {
        (Value::Object(mut overrides), Value::Object(values)) => {
            for (key, value) in values {
                let merged = match overrides.remove(&key) {
                    Some(override_value) => coalesce_tables(override_value, value),
                    None => value,
                };
                overrides.insert(key, merged);
            }
            Value::Object(overrides)
        }
        (overrides, _) => overrides,
    }
}

/// `webui.enabled` and `webui.redis` exist only in the falcosidekick
/// defaults. `helm lint -f {falcosidekick: {enabled: true, webui:
/// {enabled: true}}}` validates the root values with that override and no
/// falcosidekick defaults, so `webui.redis` is absent, and it must pass:
/// `helm template` renders the same override (Helm v4.2.3).
#[test]
fn falco_lint_document_with_a_dependency_only_toggle_is_accepted() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("falco")?;
    let validator = jsonschema::validator_for(&schema)?;
    let values: Value = serde_yaml::from_str(&std::fs::read_to_string(
        test_util::workspace_testdata().join("charts/falco/values.yaml"),
    )?)?;
    let overrides = serde_json::json!({
        "falcosidekick": { "enabled": true, "webui": { "enabled": true } }
    });
    let lint_document = coalesce_tables(overrides.clone(), values);
    let errors = validator
        .iter_errors(&lint_document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "`helm lint -f` passes: {errors:?}");
    assert!(
        validator.is_valid(&chart_instances::with_override("falco", overrides)?),
        "`helm template` renders the coalesced document"
    );
    Ok(())
}
