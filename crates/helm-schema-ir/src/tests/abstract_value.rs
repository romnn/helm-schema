use super::*;
use test_util::prelude::sim_assert_eq;

fn path(value: &str) -> AbstractValue {
    AbstractValue::ValuesPath(value.to_string())
}

fn string(value: &str) -> AbstractValue {
    AbstractValue::StringSet(BTreeSet::from([value.to_string()]))
}

fn paths(values: &[&str]) -> BTreeSet<String> {
    values
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

fn join(values: Vec<AbstractValue>) -> AbstractValue {
    AbstractValue::join_all(values).expect("join should produce a value")
}

#[test]
fn join_is_idempotent() {
    let value = path("image.tag");

    sim_assert_eq!(have: join(vec![value.clone(), value.clone()]), want: value);
}

#[test]
fn join_is_commutative() {
    let left = path("image.repository");
    let right = string("nginx");

    sim_assert_eq!(
        have: join(vec![left.clone(), right.clone()]),
        want: join(vec![right, left])
    );
}

#[test]
fn join_is_associative() {
    let left = path("image.repository");
    let middle = string("nginx");
    let right = path("image.tag");

    let left_grouped = join(vec![
        join(vec![left.clone(), middle.clone()]),
        right.clone(),
    ]);
    let right_grouped = join(vec![left, join(vec![middle, right])]);

    sim_assert_eq!(have: left_grouped, want: right_grouped);
}

#[test]
fn top_widens_join_but_keeps_alternatives() {
    sim_assert_eq!(
        have: join(vec![path("image.tag"), AbstractValue::Top]),
        want: AbstractValue::Choice(BTreeSet::from([AbstractValue::Top, path("image.tag")]))
    );
}

#[test]
fn unknown_widens_join_but_keeps_alternatives() {
    sim_assert_eq!(
        have: join(vec![path("image.tag"), AbstractValue::Unknown]),
        want: AbstractValue::Choice(BTreeSet::from([AbstractValue::Top, path("image.tag")]))
    );
}

#[test]
fn top_inside_choice_stays_one_width_marker() {
    let nested = AbstractValue::Choice(BTreeSet::from([AbstractValue::Top, path("name")]));

    sim_assert_eq!(
        have: join(vec![path("image.tag"), nested]),
        want: AbstractValue::Choice(BTreeSet::from([
            AbstractValue::Top,
            path("image.tag"),
            path("name"),
        ]))
    );
}

#[test]
fn top_propagates_through_descent() {
    sim_assert_eq!(
        have: AbstractValue::Top.apply_to_path(&["nested".to_string()]),
        want: Some(AbstractValue::Top)
    );
}

#[test]
fn guard_metadata_preserves_raw_identity_through_member_selection() {
    let mut metadata = HelperOutputMeta {
        input_identity: true,
        ..HelperOutputMeta::default()
    };
    metadata
        .predicates
        .insert(BTreeSet::from([Predicate::truthy_path(
            "workers.celery.enableDefault",
        )]));
    let value = path("workers.celery.sets.*").with_output_meta(&BTreeMap::from([(
        "workers.celery.sets.*".to_string(),
        metadata,
    )]));
    let selected = value.apply_to_path(&["securityContexts".to_string(), "pod".to_string()]);
    let mut expected_meta = HelperOutputMeta {
        input_identity: true,
        ..HelperOutputMeta::default()
    };
    expected_meta
        .predicates
        .insert(BTreeSet::from([Predicate::truthy_path(
            "workers.celery.enableDefault",
        )]));
    let expected = AbstractValue::OutputPath(
        "workers.celery.sets.*.securityContexts.pod".to_string(),
        expected_meta,
    );

    sim_assert_eq!(have: selected.as_ref(), want: Some(&expected));
    sim_assert_eq!(
        have: crate::value_path_context::value_has_key(&expected, "runAsUser"),
        want: Some(
            Predicate::from(helm_schema_core::Guard::Absent {
                path: "workers.celery.sets.*.securityContexts.pod.runAsUser".to_string(),
            })
            .negated()
        )
    );
}

#[test]
fn transformed_metadata_cannot_promote_a_guarded_path_to_input_identity() {
    let metadata = HelperOutputMeta {
        input_identity: true,
        derived_text: true,
        ..HelperOutputMeta::default()
    };
    let value = AbstractValue::OutputPath("source".to_string(), metadata.clone());

    sim_assert_eq!(
        have: value.apply_to_path(&["member".to_string()]),
        want: Some(AbstractValue::OutputPath("source".to_string(), metadata))
    );
}

#[test]
fn omit_metadata_is_consumed_by_member_selection() {
    let metadata = HelperOutputMeta {
        input_identity: true,
        omitted_keys: BTreeMap::from([("secret".to_string(), Vec::new())]),
        ..HelperOutputMeta::default()
    };
    let value = AbstractValue::OutputPath("service".to_string(), metadata.clone());
    let mut selected_metadata = metadata.clone();
    selected_metadata.omitted_keys.clear();

    sim_assert_eq!(
        have: value.apply_to_path(&["enabled".to_string()]),
        want: Some(AbstractValue::OutputPath(
            "service.enabled".to_string(),
            selected_metadata
        ))
    );
    sim_assert_eq!(
        have: value.apply_to_path(&["secret".to_string()]),
        want: Some(AbstractValue::OutputPath("service".to_string(), metadata))
    );
}

#[test]
fn omit_keys_removes_known_map_entries_but_preserves_values_root() {
    let value = AbstractValue::Overlay {
        entries: BTreeMap::from([
            ("enabled".to_string(), path("probe.enabled")),
            ("timeoutSeconds".to_string(), path("probe.timeoutSeconds")),
        ]),
        fallback: Box::new(path("probe")),
    };

    sim_assert_eq!(
        have: value.omit_keys(&BTreeSet::from(["enabled".to_string()])),
        want: AbstractValue::Overlay {
            entries: BTreeMap::from([(
                "timeoutSeconds".to_string(),
                path("probe.timeoutSeconds")
            )]),
            fallback: Box::new(path("probe")),
        }
    );
}

#[test]
fn paths_descend_structured_maps() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "metadata".to_string(),
        AbstractValue::ValuesPath("podLabels".to_string()),
    )]));

    sim_assert_eq!(have: value.paths(), want: paths(&["podLabels"]));
}

