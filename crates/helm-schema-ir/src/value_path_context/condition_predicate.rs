use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::{Guard, GuardValue};
use helm_schema_ast::type_is_schema_type;
use helm_schema_ast::{
    is_string_predicate_function, is_string_transform_function, is_total_numeric_cast_function,
    is_total_stringification_function,
};
use helm_schema_core::Predicate;

use super::ValuePathContext;

fn is_files_get_function(function: &str) -> bool {
    function == "Files.Get" || function.ends_with(".Files.Get")
}

/// Split a printf format around a single `%s` verb; any other verb (or a
/// second `%s`) makes the produced name undecodable.
fn single_string_verb_split(format: &str) -> Option<(&str, &str)> {
    let index = format.find("%s")?;
    let (prefix, rest) = format.split_at(index);
    let suffix = &rest[2..];
    (!prefix.contains('%') && !suffix.contains('%')).then_some((prefix, suffix))
}

/// Runtime transform facts a condition expression binds on its direct
/// `.Values` subjects: unconditional string contracts, `default`-guarded
/// string contracts (the raw path is consumed only when truthy), and
/// total-conversion shape erasure.
#[derive(Debug, Default)]
pub(crate) struct ConditionTransformFacts {
    pub(crate) string_contracts: BTreeSet<String>,
    pub(crate) defaulted_string_contracts: BTreeSet<String>,
    pub(crate) shape_erased: BTreeSet<String>,
}

