mod branch_join;
mod local_join;
mod local_state;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_core::Predicate;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::GetBinding;
use crate::eval_env::LocalBinding;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::ScalarValueDispatch;
use crate::symbolic_local_state::IntCastSource;

pub(crate) type LocalSlots = HashMap<String, LocalSlot>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LocalSlot {
    pub(crate) range_domain: Option<Vec<String>>,
    pub(crate) get_binding: Option<GetBinding>,
    pub(crate) binding: Option<LocalBinding>,
    pub(crate) default_paths: Option<BTreeSet<helm_schema_core::ValuesPath>>,
    pub(crate) output_meta: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Runtime scalar values retained independently of fragment provenance.
    /// Branch joins guard each alternative by the branch that assigned it,
    /// so later helper conditions consume the same value semantics Helm does.
    pub(crate) scalar_dispatch: Option<ScalarValueDispatch>,
    /// Sufficient conditions under which a monotone local accumulator is
    /// nonempty. An explicit [`Predicate::False`] is the empty seed; a
    /// missing entry means the local's truthiness is not structurally known.
    pub(crate) truthy_reduction: Option<Predicate>,
    /// Locals whose branch predicate exceeded the bounded stamping budget.
    /// Their truthiness must abstain instead of falling back to the local's
    /// unguarded value paths.
    pub(crate) truthiness_abstained: bool,
    /// Locals that received a reassignment which does not imply truthiness
    /// (an explicit falsy write, or a condition-valued write). Such a write
    /// makes the accumulator last-write-wins instead of monotone, so a
    /// range exit must drop the local's reduction rather than read it
    /// existentially. Straight-line `if`/`else` joins stay exact and keep
    /// consuming the reduction. A declaration seeds a fresh accumulator and
    /// resets the mark.
    pub(crate) truthiness_cleared: bool,
    /// Locals bound to a type descriptor. Each described path retains the
    /// predicates under which that path supplied the selected value.
    pub(crate) typeof_source: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Locals bound to a total integer cast of one values path
    /// (`$replicas := int (default 1 .Values.controller.replicas)`):
    /// comparisons on the local may strengthen through the raw-integer
    /// sound subsets exactly as the inline cast expression would.
    pub(crate) int_cast_source: Option<IntCastSource>,
    /// Range variables bound to the MEMBER identity of a directly ranged
    /// path (`$v` in `range $k, $v := .Values.x` holds each `x.*` value).
    /// Conditions and assignments resolve through these; hole rendering
    /// does not, so member reads do not manufacture placed rows.
    pub(crate) range_member_value: Option<AbstractValue>,
    /// For a range variable bound over a local-dict OVERLAY (`$services :=
    /// .Values.service.additionalServices` + `set $services "default"
    /// (omit …)`): one literal entry the range DEFINITELY iterates on
    /// every render. Unfaithful member conditions re-decode under this
    /// binding as a sound subset (positive-polarity consumers only).
    pub(crate) definite_range_member_value: Option<AbstractValue>,
}

impl LocalSlot {
    pub(crate) fn has_current_value(&self) -> bool {
        let Self {
            range_domain,
            get_binding,
            binding,
            default_paths,
            output_meta,
            scalar_dispatch,
            truthy_reduction,
            truthiness_abstained,
            truthiness_cleared: _,
            typeof_source,
            int_cast_source,
            range_member_value,
            definite_range_member_value: _,
        } = self;
        range_domain.is_some()
            || get_binding.is_some()
            || binding.is_some()
            || default_paths.is_some()
            || output_meta.is_some()
            || scalar_dispatch.is_some()
            || truthy_reduction.is_some()
            || *truthiness_abstained
            || typeof_source.is_some()
            || int_cast_source.is_some()
            || range_member_value.is_some()
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
    pub(crate) value: Option<AbstractValue>,
    pub(crate) truth: Option<Predicate>,
    pub(crate) dispatch: Option<ScalarValueDispatch>,
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

pub(crate) fn root_values(
    slots: &std::collections::HashMap<String, RootSlot>,
) -> std::collections::HashMap<String, AbstractValue> {
    slots
        .iter()
        .filter_map(|(name, slot)| slot.value.clone().map(|value| (name.clone(), value)))
        .collect()
}

pub(crate) fn root_slots(
    values: std::collections::HashMap<String, AbstractValue>,
) -> std::collections::HashMap<String, RootSlot> {
    values
        .into_iter()
        .map(|(name, value)| {
            (
                name,
                RootSlot {
                    value: Some(value),
                    truth: None,
                    dispatch: None,
                },
            )
        })
        .collect()
}
