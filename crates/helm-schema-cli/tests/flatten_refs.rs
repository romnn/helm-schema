//! Regression coverage for output-only `$ref` handling. External refs can be
//! re-homed into local `$defs`, fully inlined for export, or preserved
//! literally when requested.
//!
//! Tests exercise the lower-level `flatten_with_retriever` API with an
//! in-memory `Retrieve` keyed by URI — no temp dirs, no real filesystem
//! activity. The same retrieval abstraction handles production
//! filesystem-backed calls.

use std::collections::HashMap;

use color_eyre::eyre::WrapErr;
use helm_schema_cli::flatten;
use helm_schema_cli::schema_override;
use jsonschema::{Retrieve, Uri};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

const BASE_URI: &str = "file:///chart/";

/// Map-backed `Retrieve` used in tests. Pre-populates URIs to JSON
/// content so the dereferencer can resolve refs without touching disk.
struct InlineRetriever(HashMap<String, Value>);

impl InlineRetriever {
    fn new<I: IntoIterator<Item = (&'static str, Value)>>(entries: I) -> Self {
        let mut m = HashMap::new();
        for (uri, value) in entries {
            m.insert(uri.to_string(), value);
        }
        Self(m)
    }
}

impl Retrieve for InlineRetriever {
    fn retrieve(
        &self,
        uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("InlineRetriever: no entry for {uri}").into())
    }
}

/// Base schema a real generation would have produced for a minimal
/// chart. Kept inline so the test is self-contained against generation
/// behaviour drift.
fn base_schema() -> Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {},
        "type": "object"
    })
}

const SHARED_CLOUD_SCHEMA: &str = r#"{"enum": [null, "azure", "minikube"]}"#;

fn cloud_schema() -> Value {
    serde_json::from_str(SHARED_CLOUD_SCHEMA).expect("parse cloud schema")
}

fn cloud_override() -> Value {
    serde_json::json!({
        "properties": { "cloud": { "$ref": "../schemas/cloud.json" } }
    })
}

#[test]
fn external_refs_can_be_bundled_into_defs() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = schema_override::apply_schema_override(base_schema(), cloud_override());

    // Relative ref `../schemas/cloud.json` from base `file:///chart/`
    // resolves to `file:///schemas/cloud.json`.
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);
    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {
            "schema1": { "enum": [null, "azure", "minikube"] }
        },
        "additionalProperties": false,
        "properties": {
            "cloud": { "$ref": "#/$defs/schema1" }
        },
        "type": "object"
    });

    sim_assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn repeated_external_refs_share_one_definition() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "cloud": { "$ref": "../schemas/cloud.json" },
            "provider": { "$ref": "../schemas/cloud.json" }
        }
    });
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);

    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    sim_assert_eq!(
        actual.pointer("/properties/cloud/$ref"),
        Some(&Value::String("#/$defs/schema1".to_string()))
    );
    sim_assert_eq!(
        actual.pointer("/properties/provider/$ref"),
        Some(&Value::String("#/$defs/schema1".to_string()))
    );
    sim_assert_eq!(actual.pointer("/$defs/schema1"), Some(&cloud_schema()));

    Ok(())
}

#[test]
fn bundled_definition_names_do_not_overwrite_existing_defs() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": {
            "schema1": { "type": "number" }
        },
        "type": "object",
        "properties": {
            "cloud": { "$ref": "../schemas/cloud.json" }
        }
    });
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);

    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    sim_assert_eq!(
        actual.pointer("/$defs/schema1"),
        Some(&serde_json::json!({ "type": "number" }))
    );
    sim_assert_eq!(actual.pointer("/$defs/schema2"), Some(&cloud_schema()));
    sim_assert_eq!(
        actual.pointer("/properties/cloud/$ref"),
        Some(&Value::String("#/$defs/schema2".to_string()))
    );

    Ok(())
}

