use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdsCatalogSchemaProvider, K8sSchemaProvider};

fn resource() -> ResourceRef {
    ResourceRef {
        api_version: "secrets-store.csi.x-k8s.io/v1".to_string(),
        kind: "SecretProviderClass".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    }
}

#[test]
fn secretproviderclass_metadata_name_uses_objectmeta_string_schema() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .schema_fragment_for_resource_path(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        )
        .expect("metadata.name schema")
        .into_schema();

    assert_eq!(
        schema.get("type").and_then(serde_json::Value::as_str),
        Some("string"),
        "metadata.name should use the standard objectmeta string schema, got {schema}"
    );
}

#[test]
fn secretproviderclass_metadata_labels_use_objectmeta_string_map() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .schema_fragment_for_resource_path(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        )
        .expect("metadata.labels schema")
        .into_schema();

    assert_eq!(
        schema
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        Some("string"),
        "metadata.labels should use the standard objectmeta string-map schema, got {schema}"
    );
}

#[test]
fn secretproviderclass_metadata_annotations_use_objectmeta_string_map() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .schema_fragment_for_resource_path(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "annotations".to_string()]),
        )
        .expect("metadata.annotations schema")
        .into_schema();

    assert_eq!(
        schema
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        Some("string"),
        "metadata.annotations should use the standard objectmeta string-map schema, got {schema}"
    );
}
