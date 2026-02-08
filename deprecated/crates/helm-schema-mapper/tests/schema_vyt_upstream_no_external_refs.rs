use color_eyre::eyre;
use helm_schema_mapper::schema::UpstreamK8sSchemaProvider;
use helm_schema_mapper::vyt::{ResourceRef, YPath};
use serde_json::Value;
use std::path::PathBuf;

fn has_any_ref(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            if o.contains_key("$ref") {
                return true;
            }
            o.values().any(has_any_ref)
        }
        Value::Array(a) => a.iter().any(has_any_ref),
        _ => false,
    }
}

#[test]
fn upstream_provider_returns_fully_expanded_leaf_schema() -> eyre::Result<()> {
    // This uses the vendored mini-cache under testdata.
    let cache_dir =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/kubernetes-json-schema");
    let provider = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(cache_dir)
        .with_allow_download(false);

    let deployment = ResourceRef {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
    };

    // Path that lands on the *container item schema*, which contains nested refs
    // (volumeMounts/items/securityContext/etc). We require that it comes back
    // fully expanded with no $ref anywhere.
    let path = YPath(vec![
        "spec".into(),
        "template".into(),
        "spec".into(),
        "initContainers[*]".into(),
    ]);

    let schema = provider
        .schema_for_resource_ypath(&deployment, &path)?
        .ok_or_else(|| eyre::eyre!("expected schema for path"))?;

    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));
    assert!(
        schema
            .pointer("/properties/securityContext/type")
            .and_then(|v| v.as_str())
            == Some("object"),
        "expected securityContext object; got: {schema}"
    );
    assert!(
        !has_any_ref(&schema),
        "schema should not contain $ref: {schema}"
    );
    Ok(())
}
