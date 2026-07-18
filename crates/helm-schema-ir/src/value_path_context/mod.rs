use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::GetBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::HelperOutputMeta;
use crate::symbolic_local_state::IntCastSource;
use helm_schema_core::Predicate;

mod condition_predicate;
mod path_resolution;

pub(crate) struct ValuePathContext<'a> {
    pub(crate) root_bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) root_truthy_predicates: &'a HashMap<String, Predicate>,
    /// Fragment-value locals merged with condition-visible range member
    /// bindings (the render lane resolves fragment values only).
    pub(crate) template_bindings: HashMap<String, AbstractValue>,
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
