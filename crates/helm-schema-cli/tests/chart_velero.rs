//! Semantic assertions for the velero chart: the storage-location paths are
//! guarded by `typeIs "[]interface {}"` before being ranged. Strings skip
//! those branches, legacy maps terminate in `NOTES.txt`, and list items are
//! consumed with field access and must be objects. Values validation and the
//! full-schema pin live in `chart_corpus.rs`.

use color_eyre::eyre;

#[path = "common/chart_instances.rs"]
mod chart_instances;
#[path = "common/schema_roundtrip.rs"]
mod schema_roundtrip;
#[path = "common/values_yaml.rs"]
mod values_yaml;

#[test]
fn velero_storage_locations_type_dispatch_holds() -> eyre::Result<()> {
    let schema = schema_roundtrip::generate_chart_schema_for_path("velero")?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    for path in ["backupStorageLocation", "volumeSnapshotLocation"] {
        let instance = |value: serde_json::Value| {
            let mut configuration = serde_json::json!({
                "backupStorageLocation": [
                    { "name": "default", "provider": "aws", "bucket": "b" }
                ],
                "volumeSnapshotLocation": [
                    { "name": "default", "provider": "aws" }
                ]
            });
            configuration
                .as_object_mut()
                .expect("configuration object")
                .insert(path.to_string(), value);
            chart_instances::with_override(
                "velero",
                serde_json::json!({ "configuration": configuration }),
            )
        };
        let valid_location = instance(serde_json::json!([
            { "name": "default", "provider": "aws", "bucket": "b" }
        ]))?;
        let valid_location_errors = validator
            .iter_errors(&valid_location)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect::<Vec<_>>();
        assert!(
            valid_location_errors.is_empty(),
            "a valid location list renders ({path}): {valid_location_errors:#?}"
        );
        assert!(
            validator.is_valid(&instance(serde_json::json!("ignored"))?),
            "a string skips both the list renderer and map migration check ({path})"
        );
        assert!(
            !validator.is_valid(&instance(serde_json::json!({ "name": "legacy" }))?),
            "the notes migration accumulator rejects the legacy map form ({path})"
        );
        assert!(
            !validator.is_valid(&instance(serde_json::json!([7]))?),
            "a scalar item fails field access inside the range ({path})"
        );
    }
    Ok(())
}
