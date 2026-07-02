use super::*;
use test_util::prelude::sim_assert_eq;

fn path(value: &str) -> AbstractValue {
    AbstractValue::ValuesPath(value.to_string())
}

fn string(value: &str) -> AbstractValue {
    AbstractValue::StringSet(BTreeSet::from([value.to_string()]))
}

fn paths(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_string()).collect()
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

#[test]
fn widened_projects_rows_at_the_expression_slot() {
    let value = AbstractValue::Widened(paths(&["auth.existingSecret"]));
    let slot = YamlPath(vec!["data".to_string(), "password".to_string()]);
    let mut outputs = Vec::new();
    value.collect_output_uses_with_encoding(
        &mut outputs,
        &slot,
        ValueKind::Scalar,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        true,
    );

    let rows: Vec<(String, YamlPath, ValueKind)> = outputs
        .into_iter()
        .map(|output| (output.source_expr, output.relative_path, output.kind))
        .collect();
    sim_assert_eq!(
        have: rows,
        want: vec![("auth.existingSecret".to_string(), slot, ValueKind::Scalar)]
    );
}

#[test]
fn widened_range_item_scalar_projects_to_sequence_item_path() {
    let value = AbstractValue::Widened(paths(&["hosts.*"]));
    let slot = YamlPath(vec!["tls".to_string()]);
    let mut outputs = Vec::new();
    value.collect_output_uses_with_encoding(
        &mut outputs,
        &slot,
        ValueKind::Scalar,
        &BTreeSet::new(),
        &BTreeSet::new(),
        &BTreeSet::new(),
        true,
    );

    let rows: Vec<(String, YamlPath)> = outputs
        .into_iter()
        .map(|output| (output.source_expr, output.relative_path))
        .collect();
    sim_assert_eq!(
        have: rows,
        want: vec![("hosts.*".to_string(), output_path::sequence_item_path(&slot))]
    );
}
