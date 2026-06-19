use std::collections::{HashMap, HashSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_binding::HelperBinding;
use crate::helper_body_analysis::{
    ResolveBoundHelperCallParams, interpret_bound_helper_body, resolve_bound_helper_call,
};
use crate::helper_summary::HelperSummary;
use crate::helper_summary_mutation::mark_suppressed_roots_for_bound_outputs;
use crate::template_expr_analysis::walk_expr_excluding_helper_call_args;

pub(crate) fn analyze_bound_helper_calls_with_fragment_locals_in_exprs(
    exprs: &[TemplateExpr],
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    let mut analysis = HelperSummary::default();
    for expr in exprs {
        walk_expr_excluding_helper_call_args(&expr, &mut |node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if !matches!(function.as_str(), "include" | "template") {
                return;
            };
            let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                return;
            };
            let nested = context.helper_summaries().summarize_bound_helper_call(
                name,
                args.get(1),
                bindings,
                current_dot,
                fragment_locals,
                context,
                seen,
            );
            analysis.extend(nested);
        });
    }
    analysis
}

#[tracing::instrument(skip_all, fields(helper = name))]
pub(crate) fn analyze_bound_helper_call_with_fragment_locals(
    name: &str,
    arg: Option<&TemplateExpr>,
    outer_bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HelperSummary {
    if !seen.insert(name.to_string()) {
        return HelperSummary::default();
    }

    let resolution = resolve_bound_helper_call(ResolveBoundHelperCallParams {
        helper_name: name,
        arg,
        outer_bindings,
        current_dot,
        fragment_locals,
        context,
        seen,
    });
    let mut analysis = interpret_bound_helper_body(name, &resolution, context, seen);
    mark_suppressed_roots_for_bound_outputs(&mut analysis, &resolution.bindings);

    seen.remove(name);
    analysis
}
