use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::HelperOutputMeta;

mod condition_predicate;
mod path_resolution;

pub(crate) use path_resolution::computed_with_body_fragment_value_expr;

pub(crate) struct ValuePathContext<'a> {
    pub(crate) root_bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) template_bindings: &'a HashMap<String, AbstractValue>,
    pub(crate) template_default_paths: &'a HashMap<String, BTreeSet<String>>,
    pub(crate) template_output_meta: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) fragment_context: FragmentEvalContext<'a>,
    pub(crate) current_dot_fragment: Option<AbstractValue>,
    pub(crate) current_dot_binding: Option<AbstractValue>,
}
