use std::collections::{BTreeMap, HashMap, HashSet};

use helm_schema_ast::{DefineIndex, Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::binding::{FragmentBinding, HelperBinding};
use crate::define_body_cache::DefineBodyCache;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_printf_format, render_printf_string_sets};
use crate::fragment_binding_eval::{
    fragment_binding_from_helper_analysis, helper_binding_from_helper_analysis,
};
use crate::helper_binding_eval::binding_from_expr;
use crate::helper_call_analyzer::HelperCallAnalyzer;
use crate::template_expr_analysis::{expr_contains_helper_call, is_merge_function};
use crate::template_expr_cache::parse_expr_text;

#[derive(Clone, Copy)]
pub(crate) struct FragmentEvalContext<'a> {
    pub(crate) defines: &'a DefineIndex,
    pub(crate) define_bodies: &'a DefineBodyCache,
    helper_call_analyzer: &'a dyn HelperCallAnalyzer,
}

impl<'a> FragmentEvalContext<'a> {
    pub(crate) fn new(
        defines: &'a DefineIndex,
        define_bodies: &'a DefineBodyCache,
        helper_call_analyzer: &'a dyn HelperCallAnalyzer,
    ) -> Self {
        Self {
            defines,
            define_bodies,
            helper_call_analyzer,
        }
    }

    pub(crate) fn helper_call_analyzer(&self) -> &'a dyn HelperCallAnalyzer {
        self.helper_call_analyzer
    }

    pub(crate) fn fragment_binding_from_expr(
        &self,
        expr: &TemplateExpr,
        locals: &HashMap<String, FragmentBinding>,
        current_dot: Option<&FragmentBinding>,
        seen: &mut HashSet<String>,
    ) -> Option<FragmentBinding> {
        fragment_binding_from_expr(expr, locals, current_dot, *self, seen)
    }
}

pub(crate) fn helper_binding_from_expr_with_fragment_locals(
    expr: &TemplateExpr,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<HelperBinding> {
    match expr {
        TemplateExpr::Parenthesized(inner) => helper_binding_from_expr_with_fragment_locals(
            inner,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        ),
        TemplateExpr::Variable(var) if !var.is_empty() => fragment_locals
            .get(var)
            .and_then(FragmentBinding::to_helper_binding),
        TemplateExpr::Selector { operand, path } => {
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && !var.is_empty()
                && let Some(binding) = fragment_locals
                    .get(var)
                    .and_then(FragmentBinding::to_helper_binding)
            {
                return binding.apply_to_binding(path);
            }
            binding_from_expr(expr, outer, current_dot)
        }
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "include" | "template") =>
        {
            let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                return None;
            };
            let analysis = context.helper_call_analyzer().analyze_bound_helper_call(
                name,
                args.get(1),
                outer,
                current_dot,
                fragment_locals,
                context,
                seen,
            );
            helper_binding_from_helper_analysis(analysis)
        }
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut map = BTreeMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let binding = helper_binding_from_expr_with_fragment_locals(
                    &args[index + 1],
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                )
                .unwrap_or(HelperBinding::Unknown);
                map.insert(key.clone(), binding);
                index += 2;
            }
            Some(HelperBinding::Dict(map))
        }
        TemplateExpr::Call { function, args } if matches!(function.as_str(), "list" | "tuple") => {
            Some(HelperBinding::List(
                args.iter()
                    .map(|arg| {
                        helper_binding_from_expr_with_fragment_locals(
                            arg,
                            fragment_locals,
                            outer,
                            current_dot,
                            context,
                            seen,
                        )
                        .unwrap_or(HelperBinding::Unknown)
                    })
                    .collect(),
            ))
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let bindings = args
                .iter()
                .filter_map(|arg| {
                    helper_binding_from_expr_with_fragment_locals(
                        arg,
                        fragment_locals,
                        outer,
                        current_dot,
                        context,
                        seen,
                    )
                })
                .collect();
            HelperBinding::merge_all(bindings)
        }
        TemplateExpr::Pipeline(stages) => {
            let mut current = helper_binding_from_expr_with_fragment_locals(
                &stages[0],
                fragment_locals,
                outer,
                current_dot,
                context,
                seen,
            );
            for stage in &stages[1..] {
                let TemplateExpr::Call { function, args } = stage else {
                    continue;
                };
                current = match function.as_str() {
                    "default" => {
                        let mut bindings = Vec::new();
                        if let Some(current) = current {
                            bindings.push(current);
                        }
                        for arg in args {
                            if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
                                arg,
                                fragment_locals,
                                outer,
                                current_dot,
                                context,
                                seen,
                            ) {
                                bindings.push(binding);
                            }
                        }
                        HelperBinding::choice(bindings)
                    }
                    function if is_merge_function(function) => {
                        let mut bindings = Vec::new();
                        if let Some(current) = current {
                            bindings.push(current);
                        }
                        for arg in args {
                            if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
                                arg,
                                fragment_locals,
                                outer,
                                current_dot,
                                context,
                                seen,
                            ) {
                                bindings.push(binding);
                            }
                        }
                        HelperBinding::merge_all(bindings)
                    }
                    "toYaml" | "fromYaml" | "toJson" | "fromJson" | "quote" | "toString"
                    | "deepCopy" | "tpl" | "nindent" | "indent" => current,
                    _ => None,
                };
            }
            current
        }
        _ => binding_from_expr(expr, outer, current_dot),
    }
}

