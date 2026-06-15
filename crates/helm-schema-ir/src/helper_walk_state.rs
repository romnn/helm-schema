use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::{BoundHelperAnalysis, HelperFragmentOutputUse, HelperOutputMeta};

pub(crate) struct HelperValuesWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) local_output_meta: &'state mut HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) analysis: &'state mut BoundHelperAnalysis,
}

pub(crate) struct FragmentOutputWalkState<'context, 'state> {
    pub(crate) local_bindings: &'state mut HashMap<String, FragmentBinding>,
    pub(crate) local_default_paths: &'state mut HashMap<String, BTreeSet<String>>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'state mut HashSet<String>,
    pub(crate) outputs: &'state mut Vec<HelperFragmentOutputUse>,
}
