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

/// Wrapper RESULT compatibility (F104 remainder): the engine substitutes
/// the decoded program before consumers read the tree, so a static
/// `$tplYaml` program whose decoded kind cannot inhabit the node rejects
/// (an extraResources item must decode to a mapping), and a
/// `$tplYamlSpread` program can only spread a slice onto a slice or a map
/// onto a map — a scalar result always aborts, and the values root
/// refuses the spread wrapper outright. Every polarity below reproduces
/// under `helm template` on the vendored chart.
#[test]
fn nats_wrapper_results_must_be_compatible_with_their_sinks() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (overrides, want) in [
        // Replace programs at the item node: Helm decodes each rendered
        // extraResources document as a mapping, so scalar and list
        // decodings abort even though the wrapper map itself is an object.
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": "true" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": "4333" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": "audit" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": "[a]" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": "{{ .Values.global }}" }] }),
            true,
        ),
        // tpl aborts on a non-string program even where objects are
        // otherwise acceptable: the engine intercepts every singleton
        // sentinel map before the node's ordinary domain sees it.
        (
            serde_json::json!({ "extraResources": [{ "$tplYaml": true }] }),
            false,
        ),
        // Spread programs: result kind must match the parent collection.
        (
            serde_json::json!({ "extraResources": [{ "$tplYamlSpread": "{a: 1}" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYamlSpread": "audit" }] }),
            false,
        ),
        (
            serde_json::json!({ "extraResources": [{ "$tplYamlSpread":
                "- {apiVersion: v1, kind: ConfigMap, metadata: {name: x}}" }] }),
            true,
        ),
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYamlSpread": "[]" } }
            }),
            false,
        ),
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYamlSpread": "7" } }
            }),
            false,
        ),
        (
            serde_json::json!({
                "podTemplate": { "topologySpreadConstraints": { "$tplYamlSpread": "{a: 1}" } }
            }),
            true,
        ),
    ] {
        let instance = chart_instances::with_override("nats", overrides.clone())?;
        assert!(
            validator.is_valid(&instance) == want,
            "wrapper results intersect their sinks: overrides={overrides}; want={want}"
        );
    }

    // The engine refuses to spread at the recursion root, so the values
    // document itself must not be a singleton spread wrapper; the replace
    // wrapper stays legal there.
    assert!(
        !validator.is_valid(&serde_json::json!({ "$tplYamlSpread": "a: 1" })),
        "a singleton spread wrapper at the values root aborts rendering"
    );
    assert!(
        validator.is_valid(&serde_json::json!({ "$tplYaml": "a: 1" })),
        "a singleton replace wrapper at the values root is substituted"
    );

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

/// The `_jsonpatch.tpl` op grammar binds through the HELPER-SCOPE range
/// (F108): every values `patch` list rides `nats.loadMergePatch` into the
/// `jsonpatch` helper, whose `range $patch := $patches` gates each member
/// on `hasKey "op"`/`hasKey "path"` and the op enum. Member identities now
/// survive the JSON-roundtripped call dict, so an unknown or missing op
/// rejects while valid patches, the empty default, and the wrapper-item
/// lane (`$tplYamlSpread` inside `patch`) render — all polarities verified
/// under `helm template` on the vendored chart.
#[test]
fn nats_jsonpatch_ops_bind_through_the_helper_range() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (overrides, want) in [
        (
            serde_json::json!({ "service": { "patch": [
                { "op": "add", "path": "/metadata/labels/x", "value": "y" }
            ] } }),
            true,
        ),
        (serde_json::json!({ "service": { "patch": [] } }), true),
        (
            serde_json::json!({ "service": { "patch": [
                { "op": "bogus", "path": "/x" }
            ] } }),
            false,
        ),
        (
            serde_json::json!({ "service": { "patch": [{ "path": "/x" }] } }),
            false,
        ),
        // The wrapper-item lane stays open: the engine replaces the
        // sentinel item before jsonpatch ranges the list.
        (
            serde_json::json!({ "service": { "patch": [
                { "$tplYamlSpread": "- {op: add, path: /metadata/labels/x, value: y}" }
            ] } }),
            true,
        ),
    ] {
        let instance = chart_instances::with_override("nats", overrides.clone())?;
        assert!(
            validator.is_valid(&instance) == want,
            "jsonpatch ops bind through the helper range: overrides={overrides}; want={want}"
        );
    }
    Ok(())
}

/// Wrapper consumers BEFORE the tree rewrite (F104 residual):
/// `nats.defaultValues` calls `nats.fullname` — which truncs
/// `nameOverride`/`fullnameOverride` raw — BEFORE the `$tplYaml` engine
/// substitutes wrapper programs, so a wrapper map at those paths aborts
/// rendering. Tolerant pre-rewrite reads (the `.name` default selections
/// only copy the value into the tree the engine then rewrites) and
/// post-rewrite consumers keep their wrapper alternatives — every
/// polarity verified under `helm template` on the vendored chart.
#[test]
fn nats_pre_rewrite_strict_consumers_reject_wrapper_programs() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("nats")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (overrides, want) in [
        (
            serde_json::json!({ "nameOverride": { "$tplYaml": "x" } }),
            false,
        ),
        (
            serde_json::json!({ "fullnameOverride":
                { "$tplYaml": "{{ .Release.Name | quote }}" } }),
            false,
        ),
        (serde_json::json!({ "nameOverride": "plain" }), true),
        (
            serde_json::json!({ "configMap": { "name":
                { "$tplYaml": "{{ .Release.Name | quote }}" } } }),
            true,
        ),
    ] {
        let instance = chart_instances::with_override("nats", overrides.clone())?;
        assert!(
            validator.is_valid(&instance) == want,
            "pre-rewrite strict consumers exclude wrappers: overrides={overrides}; want={want}"
        );
    }
    Ok(())
}
