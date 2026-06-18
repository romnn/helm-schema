use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::bound_value_analysis::{GetBindingPlan, parse_get_binding_from_exprs};
use crate::fragment_assignment::{AssignmentKind, parse_helper_assignment_from_exprs};
use crate::fragment_binding::FragmentBinding;
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_expr};
use crate::helper_binding::HelperBinding;
use crate::helper_binding_projection::helper_to_fragment_binding;
use crate::template_expr_cache::ParsedTemplateSnippet;

pub(crate) struct AssignmentActionPlan {
    pub(crate) get_binding: Option<GetBindingPlan>,
    pub(crate) local_assignment: Option<LocalAssignmentPlan>,
}

pub(crate) struct LocalAssignmentPlan {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) fragment_binding: Option<FragmentBinding>,
    pub(crate) rhs_expr: TemplateExpr,
}

pub(crate) fn plan_assignment_action(
    snippet: &ParsedTemplateSnippet,
    fragment_context: FragmentEvalContext<'_>,
    template_bindings: &HashMap<String, FragmentBinding>,
    root_bindings: &HashMap<String, HelperBinding>,
    current_dot_binding: Option<&HelperBinding>,
) -> AssignmentActionPlan {
    let local_assignment = parse_helper_assignment_from_exprs(snippet.exprs()).map(|assignment| {
        let mut locals = template_bindings.clone();
        for (key, value) in root_bindings {
            locals.insert(key.clone(), helper_to_fragment_binding(value));
        }
        let current_dot = current_dot_binding.map(helper_to_fragment_binding);
        let mut seen = HashSet::new();
        let fragment_binding = fragment_binding_from_expr(
            &assignment.rhs_expr,
            &locals,
            current_dot.as_ref(),
            fragment_context,
            &mut seen,
        );

        LocalAssignmentPlan {
            variable: assignment.variable,
            kind: assignment.kind,
            fragment_binding,
            rhs_expr: assignment.rhs_expr,
        }
    });

    AssignmentActionPlan {
        get_binding: parse_get_binding_from_exprs(snippet.exprs()),
        local_assignment,
    }
}
