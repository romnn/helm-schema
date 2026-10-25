mod branch_join;
pub(crate) mod kinds;
mod local_join;
mod local_state;
pub(crate) mod scalar_value;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_core::Predicate;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::GetBinding;
use crate::eval_env::LocalBinding;
use crate::helper_meta::HelperOutputMeta;
use crate::symbolic_local_state::IntCastSource;
use scalar_value::ScalarContent;

pub(crate) type LocalSlots = HashMap<String, LocalSlot>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalSlot {
    range_domain: Option<Vec<String>>,
    get_binding: Option<GetBinding>,
    binding: Option<LocalBinding>,
    default_paths: Option<BTreeSet<helm_schema_core::ValuesPath>>,
    output_meta: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Runtime scalar values retained independently of fragment provenance.
    /// Branch joins guard each alternative by the branch that assigned it,
    /// so later helper conditions consume the same value semantics Helm does.
    scalar_dispatch: Option<ScalarValueDispatch>,
    /// Sufficient conditions under which a monotone local accumulator is
    /// nonempty. An explicit [`Predicate::False`] is the empty seed; a
    /// missing entry means the local's truthiness is not structurally known.
    truthy_reduction: Option<Predicate>,
    /// Locals whose branch predicate exceeded the bounded stamping budget.
    /// Their truthiness must abstain instead of falling back to the local's
    /// unguarded value paths.
    truthiness_abstained: bool,
    /// Locals that received a reassignment which does not imply truthiness
    /// (an explicit falsy write, or a condition-valued write). Such a write
    /// makes the accumulator last-write-wins instead of monotone, so a
    /// range exit must drop the local's reduction rather than read it
    /// existentially. Straight-line `if`/`else` joins stay exact and keep
    /// consuming the reduction. A declaration seeds a fresh accumulator and
    /// resets the mark.
    truthiness_cleared: bool,
    /// Locals bound to a type descriptor. Each described path retains the
    /// predicates under which that path supplied the selected value.
    typeof_source: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Locals bound to a total integer cast of one values path
    /// (`$replicas := int (default 1 .Values.controller.replicas)`):
    /// comparisons on the local may strengthen through the raw-integer
    /// sound subsets exactly as the inline cast expression would.
    int_cast_source: Option<IntCastSource>,
    /// Range variables bound to the MEMBER identity of a directly ranged
    /// path (`$v` in `range $k, $v := .Values.x` holds each `x.*` value).
    /// Conditions and assignments resolve through these; hole rendering
    /// does not, so member reads do not manufacture placed rows.
    range_member_value: Option<AbstractValue>,
    /// For a range variable bound over a local-dict OVERLAY (`$services :=
    /// .Values.service.additionalServices` + `set $services "default"
    /// (omit …)`): one literal entry the range DEFINITELY iterates on
    /// every render. Unfaithful member conditions re-decode under this
    /// binding as a sound subset (positive-polarity consumers only).
    definite_range_member_value: Option<AbstractValue>,
}

impl LocalSlot {
    pub(crate) fn has_current_value(&self) -> bool {
        !matches!(
            self,
            Self {
                range_domain: None,
                get_binding: None,
                binding: None,
                default_paths: None,
                output_meta: None,
                scalar_dispatch: _,
                truthy_reduction: None,
                truthiness_abstained: false,
                truthiness_cleared: _,
                typeof_source: None,
                int_cast_source: None,
                range_member_value: None,
                definite_range_member_value: _,
            }
        ) || self
            .scalar_dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.content().is_some())
    }

    pub(crate) const EMPTY: Self = Self {
        range_domain: None,
        get_binding: None,
        binding: None,
        default_paths: None,
        output_meta: None,
        scalar_dispatch: None,
        truthy_reduction: None,
        truthiness_abstained: false,
        truthiness_cleared: false,
        typeof_source: None,
        int_cast_source: None,
        range_member_value: None,
        definite_range_member_value: None,
    };
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RootSlot {
    value: Option<AbstractValue>,
    truth: Option<Predicate>,
    dispatch: Option<ScalarValueDispatch>,
}

impl RootSlot {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(
            self,
            Self {
                value: None,
                truth: None,
                dispatch: None
            }
        )
    }

    pub(crate) fn merge(&mut self, incoming: Self) {
        let Self {
            value,
            truth,
            dispatch,
        } = incoming;
        if let Some(value) = value {
            self.truth = None;
            self.dispatch = None;
            self.value = Some(value);
        }
        self.truth = truth.or(self.truth.take());
        self.dispatch = dispatch.or(self.dispatch.take());
    }
}

pub(crate) fn root_values(slots: &HashMap<String, RootSlot>) -> HashMap<String, AbstractValue> {
    slots
        .iter()
        .filter_map(|(name, slot)| {
            let RootSlot {
                value,
                truth: _,
                dispatch: _,
            } = slot;
            value.clone().map(|value| (name.clone(), value))
        })
        .collect()
}

