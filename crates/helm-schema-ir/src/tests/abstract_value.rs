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
fn top_absorbs_join() {
    sim_assert_eq!(
        have: join(vec![path("image.tag"), AbstractValue::Top]),
        want: AbstractValue::Top
    );
}

#[test]
fn compatibility_unknown_widens_joins_to_top() {
    sim_assert_eq!(
        have: join(vec![path("image.tag"), AbstractValue::Unknown]),
        want: AbstractValue::Top
    );
}

#[test]
fn top_inside_choice_absorbs_join() {
    let nested = AbstractValue::Choice(BTreeSet::from([AbstractValue::Top, path("name")]));

    sim_assert_eq!(have: join(vec![path("image.tag"), nested]), want: AbstractValue::Top);
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
fn shallow_paths_do_not_descend_structured_maps() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "metadata".to_string(),
        AbstractValue::ValuesPath("podLabels".to_string()),
    )]));

    sim_assert_eq!(have: value.shallow_paths(), want: BTreeSet::new());
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
fn helper_and_fragment_range_items_keep_distinct_structural_policy() {
    let value = AbstractValue::Dict(BTreeMap::from([(
        "name".to_string(),
        AbstractValue::ValuesPath("containers.name".to_string()),
    )]));

    sim_assert_eq!(have: value.fragment_range_item(), want: None);
    sim_assert_eq!(
        have: value.helper_range_item(),
        want: Some(AbstractValue::ValuesPath("containers.name".to_string()))
    );
}

#[test]
fn output_meta_preserves_values_paths_and_output_set_metadata() {
    let meta = HelperOutputMeta {
        predicates: BTreeSet::new(),
        defaulted: true,
        provenance: Vec::new(),
    };
    let value = AbstractValue::Overlay {
        entries: BTreeMap::from([("name".to_string(), path("serviceAccount.name"))]),
        fallback: Box::new(AbstractValue::OutputSet(BTreeMap::from([(
            "global.nameOverride".to_string(),
            meta.clone(),
        )]))),
    };

    sim_assert_eq!(
        have: value.output_meta(),
        want: BTreeMap::from([
            (
                "serviceAccount.name".to_string(),
                HelperOutputMeta::default()
            ),
            ("global.nameOverride".to_string(), meta),
        ])
    );
}
