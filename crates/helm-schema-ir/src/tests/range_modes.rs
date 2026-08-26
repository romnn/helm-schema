use test_util::prelude::sim_assert_eq;

use crate::range_modes::RangeModes;

#[test]
fn range_modes_keep_escaped_path_identity_and_legacy_order() {
    let mut modes = RangeModes::default();
    modes.mark_input_identity(r"service.prometheus\.io");
    modes.mark_json_decoded(r"service.prometheus\.io");
    modes.mark_member_identity(r"service.a\\b");

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

    let mode = modes.mode(r"service.prometheus\.io");
    assert!(mode.input_identity && mode.json_decoded && !mode.member_identity);
}

#[test]
fn range_mode_remapping_unions_collapsed_structural_paths() {
    let mut modes = RangeModes::default();
    modes.mark_input_identity("first.items");
    modes.mark_member_identity("second.items");
    modes.map_value_paths(&mut |_| "selected.items".to_string());

    let selected = modes.mode("selected.items");
    assert!(selected.input_identity && selected.member_identity);
    sim_assert_eq!(have: modes.iter().count(), want: 1);
}
