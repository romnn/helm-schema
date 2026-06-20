use std::collections::{HashMap, HashSet};

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBindingPlan, parse_get_binding_from_exprs};
use crate::fragment_assignment::{AssignmentKind, parse_helper_assignment_from_exprs};
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_expr};

pub(crate) struct AssignmentActionPlan {
    pub(crate) get_binding: Option<GetBindingPlan>,
    pub(crate) local_assignment: Option<LocalAssignmentPlan>,
}

pub(crate) struct LocalAssignmentPlan {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) fragment_binding: Option<AbstractValue>,
    pub(crate) rhs_expr: TemplateExpr,
}

pub(crate) fn plan_assignment_action(
    exprs: &[TemplateExpr],
    fragment_context: FragmentEvalContext<'_>,
    template_bindings: &HashMap<String, AbstractValue>,
    root_bindings: &HashMap<String, AbstractValue>,
    current_dot_binding: Option<&AbstractValue>,
) -> AssignmentActionPlan {
    let local_assignment = parse_helper_assignment_from_exprs(exprs).map(|assignment| {
        let mut locals = template_bindings.clone();
        for (key, value) in root_bindings {
            locals.insert(key.clone(), value.to_context_value());
        }
        let current_dot = current_dot_binding.map(AbstractValue::to_context_value);
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
        get_binding: parse_get_binding_from_exprs(exprs),
        local_assignment,
    }
}