pub(crate) fn root_slots(values: HashMap<String, AbstractValue>) -> HashMap<String, RootSlot> {
    values
        .into_iter()
        .map(|(name, value)| (name, RootSlot::new(Some(value), None, None)))
        .collect()
}

/// Known scalar alternatives for one evaluated value.
///
/// The arm conditions are mutually exclusive. [`Self::complete`] records
/// whether they also cover the whole input domain; helper output can retain
/// exact arms even when another branch remains unknown. Root-context fields
/// assigned across a complete `if`/`else` chain and evaluated helper output
/// both use this representation, so equality and truthiness consume the same
/// semantic fact.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValueDispatch {
    arms: Vec<(helm_schema_core::Predicate, ScalarValue)>,
    complete: bool,
}

/// One sealed leaf carries independent runtime type and optional legacy content.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValue {
    go_type: kinds::GoType,
    content: Option<ScalarContent>,
}

/// Borrowed legacy content; type-only alternatives never become a transport projection.
#[derive(Clone, Copy)]
pub(crate) struct ContentView<'a> {
    dispatch: &'a ScalarValueDispatch,
    pub(crate) complete: bool,
}

impl<'a> ContentView<'a> {
    pub(crate) fn arms(self) -> impl Iterator<Item = (&'a Predicate, &'a ScalarContent)> {
        self.dispatch.arms.iter().filter_map(|(condition, value)| {
            value.content.as_ref().map(|content| (condition, content))
        })
    }

    pub(crate) fn single(self) -> Option<(&'a Predicate, &'a ScalarContent)> {
        let mut arms = self.arms();
        let first = arms.next()?;
        arms.next().is_none().then_some(first)
    }
}

impl ScalarValueDispatch {
    pub(crate) fn content(&self) -> Option<ContentView<'_>> {
        if !self.arms.is_empty() && self.arms.iter().all(|(_, value)| value.content.is_none()) {
            return None;
        }
        Some(ContentView {
            dispatch: self,
            complete: self.complete && self.arms.iter().all(|(_, value)| value.content.is_some()),
        })
    }

    pub(crate) fn bind_content(arms: Vec<(Predicate, ScalarContent)>, complete: bool) -> Self {
        Self {
            arms: arms
                .into_iter()
                .map(|(condition, content)| (condition, ScalarValue::untyped(content)))
                .collect(),
            complete,
        }
    }
}

pub(crate) const MAX_SCALAR_DISPATCH_STATES: usize = 128;

#[derive(Clone, Copy)]
pub(crate) struct LocalSlotView<'a> {
    pub(crate) range_domain: &'a Option<Vec<String>>,
    pub(crate) get_binding: &'a Option<GetBinding>,
    pub(crate) binding: &'a Option<LocalBinding>,
    pub(crate) default_paths: &'a Option<BTreeSet<helm_schema_core::ValuesPath>>,
    pub(crate) output_meta: &'a Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    pub(crate) scalar_dispatch: &'a Option<ScalarValueDispatch>,
    pub(crate) truthy_reduction: &'a Option<Predicate>,
    pub(crate) truthiness_abstained: bool,
    pub(crate) typeof_source: &'a Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    pub(crate) int_cast_source: &'a Option<IntCastSource>,
    pub(crate) definite_range_member_value: &'a Option<AbstractValue>,
}

impl LocalSlot {
    pub(crate) fn binding(&self) -> LocalSlotView<'_> {
        LocalSlotView {
            range_domain: &self.range_domain,
            get_binding: &self.get_binding,
            binding: &self.binding,
            default_paths: &self.default_paths,
            output_meta: &self.output_meta,
            scalar_dispatch: &self.scalar_dispatch,
            truthy_reduction: &self.truthy_reduction,
            truthiness_abstained: self.truthiness_abstained,
            typeof_source: &self.typeof_source,
            int_cast_source: &self.int_cast_source,
            definite_range_member_value: &self.definite_range_member_value,
        }
    }
}

impl LocalSlot {
    pub(crate) fn merge_omitted_keys(
        &mut self,
        additions: &BTreeMap<String, Vec<helm_schema_core::Guard>>,
    ) {
        if let Some(metas) = self.output_meta.as_mut() {
            for meta in metas.values_mut() {
                meta.predicates.clear();
                meta.omitted_keys.extend(additions.clone());
            }
        }
    }
}

