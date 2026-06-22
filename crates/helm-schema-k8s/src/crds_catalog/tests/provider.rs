use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;
use crate::cache::default_source_id;

fn widget_resource() -> ResourceRef {
    ResourceRef {
        api_version: "example.com/v1".to_string(),
        kind: "Widget".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    }
}

#[test]
fn catalog_lookup_attaches_provider_source() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let cache_dir = std::env::temp_dir().join(format!("helm-schema-crd-source-{unique}"));
    let relative_path = "example.com/widget_v1.json";
    let schema_path = crd_cache_path(&cache_dir, default_source_id(), relative_path);
    std::fs::create_dir_all(
        schema_path
            .parent()
            .expect("schema cache path should have parent"),
    )
    .expect("create crd cache test directory");
    std::fs::write(
        &schema_path,
        serde_json::to_vec(&json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "#/definitions/Spec"
                }
            },
            "definitions": {
                "Spec": {
                    "type": "object",
                    "properties": {
                        "size": { "type": "integer" }
                    }
                }
            }
        }))
        .expect("serialize crd cache schema"),
    )
    .expect("write crd cache schema");

    let provider = CrdsCatalogSchemaProvider::new().with_cache_dir(cache_dir);
    let result = provider.lookup(
        &widget_resource(),
        &YamlPath(vec!["spec".to_string(), "size".to_string()]),
    );
    let ProviderLookupResult::Found { schema, .. } = result else {
        panic!("catalog lookup should resolve spec.size");
    };
    let source = schema.source().expect("catalog source should attach");

    sim_assert_eq!(have: source.origin(), want: ProviderOrigin::DefaultCatalog);
    sim_assert_eq!(have: source.source_id(), want: default_source_id());
    sim_assert_eq!(have: source.filename(), want: relative_path);
    sim_assert_eq!(have: source.pointer(), want: "/definitions/Spec/properties/size");
}
