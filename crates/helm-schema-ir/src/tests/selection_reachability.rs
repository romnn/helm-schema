use std::collections::BTreeSet;

use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{
    Effects, EvalResult, SelectionPolarity, SelectionReachability, SelectionTruthReachability,
    SelectionTruthSource,
};
use crate::expr_call_eval::default_primary_selection;
use crate::scalar_value::{ScalarValue, ScalarValueDispatch, TruthCondition};

#[test]
fn truth_and_dispatch_adapters_preserve_exactness_and_truth_source() {
    let raw_truth = Predicate::truthy_path("alpha");
    let exact = SelectionReachability::from((
        &TruthCondition::exact(raw_truth.clone()),
        SelectionPolarity::Truthy,
        SelectionTruthSource::RawInput,
    ));
    sim_assert_eq!(
        have: exact,
        want: SelectionReachability::exact(raw_truth.clone(), SelectionTruthSource::RawInput)
    );

    let falsy = SelectionReachability::from((
        &TruthCondition::exact(raw_truth.clone()),
        SelectionPolarity::Falsy,
        SelectionTruthSource::RawInput,
    ));
    sim_assert_eq!(
        have: falsy,
        want: SelectionReachability::exact(
            raw_truth.negated(),
            SelectionTruthSource::RawInput,
        )
    );

    let rendered = SelectionReachability::from((
        &ScalarValueDispatch::identity("alpha"),
        SelectionPolarity::Truthy,
    ));
    sim_assert_eq!(
        have: rendered.truth_source(),
        want: SelectionTruthSource::RenderedScalar
    );
}

#[test]
fn condition_reachability_distinguishes_raw_identities_from_rendered_scalars() {
    let raw = EvalResult::from_value(AbstractValue::ValuesPath("alpha".to_string()));
    sim_assert_eq!(
        have: raw.selection_reachability.as_ref(),
        want: Some(&SelectionReachability::exact(
            Predicate::truthy_path("alpha"),
            SelectionTruthSource::RawInput,
        ))
    );
    sim_assert_eq!(
        have: raw
            .output_reachability(SelectionPolarity::Truthy)
            .truth_source(),
        want: SelectionTruthSource::RawInput
    );

    let dispatch = ScalarValueDispatch {
        arms: vec![(
            Predicate::True,
            ScalarValue::PrintfStringIdentity("alpha".to_string()),
        )],
        complete: true,
    };
    let rendered =
        EvalResult::from_value(AbstractValue::Unknown).with_scalar_dispatch(dispatch.clone());
    sim_assert_eq!(
        have: rendered.selection_reachability.as_ref(),
        want: Some(&SelectionReachability::from((&dispatch, SelectionPolarity::Truthy)))
    );
    sim_assert_eq!(
        have: rendered
            .output_reachability(SelectionPolarity::Truthy)
            .truth_source(),
        want: SelectionTruthSource::RenderedScalar
    );
}

#[test]
fn partial_truth_becomes_approximate_without_inverting_its_sound_subset() {
    let subset = Predicate::truthy_path("alpha");
    let partial = TruthCondition::from_subsets(subset.clone(), Predicate::False, false);
    let selected = SelectionReachability::from((
        &partial,
        SelectionPolarity::Truthy,
        SelectionTruthSource::RenderedScalar,
    ));
    sim_assert_eq!(
        have: selected,
        want: SelectionReachability::approximate(
            Some(subset),
            SelectionTruthSource::RenderedScalar,
        )
    );

    let unproven_complement = SelectionReachability::from((
        &partial,
        SelectionPolarity::Falsy,
        SelectionTruthSource::RenderedScalar,
    ));
    sim_assert_eq!(
        have: unproven_complement,
        want: SelectionReachability::approximate(
            None,
            SelectionTruthSource::RenderedScalar,
        )
    );
}

#[test]
fn condition_polarities_preserve_independent_partial_proofs() {
    let when_true = Predicate::truthy_path("enabled");
    let when_false = Predicate::truthy_path("disabled");
    let partial = TruthCondition::from_subsets(when_true.clone(), when_false.clone(), false);
    let reachability =
        SelectionTruthReachability::from_condition(&partial, SelectionTruthSource::RawInput);

    sim_assert_eq!(
        have: reachability.when_true(),
        want: &SelectionReachability::approximate(
            Some(when_true),
            SelectionTruthSource::RawInput,
        )
    );
    sim_assert_eq!(
        have: reachability.when_true().complement(),
        want: SelectionReachability::approximate(None, SelectionTruthSource::RawInput)
    );
    sim_assert_eq!(have: reachability.truth_condition(), want: partial);
}

#[test]
fn approximate_reachability_lowers_distinct_output_and_execution_roles() {
    let subset = Predicate::truthy_path("alpha");
    let paths = BTreeSet::from(["alpha".to_string()]);
    let reachability = SelectionReachability::approximate(
        Some(subset.clone()),
        SelectionTruthSource::RenderedScalar,
    );

    sim_assert_eq!(
        have: reachability.output_selection_predicate("selected arm", paths.clone()),
        want: Predicate::approximate_output_selection(
            "selected arm",
            paths.clone(),
            subset.clone(),
        )
    );
    sim_assert_eq!(
        have: reachability.execution_predicate("executed arm", paths.clone()),
        want: Predicate::approximate_with_sound_predicate("executed arm", paths, subset)
    );
    sim_assert_eq!(
        have: reachability.has_proven_selection_condition(),
        want: false
    );
    sim_assert_eq!(
        have: SelectionReachability::exact(
            Predicate::truthy_path("alpha"),
            SelectionTruthSource::RawInput,
        )
        .has_proven_selection_condition(),
        want: true
    );
}

