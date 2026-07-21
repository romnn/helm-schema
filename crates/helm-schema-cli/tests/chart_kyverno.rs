//! Semantic assertions for the kyverno chart: the shared `kyverno.image`
//! helper explicitly fails on non-string image tags, and its chart-version
//! helper requires a version when global templating is enabled. The replicas
//! helper's zero-check does not decode and must not manufacture requirements.
//! Values validation and the full-schema pin live in `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn kyverno_image_tag_validator_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let numeric_tag = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "admissionController": { "container": { "image": { "tag": 7 } } }
        }),
    )?;
    assert!(
        !validator.is_valid(&numeric_tag),
        "the kyverno.image helper fails on non-string tags"
    );
    let string_tag = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "admissionController": { "container": { "image": { "tag": "v1.16.1" } } }
        }),
    )?;
    assert!(validator.is_valid(&string_tag), "string tags render");
    let replicas = chart_instances::with_override(
        "kyverno",
        serde_json::json!({ "backgroundController": { "replicas": 3 } }),
    )?;
    assert!(
        validator.is_valid(&replicas),
        "the replicas zero-check does not decode, so it must not reject normal counts"
    );
    Ok(())
}

/// The per-controller pull-secrets chain (`with .imagePullSecrets |
/// default .global.imagePullSecrets` feeding `kyverno.sortedImagePullSecrets`'
/// bare-dot range) binds each candidate's iterable domain exactly on its
/// selected states: a truthy scalar aborts wherever the selection picks
/// it, a truthy scalar beside a selected list never ranges, and falsy
/// spellings skip the with-body entirely (all helm-verified).
#[test]
fn kyverno_image_pull_secret_chains_bind_per_candidate_iterables() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for (label, overrides) in [
        (
            "a truthy scalar global fallback is selected past the falsy defaults and aborts",
            serde_json::json!({ "global": { "imagePullSecrets": "oops" } }),
        ),
        (
            "a truthy scalar controller primary is selected and aborts",
            serde_json::json!({ "admissionController": { "imagePullSecrets": "oops" } }),
        ),
        (
            "a truthy boolean global fallback aborts the same way",
            serde_json::json!({ "global": { "imagePullSecrets": true } }),
        ),
    ] {
        let instance = chart_instances::with_override("kyverno", overrides)?;
        assert!(!validator.is_valid(&instance), "{label}");
    }
    for (label, overrides) in [
        (
            "a falsy controller primary defers to the empty global default and renders",
            serde_json::json!({ "admissionController": { "imagePullSecrets": "" } }),
        ),
        (
            "an empty-map controller primary is falsy and renders",
            serde_json::json!({ "reportsController": { "imagePullSecrets": {} } }),
        ),
        (
            "secret lists render on both candidates",
            serde_json::json!({
                "global": { "imagePullSecrets": [{ "name": "g" }] },
                "admissionController": { "imagePullSecrets": [{ "name": "a" }] },
            }),
        ),
        (
            "a truthy scalar global beside selected lists everywhere is never ranged",
            serde_json::json!({
                "global": { "imagePullSecrets": "oops" },
                "admissionController": { "imagePullSecrets": [{ "name": "a" }] },
                "backgroundController": { "imagePullSecrets": [{ "name": "b" }] },
                "cleanupController": { "imagePullSecrets": [{ "name": "c" }] },
                "reportsController": { "imagePullSecrets": [{ "name": "r" }] },
                "crds": { "migration": { "imagePullSecrets": [{ "name": "m" }] } },
                "webhooksCleanup": { "imagePullSecrets": [{ "name": "w" }] },
                "test": { "imagePullSecrets": [{ "name": "t" }] },
            }),
        ),
    ] {
        let instance = chart_instances::with_override("kyverno", overrides)?;
        assert!(validator.is_valid(&instance), "{label}");
    }
    Ok(())
}

#[test]
fn kyverno_templating_version_validator_survives_nested_helper_arguments()
-> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("kyverno")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let disabled = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": false, "version": "" } }
        }),
    )?;
    assert!(
        validator.is_valid(&disabled),
        "disabled templating does not evaluate the required call"
    );
    let enabled = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": true, "version": "1.16.1" } }
        }),
    )?;
    assert!(
        validator.is_valid(&enabled),
        "enabled templating accepts a nonempty version"
    );
    let empty_version = chart_instances::with_override(
        "kyverno",
        serde_json::json!({
            "global": { "templating": { "enabled": true, "version": "" } }
        }),
    )?;
    assert!(
        !validator.is_valid(&empty_version),
        "the nested chartVersion helper rejects an empty version"
    );

    Ok(())
}
