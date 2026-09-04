use std::collections::BTreeSet;

use color_eyre::eyre;
use helm_schema_core::{ConditionalGuard, ValuesPath};
use test_util::prelude::sim_assert_eq;

use crate::condition_encoding::{
    AbsenceDefaults, ConditionFragmentCache, ConditionPolarity, build_condition_clauses,
    build_condition_clauses_cached,
};

#[test]
fn condition_cache_separates_selected_values_documents() -> eyre::Result<()> {
    let base = serde_yaml::from_str("feature: {}")?;
    let guarded = serde_yaml::from_str("feature: {enabled: true}")?;
    let absent = serde_yaml::Value::Null;
    let dependency_roots = BTreeSet::new();
    let absence = AbsenceDefaults {
        deeper_stage: &absent,
        dependency_refill: &absent,
        dependency_roots: &dependency_roots,
    };
    let guards = vec![ConditionalGuard::Truthy {
        path: ValuesPath::parse("feature"),
    }];
    let ancestor = Vec::new();
    let mut cache = ConditionFragmentCache::new();

    let base_fragments =
        build_condition_clauses_cached(&guards, &ancestor, None, &base, absence, &mut cache);
    let guarded_fragments =
        build_condition_clauses_cached(&guards, &ancestor, Some(0), &guarded, absence, &mut cache);
    let expected_guarded = build_condition_clauses(
        &guards,
        &ancestor,
        &guarded,
        absence,
        ConditionPolarity::Widen,
    );

    sim_assert_eq!(have: guarded_fragments, want: expected_guarded);
    eyre::ensure!(
        base_fragments != guarded_fragments,
        "distinct values documents must produce distinct cached fragments"
    );
    Ok(())
}