#[test]
fn exactness_demotes_approximation_and_canonicalizes_true_subsets() {
    let subset = Predicate::truthy_path("alpha");
    let approximate = Predicate::approximate_with_sound_predicate(
        "computed output selection",
        BTreeSet::from(["alpha".to_string()]),
        subset.clone(),
    );
    sim_assert_eq!(
        have: SelectionReachability::exact(
            approximate,
            SelectionTruthSource::RenderedScalar,
        ),
        want: SelectionReachability::approximate(
            Some(subset),
            SelectionTruthSource::RenderedScalar,
        )
    );
    sim_assert_eq!(
        have: SelectionReachability::approximate(
            Some(Predicate::True),
            SelectionTruthSource::RawInput,
        ),
        want: SelectionReachability::always(SelectionTruthSource::RawInput)
    );
}

#[test]
fn complement_preserves_only_invertible_selection_knowledge() {
    let source = SelectionTruthSource::RawInput;
    let predicate = Predicate::truthy_path("primary").negated();
    sim_assert_eq!(
        have: SelectionReachability::always(source).complement(),
        want: SelectionReachability::never(source)
    );
    sim_assert_eq!(
        have: SelectionReachability::never(source).complement(),
        want: SelectionReachability::always(source)
    );
    sim_assert_eq!(
        have: SelectionReachability::exact(predicate.clone(), source).complement(),
        want: SelectionReachability::exact(predicate.negated(), source)
    );
    sim_assert_eq!(
        have: SelectionReachability::approximate(
            Some(Predicate::truthy_path("primary")),
            source,
        )
        .complement(),
        want: SelectionReachability::approximate(None, source)
    );
}

#[test]
fn default_selection_adapter_exposes_all_states_with_owned_truth_sources() {
    let always = EvalResult::from_value(AbstractValue::StringSet(BTreeSet::from([String::new()])));
    sim_assert_eq!(
        have: default_primary_selection(&always),
        want: SelectionReachability::always(SelectionTruthSource::RawInput)
    );

    let never =
        EvalResult::from_value(AbstractValue::StringSet(BTreeSet::from(
            ["set".to_string()],
        )));
    sim_assert_eq!(
        have: default_primary_selection(&never),
        want: SelectionReachability::never(SelectionTruthSource::RawInput)
    );

    let raw = EvalResult::from_value(AbstractValue::ValuesPath("alpha".to_string()));
    sim_assert_eq!(
        have: default_primary_selection(&raw),
        want: SelectionReachability::exact(
            Predicate::truthy_path("alpha").negated(),
            SelectionTruthSource::RawInput,
        )
    );

    let opaque = EvalResult::from_value(AbstractValue::Unknown);
    sim_assert_eq!(
        have: default_primary_selection(&opaque),
        want: SelectionReachability::approximate(None, SelectionTruthSource::RawInput)
    );

    let dispatch = ScalarValueDispatch {
        arms: vec![(
            Predicate::True,
            ScalarValue::PrintfStringIdentity("alpha".to_string()),
        )],
        complete: true,
    };
    let rendered = EvalResult::from_value(AbstractValue::ValuesPath("alpha".to_string()))
        .with_scalar_dispatch(dispatch.clone());
    sim_assert_eq!(
        have: default_primary_selection(&rendered),
        want: SelectionReachability::from((&dispatch, SelectionPolarity::Falsy))
    );

    let chain = EvalResult::from_value(AbstractValue::FirstTruthy(vec![
        AbstractValue::ValuesPath("alpha".to_string()),
        AbstractValue::JsonDecodedPath("beta".to_string()),
    ]));
    sim_assert_eq!(
        have: default_primary_selection(&chain).truth_source(),
        want: SelectionTruthSource::RawInput
    );
}

#[test]
fn dead_output_selection_retains_eager_effects() {
    let mut effects = Effects::default();
    effects.add_default_paths(BTreeSet::from(["fallback".to_string()]));
    let mut result = EvalResult::with_effects(None, effects);
    sim_assert_eq!(have: result.selection_reachability, want: None);
    sim_assert_eq!(
        have: result.output_reachability(SelectionPolarity::Truthy),
        want: SelectionReachability::approximate(None, SelectionTruthSource::RawInput)
    );
    result.selection_reachability = Some(SelectionReachability::never(
        SelectionTruthSource::RenderedScalar,
    ));

    sim_assert_eq!(
        have: result.effects.defaults,
        want: BTreeSet::from(["fallback".to_string()])
    );
    sim_assert_eq!(
        have: result.selection_reachability,
        want: Some(SelectionReachability::never(
            SelectionTruthSource::RenderedScalar,
        ))
    );
}
