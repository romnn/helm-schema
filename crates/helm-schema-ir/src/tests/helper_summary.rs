use std::collections::{BTreeMap, BTreeSet, HashMap};
use test_util::prelude::sim_assert_eq;

use super::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
use crate::abstract_value::AbstractValue;
use crate::predicate::Predicate;
use crate::{Guard, ValueKind, YamlPath};

fn output_paths(paths: impl IntoIterator<Item = String>) -> AbstractValue {
    AbstractValue::OutputSet(
        paths
            .into_iter()
            .map(|path| (path, HelperOutputMeta::default()))
            .collect(),
    )
}

#[test]
fn helper_output_meta_projects_predicates_to_contract_guard_sets() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::from([BTreeSet::from([Predicate::Not(Box::new(
            Predicate::truthy_path("feature.enabled"),
        ))])]),
        defaulted: true,
        provenance: Vec::new(),
    };

    sim_assert_eq!(
        have: meta.contract_guard_sets("serviceAccount.name"),
        want: vec![vec![
            Guard::Not {
                path: "feature.enabled".to_string(),
            },
            Guard::Default {
                path: "serviceAccount.name".to_string(),
            },
        ]]
    );
}

#[test]
fn helper_output_meta_preserves_alternative_guard_sets() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::from([
            BTreeSet::from([
                Predicate::truthy_path("feature.enabled"),
                Predicate::truthy_path("component.enabled"),
            ]),
            BTreeSet::from([
                Predicate::truthy_path("feature.enabled").negated(),
                Predicate::truthy_path("component.enabled"),
            ]),
        ]),
        defaulted: true,
        provenance: Vec::new(),
    };

    sim_assert_eq!(
        have: meta.contract_guard_sets("serviceAccount.name"),
        want: vec![
            vec![
                Guard::Truthy {
                    path: "component.enabled".to_string(),
                },
                Guard::Truthy {
                    path: "feature.enabled".to_string(),
                },
                Guard::Default {
                    path: "serviceAccount.name".to_string(),
                },
            ],
            vec![
                Guard::Truthy {
                    path: "component.enabled".to_string(),
                },
                Guard::Not {
                    path: "feature.enabled".to_string(),
                },
                Guard::Default {
                    path: "serviceAccount.name".to_string(),
                },
            ],
        ]
    );
}

#[test]
fn helper_summary_merges_fragment_output_uses() {
    let mut summary = HelperSummary::default();
    summary.add_fragment_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        HelperOutputMeta::default(),
    ));

    let outputs = summary
        .path_facts()
        .flat_map(|(_path, facts)| facts.fragment_output_uses.iter().cloned())
        .collect::<Vec<_>>();

    sim_assert_eq!(have: outputs.len(), want: 1);
}

#[test]
fn helper_summary_helper_projection_preserves_structured_output_metadata() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
            "enabled".to_string(),
        )])]),
        defaulted: true,
        provenance: Vec::new(),
    };
    let mut summary = HelperSummary::default();
    summary.add_fragment_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        meta.clone(),
    ));

    sim_assert_eq!(
        have: summary.project_helper_value(),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "app".to_string(),
            AbstractValue::OutputSet(BTreeMap::from([("podLabels".to_string(), meta)])),
        )])))
    );
}

#[test]
fn helper_summary_fragment_projection_preserves_structured_output_path() {
    let mut summary = HelperSummary::default();
    summary.add_fragment_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        HelperOutputMeta::default(),
    ));

    sim_assert_eq!(
        have: summary.project_fragment_value(),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "app".to_string(),
            output_paths(["podLabels".to_string()]),
        )])))
    );
}

#[test]
fn helper_summary_fragment_projection_merges_scalar_outputs_into_one_output_set() {
    let mut summary = HelperSummary::default();
    summary.add_output_meta("image.repository".to_string(), HelperOutputMeta::default());
    summary.add_output_meta("image.tag".to_string(), HelperOutputMeta::default());

    sim_assert_eq!(
        have: summary.project_fragment_value(),
        want: Some(output_paths([
            "image.repository".to_string(),
            "image.tag".to_string(),
        ]))
    );
}

#[test]
fn suppresses_bound_root_when_helper_outputs_descendant_path() {
    let mut analysis = HelperSummary::default();
    analysis.add_output_meta(
        "serviceAccount.name".to_string(),
        HelperOutputMeta::default(),
    );
    let bindings = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("serviceAccount".to_string()),
    )]);

    analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

    sim_assert_eq!(
        have: analysis.suppress_roots,
        want: BTreeSet::from(["serviceAccount".to_string()])
    );
}

#[test]
fn does_not_suppress_bound_root_for_exact_root_output() {
    let mut analysis = HelperSummary::default();
    analysis.add_output_meta("serviceAccount".to_string(), HelperOutputMeta::default());
    let bindings = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("serviceAccount".to_string()),
    )]);

    analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

    assert!(analysis.suppress_roots.is_empty());
}
