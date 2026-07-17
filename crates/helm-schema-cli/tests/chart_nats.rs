//! Semantic assertions for NATS values after its default-values helper
//! replaces `.Values` with a JSON-decoded tree. Full-schema equality and
//! default-values validation live in `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/helm_samples.rs"]
mod helm_samples;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn nats_json_decoded_extra_resources_exclude_integer_iteration() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let resource = serde_json::json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": { "name": "extra" },
        "data": { "key": "value" }
    });
    let valid = chart_instances::with_override(
        "nats",
        serde_json::json!({ "extraResources": [resource] }),
    )?;
    assert!(
        validator.is_valid(&valid),
        "a list of Kubernetes resources survives the JSON roundtrip and renders"
    );

    let invalid =
        chart_instances::with_override("nats", serde_json::json!({ "extraResources": 7 }))?;
    assert!(
        !validator.is_valid(&invalid),
        "JSON decoding turns the raw integer into a non-iterable number"
    );

    Ok(())
}

#[test]
fn nats_tpl_yaml_sentinels_stay_nested_and_do_not_seed_root_properties()
-> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;

    assert!(
        schema.pointer("/properties/$tplYaml").is_none()
            && schema.pointer("/properties/$tplYamlSpread").is_none(),
        "the recursive tplYaml walker must not turn its nested sentinel keys into root values properties"
    );

    helm_samples::assert_generated_schema_accepts_helm_samples_for_path(
        "nats",
        &schema,
        &[helm_samples::HelmValidationSample {
            name: "nested tplYaml sentinel",
            values_yaml: Some(
                r#"
extraResources:
  - apiVersion: v1
    kind: ConfigMap
    metadata:
      name:
        $tplYaml: '{{ .Release.Name | quote }}'
"#,
            ),
        }],
    )?;

    Ok(())
}

/// The `$tplYaml` engine substitutes singleton wrapper maps at ANY values
/// node before consumers read the tree, so a typed node accepts a wrapper
/// program beside its ordinary domain. The program must be a string
/// (`tpl` errors on other kinds) and the wrapper exactly one sentinel
/// member — a wider map passes through as a plain object and fails the
/// node's ordinary domain.
#[test]
fn nats_program_wrappers_inhabit_typed_leaves() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (overrides, want) in [
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYaml": "{}" } }
            }),
            true,
        ),
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYaml": true } }
            }),
            false,
        ),
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYaml": "{}", "x": 1 } }
            }),
            false,
        ),
        (
            serde_json::json!({ "podTemplate": { "topologySpreadConstraints": 7 } }),
            false,
        ),
    ] {
        let instance = chart_instances::with_override("nats", overrides.clone())?;
        assert!(
            validator.is_valid(&instance) == want,
            "program wrappers inhabit typed leaves: overrides={overrides}; want={want}"
        );
    }
    Ok(())
}
