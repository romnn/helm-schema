use super::ReferenceMode;
use test_util::prelude::sim_assert_eq;

#[test]
fn reference_mode_defaults_to_self_contained_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(false, false),
        want: ReferenceMode::SelfContained
    );
}

#[test]
fn keep_refs_selects_reference_preserving_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(true, false),
        want: ReferenceMode::PreserveRefs
    );
}

#[test]
fn inline_refs_selects_fully_inlined_export_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(false, true),
        want: ReferenceMode::FullyInlinedExport
    );
}
