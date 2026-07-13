//! Semantic assertions for the velero chart: the storage-location paths are
//! guarded by `typeIs "[]interface {}"` before being ranged, so a non-list
//! value skips the branch and renders nothing (valid), while list items are
//! consumed with field access and must be objects. Values validation and
//! the full-schema pin live in `chart_corpus.rs`.

#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;

#[test]
fn velero_storage_locations_type_dispatch_holds() -> color_eyre::eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("velero")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for path in ["backupStorageLocation", "volumeSnapshotLocation"] {
        let instance =
            |value: serde_json::Value| serde_json::json!({ "configuration": { path: value } });
        assert!(
            validator.is_valid(&instance(serde_json::json!([
                { "name": "default", "bucket": "b" }
            ]))),
            "a valid location list renders ({path})"
        );
        assert!(
            validator.is_valid(&instance(serde_json::json!("ignored"))),
            "a non-list skips the typeIs guard and renders nothing ({path})"
        );
        assert!(
            !validator.is_valid(&instance(serde_json::json!([7]))),
            "a scalar item fails field access inside the range ({path})"
        );
    }
    Ok(())
}