impl LocalSlot {
    pub(crate) fn replace_range_member(
        &mut self,
        value: Option<AbstractValue>,
    ) -> Option<AbstractValue> {
        std::mem::replace(&mut self.range_member_value, value)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct RootSlotView<'a> {
    pub(crate) value: &'a Option<AbstractValue>,
    pub(crate) truth: &'a Option<Predicate>,
    pub(crate) dispatch: &'a Option<ScalarValueDispatch>,
}

impl RootSlot {
    pub(crate) fn binding(&self) -> RootSlotView<'_> {
        RootSlotView {
            value: &self.value,
            truth: &self.truth,
            dispatch: &self.dispatch,
        }
    }

    pub(crate) fn new(
        value: Option<AbstractValue>,
        truth: Option<Predicate>,
        dispatch: Option<ScalarValueDispatch>,
    ) -> Self {
        Self {
            value,
            truth,
            dispatch,
        }
    }

    pub(crate) fn map_value(
        &mut self,
        transform: impl FnOnce(&AbstractValue) -> Option<AbstractValue>,
    ) {
        if let Some(value) = &self.value
            && let Some(replacement) = transform(value)
        {
            self.value = Some(replacement);
        }
    }

    pub(crate) fn seed_value(&mut self, value: &AbstractValue) {
        self.value.get_or_insert_with(|| value.clone());
    }

    pub(crate) fn clear_facts(&mut self) {
        self.truth = None;
        self.dispatch = None;
    }
}

pub(crate) struct AssignmentBinding {
    pub(crate) binding: Option<LocalBinding>,
    pub(crate) scalar_dispatch: Option<ScalarValueDispatch>,
    pub(crate) truthy_reduction: Option<Predicate>,
    pub(crate) helper_scope: bool,
    pub(crate) self_preserving: bool,
}

impl LocalSlot {
    pub(crate) fn bind_runtime(
        &mut self,
        binding: Option<LocalBinding>,
        dispatch: Option<ScalarValueDispatch>,
    ) {
        self.binding = binding;
        self.scalar_dispatch = dispatch;
    }

    /// Preserves reassignment poison across reductions and resets it for a fresh declaration.
    ///
    /// A falsy-capable write makes range accumulation last-write-wins;
    /// self-preserving nonempty accumulation remains monotone.
    pub(crate) fn bind_truth(
        &mut self,
        kind: crate::fragment_assignment::AssignmentKind,
        reduction: Option<Predicate>,
        self_preserving: bool,
    ) {
        self.truthy_reduction = reduction;
        match kind {
            crate::fragment_assignment::AssignmentKind::Declaration => {
                self.truthiness_abstained = false;
                self.truthiness_cleared = false;
            }
            crate::fragment_assignment::AssignmentKind::Assignment => {
                if self.truthiness_abstained {
                    self.truthy_reduction = None;
                }
                self.truthiness_cleared |=
                    !self_preserving && self.truthy_reduction.as_ref() != Some(&Predicate::True);
            }
        }
    }

    pub(crate) fn bind_metadata(
        &mut self,
        default_paths: Option<BTreeSet<helm_schema_core::ValuesPath>>,
        output_meta: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    ) {
        self.default_paths = default_paths;
        self.output_meta = output_meta;
    }

    /// Keeps present empty metadata distinct from absent facts at every-exit joins.
    pub(crate) fn record_assignment_metadata(
        &mut self,
        source: Option<IntCastSource>,
        metadata: crate::eval_env::BindingValueMetadata,
    ) {
        if let Some(source) = source {
            self.int_cast_source = Some(source);
        }
        let crate::eval_env::BindingValueMetadata {
            default_paths,
            output_meta,
        } = metadata;
        self.bind_metadata(Some(default_paths), Some(output_meta));
    }

    pub(crate) fn record_type_source(
        &mut self,
        sources: BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>,
    ) {
        self.typeof_source = Some(sources);
    }

    pub(crate) fn record_iteration_domain(&mut self, domain: Vec<String>) {
        self.range_domain = Some(domain);
    }
    pub(crate) fn record_definite_member(&mut self, member: AbstractValue) {
        self.definite_range_member_value = Some(member);
    }
}

impl RootSlot {
    /// Writes truth before value while retaining an earlier dispatch, as direct root writes do.
    pub(crate) fn overwrite(&mut self, value: AbstractValue, truth: Option<Predicate>) {
        if let Some(truth) = truth {
            self.truth = Some(truth);
        }
        self.value = Some(value);
    }
}

pub(crate) fn argument_slots(
    bindings: HashMap<String, AbstractValue>,
    dispatches: BTreeMap<String, ScalarValueDispatch>,
) -> BTreeMap<String, RootSlot> {
    let mut slots = BTreeMap::<String, RootSlot>::new();
    for (name, value) in bindings {
        slots.entry(name).or_default().value = Some(value);
    }
    for (name, dispatch) in dispatches {
        slots.entry(name).or_default().dispatch = Some(dispatch);
    }
    slots
}

#[cfg(test)]
#[path = "../tests/scalar_kinds.rs"]
mod tests;