impl ValuePathContext<'_> {
    /// Transform facts a condition expression binds on its DIRECT selector
    /// subjects (anything that went through another call is derived text
    /// and claims nothing about the raw path):
    /// - string-consuming calls (`regexMatch "…" .Values.name`,
    ///   `gt (len (.Values.name | replace "-" "")) 32`) fail template
    ///   evaluation for non-string values — a runtime string contract;
    /// - total conversions (`eq (.Values.x | toString) "true"`,
    ///   `int .Values.x`) render ANY input — shape erasure, exactly like
    ///   the same conversion at a render site or in a `set` expression.
    pub(crate) fn condition_transform_facts(&self, expr: &TemplateExpr) -> ConditionTransformFacts {
        fn is_string_consumer(function: &str) -> bool {
            (is_string_transform_function(function) && !is_total_stringification_function(function))
                || is_string_predicate_function(function)
                || helm_schema_ast::is_string_splitting_function(function)
        }
        fn is_total_conversion(function: &str) -> bool {
            is_total_stringification_function(function) || is_total_numeric_cast_function(function)
        }
        fn subject_paths(
            context: &ValuePathContext<'_>,
            subject: &TemplateExpr,
        ) -> Option<BTreeSet<String>> {
            matches!(
                subject.deparen(),
                TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
            )
            .then(|| context.paths_for_expr(subject))
        }
        /// A subject of the form `<selector> | default <fallback>` (any
        /// argument order for the prefix form): the raw path is consumed
        /// only when TRUTHY, so its contract is conditional.
        fn defaulted_subject_paths(
            context: &ValuePathContext<'_>,
            subject: &TemplateExpr,
        ) -> Option<BTreeSet<String>> {
            match subject.deparen() {
                TemplateExpr::Pipeline(stages) if stages.len() == 2 => {
                    let is_default = matches!(
                        stages[1].deparen(),
                        TemplateExpr::Call { function, .. } if function == "default"
                    );
                    is_default
                        .then(|| subject_paths(context, &stages[0]))
                        .flatten()
                }
                TemplateExpr::Call { function, args }
                    if function == "default" && args.len() == 2 =>
                {
                    subject_paths(context, &args[1])
                }
                _ => None,
            }
        }
        fn walk(
            context: &ValuePathContext<'_>,
            expr: &TemplateExpr,
            facts: &mut ConditionTransformFacts,
        ) {
            match expr.deparen() {
                TemplateExpr::Call { function, args } => {
                    if let Some(subject) = args.last() {
                        if let Some(paths) = subject_paths(context, subject) {
                            if is_string_consumer(function) {
                                facts.string_contracts.extend(paths);
                            } else if is_total_conversion(function) {
                                facts.shape_erased.extend(paths);
                            }
                        } else if is_string_consumer(function)
                            && let Some(paths) = defaulted_subject_paths(context, subject)
                        {
                            facts.defaulted_string_contracts.extend(paths);
                        }
                    }
                    for arg in args {
                        walk(context, arg, facts);
                    }
                }
                TemplateExpr::Pipeline(stages) => {
                    if let Some(first) = stages.first()
                        && let Some(paths) = subject_paths(context, first)
                    {
                        // Stages run left-to-right: the FIRST classifying
                        // stage decides the raw value's fate. A consumer
                        // after a total conversion (`x | toString | trim`)
                        // operates on the converted text and claims nothing
                        // about the raw input; a consumer after `default`
                        // sees the raw value only when it is truthy.
                        let first_classifier = stages.iter().skip(1).find_map(|stage| match stage
                            .deparen()
                        {
                            TemplateExpr::Call { function, .. }
                                if is_string_consumer(function)
                                    || is_total_conversion(function)
                                    || function == "default" =>
                            {
                                Some(function.as_str())
                            }
                            _ => None,
                        });
                        match first_classifier {
                            Some(function) if is_string_consumer(function) => {
                                facts.string_contracts.extend(paths.iter().cloned());
                            }
                            Some("default") => {
                                let consumes = stages.iter().skip(1).any(|stage| {
                                    matches!(
                                        stage.deparen(),
                                        TemplateExpr::Call { function, .. }
                                            if is_string_consumer(function)
                                    )
                                });
                                if consumes {
                                    facts
                                        .defaulted_string_contracts
                                        .extend(paths.iter().cloned());
                                }
                            }
                            Some(_) => {
                                facts.shape_erased.extend(paths);
                            }
                            None => {}
                        }
                    }
                    for stage in stages {
                        walk(context, stage, facts);
                    }
                }
                _ => {}
            }
        }
        let mut facts = ConditionTransformFacts::default();
        walk(self, expr, &mut facts);
        facts
    }

    /// Whether `condition_predicate_expr` represents this expression
    /// EXACTLY, with no truthy fallback and no silently dropped conjunct.
    /// Rows tolerate approximate (wider) conditions; fail-branch NEGATION
    /// does not, so it consults this before trusting a captured stack.
    pub(crate) fn condition_lowering_is_faithful(&self, expr: &TemplateExpr) -> bool {
        match expr.deparen() {
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. } | TemplateExpr::Variable(_) => {
                !self.paths_for_expr(expr).is_empty()
            }
            TemplateExpr::Call { function, args } => match function.as_str() {
                "and" | "or" => args
                    .iter()
                    .all(|arg| self.condition_lowering_is_faithful(arg)),
                "not" => {
                    args.len() == 1
                        && self.condition_lowering_is_faithful(&args[0])
                        && self.not_predicate(args).is_some()
                }
                "eq" => self.value_comparison_predicate(args, false).is_some(),
                "ne" => self.value_comparison_predicate(args, true).is_some(),
                "typeIs" | "kindIs" => self.type_is_predicate(args).is_some(),
                "hasKey" => self.has_key_predicate(args).is_some(),
                "empty" => self.empty_predicate(args).is_some(),
                "coalesce" => self.coalesce_truthy_predicate(args).is_some(),
                function if is_files_get_function(function) => {
                    self.files_get_printf_predicate(args).is_some()
                }
                _ => false,
            },
            _ => false,
        }
    }

    pub(crate) fn condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        if let Some(predicate) = self.condition_predicate(expr) {
            return predicate;
        }
        if self.condition_has_unrepresentable_values_comparison_expr(expr) {
            return Predicate::True;
        }
        self.truthy_predicate(expr).unwrap_or(Predicate::True)
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(
            self.condition_predicate_expr(expr)
                .with_context_predicates(),
        )
    }

    fn condition_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return self.truthy_predicate(expr);
        };
        match function.as_str() {
            "and" => self.and_predicate(args),
            "not" => self.not_predicate(args),
            "empty" => self.empty_predicate(args),
            "hasKey" => self.has_key_predicate(args),
            "or" => self.or_predicate(args),
            "eq" => self.value_comparison_predicate(args, false),
            "ne" => self.value_comparison_predicate(args, true),
            "typeIs" | "kindIs" => self.type_is_predicate(args),
            "coalesce" => self.coalesce_truthy_predicate(args),
            function if is_files_get_function(function) => self
                .files_get_printf_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            _ => self.truthy_predicate(expr),
        }
    }

    /// `coalesce` returns its first non-empty argument, so the RESULT is
    /// truthy exactly when some argument is truthy: the disjunction is the
    /// precise condition, unlike the generic all-paths-truthy fallback.
    fn coalesce_truthy_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let mut arms = Vec::new();
        for arg in args {
            arms.push(self.truthy_predicate(arg)?);
        }
        if arms.len() == 1 {
            return arms.pop();
        }
        (!arms.is_empty()).then_some(Predicate::Or(arms))
    }

    /// `Files.Get (printf "files/profile-%s.yaml" X)` truthiness decodes
    /// to a FINITE predicate: the chart ships a fixed file set, so the
    /// read is non-empty exactly when X names one of the matching files'
    /// captured segments. With several candidate paths for X (a
    /// `coalesce`), the union over paths is wider than the render-time
    /// pick, which rows tolerate; its negation only holds when NO
    /// candidate carries a valid name — states where rendering fails for
    /// every pick — so fail-branch negation stays sound.
    fn files_get_printf_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [arg] = args else {
            return None;
        };
        let TemplateExpr::Call {
            function,
            args: printf_args,
        } = arg.deparen()
        else {
            return None;
        };
        if function != "printf" || printf_args.len() != 2 {
            return None;
        }
        let format = match printf_args[0].deparen() {
            TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => value,
            _ => return None,
        };
        let (prefix, suffix) = single_string_verb_split(format)?;
        let subject_paths = self.paths_for_expr(&printf_args[1]);
        if subject_paths.is_empty() {
            return None;
        }
        let names: Vec<String> = self
            .fragment_context
            .analysis_db
            .file_source_paths()
            .into_iter()
            .filter_map(|path| {
                path.strip_prefix(prefix)
                    .and_then(|rest| rest.strip_suffix(suffix))
                    .filter(|middle| !middle.is_empty())
                    .map(str::to_string)
            })
            .collect();
        if names.is_empty() {
            return None;
        }
        let mut arms = Vec::new();
        for path in &subject_paths {
            for name in &names {
                arms.push(Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: GuardValue::string(name),
                }));
            }
        }
        if arms.len() == 1 {
            return arms.pop();
        }
        Some(Predicate::Or(arms))
    }

    fn and_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let predicates = args
            .iter()
            .filter_map(|arg| self.condition_predicate(arg))
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn not_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [arg] = args else {
            return None;
        };

        match arg.deparen() {
            TemplateExpr::Call { function, args } if function == "empty" => self
                .empty_predicate(args)
                .map(|predicate| predicate.negated()),
            TemplateExpr::Call { function, args } if function == "or" => {
                self.negated_or_predicate(args)
            }
            TemplateExpr::Call { function, args } if function == "eq" => {
                self.value_comparison_predicate(args, true)
            }
            TemplateExpr::Call { function, args } if function == "ne" => {
                self.value_comparison_predicate(args, false)
            }
            // `not (typeIs …)` negates the TYPE TEST; degrading it to a
            // negated truthiness would conflate "not this type" with
            // "falsy".
            TemplateExpr::Call { function, args }
                if function == "typeIs" || function == "kindIs" =>
            {
                self.type_is_predicate(args).map(|p| p.negated())
            }
            TemplateExpr::Call { function, args } if function == "hasKey" => {
                self.has_key_predicate(args).map(|p| p.negated())
            }
            TemplateExpr::Call { function, args } if function == "and" => {
                self.and_predicate(args).map(|p| p.negated())
            }
            _ => {
                let paths = self.paths_for_expr(arg);
                if paths.len() == 1 {
                    return paths
                        .into_iter()
                        .next()
                        .map(|path| Predicate::truthy_path(path).negated());
                }
                None
            }
        }
    }

    fn negated_or_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let predicates = args
            .iter()
            .map(|arg| {
                self.single_truthy_predicate(arg)
                    .map(|predicate| predicate.negated())
            })
            .collect::<Option<Vec<_>>>()?;
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn empty_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [arg] = args else {
            return None;
        };
        self.single_truthy_predicate(arg)
            .map(|predicate| predicate.negated())
    }

    fn has_key_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [map, key] = args else {
            return None;
        };
        let TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) = key.deparen()
        else {
            return None;
        };
        self.with_body_fragment_value_expr(map)
            .and_then(|value| value_has_key(&value, key))
    }

    fn or_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let mut truthy_paths = BTreeSet::new();
        let mut alternatives = Vec::new();
        for arg in args {
            let paths = self.paths_for_expr(arg);
            if !paths.is_empty() && !matches!(arg.deparen(), TemplateExpr::Call { .. }) {
                truthy_paths.extend(paths);
                continue;
            }
            alternatives.push(self.condition_predicate(arg)?);
        }
        if !truthy_paths.is_empty() {
            let predicate = Predicate::Or(
                truthy_paths
                    .into_iter()
                    .map(Predicate::truthy_path)
                    .collect(),
            );
            alternatives.push(predicate);
        }
        (!alternatives.is_empty()).then_some(Predicate::Or(alternatives))
    }

    fn type_is_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let schema_type = type_is_schema_type(args.first())?;
        let predicates = args
            .iter()
            .skip(1)
            .flat_map(|arg| self.paths_for_expr(arg))
            .map(|path| {
                Predicate::from(Guard::TypeIs {
                    path,
                    schema_type: schema_type.clone(),
                })
            })
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    fn truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let paths = self.paths_for_expr(expr);
        (!paths.is_empty())
            .then(|| Predicate::all(paths.into_iter().map(Predicate::truthy_path).collect()))
    }

    fn single_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let mut paths = self.paths_for_expr(expr).into_iter();
        let path = paths.next()?;
        paths.next().is_none().then(|| Predicate::truthy_path(path))
    }

    fn condition_has_unrepresentable_values_comparison_expr(&self, expr: &TemplateExpr) -> bool {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return false;
        };
        match function.as_str() {
            "eq" | "ne" => self.comparison_has_unrepresentable_values(args),
            "typeIs" => {
                args.iter()
                    .any(|arg| self.expr_needs_context_value_resolution(arg))
                    && self.type_is_predicate(args).is_none()
            }
            _ => false,
        }
    }

    fn value_comparison_predicate(
        &self,
        args: &[TemplateExpr],
        negated: bool,
    ) -> Option<Predicate> {
        let [left, right] = args else {
            return None;
        };
        // `eq (typeOf .Values.x) "string"` (also through a bound local
        // `$tp := typeOf .Values.x`) is a TYPE TEST on the path, never a
        // value equality.
        if let Some(predicate) = self.type_descriptor_comparison(left, right, negated) {
            return Some(predicate);
        }
        // Only a DIRECT selector operand claims a value equality: seeing
        // through a call (`eq (typeOf .Values.x) "string"`) would compare
        // the call's OUTPUT, not the path's value.
        let direct_selector = |expr: &TemplateExpr| match expr.deparen() {
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. } => true,
            TemplateExpr::Variable(name) => !self
                .typeof_bindings
                .contains_key(name.trim_start_matches('$')),
            _ => false,
        };
        let (value, paths) = match (guard_value_literal(left), guard_value_literal(right)) {
            (Some(value), None) if direct_selector(right) => (value, self.paths_for_expr(right)),
            (None, Some(value)) if direct_selector(left) => (value, self.paths_for_expr(left)),
            _ => return None,
        };
        let predicates = paths
            .iter()
            .cloned()
            .map(|path| {
                if negated {
                    Predicate::from(Guard::NotEq {
                        path,
                        value: value.clone(),
                    })
                } else {
                    Predicate::from(Guard::Eq {
                        path,
                        value: value.clone(),
                    })
                }
            })
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    /// A `typeOf`/`kindOf` comparison against a string literal, either
    /// directly (`eq (typeOf .Values.x) "string"`) or through a bound local
    /// (`$tp := typeOf .Values.x` then `eq $tp "string"`). Lowers to a
    /// [`Guard::TypeIs`] on the described path.
    fn type_descriptor_comparison(
        &self,
        left: &TemplateExpr,
        right: &TemplateExpr,
        negated: bool,
    ) -> Option<Predicate> {
        let described_path = |expr: &TemplateExpr| -> Option<String> {
            match expr.deparen() {
                TemplateExpr::Call { function, args }
                    if matches!(function.as_str(), "typeOf" | "kindOf") && args.len() == 1 =>
                {
                    // Selectors and bound locals (a range's value variable,
                    // a `$x := .Values.y` binding) both describe a single
                    // resolvable path.
                    let subject = args[0].deparen();
                    matches!(
                        subject,
                        TemplateExpr::Field(_)
                            | TemplateExpr::Selector { .. }
                            | TemplateExpr::Variable(_)
                    )
                    .then(|| self.paths_for_expr(subject))
                    .filter(|paths| paths.len() == 1)
                    .and_then(|paths| paths.into_iter().next())
                }
                TemplateExpr::Variable(name) => self
                    .typeof_bindings
                    .get(name.trim_start_matches('$'))
                    .cloned(),
                _ => None,
            }
        };
        fn type_literal(expr: &TemplateExpr) -> Option<&str> {
            match expr.deparen() {
                TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name)) => {
                    Some(name.as_str())
                }
                _ => None,
            }
        }
        let (path, type_name) = match (described_path(left), described_path(right)) {
            (Some(path), None) => (path, type_literal(right)?),
            (None, Some(path)) => (path, type_literal(left)?),
            _ => return None,
        };
        let schema_type = helm_schema_ast::go_type_schema_type(type_name)?;
        let guard = Predicate::from(Guard::TypeIs {
            path,
            schema_type: schema_type.to_string(),
        });
        Some(if negated { guard.negated() } else { guard })
    }

    fn comparison_has_unrepresentable_values(&self, args: &[TemplateExpr]) -> bool {
        if !args
            .iter()
            .any(|arg| self.expr_needs_context_value_resolution(arg))
        {
            return false;
        }
        let [left, right] = args else {
            return true;
        };
        !matches!(
            (guard_value_literal(left), guard_value_literal(right)),
            (Some(_), None) | (None, Some(_))
        )
    }
}