#[test]
fn values_root_abstains_from_fragment_path_extraction() {
    let value = AbstractValue::values_root();

    sim_assert_eq!(have: value.fragment_source_paths(), want: BTreeSet::new());
    sim_assert_eq!(have: value.fragment_rendered_paths(), want: BTreeSet::new());
}

#[test]
fn fragment_paths_stay_shallow_while_rendered_paths_descend_structures() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "metadata".to_string(),
        AbstractValue::ValuesPath("podLabels".to_string()),
    )]));

    sim_assert_eq!(have: value.fragment_source_paths(), want: BTreeSet::new());
    sim_assert_eq!(
        have: value.fragment_rendered_paths(),
        want: BTreeSet::from(["podLabels".to_string()])
    );
}

#[test]
fn fragment_range_item_does_not_iterate_map_values() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "name".to_string(),
        AbstractValue::ValuesPath("containers.name".to_string()),
    )]));

    sim_assert_eq!(have: value.fragment_range_item(), want: None);
}

#[test]
fn widened_carries_paths_for_attribution_but_is_no_fragment_source() {
    let value = AbstractValue::Widened(paths(&["auth.existingSecret"]));

    sim_assert_eq!(have: value.paths(), want: paths(&["auth.existingSecret"]));
    sim_assert_eq!(have: value.fragment_source_paths(), want: BTreeSet::new());
    sim_assert_eq!(have: value.fragment_rendered_paths(), want: BTreeSet::new());
    sim_assert_eq!(have: value.apply_to_path(&["data".to_string()]), want: None);
}

#[test]
fn without_widened_drops_widened_alternatives() {
    let widened = AbstractValue::Widened(paths(&["name"]));

    sim_assert_eq!(have: widened.clone().without_widened(), want: None);
    sim_assert_eq!(
        have: join(vec![path("image.tag"), widened]).without_widened(),
        want: Some(path("image.tag"))
    );
}
