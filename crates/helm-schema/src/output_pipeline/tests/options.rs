use super::ReferenceMode;
use test_util::prelude::sim_assert_eq;

#[test]
fn reference_mode_defaults_to_self_contained_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(false, false),
        want: ReferenceMode::SelfContained
    );
    assert!(ReferenceMode::SelfContained.bundles_refs());
    assert!(!ReferenceMode::SelfContained.fully_inlines_refs());
}

#[test]
fn keep_refs_selects_reference_preserving_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(true, false),
        want: ReferenceMode::PreserveRefs
    );
    assert!(!ReferenceMode::PreserveRefs.bundles_refs());
    assert!(!ReferenceMode::PreserveRefs.fully_inlines_refs());
}

#[test]
fn inline_refs_selects_fully_inlined_export_output() {
    sim_assert_eq!(
        have: ReferenceMode::from_flags(false, true),
        want: ReferenceMode::FullyInlinedExport
    );
    assert!(!ReferenceMode::FullyInlinedExport.bundles_refs());
    assert!(ReferenceMode::FullyInlinedExport.fully_inlines_refs());
}