pub(crate) fn bindings_for_helper_arg_with_fragment_locals(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    fragment_locals: &HashMap<String, FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> HashMap<String, HelperBinding> {
    let Some(arg) = arg else {
        return HashMap::new();
    };

    match arg {
        TemplateExpr::Parenthesized(inner) => bindings_for_helper_arg_with_fragment_locals(
            Some(inner),
            outer,
            current_dot,
            fragment_locals,
            context,
            seen,
        ),
        TemplateExpr::Field(path) if path.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Variable(var) if var.is_empty() => outer.cloned().unwrap_or_default(),
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut bindings = HashMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) =
                    &args[index]
                else {
                    index += 1;
                    continue;
                };
                let binding = helper_binding_from_expr_with_fragment_locals(
                    &args[index + 1],
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                )
                .unwrap_or(HelperBinding::Unknown);
                bindings.insert(key.clone(), binding);
                index += 2;
            }
            bindings
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut merged = HashMap::new();
            for arg in args {
                match helper_binding_from_expr_with_fragment_locals(
                    arg,
                    fragment_locals,
                    outer,
                    current_dot,
                    context,
                    seen,
                ) {
                    Some(HelperBinding::Dict(map)) => {
                        for (key, value) in map {
                            merged.insert(key, value);
                        }
                    }
                    Some(HelperBinding::RootContext) => {
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
        _ => HashMap::new(),
    }
}

pub(crate) fn fragment_binding_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    if !expr_contains_helper_call(expr)
        && let Some(binding) = shared_fragment_binding_from_expr(expr, locals, current_dot)
    {
        return Some(binding);
    }

    match expr {
        TemplateExpr::Parenthesized(inner) => {
            fragment_binding_from_expr(inner, locals, current_dot, context, seen)
        }
        TemplateExpr::Selector { operand, path } => {
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && var.is_empty()
                && let Some((head, tail)) = path.split_first()
                && let Some(binding) = locals.get(head)
            {
                return binding.apply_to_binding(tail);
            }
            let binding = fragment_binding_from_expr(operand, locals, current_dot, context, seen)?;
            binding.apply_to_binding(path)
        }
        TemplateExpr::Call { function, args } if matches!(function.as_str(), "list" | "tuple") => {
            let mut items = Vec::new();
            for arg in args {
                items.push(
                    fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                        .unwrap_or(FragmentBinding::Unknown),
                );
            }
            Some(FragmentBinding::List(items))
        }
        TemplateExpr::Call { function, args } if function == "append" => {
            let mut items =
                match fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)
                {
                    Some(FragmentBinding::List(items)) => items,
                    Some(binding) => vec![binding],
                    None => Vec::new(),
                };
            for arg in &args[1..] {
                if let Some(binding) =
                    fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                {
                    items.push(binding);
                }
            }
            Some(FragmentBinding::List(items))
        }
        TemplateExpr::Call { function, args } if function == "dict" => {
            let mut map = BTreeMap::new();
            let mut index = 0usize;
            while index + 1 < args.len() {
                let TemplateExpr::Literal(Literal::String(key)) = &args[index] else {
                    index += 1;
                    continue;
                };
                if let Some(binding) =
                    fragment_binding_from_expr(&args[index + 1], locals, current_dot, context, seen)
                {
                    map.insert(key.clone(), binding);
                }
                index += 2;
            }
            Some(FragmentBinding::Dict(map))
        }
        TemplateExpr::Call { function, args } if is_merge_function(function) => {
            let mut bindings = Vec::new();
            for arg in args {
                let Some(binding) =
                    fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                else {
                    continue;
                };
                bindings.push(binding);
            }
            FragmentBinding::merge_all(bindings)
        }
        TemplateExpr::Call { function, args } if function == "coalesce" => {
            let mut choices = Vec::new();
            for arg in args {
                if let Some(binding) =
                    fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                {
                    choices.push(binding);
                }
            }
            FragmentBinding::choice(choices)
        }
        TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
            let mut choices = Vec::new();
            if let Some(binding) =
                fragment_binding_from_expr(&args[1], locals, current_dot, context, seen)
            {
                choices.push(binding);
            }
            if let Some(binding) =
                fragment_binding_from_expr(&args[0], locals, current_dot, context, seen)
            {
                choices.push(binding);
            }
            FragmentBinding::choice(choices)
        }
        TemplateExpr::Call { function, args }
            if matches!(
                function.as_str(),
                "toYaml" | "fromYaml" | "quote" | "toString" | "int" | "tpl" | "b64enc" | "b64dec"
            ) =>
        {
            fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)
        }
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "indent" | "nindent" | "trimAll") =>
        {
            fragment_binding_from_expr(args.last()?, locals, current_dot, context, seen)
        }
        TemplateExpr::Call { function, args } if function == "printf" => {
            let format = literal_printf_format(args)?;
            let mut arg_strings = Vec::new();
            for arg in &args[1..] {
                let strings = FragmentBinding::strings(&fragment_binding_from_expr(
                    arg,
                    locals,
                    current_dot,
                    context,
                    seen,
                )?);
                if strings.is_empty() {
                    return None;
                }
                arg_strings.push(strings);
            }
            Some(FragmentBinding::StringSet(render_printf_string_sets(
                format,
                &arg_strings,
            )?))
        }
        TemplateExpr::Call { function, args } if function == "index" => {
            let base =
                fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)?;
            match base {
                FragmentBinding::List(items) if args.len() == 2 => {
                    let index = match &args[1] {
                        TemplateExpr::Literal(Literal::Int(value)) => {
                            usize::try_from(*value).ok()?
                        }
                        _ => {
                            let strings = FragmentBinding::strings(&fragment_binding_from_expr(
                                &args[1],
                                locals,
                                current_dot,
                                context,
                                seen,
                            )?);
                            strings.iter().next()?.parse::<usize>().ok()?
                        }
                    };
                    items.get(index).cloned()
                }
                binding => {
                    let mut segment_options: Vec<Vec<String>> = Vec::new();
                    for arg in &args[1..] {
                        let arg_binding =
                            fragment_binding_from_expr(arg, locals, current_dot, context, seen);
                        let strings = FragmentBinding::strings(&arg_binding?);
                        if strings.is_empty() {
                            return None;
                        }
                        segment_options.push(strings.into_iter().collect());
                    }

                    let mut bindings = vec![binding.clone()];
                    for options in segment_options {
                        let mut next = Vec::new();
                        for binding in &bindings {
                            for option in &options {
                                if let Some(bound) =
                                    binding.apply_to_binding(std::slice::from_ref(option))
                                {
                                    next.push(bound);
                                }
                            }
                        }
                        bindings = next;
                    }
                    FragmentBinding::choice(bindings)
                }
            }
        }
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "include" | "template") =>
        {
            let Some(TemplateExpr::Literal(Literal::String(name))) = args.first() else {
                return None;
            };
            let current_dot_helper = current_dot.and_then(FragmentBinding::to_helper_binding);
            let analysis = context.helper_call_analyzer().analyze_bound_helper_call(
                name,
                args.get(1),
                None,
                current_dot_helper.as_ref(),
                locals,
                context,
                seen,
            );
            fragment_binding_from_helper_analysis(analysis)
        }
        TemplateExpr::Call { function, args } if function == "tpl" => {
            fragment_binding_from_expr(args.first()?, locals, current_dot, context, seen)
        }
        TemplateExpr::Call { function, args } if function == "ternary" => {
            let mut choices = Vec::new();
            for arg in args.iter().take(2) {
                if let Some(binding) =
                    fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                {
                    choices.push(binding);
                }
            }
            FragmentBinding::choice(choices)
        }
        TemplateExpr::Pipeline(stages) => {
            let mut current =
                fragment_binding_from_expr(&stages[0], locals, current_dot, context, seen);
            for stage in &stages[1..] {
                let TemplateExpr::Call { function, args } = stage else {
                    continue;
                };
                current = match function.as_str() {
                    "quote" | "toString" | "toYaml" | "fromYaml" | "indent" | "nindent"
                    | "trimAll" | "trimPrefix" | "trimSuffix" | "trunc" | "replace" | "int"
                    | "uniq" | "b64enc" | "b64dec" => current,
                    function if is_merge_function(function) => {
                        let mut bindings = Vec::new();
                        if let Some(current) = current {
                            bindings.push(current);
                        }
                        for arg in args {
                            if let Some(binding) =
                                fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                            {
                                bindings.push(binding);
                            }
                        }
                        FragmentBinding::merge_all(bindings)
                    }
                    "default" => {
                        let mut choices = Vec::new();
                        if let Some(current) = current {
                            choices.push(current);
                        }
                        for arg in args {
                            if let Some(binding) =
                                fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                            {
                                choices.push(binding);
                            }
                        }
                        FragmentBinding::choice(choices)
                    }
                    "ternary" => {
                        let mut choices = Vec::new();
                        if let Some(current) = current {
                            choices.push(current);
                        }
                        for arg in args {
                            if let Some(binding) =
                                fragment_binding_from_expr(arg, locals, current_dot, context, seen)
                            {
                                choices.push(binding);
                            }
                        }
                        FragmentBinding::choice(choices)
                    }
                    _ => return None,
                };
            }
            current
        }
        _ => None,
    }
}

