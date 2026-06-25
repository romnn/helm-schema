use std::collections::{BTreeMap, BTreeSet, HashMap};
use test_util::prelude::sim_assert_eq;

use super::{HelperFragmentOutputUse, HelperOutputMeta, HelperSummary};
use crate::abstract_value::AbstractValue;
use crate::predicate::Predicate;
use crate::{Guard, ValueKind, YamlPath};

fn output_paths(paths: impl IntoIterator<Item = String>) -> AbstractValue {
    AbstractValue::choice(
        paths
            .into_iter()
            .map(|path| AbstractValue::OutputPath(path, HelperOutputMeta::default()))
            .collect(),
    )
    .expect("paths should project")
}

#[test]
fn helper_output_meta_projects_predicates_to_contract_guard_sets() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::from([BTreeSet::from([Predicate::Not(Box::new(
            Predicate::truthy_path("feature.enabled"),
        ))])]),
        defaulted: true,
        provenance: Vec::new(),
        ..HelperOutputMeta::default()
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
        ..HelperOutputMeta::default()
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
fn helper_output_meta_suppresses_dynamic_traversal_parent_guards_only_for_selected_child() {
    let mut meta = HelperOutputMeta {
        predicates: BTreeSet::from([BTreeSet::from([
            Predicate::truthy_path("auth"),
            Predicate::truthy_path("auth.replicationPassword"),
        ])]),
        ..HelperOutputMeta::default()
    };
    meta.suppress_predicate_path("auth");

    sim_assert_eq!(
        have: meta.contract_guard_sets("auth.replicationPassword"),
        want: vec![vec![Guard::Truthy {
            path: "auth.replicationPassword".to_string(),
        }]]
    );
    sim_assert_eq!(
        have: meta.contract_guard_sets("auth"),
        want: vec![vec![
            Guard::Truthy {
                path: "auth".to_string(),
            },
            Guard::Truthy {
                path: "auth.replicationPassword".to_string(),
            },
        ]]
    );
}

#[test]
fn helper_summary_merges_fragment_output_uses() {
    let mut summary = HelperSummary::default();
    summary.add_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        HelperOutputMeta::default(),
    ));

    let outputs = &summary.output_uses;

    sim_assert_eq!(have: outputs.len(), want: 1);
}

#[test]
fn helper_summary_projection_preserves_structured_output_path_and_keeps_metadata_on_use() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
            "enabled".to_string(),
        )])]),
        defaulted: true,
        provenance: Vec::new(),
        ..HelperOutputMeta::default()
    };
    let mut summary = HelperSummary::default();
    summary.add_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        meta.clone(),
    ));

    sim_assert_eq!(
        have: summary.project_value(),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "app".to_string(),
            AbstractValue::OutputPath("podLabels".to_string(), meta.clone()),
        )])))
    );
    sim_assert_eq!(have: summary.output_uses[0].meta.clone(), want: meta);
}

#[test]
fn helper_summary_projection_preserves_structured_output_path() {
    let mut summary = HelperSummary::default();
    summary.add_output_use(HelperFragmentOutputUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["app".to_string()]),
        ValueKind::Fragment,
        HelperOutputMeta::default(),
    ));

    sim_assert_eq!(
        have: summary.project_value(),
        want: Some(AbstractValue::Dict(BTreeMap::from([(
            "app".to_string(),
            AbstractValue::OutputPath("podLabels".to_string(), HelperOutputMeta::default()),
        )])))
    );
}

#[test]
fn helper_summary_projection_merges_scalar_outputs_into_one_choice() {
    let mut summary = HelperSummary::default();
    summary.merge_output_meta("image.repository".to_string(), HelperOutputMeta::default());
    summary.merge_output_meta("image.tag".to_string(), HelperOutputMeta::default());

    sim_assert_eq!(
        have: summary.project_value(),
        want: Some(output_paths([
            "image.repository".to_string(),
            "image.tag".to_string(),
        ]))
    );
}

#[test]
fn suppresses_bound_root_when_helper_outputs_descendant_path() {
    let mut analysis = HelperSummary::default();
    analysis.merge_output_meta(
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
    analysis.merge_output_meta("serviceAccount".to_string(), HelperOutputMeta::default());
    let bindings = HashMap::from([(
        "config".to_string(),
        AbstractValue::ValuesPath("serviceAccount".to_string()),
    )]);

    analysis.mark_suppressed_roots_for_bound_outputs(&bindings);

    assert!(analysis.suppress_roots.is_empty());
}
