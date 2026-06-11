use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_helper_call_analysis::analyze_bound_helper_calls_with_fragment_locals;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::BoundHelperAnalysis;

pub(crate) struct HelperSummaryCache {
    bound_helper_calls: RefCell<BTreeMap<BoundHelperCallsCacheKey, BoundHelperAnalysis>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct BoundHelperCallsCacheKey {
    text: String,
    current_dot: Option<HelperBinding>,
    root_bindings: BTreeMap<String, HelperBinding>,
    fragment_locals: BTreeMap<String, FragmentBinding>,
}

impl HelperSummaryCache {
    pub(crate) fn new() -> Self {
        Self {
            bound_helper_calls: RefCell::new(BTreeMap::new()),
        }
    }

    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    pub(crate) fn analyze_bound_calls(
        &self,
        text: &str,
        root_bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
    ) -> BoundHelperAnalysis {
        let root_bindings_key: BTreeMap<String, HelperBinding> = root_bindings
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, FragmentBinding> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallsCacheKey {
            text: text.to_string(),
            current_dot: current_dot.clone(),
            root_bindings: root_bindings_key,
            fragment_locals: fragment_locals_key,
        };

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            return cached.clone();
        }

        let mut seen = HashSet::new();
        let analysis = analyze_bound_helper_calls_with_fragment_locals(
            text,
            if root_bindings.is_empty() {
                None
            } else {
                Some(root_bindings)
            },
            current_dot.as_ref(),
            fragment_locals,
            context,
            &mut seen,
        );
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, analysis.clone());
        analysis
    }
}
