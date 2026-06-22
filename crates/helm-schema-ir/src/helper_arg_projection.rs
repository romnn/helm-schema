use std::collections::HashMap;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::template_expr_analysis::is_merge_function;

pub(crate) fn bindings_for_helper_arg_with(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    mut eval_binding: impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    bindings_for_helper_arg_inner(arg, outer, &mut eval_binding)
}

fn bindings_for_helper_arg_inner(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg {
        TemplateExpr::Parenthesized(inner) => {
            bindings_for_helper_arg_inner(Some(inner), outer, eval_binding)
        }
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if function == "dict" => {
            bindings_from_dict_args(args, eval_binding)
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            bindings_from_merge_args(args, outer, eval_binding)
        }
        _ => HashMap::new(),
    }
}

fn bindings_from_dict_args(
    args: &[TemplateExpr],
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut bindings = HashMap::new();
    let mut index = 0usize;
    while index + 1 < args.len() {
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) = &args[index]
        else {
            index += 1;
            continue;
        };
        let binding = eval_binding(&args[index + 1]).unwrap_or(AbstractValue::Unknown);
        bindings.insert(key.clone(), binding);
        index += 2;
    }
    bindings
}

fn bindings_from_merge_args(
    args: &[TemplateExpr],
    outer: Option<&HashMap<String, AbstractValue>>,
    eval_binding: &mut impl FnMut(&TemplateExpr) -> Option<AbstractValue>,
) -> HashMap<String, AbstractValue> {
    let mut merged = HashMap::new();
    for arg in args {
        match eval_binding(arg) {
            Some(AbstractValue::Dict(map)) => {
                for (key, value) in map {
                    merged.insert(key, value);
                }
            }
            Some(AbstractValue::RootContext) => {
                if let Some(outer) = outer {
                    for (key, value) in outer {
                        merged.insert(key.clone(), value.clone());
                    }
                }
            }
            _ => {}
        }
    }
    merged
}

#[cfg(test)]
#[path = "tests/helper_arg_projection.rs"]
mod tests;
