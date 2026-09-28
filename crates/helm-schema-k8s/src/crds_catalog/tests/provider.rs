use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;

use super::*;
use crate::cache::default_source_id;

fn widget_resource() -> ResourceRef {
    ResourceRef::concrete("example.com/v1".to_string(), "Widget".to_string())
}

#[test]
fn catalog_lookup_attaches_provider_source() {
    let scratch = ScratchDir::new("crd-source").expect("create CRD cache scratch");
    let cache_dir = scratch.path().to_path_buf();
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
