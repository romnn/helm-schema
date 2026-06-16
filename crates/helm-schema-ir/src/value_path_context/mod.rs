use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_binding::HelperBinding;
use crate::helper_summary::HelperOutputMeta;

mod alias_projection;
mod condition_predicate;
mod path_resolution;

pub(crate) use path_resolution::computed_with_body_fragment_binding;

pub(crate) struct ValuePathContext<'a> {
    pub(crate) root_bindings: &'a HashMap<String, HelperBinding>,
    pub(crate) template_bindings: &'a HashMap<String, FragmentBinding>,
    pub(crate) template_default_paths: &'a HashMap<String, BTreeSet<String>>,
    pub(crate) template_output_meta: &'a HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) fragment_context: FragmentEvalContext<'a>,
    pub(crate) current_dot_fragment: Option<FragmentBinding>,
    pub(crate) current_dot_binding: Option<HelperBinding>,
}
