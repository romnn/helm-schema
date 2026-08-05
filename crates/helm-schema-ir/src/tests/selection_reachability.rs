use std::collections::BTreeSet;

use helm_schema_core::Predicate;
use test_util::prelude::sim_assert_eq;

use crate::eval_effect::{
    Effects, EvalResult, SelectionPolarity, SelectionReachability, SelectionState,
    SelectionTruthSource,
};
use crate::expr_call_eval::DefaultPrimarySelection;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};

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
        want: SelectionReachability {
            state: SelectionState::Exact(raw_truth.clone()),
            truth_source: SelectionTruthSource::RawInput,
        }
    );

    let falsy = SelectionReachability::from((
        &TruthCondition::exact(raw_truth.clone()),
        SelectionPolarity::Falsy,
        SelectionTruthSource::RawInput,
    ));
    sim_assert_eq!(
        have: falsy,
        want: SelectionReachability {
            state: SelectionState::Exact(raw_truth.negated()),
            truth_source: SelectionTruthSource::RawInput,
        }
    );

    let rendered = SelectionReachability::from((
        &ScalarValueDispatch::identity("alpha"),
        SelectionPolarity::Truthy,
    ));
    sim_assert_eq!(
        have: rendered.truth_source,
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
        want: SelectionReachability {
            state: SelectionState::Approximate {
                sound_subset: Some(subset),
            },
            truth_source: SelectionTruthSource::RenderedScalar,
        }
    );

    let unproven_complement = SelectionReachability::from((
        &partial,
        SelectionPolarity::Falsy,
        SelectionTruthSource::RenderedScalar,
    ));
    sim_assert_eq!(
        have: unproven_complement.state,
        want: SelectionState::Approximate { sound_subset: None }
    );
}

#[test]
fn default_selection_adapter_exposes_all_four_reachability_states() {
    let source = SelectionTruthSource::RawInput;
    sim_assert_eq!(
        have: SelectionReachability::from((&DefaultPrimarySelection::AlwaysFallback, source)),
        want: SelectionReachability::always(source)
    );
    sim_assert_eq!(
        have: SelectionReachability::from((&DefaultPrimarySelection::NeverFallback, source)),
        want: SelectionReachability::never(source)
    );

    let predicate = Predicate::truthy_path("primary").negated();
    let conditional = DefaultPrimarySelection::Conditional(BTreeSet::from([predicate.clone()]));
    sim_assert_eq!(
        have: SelectionReachability::from((&conditional, source)),
        want: SelectionReachability::exact(predicate, source)
    );
    sim_assert_eq!(
        have: SelectionReachability::from((&DefaultPrimarySelection::Opaque, source)),
        want: SelectionReachability::approximate(None, source)
    );
}

#[test]
fn dead_output_selection_retains_eager_effects() {
    let mut effects = Effects::default();
    effects.add_default_paths(BTreeSet::from(["fallback".to_string()]));
    let mut result = EvalResult::with_effects(None, effects);
    result.selection_reachability =
        SelectionReachability::never(SelectionTruthSource::RenderedScalar);

    sim_assert_eq!(
        have: result.effects.defaults,
        want: BTreeSet::from(["fallback".to_string()])
    );
    sim_assert_eq!(have: result.selection_reachability.state, want: SelectionState::Never);
}
