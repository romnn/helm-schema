use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::expression_analysis::resolved_default_fallback_paths_for_text;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_binding_projection::fragment_strings;
use crate::fragment_expr_eval::{
    FragmentEvalContext, fragment_binding_from_expr,
    fragment_binding_from_text_with_helper_context, helper_binding_from_expr_with_fragment_locals,
};
use crate::helper_binding::HelperBinding;
use crate::helper_binding_projection::{helper_strings, helper_to_fragment_binding};
use crate::helper_output_projection::helper_binding_output_meta;
use crate::helper_summary::{HelperOutputMeta, HelperSummary};
use crate::local_projection::local_default_paths_from_text;
use crate::template_expr_cache::parse_expr_text;

pub(crate) struct BoundHelperEnv<'bindings, 'context> {
    bindings: &'bindings HashMap<String, HelperBinding>,
    current_dot: Option<&'bindings HelperBinding>,
    context: FragmentEvalContext<'context>,
}

impl<'bindings, 'context> BoundHelperEnv<'bindings, 'context> {
    pub(crate) fn new(
        bindings: &'bindings HashMap<String, HelperBinding>,
        current_dot: Option<&'bindings HelperBinding>,
        context: FragmentEvalContext<'context>,
    ) -> Self {
        Self {
            bindings,
            current_dot,
            context,
        }
    }

    pub(crate) fn current_dot_fragment(&self) -> Option<FragmentBinding> {
        self.current_dot.map(helper_to_fragment_binding)
    }

    pub(crate) fn external_default_fallback_paths(&self, text: &str) -> BTreeSet<String> {
        resolved_default_fallback_paths_for_text(text, Some(self.bindings), self.current_dot)
    }

    pub(crate) fn local_default_fallback_paths(
        &self,
        text: &str,
        local_default_paths: &HashMap<String, BTreeSet<String>>,
    ) -> BTreeSet<String> {
        local_default_paths_from_text(text, local_default_paths)
    }

    pub(crate) fn summarize_calls(
        &self,
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> HelperSummary {
        self.context
            .helper_summaries()
            .summarize_bound_helper_calls(
                text,
                Some(self.bindings),
                self.current_dot,
                local_bindings,
                self.context,
                seen,
            )
    }

    pub(crate) fn helper_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        local_bindings: &HashMap<String, FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> Option<HelperBinding> {
        helper_binding_from_expr_with_fragment_locals(
            expr,
            local_bindings,
            Some(self.bindings),
            self.current_dot,
            self.context,
            seen,
        )
    }

    pub(crate) fn fragment_binding_from_text(
        &self,
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        fragment_binding_from_text_with_helper_context(
            text,
            local_bindings,
            Some(self.bindings),
            self.current_dot,
            self.context,
            seen,
        )
    }

    pub(crate) fn helper_binding_output_meta_from_text(
        &self,
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        seen_seed: &HashSet<String>,
    ) -> BTreeMap<String, HelperOutputMeta> {
        let mut out: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut seen = seen_seed.clone();
        for expr in parse_expr_text(text) {
            if let Some(binding) = self.helper_binding_from_expr(&expr, local_bindings, &mut seen) {
                for (path, meta) in helper_binding_output_meta(&binding) {
                    out.entry(path).or_default().merge(meta);
                }
            }
        }
        out
    }

    pub(crate) fn string_outputs_from_text(
        &self,
        text: &str,
        local_bindings: &HashMap<String, FragmentBinding>,
        seen_seed: &HashSet<String>,
    ) -> BTreeSet<String> {
        let mut strings = BTreeSet::new();
        let mut seen = seen_seed.clone();
        let current_dot_fragment = self.current_dot_fragment();
        for expr in parse_expr_text(text) {
            if let Some(binding) = self.helper_binding_from_expr(&expr, local_bindings, &mut seen) {
                strings.extend(helper_strings(&binding));
                continue;
            }
            if let Some(binding) = fragment_binding_from_expr(
                &expr,
                local_bindings,
                current_dot_fragment.as_ref(),
                self.context,
                &mut seen,
            ) {
                strings.extend(fragment_strings(&binding));
            }
        }
        strings
    }
}
