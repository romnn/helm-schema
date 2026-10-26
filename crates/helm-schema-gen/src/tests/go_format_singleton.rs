use color_eyre::eyre;
use helm_schema_core::{PlainScalarPlacement, ValueKind};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::top_type_preimage;

#[test]
fn singleton_colon_space_maps_are_rejected_only_at_proven_block_values() -> eyre::Result<()> {
    for placement in [
        PlainScalarPlacement::Unknown,
        PlainScalarPlacement::BlockValue,
    ] {
        let schema = top_type_preimage(
            &json!({"type":"string"}),
            ValueKind::GoFormattedMap,
            placement,
        );
        let validator = jsonschema::validator_for(&schema)?;
        for value in [
            json!({"$tplYaml":"a: b"}),
            json!({"token":"a:\tb"}),
            json!({"token":"a:  b"}),
        ] {
            sim_assert_eq!(have: validator.is_valid(&value), want: placement == PlainScalarPlacement::Unknown, "{value}; {schema}");
        }
        for value in [
            json!({"$tplYaml":"4333"}),
            json!({"token":"a:b"}),
            json!({"token":"safe\n  injected: value"}),
            json!({"a":"safe\n  injected: |\n    text","z":"a: b"}),
            json!({"token":"<no value>"}),
            json!({"token":"a: <no value>b"}),
            json!({"token":"a: b\n"}),
            json!({"token":"a: b\u{2028}"}),
            json!({"unsafe key":"a: b"}),
            json!({"token":{"nested":"a: b"}}),
        ] {
            sim_assert_eq!(have: validator.is_valid(&value), want: true, "{value}; {schema}");
        }
    }
    Ok(())
}

#[test]
fn singleton_map_exclusion_does_not_change_sequence_preimages() {
    sim_assert_eq!(have: top_type_preimage(&json!({"type":"array"}), ValueKind::GoFormattedSequence, PlainScalarPlacement::BlockValue), want: json!({}));
}