#[test]
fn bundled_external_document_id_updates_relative_ref_base() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "external": { "$ref": "shared.json" }
        }
    });
    let shared = serde_json::json!({
        "$id": "schemas/shared.json",
        "type": "object",
        "properties": {
            "name": { "$ref": "defs.json#/$defs/name" }
        }
    });
    let definitions = serde_json::json!({
        "$defs": {
            "name": { "type": "string" }
        }
    });
    let retriever = InlineRetriever::new([
        ("file:///chart/shared.json", shared),
        ("file:///chart/schemas/defs.json", definitions),
    ]);

    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    sim_assert_eq!(
        actual.pointer("/properties/external/$ref"),
        Some(&Value::String("#/$defs/schema1".to_string()))
    );
    sim_assert_eq!(
        actual.pointer("/$defs/schema1/properties/name/$ref"),
        Some(&Value::String("#/$defs/schema2".to_string()))
    );
    sim_assert_eq!(
        actual.pointer("/$defs/schema2"),
        Some(&serde_json::json!({ "type": "string" }))
    );

    Ok(())
}

#[test]
fn root_document_id_does_not_turn_local_refs_into_external_fetches() -> color_eyre::eyre::Result<()>
{
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$id": "https://example.test/root.schema.json",
        "$defs": {
            "name": { "type": "string" }
        },
        "type": "object",
        "properties": {
            "name": { "$ref": "#/$defs/name" },
            "cloud": { "$ref": "../schemas/cloud.json" }
        }
    });
    let retriever =
        InlineRetriever::new([("https://example.test/schemas/cloud.json", cloud_schema())]);

    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    sim_assert_eq!(
        actual.pointer("/properties/name/$ref"),
        Some(&Value::String("#/$defs/name".to_string()))
    );
    sim_assert_eq!(
        actual.pointer("/properties/cloud/$ref"),
        Some(&Value::String("#/$defs/schema1".to_string()))
    );

    Ok(())
}

#[test]
fn bundling_does_not_rewrite_ref_keys_inside_enum_data() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "literal": {
                "enum": [
                    { "$ref": "this is data, not a schema reference" }
                ]
            },
            "cloud": { "$ref": "../schemas/cloud.json" }
        }
    });
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);

    let actual = flatten::bundle_with_retriever(merged, BASE_URI, retriever).wrap_err("bundle")?;

    sim_assert_eq!(
        actual.pointer("/properties/literal/enum/0/$ref"),
        Some(&Value::String(
            "this is data, not a schema reference".to_string()
        ))
    );
    sim_assert_eq!(
        actual.pointer("/properties/cloud/$ref"),
        Some(&Value::String("#/$defs/schema1".to_string()))
    );

    Ok(())
}

#[test]
fn bundling_rejects_non_object_root_defs_before_writing_dangling_refs() {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "$defs": false,
        "type": "object",
        "properties": {
            "cloud": { "$ref": "../schemas/cloud.json" }
        }
    });
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);

    let err = flatten::bundle_with_retriever(merged, BASE_URI, retriever)
        .expect_err("non-object $defs should be rejected");

    assert!(
        err.to_string().contains("root $defs is not an object"),
        "unexpected error: {err}"
    );
}

#[test]
fn external_refs_can_be_fully_inlined_for_export() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = schema_override::apply_schema_override(base_schema(), cloud_override());
    let retriever = InlineRetriever::new([("file:///schemas/cloud.json", cloud_schema())]);
    let actual =
        flatten::flatten_with_retriever(merged, BASE_URI, retriever).wrap_err("flatten")?;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "cloud": { "enum": [null, "azure", "minikube"] }
        },
        "type": "object"
    });

    sim_assert_eq!(actual, expected);
    Ok(())
}

/// `--keep-refs` preserves references through the output pipeline. Verify the
/// override-merged document keeps its literal `$ref` string.
#[test]
fn keep_refs_path_preserves_literal_ref_strings() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = schema_override::apply_schema_override(base_schema(), cloud_override());

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "cloud": { "$ref": "../schemas/cloud.json" }
        },
        "type": "object"
    });

    sim_assert_eq!(merged, expected);
    Ok(())
}

