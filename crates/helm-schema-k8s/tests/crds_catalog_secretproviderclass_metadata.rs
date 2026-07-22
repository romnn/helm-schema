//! CRD-catalog metadata lookup regression for `SecretProviderClass`.

use helm_schema_core::{ResourceRef, YamlPath};
use helm_schema_k8s::{CrdsCatalogSchemaProvider, K8sSchemaProvider};
use test_util::prelude::sim_assert_eq;

fn resource() -> ResourceRef {
    ResourceRef::concrete(
        "secrets-store.csi.x-k8s.io/v1".to_string(),
        "SecretProviderClass".to_string(),
    )
}

#[test]
fn secretproviderclass_metadata_name_uses_objectmeta_string_schema() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .lookup(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        )
        .into_schema_fragment()
        .expect("metadata.name schema")
        .into_schema();

    sim_assert_eq!(
        have: schema.get("type").and_then(serde_json::Value::as_str),
        want: Some("string"),
        "metadata.name should use the standard objectmeta string schema, got {schema}"
    );
}

#[test]
fn secretproviderclass_metadata_labels_use_objectmeta_string_map() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .lookup(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        )
        .into_schema_fragment()
        .expect("metadata.labels schema")
        .into_schema();

    sim_assert_eq!(
        have: schema
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        want: Some("string"),
        "metadata.labels should use the standard objectmeta string-map schema, got {schema}"
    );
}

#[test]
fn secretproviderclass_metadata_annotations_use_objectmeta_string_map() {
    let provider = CrdsCatalogSchemaProvider::new().with_allow_download(true);

    let schema = provider
        .lookup(
            &resource(),
            &YamlPath(vec!["metadata".to_string(), "annotations".to_string()]),
        )
        .into_schema_fragment()
        .expect("metadata.annotations schema")
        .into_schema();

    sim_assert_eq!(
        have: schema
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        want: Some("string"),
        "metadata.annotations should use the standard objectmeta string-map schema, got {schema}"
    );
}
