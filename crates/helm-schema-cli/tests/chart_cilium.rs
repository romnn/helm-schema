//! Semantic assertions for the cilium chart (F37): the SPIRE agent and
//! server images are string-or-object type dispatches NESTED under outer
//! enable guards, and the string arm must stay valid with every outer
//! guard ACTIVE. A sparse values document does not exercise this — the
//! historical regression only appeared once the authentication, SPIRE,
//! and install guards were all enabled. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn cilium_spire_images_accept_strings_under_active_guards() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("cilium")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // `authentication.enabled` must be on: the chart's own validator
    // (`fail` in validate.yaml) genuinely rejects SPIRE without it, and the
    // schema correctly encodes that clause.
    for image in [
        serde_json::json!("repo/image:tag"),
        serde_json::json!({ "repository": "repo/image", "tag": "tag" }),
    ] {
        let instance = serde_json::json!({
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
        });
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
