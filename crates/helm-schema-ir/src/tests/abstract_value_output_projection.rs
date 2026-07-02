use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::{AbstractValue, OutputProjectionScope};
use crate::helper_summary::{HelperFragmentOutputUse, HelperOutputMeta};
use crate::{ValueKind, YamlPath};
use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

#[test]
fn structural_values_project_rows_at_slot_relative_paths() {
    let value = AbstractValue::List(vec![
        AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("containers.name".to_string()),
        )])),
        AbstractValue::ValuesPath("containers.image".to_string()),
    ]);
    let relative_path = YamlPath(vec!["spec".to_string(), "containers".to_string()]);
    let predicates = BTreeSet::from([Predicate::truthy_path("containers.enabled".to_string())]);
    let defaulted_paths = BTreeSet::from(["containers.image".to_string()]);
    let no_paths = BTreeSet::new();
    let no_meta = BTreeMap::new();

    let mut outputs = Vec::new();
    value.collect_output_uses(
        &mut outputs,
        &relative_path,
        ValueKind::Fragment,
        &OutputProjectionScope {
            root: &relative_path,
            encoded_paths: &no_paths,
            active_output_predicates: &predicates,
            defaulted_paths: &defaulted_paths,
            path_meta: &no_meta,
            local_rendered_paths: &no_paths,
            local_defaulted_paths: &no_paths,
        },
    );

    sim_assert_eq!(
        have: output_rows(&outputs),
        want: vec![
            (
                "containers.name".to_string(),
                vec![
                    "spec".to_string(),
                    "containers[*]".to_string(),
                    "name".to_string()
                ],
                ValueKind::Scalar,
                false,
            ),
            (
                "containers.image".to_string(),
                vec!["spec".to_string(), "containers[*]".to_string()],
                ValueKind::Scalar,
                true,
            ),
        ]
    );
}

#[test]
fn path_meta_scope_enriches_matching_source_rows() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "tag".to_string(),
        AbstractValue::ValuesPath("image.tag".to_string()),
    )]));
    let tag_meta = HelperOutputMeta {
        defaulted: true,
        predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
            "image.enabled".to_string(),
        )])]),
        ..HelperOutputMeta::default()
    };
    let path_meta = BTreeMap::from([("image.tag".to_string(), tag_meta)]);
    let no_paths = BTreeSet::new();
    let no_predicates = BTreeSet::new();

    let mut outputs = Vec::new();
    value.collect_output_uses(
        &mut outputs,
        &YamlPath(Vec::new()),
        ValueKind::Fragment,
        &OutputProjectionScope {
            root: &YamlPath(Vec::new()),
            encoded_paths: &no_paths,
            active_output_predicates: &no_predicates,
            defaulted_paths: &no_paths,
            path_meta: &path_meta,
            local_rendered_paths: &no_paths,
            local_defaulted_paths: &no_paths,
        },
    );

    sim_assert_eq!(
        have: output_rows(&outputs),
        want: vec![(
            "image.tag".to_string(),
            vec!["tag".to_string()],
            ValueKind::Scalar,
            true,
        )]
    );
    sim_assert_eq!(
        have: outputs[0].meta.predicates,
        want: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
            "image.enabled".to_string(),
        )])])
    );
}

#[test]
fn nested_fragment_values_root_still_abstains_from_output_projection() {
    let fragment_value = AbstractValue::Dict(BTreeMap::from([(
        "values".to_string(),
        AbstractValue::values_root(),
    )]));
    let no_paths = BTreeSet::new();
    let no_predicates = BTreeSet::new();
    let no_meta = BTreeMap::new();
    let mut outputs = Vec::new();

    fragment_value.collect_output_uses(
        &mut outputs,
        &YamlPath(Vec::new()),
        ValueKind::Fragment,
        &OutputProjectionScope {
            root: &YamlPath(Vec::new()),
            encoded_paths: &no_paths,
            active_output_predicates: &no_predicates,
            defaulted_paths: &no_paths,
            path_meta: &no_meta,
            local_rendered_paths: &no_paths,
            local_defaulted_paths: &no_paths,
        },
    );

    assert!(outputs.is_empty());
}

fn output_rows(outputs: &[HelperFragmentOutputUse]) -> Vec<(String, Vec<String>, ValueKind, bool)> {
    outputs
        .iter()
        .map(|output| {
            (
                output.source_expr.clone(),
                output.relative_path.0.clone(),
                output.kind,
                output.meta.defaulted,
            )
        })
        .collect()
}
