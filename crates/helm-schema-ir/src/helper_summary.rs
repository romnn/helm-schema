use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_helper_call_analysis::{
    analyze_bound_helper_call_with_fragment_locals, analyze_bound_helper_calls_with_fragment_locals,
};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_call_analyzer::HelperCallAnalyzer;

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

    pub(crate) fn analyze_bound_calls(
        &self,
        text: &str,
        root_bindings: &HashMap<String, HelperBinding>,
        current_dot: Option<HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
    ) -> BoundHelperAnalysis {
        let mut seen = HashSet::new();
        self.analyze_bound_helper_calls(
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
        )
    }
}

impl HelperCallAnalyzer for HelperSummaryCache {
    #[tracing::instrument(skip_all, fields(bytes = text.len()))]
    fn analyze_bound_helper_calls(
        &self,
        text: &str,
        bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        if !seen.is_empty() {
            return analyze_bound_helper_calls_with_fragment_locals(
                text,
                bindings,
                current_dot,
                fragment_locals,
                context,
                seen,
            );
        }

        let root_bindings_key: BTreeMap<String, HelperBinding> = bindings
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let fragment_locals_key: BTreeMap<String, FragmentBinding> = fragment_locals
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let key = BoundHelperCallsCacheKey {
            text: text.to_string(),
            current_dot: current_dot.cloned(),
            root_bindings: root_bindings_key,
            fragment_locals: fragment_locals_key,
        };

        if let Some(cached) = self.bound_helper_calls.borrow().get(&key) {
            return cached.clone();
        }

        let analysis = analyze_bound_helper_calls_with_fragment_locals(
            text,
            bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        );
        self.bound_helper_calls
            .borrow_mut()
            .insert(key, analysis.clone());
        analysis
    }

    #[tracing::instrument(skip_all)]
    fn analyze_bound_helper_call(
        &self,
        name: &str,
        arg: Option<&helm_schema_ast::TemplateExpr>,
        outer_bindings: Option<&HashMap<String, HelperBinding>>,
        current_dot: Option<&HelperBinding>,
        fragment_locals: &HashMap<String, FragmentBinding>,
        context: FragmentEvalContext<'_>,
        seen: &mut HashSet<String>,
    ) -> BoundHelperAnalysis {
        analyze_bound_helper_call_with_fragment_locals(
            name,
            arg,
            outer_bindings,
            current_dot,
            fragment_locals,
            context,
            seen,
        )
    }
}
