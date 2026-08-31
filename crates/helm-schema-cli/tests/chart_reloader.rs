//! Semantic assertions for reloader: the un-spaced pipe idiom
//! (`{{- toYaml .| nindent 6 }}` under `with .Values.reloader.podMonitor.
//! tlsConfig`) must type `tlsConfig` as the `PodMonitor` CRD object. The
//! v0.0.6 grammar absorbed the pipe into the argument and emitted an
//! unsatisfiable "truthy tlsConfig must be null-or-string" clause, so a
//! user setting the map the CRD requires was rejected while `helm template`
//! rendered it fine. Values validation and the full-schema pin live in
//! `chart_corpus.rs`; these behavior assertions keep a future fixture
//! regeneration from silently re-pinning the defect.

use color_eyre::eyre;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

#[test]
fn reloader_pod_monitor_tls_config_accepts_the_map_helm_renders() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("reloader")?;
    let validator = jsonschema::validator_for(&schema)?;

    // `helm template` renders this overlay byte-identically to the spaced
    // `toYaml . | nindent 6` form; the schema must accept it.
    let map_instance = chart_instances::with_override(
        "reloader",
        serde_json::json!({
            "reloader": {
                "podMonitor": {
                    "enabled": true,
                    "tlsConfig": { "insecureSkipVerify": true },
                }
            }
        }),
    )?;
    let errors: Vec<String> = validator
        .iter_errors(&map_instance)
        .map(|error| error.to_string())
        .collect();
    assert!(
        errors.is_empty(),
        "podMonitor.tlsConfig map must validate: {errors:?}"
    );

    // A scalar where the CRD requires an object still fails: the fix must
    // not degrade the sink typing into "anything goes".
    let scalar_instance = chart_instances::with_override(
        "reloader",
        serde_json::json!({
            "reloader": {
                "podMonitor": { "enabled": true, "tlsConfig": "text" }
            }
        }),
    )?;
    assert!(
        !validator.is_valid(&scalar_instance),
        "a scalar podMonitor.tlsConfig must still be rejected"
    );
    Ok(())
}
