use std::collections::BTreeSet;

use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use crate::fragment_eval::{StringPart, TaintPart, over_cap_scalar_taint};

#[test]
fn over_cap_scalar_arms_keep_only_influencing_paths() {
    let arms = (0..65)
        .map(|index| {
            let path = format!("items.{index}");
            (
                Predicate::truthy_path(format!("guards.{index}")),
                vec![
                    StringPart::Text(BTreeSet::from([format!("literal-{index}")])),
                    StringPart::Taint(TaintPart::new(BTreeSet::from([path]))),
                ],
            )
        })
        .collect();

    sim_assert_eq!(
        have: over_cap_scalar_taint(arms),
        want: vec![(
            Predicate::True,
            vec![StringPart::Taint(TaintPart::abstaining(
                (0..65).map(|index| format!("items.{index}")).collect()
            ))],
        )]
    );
}
