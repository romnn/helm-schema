use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::GetBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use crate::symbolic_local_state::IntCastSource;
use helm_schema_core::Predicate;

mod condition_predicate;
mod path_resolution;

pub(crate) use condition_predicate::{
    guard_value_is_truthy, predicate_any, stringified_equality_preimage, value_has_key,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeSubjectIdentity {
    pub(crate) path: String,
    pub(crate) json_decoded: bool,
}

/// One structural interpretation of a range header, shared by document
/// ranges and inline ranges embedded in scalars.
///
/// `influence_paths` attribute evaluation effects. `input_identity` says
/// the iterable itself is one values path, while `member_identity` says
/// values-backed members of a derived iterable still come from one path.
/// Keeping those facts separate prevents a transformation such as
/// `splitList` from turning its string input into a collection contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeSubject {
    pub(crate) influence_paths: BTreeSet<String>,
    pub(crate) value: Option<AbstractValue>,
    pub(crate) truth: TruthCondition,
    pub(crate) input_identity: Option<RangeSubjectIdentity>,
    pub(crate) member_identity: Option<RangeSubjectIdentity>,
    pub(crate) member_value: Option<AbstractValue>,
}

pub(crate) struct ValuePathContext<'a> {
    pub(crate) root_bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) root_truthy_predicates: &'a HashMap<String, Predicate>,
    /// Joined value alternatives for root-context fields set across
    /// complete if/else chains; root-field equalities decode through them.
    pub(crate) root_value_dispatches: &'a HashMap<String, ScalarValueDispatch>,
    pub(crate) root_field_semantics_on_current_dot: bool,
    /// Fragment-value locals merged with condition-visible range member
    /// bindings (the render lane resolves fragment values only).
    pub(crate) template_bindings: HashMap<String, AbstractValue>,
    /// Exact scalar values carried by locals after branch-dependent
    /// assignments and transformations.
    pub(crate) template_scalar_dispatches: &'a HashMap<String, ScalarValueDispatch>,
    /// Which of `template_bindings` came from a `:=`/`=` pipeline rather than
    /// from `range`; only those are nil-safe to navigate.
    pub(crate) pipeline_bound_bindings: std::collections::HashSet<String>,
    pub(crate) range_domains: &'a HashMap<String, Vec<String>>,
    pub(crate) get_bindings: &'a HashMap<String, GetBinding>,
    pub(crate) template_default_paths: &'a HashMap<String, BTreeSet<String>>,
    pub(crate) template_output_meta: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) template_truthy_reductions: &'a HashMap<String, Predicate>,
    pub(crate) typeof_bindings: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) int_cast_bindings: &'a HashMap<String, IntCastSource>,
    pub(crate) fragment_context: FragmentEvalContext<'a>,
    pub(crate) current_dot_fragment: Option<AbstractValue>,
    pub(crate) current_dot_binding: Option<AbstractValue>,
}
