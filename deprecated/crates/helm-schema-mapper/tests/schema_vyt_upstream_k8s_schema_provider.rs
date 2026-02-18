use color_eyre::eyre;
use helm_schema_mapper::schema::UpstreamK8sSchemaProvider;
use helm_schema_mapper::schema::VytSchemaProvider;
use helm_schema_mapper::vyt::{ResourceRef, VYKind, VYUse, YPath};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("helm-schema-{}-{}", name, nanos))
}

fn write_json(path: &PathBuf, content: &str) -> eyre::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

#[test]
fn upstream_provider_traverses_arrays_and_resolves_cross_file_refs() -> eyre::Result<()> {
    let cache_dir = unique_temp_dir("upstream-provider");
    let version_dir = "vtest";

    // Expected yannh-style filename for (Deployment, apps/v1)
    let deployment_file = cache_dir.join(version_dir).join("deployment-apps-v1.json");
    let defs_file = cache_dir.join(version_dir).join("defs.json");

    write_json(
        &defs_file,
        r#"{
  "definitions": {
    "SecurityContext": {
      "type": "object",
      "properties": {
        "runAsNonRoot": {"type": "boolean"}
      }
    }
  }
}"#,
    )?;

    // Minimal root schema for a Deployment with containers[*].securityContext $ref
    write_json(
        &deployment_file,
        r#"{
  "type": "object",
  "properties": {
    "spec": {
      "type": "object",
      "properties": {
        "template": {
          "type": "object",
          "properties": {
            "spec": {
              "type": "object",
              "properties": {
                "containers": {
                  "type": "array",
                  "items": {
                    "type": "object",
                    "properties": {
                      "securityContext": {"$ref": "defs.json#/definitions/SecurityContext"}
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}"#,
    )?;

    let provider = UpstreamK8sSchemaProvider::new(version_dir)
        .with_cache_dir(cache_dir.clone())
        .with_allow_download(false);

    let resource = ResourceRef {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
    };

    // Sanity: resource schema loads (proves filename mapping)
    provider.schema_for_resource(&resource)?;

    // Traverse through array + $ref: containers[*].securityContext.runAsNonRoot
    let u = VYUse {
        source_expr: "dummy".to_string(),
        path: YPath(vec![
            "spec".into(),
            "template".into(),
            "spec".into(),
            "containers[*]".into(),
            "securityContext".into(),
            "runAsNonRoot".into(),
        ]),
        kind: VYKind::Scalar,
        guards: vec![],
        resource: Some(resource),
    };

    let schema: serde_json::Value = provider
        .schema_for_use(&u)
        .ok_or_else(|| eyre::eyre!("expected schema for use"))?;

    let ty = schema
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| eyre::eyre!("expected type in schema: {schema:?}"))?;

    assert_eq!(ty, "boolean");

    // Best-effort cleanup.
    let _ = fs::remove_dir_all(&cache_dir);

    Ok(())
}
