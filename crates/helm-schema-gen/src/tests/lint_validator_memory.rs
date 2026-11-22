use std::collections::BTreeMap;
use std::rc::Rc;

use color_eyre::eyre::{self, OptionExt as _};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::LintValidators;

/// Compiled judges expire at the phase boundary and can rebuild the same validation contract.
#[test]
fn compiled_lint_judges_end_with_the_validation_phase() -> eyre::Result<()> {
    let providers = BTreeMap::from([("text".to_owned(), json!({"type": "string"}))]);
    let mut validators = LintValidators::new(&providers);
    let condition = json!({"required": ["gate"]});
    let then_value = json!({
        "properties": {"value": {"$ref": "#/definitions/text"}}, "required": ["value"],
    });
    let instances = [
        json!({}),
        json!({"gate": true, "value": "ok"}),
        json!({"gate": true, "value": 7}),
    ];
    let check = |validators: &LintValidators<'_>| -> eyre::Result<Vec<bool>> {
        let compiled = validators
            .constraint_validator(&condition, &then_value, &BTreeMap::new())
            .ok_or_eyre("compile lint constraint")?;
        Ok(instances
            .iter()
            .map(|instance| compiled.is_valid(instance))
            .collect())
    };
    let before = check(&validators)?;
    sim_assert_eq!(have: &before, want: &vec![true, true, false]);
    let retained = validators
        .parts
        .borrow()
        .values()
        .filter_map(Option::as_ref)
        .map(Rc::downgrade)
        .collect::<Vec<_>>();
    assert!(
        !retained.is_empty(),
        "the cache must hold actual compiled judges"
    );
    validators.release_compiled_parts();
    sim_assert_eq!(have: retained.iter().filter(|judge| judge.upgrade().is_some()).count(), want: 0);
    sim_assert_eq!(have: check(&validators)?, want: before);
    Ok(())
}
