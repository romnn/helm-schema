//! Semantic assertions for the falco chart: the falcosidekick `rolearn`
//! branch pair. The quote branch (`useirsa=true`, service-account
//! annotation) renders any value, while the b64enc branch (`useirsa=false`,
//! secret data) fails rendering for non-strings — even though the quote
//! branch sits behind the compound guard
//! `or .Values.config.azure.workloadIdentityClientID (and .Values.config.aws.useirsa .Values.config.aws.rolearn)`.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn falco_rolearn_contract_is_branch_scoped() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("falco")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let instance = |rolearn: serde_json::Value, useirsa: bool| {
        serde_json::json!({
            "falcosidekick": {
                "enabled": true,
                "config": { "aws": { "rolearn": rolearn, "useirsa": useirsa } }
            }
        })
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
