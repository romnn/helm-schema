use std::collections::{HashMap, HashSet};

use crate::binding::{FragmentBinding, HelperBinding};
use crate::bound_value_analysis::{GetBindingPlan, parse_get_binding};
use crate::fragment_assignment::{AssignmentKind, parse_helper_assignment};
use crate::fragment_expr_eval::{FragmentEvalContext, fragment_binding_from_expr};

pub(crate) struct AssignmentActionPlan {
    pub(crate) get_binding: Option<GetBindingPlan>,
    pub(crate) local_assignment: Option<LocalAssignmentPlan>,
}

pub(crate) struct LocalAssignmentPlan {
    pub(crate) variable: String,
    pub(crate) kind: AssignmentKind,
    pub(crate) fragment_binding: Option<FragmentBinding>,
    pub(crate) rhs: String,
}

pub(crate) fn plan_assignment_action(
    text: &str,
    fragment_context: FragmentEvalContext<'_>,
    template_bindings: &HashMap<String, FragmentBinding>,
    root_bindings: &HashMap<String, HelperBinding>,
    current_dot_binding: Option<&HelperBinding>,
) -> AssignmentActionPlan {
    let local_assignment = parse_helper_assignment(text).map(|assignment| {
        let mut locals = template_bindings.clone();
        for (key, value) in root_bindings {
            locals.insert(key.clone(), value.to_fragment_binding());
        }
        let current_dot = current_dot_binding.map(HelperBinding::to_fragment_binding);
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
            rhs: assignment.rhs,
        }
    });

    AssignmentActionPlan {
        get_binding: parse_get_binding(text),
        local_assignment,
    }
}
