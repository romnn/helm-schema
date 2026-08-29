use test_util::prelude::sim_assert_eq;

use crate::range_modes::RangeModes;

#[test]
fn range_modes_keep_escaped_path_identity_and_legacy_order() {
    let mut modes = RangeModes::default();
    let prometheus = helm_schema_core::ValuesPath::parse(r"service.prometheus\.io");
    let backslash = helm_schema_core::ValuesPath::parse(r"service.a\\b");
    modes.mark_input_identity(&prometheus);
    modes.mark_json_decoded(&prometheus);
    modes.mark_member_identity(&backslash);

    let encoded = modes
        .iter()
        .map(|(path, _)| path.encode())
        .collect::<Vec<_>>();
    let mut expected = vec![
        r"service.prometheus\.io".to_string(),
        r"service.a\\b".to_string(),
    ];
    expected.sort();
    sim_assert_eq!(have: encoded, want: expected);

    let mode = modes.mode(&prometheus);
    assert!(mode.input_identity && mode.json_decoded && !mode.member_identity);
}

#[test]
fn range_mode_remapping_unions_collapsed_structural_paths() {
    let mut modes = RangeModes::default();
    modes.mark_input_identity(&helm_schema_core::ValuesPath::parse("first.items"));
    modes.mark_member_identity(&helm_schema_core::ValuesPath::parse("second.items"));
    modes.map_value_paths(&mut |_| helm_schema_core::ValuesPath::parse("selected.items"));

    let selected = modes.mode(&helm_schema_core::ValuesPath::parse("selected.items"));
    assert!(selected.input_identity && selected.member_identity);
    sim_assert_eq!(have: modes.iter().count(), want: 1);
}