fn value_has_key(value: &AbstractValue, key: &str) -> Option<Predicate> {
    match value {
        AbstractValue::Dict(entries) => Some(bool_predicate(entries.contains_key(key))),
        AbstractValue::Overlay { entries, fallback } => entries
            .contains_key(key)
            .then_some(Predicate::True)
            .or_else(|| value_has_key(fallback, key)),
        AbstractValue::Choice(choices) => {
            let mut resolved = choices
                .iter()
                .map(|choice| value_has_key(choice, key))
                .collect::<Option<Vec<_>>>()?;
            resolved.sort();
            resolved.dedup();
            match resolved.as_slice() {
                [predicate] => Some(predicate.clone()),
                _ => None,
            }
        }
        AbstractValue::ValuesPath(path) => Some(
            Predicate::from(Guard::Absent {
                path: helm_schema_core::append_value_path(path, key),
            })
            .negated(),
        ),
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::List(_)
        | AbstractValue::Widened(_) => None,
    }
}

fn bool_predicate(value: bool) -> Predicate {
    if value {
        Predicate::True
    } else {
        Predicate::False
    }
}

fn guard_value_literal(expr: &TemplateExpr) -> Option<GuardValue> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(GuardValue::string(value))
        }
        TemplateExpr::Literal(Literal::Bool(value)) => Some(GuardValue::Bool(*value)),
        TemplateExpr::Literal(Literal::Int(value)) => Some(GuardValue::Int(*value)),
        TemplateExpr::Literal(Literal::Float(value)) => GuardValue::float(*value),
        TemplateExpr::Literal(Literal::Nil) => Some(GuardValue::Null),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/value_path_context/condition_predicate.rs"]
mod tests;
