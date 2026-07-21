//! Semantic assertions for the traefik chart: the pod template ranges
//! `experimental.plugins` and explicitly fails unless each value is an
//! object carrying both `moduleName` and `version`, so those are
//! per-member validator requirements. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn traefik_plugin_validator_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("traefik")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Compose over the chart defaults: helm validates the coalesced
    // document, where the pod template's own gates carry their declared
    // values unless the user null-deletes them.
    let plugins = |value: serde_json::Value| {
        chart_instances::with_override(
            "traefik",
            serde_json::json!({ "experimental": { "plugins": value } }),
        )
        .expect("compose instance")
    };
    for bad in [
        serde_json::json!({ "bad": 7 }),
        serde_json::json!({ "bad": { "moduleName": "x" } }),
        serde_json::json!({ "bad": { "version": "v1" } }),
    ] {
        assert!(
            !validator.is_valid(&plugins(bad.clone())),
            "plugins without moduleName+version objects fail rendering: {bad}"
        );
    }
    let complete = plugins(serde_json::json!({
        "ok": { "moduleName": "github.com/x/y", "version": "v1.0.0" }
    }));
    let errors = validator
        .iter_errors(&complete)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "a complete plugin renders: {errors:#?}; schema={:#?}",
        schema.pointer("/properties/experimental/properties/plugins")
    );
    assert!(
        validator.is_valid(&plugins(serde_json::json!({}))),
        "the declared empty map stays valid"
    );
    Ok(())
}

/// The http3-without-tls fail sits under the `$services` local-dict range
/// (`set $services "default" (omit .Values.service "additionalServices")`).
/// The "default" entry iterates on every render, so its `ne
/// $service.enabled false` gate re-decodes as a sound subset and the
/// terminal binds: http3 without `http.tls.enabled` aborts while a
/// disabled default service keeps the terminal dormant.
#[test]
fn traefik_http3_terminal_binds_through_the_services_overlay() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("traefik")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let bad_port = serde_json::json!({
        "http3": { "enabled": true },
        "http": { "tls": { "enabled": false } }
    });
    for (override_, want, label) in [
        (
            serde_json::json!({ "ports": { "websecure": bad_port.clone() } }),
            false,
            "http3 without tls aborts through the default service",
        ),
        (
            serde_json::json!({ "ports": { "websecure": { "http3": { "enabled": true } } } }),
            true,
            "http3 with the declared tls renders",
        ),
        (
            serde_json::json!({
                "service": { "enabled": false },
                "ports": { "websecure": bad_port.clone() }
            }),
            true,
            "a disabled default service keeps the terminal dormant",
        ),
    ] {
        let instance =
            chart_instances::with_override("traefik", override_).expect("compose instance");
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}

/// Each `experimental.localPlugins` member renders a Deployment
/// volumeMount whose `mountPath` splices `{{ $plugin.mountPath | quote }}`
/// through the pod-template projection. Sprig `quote` SKIPS nil operands,
/// so a member without `mountPath` renders an explicit null into the
/// provider-REQUIRED VolumeMount field — helm renders it, the committed
/// provider bundle is the rejecting stage.
#[test]
fn traefik_local_plugin_mount_paths_bind_provider_presence() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("traefik")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let plugins = |value: serde_json::Value| {
        chart_instances::with_override(
            "traefik",
            serde_json::json!({ "experimental": { "localPlugins": value } }),
        )
        .expect("compose instance")
    };
    for (instance, want, label) in [
        (
            plugins(serde_json::json!({
                "p": { "moduleName": "github.com/x/y", "hostPath": "/plugins" }
            })),
            false,
            "a legacy hostPath plugin without mountPath renders a null mount",
        ),
        (
            plugins(serde_json::json!({
                "p": {
                    "moduleName": "github.com/x/y",
                    "hostPath": "/plugins",
                    "mountPath": "/plugins-storage/p"
                }
            })),
            true,
            "a legacy hostPath plugin with mountPath renders",
        ),
        (
            plugins(serde_json::json!({
                "p": {
                    "moduleName": "github.com/x/y",
                    "type": "hostPath",
                    "hostPath": "/plugins"
                }
            })),
            false,
            "a typed plugin without mountPath renders a null mount",
        ),
        (
            plugins(serde_json::json!({
                "p": {
                    "moduleName": "github.com/x/y",
                    "type": "hostPath",
                    "hostPath": "/plugins",
                    "mountPath": "/plugins-storage/p"
                }
            })),
            true,
            "a typed plugin with mountPath renders",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}

/// Each `gateway.listeners` KEY renders as the Gateway CRD's
/// `spec.listeners[].name`, a SectionName with a lowercase RFC-1123
/// pattern plus a 1..=253 length window. Helm itself renders any key
/// spelling — the committed Gateway provider is the rejecting stage — so
/// the provider's key-slot constraints project onto the source map's
/// `propertyNames`, scoped to the gateway-live branch.
#[test]
fn traefik_listener_keys_carry_the_provider_section_name_domain() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("traefik")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    let live_listeners = |listeners: serde_json::Value| {
        serde_json::json!({
            "providers": { "kubernetesGateway": { "enabled": true } },
            "gateway": { "enabled": true, "listeners": listeners },
        })
    };
    let listener = || serde_json::json!({ "port": 8000, "protocol": "TCP" });
    for (instance, want, label) in [
        (
            live_listeners(serde_json::json!({ "Audit": listener() })),
            false,
            "an uppercase listener key violates the SectionName pattern",
        ),
        (
            live_listeners(serde_json::json!({ "audit": listener() })),
            true,
            "a lowercase listener key renders and validates",
        ),
        (
            live_listeners(serde_json::json!({ "a".repeat(254): listener() })),
            false,
            "a 254-char listener key exceeds the SectionName maxLength",
        ),
        (
            live_listeners(serde_json::json!({ "": listener() })),
            false,
            "an empty listener key violates the SectionName minLength",
        ),
        (
            serde_json::json!({
                "gateway": { "enabled": true, "listeners": { "Audit": listener() } },
            }),
            true,
            "a dormant kubernetesGateway keeps every key spelling open",
        ),
        (
            serde_json::json!({
                "providers": { "kubernetesGateway": { "enabled": true } },
                "gateway": { "enabled": false, "listeners": { "Audit": listener() } },
            }),
            true,
            "a disabled gateway keeps every key spelling open",
        ),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}
