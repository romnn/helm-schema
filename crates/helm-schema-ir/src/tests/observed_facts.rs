use std::collections::{BTreeMap, BTreeSet};

use test_util::prelude::sim_assert_eq;

use crate::observed_facts::{HintGrade, HintIntent, HintScope, ObservedFacts};

#[test]
fn hint_grades_shadow_legacy_lanes_and_absorb_exhaustively() {
    let grades = [
        HintGrade::DECLARED,
        HintGrade::GUARDED_DECLARED,
        HintGrade::FALLBACK,
        HintGrade::GUARDED_FALLBACK,
        HintGrade::TESTED,
    ];
    let mut observed = ObservedFacts::default();
    for (index, grade) in grades.into_iter().enumerate() {
        let path = format!("path{index}");
        let hints = BTreeSet::from(["string".to_owned()]);
        let mut legacy = BTreeMap::new();
        observed.extend_type_hints(&mut legacy, grade, &path, &hints);
        sim_assert_eq!(
            have: legacy,
            want: BTreeMap::from([(path, hints)])
        );
    }

    let guarded_tested = HintGrade {
        scope: HintScope::Guarded,
        intent: HintIntent::Tested,
    };
    let mut other = ObservedFacts::default();
    let mut legacy = BTreeMap::new();
    other.insert_type_hint(
        &mut legacy,
        guarded_tested,
        "predicate".to_owned(),
        "boolean",
    );
    observed.absorb(&other);

    sim_assert_eq!(
        have: observed.type_hints.get(&guarded_tested),
        want: Some(&legacy)
    );
}
