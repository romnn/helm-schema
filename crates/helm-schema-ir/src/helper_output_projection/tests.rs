use std::collections::{BTreeMap, BTreeSet};

use crate::abstract_value::AbstractValue;
use crate::helper_summary::HelperFragmentOutputUse;
use crate::helper_summary::HelperOutputMeta;
use crate::predicate::Predicate;
use crate::{ValueKind, YamlPath};

#[test]
fn abstract_value_output_meta_preserves_output_set_metadata() {
    let value = AbstractValue::Overlay {
        entries: BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("serviceAccount.name".to_string()),
        )]),
        fallback: Box::new(AbstractValue::OutputSet(BTreeMap::from([(
            "global.nameOverride".to_string(),
            HelperOutputMeta {
                predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
                    "global.enabled".to_string(),
                )])]),
                defaulted: true,
                provenance: Vec::new(),
            },
        )]))),
    };

    let meta = value.output_meta();

    assert!(meta.contains_key("serviceAccount.name"));
    assert_eq!(
        meta.get("global.nameOverride"),
        Some(&HelperOutputMeta {
            predicates: BTreeSet::from([BTreeSet::from([Predicate::truthy_path(
                "global.enabled".to_string(),
            )])]),
            defaulted: true,
            provenance: Vec::new(),
        })
    );
}

#[test]
fn direct_and_fragment_values_share_structural_output_projection() {
    let direct_value = AbstractValue::List(vec![
        AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("containers.name".to_string()),
        )])),
        AbstractValue::PathSet(BTreeSet::from(["containers.image".to_string()])),
    ]);
    let fragment_value = AbstractValue::List(vec![
        AbstractValue::Dict(BTreeMap::from([(
            "name".to_string(),
            AbstractValue::ValuesPath("containers.name".to_string()),
        )])),
        AbstractValue::PathSet(BTreeSet::from(["containers.image".to_string()])),
    ]);
    let relative_path = YamlPath(vec!["spec".to_string(), "containers".to_string()]);
    let predicates = BTreeSet::from([Predicate::truthy_path("containers.enabled".to_string())]);
    let defaulted_paths = BTreeSet::from(["containers.image".to_string()]);

    let mut helper_outputs = Vec::new();
    direct_value.collect_output_uses(
        &mut helper_outputs,
        &relative_path,
        ValueKind::Fragment,
        &predicates,
        &defaulted_paths,
    );

    let mut fragment_outputs = Vec::new();
    fragment_value.collect_fragment_output_uses(
        &mut fragment_outputs,
        &relative_path,
        ValueKind::Fragment,
        &predicates,
        &defaulted_paths,
    );

    assert_eq!(
        output_rows(&helper_outputs),
        vec![
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
    assert_eq!(output_rows(&fragment_outputs), output_rows(&helper_outputs));
}

#[test]
fn nested_fragment_values_root_still_abstains_from_output_projection() {
    let fragment_value = AbstractValue::Dict(BTreeMap::from([(
        "values".to_string(),
        AbstractValue::values_root(),
    )]));
    let mut outputs = Vec::new();

    fragment_value.collect_fragment_output_uses(
        &mut outputs,
        &YamlPath(Vec::new()),
        ValueKind::Fragment,
        &BTreeSet::new(),
        &BTreeSet::new(),
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
