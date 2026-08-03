use super::ReferencePolicy;
use test_util::prelude::sim_assert_eq;

#[test]
fn reference_mode_defaults_to_self_contained_output() {
    sim_assert_eq!(
        have: ReferencePolicy::from_flags(false, false),
        want: ReferencePolicy::SelfContained
    );
}

#[test]
fn keep_refs_selects_reference_preserving_output() {
    sim_assert_eq!(
        have: ReferencePolicy::from_flags(true, false),
        want: ReferencePolicy::PreserveRefs
    );
}

#[test]
fn inline_refs_selects_fully_inlined_export_output() {
    sim_assert_eq!(
        have: ReferencePolicy::from_flags(false, true),
        want: ReferencePolicy::FullyInlinedExport
    );
}
