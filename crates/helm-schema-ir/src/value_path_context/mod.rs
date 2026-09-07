use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::SelectionReachability;
use crate::eval_env::EvalEnv;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::HelperOutputMeta;
use crate::symbolic_local_state::IntCastSource;
use helm_schema_core::{Predicate, ValuesPath};

mod condition_predicate;
mod path_resolution;

pub(crate) use condition_predicate::{
    guard_value_is_truthy, predicate_any, stringified_equality_preimage, value_has_key,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeSubjectIdentity {
    pub(crate) path: ValuesPath,
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
    pub(crate) influence_paths: BTreeSet<ValuesPath>,
    pub(crate) value: Option<AbstractValue>,
    pub(crate) truth_reachability: SelectionReachability,
    pub(crate) input_identity: Option<RangeSubjectIdentity>,
    pub(crate) member_identity: Option<RangeSubjectIdentity>,
    pub(crate) member_value: Option<AbstractValue>,
    pub(crate) output_meta: BTreeMap<ValuesPath, HelperOutputMeta>,
    pub(crate) value_alternatives: Option<RangeValueAlternatives>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeValueAlternative {
    pub(crate) condition: Predicate,
    pub(crate) value: AbstractValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RangeValueAlternatives {
    pub(crate) known: Vec<RangeValueAlternative>,
    pub(crate) has_unresolved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RootDotIdentity {
    Unresolved,
    ExplicitRoot,
    Other,
}

pub(crate) struct ValuePathContext<'a> {
    pub(crate) helper_dispatch_depth: Cell<u8>,
    pub(crate) eval_env: EvalEnv,
    pub(crate) template_truthiness_abstentions: &'a BTreeSet<String>,
    pub(crate) typeof_bindings: &'a HashMap<String, BTreeMap<ValuesPath, HelperOutputMeta>>,
    pub(crate) int_cast_bindings: &'a HashMap<String, IntCastSource>,
    pub(crate) fragment_context: FragmentEvalContext<'a>,
    pub(crate) current_dot_fragment: Option<AbstractValue>,
    pub(crate) root_dot_identity: RootDotIdentity,
}
