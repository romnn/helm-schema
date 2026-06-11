use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::YamlPath;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::fragment_binding_eval::fragment_binding_from_outer_expr;
use crate::fragment_expr_eval::{
    FragmentEvalContext, bindings_for_helper_arg_with_fragment_locals,
    helper_binding_from_expr_with_fragment_locals,
};
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_fragment_output_uses::{
    FragmentOutputWalkState, collect_bound_fragment_output_uses_from_items,
};
use crate::helper_fragment_outputs::collect_bound_fragment_outputs_from_tree;
use crate::helper_value_analysis::{HelperValuesWalkState, collect_bound_helper_values_from_ast};
use crate::template_expr_analysis::walk_expr_excluding_helper_call_args;
use crate::template_expr_cache::parse_expr_text;

#[tracing::instrument(skip_all, fields(bytes = text.len()))]
pub(crate) fn analyze_bound_helper_calls_with_fragment_locals(
    text: &str,
    bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BoundHelperAnalysis {
    let mut analysis = BoundHelperAnalysis::default();
    for expr in parse_expr_text(text) {
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
            let nested = analyze_bound_helper_call_with_fragment_locals(
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

#[tracing::instrument(skip_all)]
pub(crate) fn analyze_bound_helper_call_with_fragment_locals(
    name: &str,
    arg: Option<&TemplateExpr>,
    outer_bindings: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BoundHelperAnalysis {
    if !seen.insert(name.to_string()) {
        return BoundHelperAnalysis::default();
    }

    let mut binding_seen = seen.clone();
    let bindings = bindings_for_helper_arg_with_fragment_locals(
        arg,
        outer_bindings,
        current_dot,
        fragment_locals,
        context,
        &mut binding_seen,
    );
    // Inside the helper body, `.` is what the caller passed as the helper
    // argument. `None` is valid when that argument cannot be statically pinned.
    let helper_body_dot = {
        let mut dot_seen = seen.clone();
        arg.and_then(|expr| {
            helper_binding_from_expr_with_fragment_locals(
                expr,
                fragment_locals,
                outer_bindings,
                current_dot,
                context,
                &mut dot_seen,
            )
        })
        .or_else(|| current_dot.cloned())
    };
    let mut analysis = BoundHelperAnalysis::default();
    if let Some(body) = context.defines.get(name) {
        let active_output_guards = BTreeSet::new();
        let mut local_bindings = HashMap::new();
        let mut local_default_paths = HashMap::new();
        let mut local_output_meta = HashMap::new();
        let mut helper_values_state = HelperValuesWalkState {
            local_bindings: &mut local_bindings,
            local_default_paths: &mut local_default_paths,
            local_output_meta: &mut local_output_meta,
            context,
            analyze_bound_helper_calls: analyze_bound_helper_calls_with_fragment_locals,
            seen,
            analysis: &mut analysis,
        };
        for node in body {
            collect_bound_helper_values_from_ast(
                node,
                &bindings,
                helper_body_dot.as_ref(),
                &active_output_guards,
                &mut helper_values_state,
            );
        }
    }

    let mut helper_fragment_locals = HashMap::new();
    let helper_dot = arg.and_then(|expr| {
        fragment_binding_from_outer_expr(expr, Some(fragment_locals), outer_bindings, current_dot)
    });
    if let Some(src) = context.define_bodies.source(name)
        && let Some(tree) = context.define_bodies.tree(name)
    {
        collect_bound_fragment_outputs_from_tree(
            tree.root_node(),
            src,
            &mut helper_fragment_locals,
            helper_dot.as_ref(),
            context,
            seen,
            &mut analysis.fragment_output,
        );
    }
    if let Some(body) = context.defines.get(name) {
        let mut fragment_output_uses = Vec::new();
        let mut local_bindings = helper_fragment_locals;
        let mut local_default_paths = HashMap::new();
        let active_output_guards = BTreeSet::new();
        let mut fragment_output_state = FragmentOutputWalkState {
            local_bindings: &mut local_bindings,
            local_default_paths: &mut local_default_paths,
            context,
            analyze_bound_helper_calls: analyze_bound_helper_calls_with_fragment_locals,
            seen,
            outputs: &mut fragment_output_uses,
        };
        collect_bound_fragment_output_uses_from_items(
            body,
            &bindings,
            helper_body_dot.as_ref(),
            helper_dot.as_ref(),
            &YamlPath(Vec::new()),
            &active_output_guards,
            &mut fragment_output_state,
        );
        for source in analysis.output.keys() {
            analysis.fragment_output.remove(source);
        }
        let structured_sources: BTreeSet<String> = fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
        for source in &structured_sources {
            analysis.output.remove(source);
            analysis.fragment_output.remove(source);
        }
        analysis.fragment_output_uses.extend(fragment_output_uses);
    }

    for binding in bindings.values() {
        let HelperBinding::ValuesPath(root) = binding else {
            continue;
        };
        let prefix = format!("{root}.");
        if analysis
            .output
            .keys()
            .chain(analysis.guard_paths.iter())
            .any(|path| path.starts_with(&prefix))
        {
            analysis.suppress_roots.insert(root.clone());
        }
    }

    seen.remove(name);
    analysis
}
