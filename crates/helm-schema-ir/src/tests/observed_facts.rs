use std::collections::{BTreeMap, BTreeSet};

use test_util::prelude::sim_assert_eq;

use crate::observed_facts::{HintGrade, HintIntent, HintScope, ObservedFacts};

#[test]
fn hint_grades_absorb_without_parallel_lanes() {
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
        observed.extend_type_hints(grade, &path, &hints);
        sim_assert_eq!(
            have: observed.type_hints.get(&grade),
            want: Some(&BTreeMap::from([(path, hints)]))
        );
    }

    let guarded_tested = HintGrade {
        scope: HintScope::Guarded,
        intent: HintIntent::Tested,
    };
    let mut other = ObservedFacts::default();
    other.insert_type_hint(guarded_tested, "predicate".to_owned(), "boolean");
    observed.absorb(&other);

    sim_assert_eq!(
        have: observed.type_hints.get(&guarded_tested),
        want: Some(&BTreeMap::from([(
            "predicate".to_owned(),
            BTreeSet::from(["boolean".to_owned()]),
        )]))
    );
}