fn shared_fragment_binding_from_expr(
    expr: &TemplateExpr,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
) -> Option<FragmentBinding> {
    let env = EvalEnv::from_fragment_context(locals, current_dot);
    eval_expr(expr, &env)
        .value
        .as_ref()
        .and_then(AbstractValue::to_fragment_binding)
}

pub(crate) fn fragment_binding_from_text(
    text: &str,
    locals: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let mut bindings = Vec::new();
    for expr in parse_expr_text(text) {
        if let Some(binding) = context.fragment_binding_from_expr(&expr, locals, current_dot, seen)
        {
            bindings.push(binding);
        }
    }
    FragmentBinding::choice(bindings)
}

pub(crate) fn fragment_binding_from_text_with_helper_context(
    text: &str,
    fragment_locals: &HashMap<String, FragmentBinding>,
    outer: Option<&HashMap<String, HelperBinding>>,
    current_dot: Option<&HelperBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let current_dot_fragment = current_dot.map(HelperBinding::to_fragment_binding);
    let mut bindings = Vec::new();
    for expr in parse_expr_text(text) {
        if let Some(binding) = helper_binding_from_expr_with_fragment_locals(
            &expr,
            fragment_locals,
            outer,
            current_dot,
            context,
            seen,
        ) {
            bindings.push(binding.to_fragment_binding());
            continue;
        }
        if let Some(binding) = fragment_binding_from_expr(
            &expr,
            fragment_locals,
            current_dot_fragment.as_ref(),
            context,
            seen,
        ) {
            bindings.push(binding);
        }
    }
    FragmentBinding::choice(bindings)
}
