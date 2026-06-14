use std::collections::{BTreeSet, HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::ValueKind;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::fragment_expr_eval::{
    FragmentEvalContext, bindings_for_helper_arg_with_fragment_locals,
    fragment_binding_from_outer_expr, helper_binding_from_expr_with_fragment_locals,
};
use crate::helper_analysis::BoundHelperAnalysis;
use crate::helper_fragment_output_uses::{
    FragmentOutputWalkState, collect_bound_fragment_output_uses_from_tree,
};
use crate::helper_value_analysis::{HelperValuesWalkState, collect_bound_helper_values_from_tree};

pub(crate) struct BoundHelperCallResolution {
    pub(crate) bindings: HashMap<String, HelperBinding>,
    pub(crate) helper_body_dot: Option<HelperBinding>,
    pub(crate) helper_fragment_dot: Option<FragmentBinding>,
}

pub(crate) struct ResolveBoundHelperCallParams<'a, 'context> {
    pub(crate) arg: Option<&'a TemplateExpr>,
    pub(crate) outer_bindings: Option<&'a HashMap<String, HelperBinding>>,
    pub(crate) current_dot: Option<&'a HelperBinding>,
    pub(crate) fragment_locals: &'a HashMap<String, FragmentBinding>,
    pub(crate) context: FragmentEvalContext<'context>,
    pub(crate) seen: &'a HashSet<String>,
}

pub(crate) fn resolve_bound_helper_call(
    params: ResolveBoundHelperCallParams<'_, '_>,
) -> BoundHelperCallResolution {
    let mut binding_seen = params.seen.clone();
    let bindings = bindings_for_helper_arg_with_fragment_locals(
        params.arg,
        params.outer_bindings,
        params.current_dot,
        params.fragment_locals,
        params.context,
        &mut binding_seen,
    );

    let mut dot_seen = params.seen.clone();
    let helper_body_dot = params
        .arg
        .and_then(|expr| {
            helper_binding_from_expr_with_fragment_locals(
                expr,
                params.fragment_locals,
                params.outer_bindings,
                params.current_dot,
                params.context,
                &mut dot_seen,
            )
        })
        .or_else(|| params.current_dot.cloned());

    let helper_fragment_dot = params.arg.and_then(|expr| {
        fragment_binding_from_outer_expr(
            expr,
            Some(params.fragment_locals),
            params.outer_bindings,
            params.current_dot,
        )
    });

    BoundHelperCallResolution {
        bindings,
        helper_body_dot,
        helper_fragment_dot,
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn interpret_bound_helper_body(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> BoundHelperAnalysis {
    let mut analysis = BoundHelperAnalysis::default();
    collect_value_facts(name, resolution, context, seen, &mut analysis);

    let mut helper_fragment_locals = HashMap::new();
    collect_fragment_output_uses(
        name,
        resolution,
        context,
        seen,
        &mut helper_fragment_locals,
        &mut analysis,
    );

    analysis
}

fn collect_value_facts(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    analysis: &mut BoundHelperAnalysis,
) {
    let (Some(src), Some(tree)) = (
        context.define_bodies.structured_source(name),
        context.define_bodies.structured_tree(name),
    ) else {
        return;
    };

    let mut local_bindings = HashMap::new();
    let mut local_default_paths = HashMap::new();
    let mut local_output_meta = HashMap::new();
    let mut helper_values_state = HelperValuesWalkState {
        local_bindings: &mut local_bindings,
        local_default_paths: &mut local_default_paths,
        local_output_meta: &mut local_output_meta,
        context,
        seen,
        analysis,
    };
    collect_bound_helper_values_from_tree(
        tree.root_node(),
        src,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        &mut helper_values_state,
    );
}

fn collect_fragment_output_uses(
    name: &str,
    resolution: &BoundHelperCallResolution,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
    helper_fragment_locals: &mut HashMap<String, FragmentBinding>,
    analysis: &mut BoundHelperAnalysis,
) {
    let (Some(src), Some(tree)) = (
        context.define_bodies.structured_source(name),
        context.define_bodies.structured_tree(name),
    ) else {
        return;
    };

    let mut fragment_output_uses = Vec::new();
    let mut local_default_paths = HashMap::new();
    let mut fragment_output_state = FragmentOutputWalkState {
        local_bindings: helper_fragment_locals,
        local_default_paths: &mut local_default_paths,
        context,
        seen,
        outputs: &mut fragment_output_uses,
    };
    collect_bound_fragment_output_uses_from_tree(
        &tree,
        src,
        &resolution.bindings,
        resolution.helper_body_dot.as_ref(),
        resolution.helper_fragment_dot.as_ref(),
        &mut fragment_output_state,
    );
    for source in analysis.output.keys() {
        analysis.fragment_output.remove(source);
    }
    let structured_sources: BTreeSet<String> = fragment_output_uses
        .iter()
        .filter(|output| output.kind == ValueKind::Fragment || !output.relative_path.0.is_empty())
        .map(|output| output.source_expr.clone())
        .collect();
    for source in &structured_sources {
        analysis.output.remove(source);
        analysis.fragment_output.remove(source);
    }
    analysis.fragment_output_uses.extend(fragment_output_uses);
}