/// URL ref with a JSON Pointer fragment descends into the loaded
/// document instead of inlining the whole thing. This is the headline
/// behaviour the `referencing`-backed dereferencer gives us for free —
/// our old hand-rolled flatten would have inlined the whole document.
#[test]
fn url_with_fragment_descends_pointer() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let podspec_like = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "securityContext": {
                "type": "object",
                "properties": {
                    "runAsNonRoot": { "type": "boolean" }
                }
            },
            "tolerations": {
                "type": "array",
                "items": { "type": "object" }
            }
        }
    });

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "securityContext": {
                "$ref": "https://example.test/podspec.json#/properties/securityContext"
            }
        }
    });

    let retriever = InlineRetriever::new([("https://example.test/podspec.json", podspec_like)]);

    let actual =
        flatten::flatten_with_retriever(merged, BASE_URI, retriever).wrap_err("flatten")?;

    let security_context = actual
        .pointer("/properties/securityContext")
        .expect("securityContext present");
    let expected_security_context = serde_json::json!({
        "type": "object",
        "properties": {
            "runAsNonRoot": { "type": "boolean" }
        }
    });
    sim_assert_eq!(security_context.clone(), expected_security_context);

    // The tolerations array from the fetched podspec must NOT have
    // leaked into our schema. If it does, the dereferencer is inlining
    // the whole document rather than descending the pointer.
    assert!(
        actual.pointer("/properties/tolerations").is_none(),
        "pointer descent failed — siblings of the targeted node leaked in: {actual}",
    );

    Ok(())
}

/// File ref with a JSON Pointer fragment: same as the URL case but
/// with a file URI. `referencing` handles both uniformly.
#[test]
fn file_with_fragment_descends_pointer() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let bundle = serde_json::json!({
        "$defs": {
            "Port": { "type": "integer", "format": "int32" },
            "Host": { "type": "string", "format": "hostname" }
        }
    });

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "port": { "$ref": "../schemas/net.json#/$defs/Port" }
        }
    });

    let retriever = InlineRetriever::new([("file:///schemas/net.json", bundle)]);

    let actual =
        flatten::flatten_with_retriever(merged, BASE_URI, retriever).wrap_err("flatten")?;

    let port = actual.pointer("/properties/port").expect("port present");
    sim_assert_eq!(
        port.clone(),
        serde_json::json!({ "type": "integer", "format": "int32" })
    );

    Ok(())
}

/// Bare fragment refs (`#/$defs/...`) resolve against the current
/// document. helm-schema's own output doesn't emit these, but overrides
/// could, and the `referencing`-backed pass handles them out of the
/// box.
#[test]
fn bare_fragment_refs_resolve_within_document() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "primary": { "$ref": "#/$defs/Address" },
            "billing": { "$ref": "#/$defs/Address" }
        },
        "$defs": {
            "Address": {
                "type": "object",
                "properties": {
                    "street": { "type": "string" },
                    "city": { "type": "string" }
                }
            }
        }
    });

    let retriever = InlineRetriever::new([]);
    let actual =
        flatten::flatten_with_retriever(merged, BASE_URI, retriever).wrap_err("flatten")?;

    let expected_address = serde_json::json!({
        "type": "object",
        "properties": {
            "street": { "type": "string" },
            "city": { "type": "string" }
        }
    });
    sim_assert_eq!(
        actual.pointer("/properties/primary").unwrap().clone(),
        expected_address
    );
    sim_assert_eq!(
        actual.pointer("/properties/billing").unwrap().clone(),
        expected_address
    );

    Ok(())
}

/// RFC 6901 escapes in JSON Pointer fragments (`~0` → `~`, `~1` → `/`)
/// resolve correctly. The `referencing` crate handles the escape rules
/// per the spec; we just verify the round-trip end-to-end.
#[test]
fn rfc6901_pointer_escapes_resolve() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let merged = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "media": { "$ref": "#/$defs/application~1json" }
        },
        "$defs": {
            "application/json": { "type": "string", "format": "media-range" }
        }
    });

    let retriever = InlineRetriever::new([]);
    let actual =
        flatten::flatten_with_retriever(merged, BASE_URI, retriever).wrap_err("flatten")?;

    sim_assert_eq!(
        actual.pointer("/properties/media").unwrap().clone(),
        serde_json::json!({ "type": "string", "format": "media-range" })
    );
    Ok(())
}
