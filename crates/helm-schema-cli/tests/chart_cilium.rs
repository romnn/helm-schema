//! Semantic assertions for the cilium chart: the SPIRE agent and
//! server images are string-or-object type dispatches NESTED under outer
//! enable guards, and the string arm must stay valid with every outer
//! guard ACTIVE. A sparse values document does not exercise this — the
//! historical regression only appeared once the authentication, SPIRE,
//! and install guards were all enabled. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

#[test]
fn cilium_spire_images_accept_strings_under_active_guards() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("cilium")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // `authentication.enabled` must be on: the chart's own validator
    // (`fail` in validate.yaml) genuinely rejects SPIRE without it, and the
    // schema correctly encodes that clause.
    for image in [
        serde_json::json!("repo/image:tag"),
        serde_json::json!({ "repository": "repo/image", "tag": "tag" }),
    ] {
        let instance = chart_instances::with_override(
            "cilium",
            serde_json::json!({
                "authentication": {
                    "enabled": true,
                    "mutual": {
                        "spire": {
                            "enabled": true,
                            "install": {
                                "enabled": true,
                                "agent": { "image": image },
                                "server": { "image": image }
                            }
                        }
                    }
                }
            }),
        )?;
        assert!(
            validator.is_valid(&instance),
            "the string and object image arms both render under active \
             outer guards: image={image}; errors: {}",
            validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        );
    }
    Ok(())
}

/// The `validate.yaml` int-cast domain checks bind their base-0 string
/// preimages exactly: under the default `cluster.name`, any `cluster.id`
/// spelling certainly coercing to nonzero aborts; a live standalone DNS
/// proxy aborts on every spelling coercing to 0 (unparseable included);
/// and `maxConnectedClusters` must certainly coerce into {255, 511} in
/// any radix. Each polarity is helm-verified against the template-level
/// validators.
#[test]
fn cilium_int_cast_validators_bind_base0_string_preimages() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("cilium")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (overlay, want, label) in [
        (
            serde_json::json!({ "cluster": { "name": "default", "id": "1" } }),
            false,
            "a nonzero cluster.id spelling under the default name aborts",
        ),
        (
            serde_json::json!({ "cluster": { "name": "default", "id": "01" } }),
            false,
            "octal cluster.id 1 aborts the same way",
        ),
        (
            serde_json::json!({ "cluster": { "name": "default", "id": "0" } }),
            true,
            "a zero-coercing cluster.id renders",
        ),
        (
            serde_json::json!({ "cluster": { "id": 1, "name": "prod" } }),
            true,
            "a non-default cluster name escapes the id check",
        ),
        (
            serde_json::json!({
                "standaloneDnsProxy": { "enabled": true },
                "dnsProxy": { "proxyPort": "0x0" },
            }),
            false,
            "a zero-coercing proxy port aborts the live DNS proxy",
        ),
        (
            serde_json::json!({
                "standaloneDnsProxy": { "enabled": true },
                "dnsProxy": { "proxyPort": "oops" },
            }),
            false,
            "an unparseable proxy port coerces to 0 and aborts",
        ),
        (
            serde_json::json!({
                "standaloneDnsProxy": { "enabled": true },
                "dnsProxy": { "proxyPort": "10094" },
            }),
            true,
            "a nonzero proxy port renders",
        ),
        (
            serde_json::json!({ "dnsProxy": { "proxyPort": "0" } }),
            true,
            "a dormant DNS proxy keeps every spelling open",
        ),
        (
            serde_json::json!({ "clustermesh": { "maxConnectedClusters": "300" } }),
            false,
            "a spelling certainly outside {255, 511} aborts",
        ),
        (
            serde_json::json!({ "clustermesh": { "maxConnectedClusters": "0x1ff" } }),
            true,
            "hex 511 renders",
        ),
        (
            serde_json::json!({ "clustermesh": { "maxConnectedClusters": "255" } }),
            true,
            "decimal 255 renders",
        ),
    ] {
        let instance = chart_instances::with_override("cilium", overlay)?;
        assert!(
            validator.is_valid(&instance) == want,
            "{label}: instance={instance}"
        );
    }
    Ok(())
}
