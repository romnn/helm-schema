//! Semantic assertions for the oauth2-proxy chart: the secret helpers
//! apply `tpl` to `config.existingSecret` (under its own truthiness) and
//! to `config.{cookieSecret,clientSecret,clientID}` under literal
//! membership in `config.requiredSecretKeys` (`has "cookie-secret"
//! .Values.config.requiredSecretKeys`). `tpl` type-asserts a Go string,
//! so those helper-local operands carry string contracts back to the
//! callers with the helper-internal `has` gates intact. Values
//! validation and the full-schema pin live in `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

/// With redis-ha live, `sentinel.quorum` and `splitBrainDetection.*`
/// render only into ConfigMap script text and the statefulset's
/// `print (include …) (include …)| sha256sum` checksum digest — the
/// un-spaced pipe reads exactly like the spaced form, so the digest
/// keeps every spelling open and numerics stay accepted.
#[test]
fn oauth2_proxy_redis_script_reads_stay_partial_text() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("oauth2-proxy")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for (label, override_) in [
        (
            "a numeric sentinel quorum renders into script text",
            serde_json::json!({
                "redis-ha": { "enabled": true, "sentinel": { "quorum": 2 } }
            }),
        ),
        (
            "a numeric split-brain interval renders into script text",
            serde_json::json!({
                "redis-ha": { "enabled": true, "splitBrainDetection": { "interval": 60 } }
            }),
        ),
        (
            "a numeric ro_replicas renders into script text",
            serde_json::json!({
                "redis-ha": { "enabled": true, "ro_replicas": 1 }
            }),
        ),
    ] {
        let instance =
            chart_instances::with_override("oauth2-proxy", override_).expect("compose instance");
        assert!(
            validator.is_valid(&instance),
            "{label}: instance={instance}"
        );
    }
    Ok(())
}

#[test]
fn oauth2_proxy_helper_tpl_operands_bind_string_contracts() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("oauth2-proxy")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Compose over the chart defaults: helm validates the coalesced
    // document, where `requiredSecretKeys` carries all three keys and
    // `proxyVarsAsSecrets` is true unless the user overrides them.
    let compose = |override_: serde_json::Value| {
        chart_instances::with_override("oauth2-proxy", override_).expect("compose instance")
    };
    for (instance, want, label) in [
        (
            compose(serde_json::json!({
                "config": { "cookieSecret": { "nested": "map" } }
            })),
            false,
            "a map cookieSecret aborts the helper-local tpl",
        ),
        (
            compose(serde_json::json!({
                "config": { "clientSecret": 7 }
            })),
            false,
            "an integer clientSecret aborts the helper-local tpl",
        ),
        (
            compose(serde_json::json!({
                "config": { "clientID": false }
            })),
            false,
            "a boolean clientID aborts the helper-local tpl",
        ),
        (
            compose(serde_json::json!({
                "config": { "existingSecret": { "nested": "map" } }
            })),
            false,
            "a map existingSecret aborts the secretName helper's tpl",
        ),
        (
            compose(serde_json::json!({
                "config": { "existingSecret": "external-secret" }
            })),
            true,
            "a string existingSecret renders",
        ),
        (
            compose(serde_json::json!({
                "config": {
                    "requiredSecretKeys": [],
                    "cookieSecret": { "nested": "map" }
                }
            })),
            true,
            "an empty requiredSecretKeys keeps every tpl dormant",
        ),
        (
            compose(serde_json::json!({
                "config": {
                    "requiredSecretKeys": ["client-id"],
                    "cookieSecret": { "nested": "map" }
                }
            })),
            true,
            "a cookie-secret-free membership keeps cookieSecret dormant",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}
