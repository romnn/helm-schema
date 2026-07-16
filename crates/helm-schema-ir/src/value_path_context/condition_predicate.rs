use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::expr_eval::eval_expr;
use crate::{Guard, GuardValue};
use helm_schema_ast::type_is_schema_type;
use helm_schema_core::Predicate;

use super::ValuePathContext;

fn is_files_get_function(function: &str) -> bool {
    function == "Files.Get" || function.ends_with(".Files.Get")
}

/// Dispatch-arm headers may themselves compare helper outputs; one level
/// covers the chart shapes seen so far, and the cap keeps mutually
/// recursive helpers from looping the decoder.
const MAX_HELPER_DISPATCH_DEPTH: u8 = 2;

thread_local! {
    static HELPER_DISPATCH_DEPTH: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// An `include`/`template` call whose context argument carries the root
/// (`.` or `$`), so the callee's `.Values.*` conditions keep their paths.
fn helper_root_call(expr: &TemplateExpr) -> Option<&str> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    let name = crate::expr_eval::literal_helper_call_callee(function, args)?;
    let [_, context] = args.as_slice() else {
        return None;
    };
    let root_context = match context.deparen() {
        TemplateExpr::Field(path) => path.is_empty(),
        TemplateExpr::Variable(variable) => variable.is_empty(),
        _ => false,
    };
    root_context.then_some(name)
}

fn literal_string(expr: &TemplateExpr) -> Option<&str> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text)) => Some(text),
        _ => None,
    }
}

fn literal_membership_string(expr: &TemplateExpr) -> Option<String> {
    if let Some(value) = literal_string(expr) {
        return Some(value.to_string());
    }
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    let [arg] = args.as_slice() else {
        return None;
    };
    let value = literal_string(arg)?;
    match function.as_str() {
        "quote" if value.is_empty() => Some("\"\"".to_string()),
        "squote" if value.is_empty() => Some("''".to_string()),
        _ => None,
    }
}

/// Split a printf format around a single `%s` verb; any other verb (or a
/// second `%s`) makes the produced name undecodable.
fn single_string_verb_split(format: &str) -> Option<(&str, &str)> {
    let index = format.find("%s")?;
    let (prefix, rest) = format.split_at(index);
    let suffix = &rest[2..];
    (!prefix.contains('%') && !suffix.contains('%')).then_some((prefix, suffix))
}

impl ValuePathContext<'_> {
    /// Whether `condition_predicate_expr` represents this expression
    /// EXACTLY, with no truthy fallback and no silently dropped conjunct.
    /// Rows tolerate approximate (wider) conditions; fail-branch NEGATION
    /// does not, so it consults this before trusting a captured stack.
    pub(crate) fn condition_lowering_is_faithful(&self, expr: &TemplateExpr) -> bool {
        match expr.deparen() {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => self.condition_lowering_is_faithful(value),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. } => {
                self.root_field_truthy_predicate(expr).is_some()
                    || !self.paths_for_expr(expr).is_empty()
            }
            TemplateExpr::Literal(_) => true,
            // A local bound to DERIVED TEXT (`$message := join "\n"
            // $messages`) is falsy when the derivation produced nothing,
            // not when its input identities are falsy: a truthy stand-in
            // over the flowing paths would let negation fire on states the
            // branch never reaches (bitnami `validateValues` aggregators).
            TemplateExpr::Variable(name) => {
                if let Some(predicate) = self.template_truthy_reductions.get(name).or_else(|| {
                    self.template_truthy_reductions
                        .get(name.trim_start_matches('$'))
                }) {
                    return !matches!(predicate, Predicate::False);
                }
                if self.get_binding_truthy_predicate(name).is_some() {
                    return true;
                }
                let paths = self.paths_for_expr(expr);
                if paths.is_empty() {
                    return false;
                }
                let metas = self
                    .template_output_meta
                    .get(name)
                    .or_else(|| self.template_output_meta.get(name.trim_start_matches('$')));
                !paths.iter().any(|path| {
                    metas
                        .and_then(|metas| metas.get(path))
                        .is_some_and(|meta| meta.derived_text || meta.shape_erased)
                })
            }
            TemplateExpr::Call { function, args } => match function.as_str() {
                "and" | "or" => args
                    .iter()
                    .all(|arg| self.condition_lowering_is_faithful(arg)),
                "list" | "tuple" | "dict" => true,
                "not" => {
                    args.len() == 1
                        && self.condition_lowering_is_faithful(&args[0])
                        && self.not_predicate(args).is_some()
                }
                "eq" => self.value_comparison_predicate(args, false).is_some(),
                "ne" => self.value_comparison_predicate(args, true).is_some(),
                "gt" | "lt" => self.positive_len_predicate(function, args).is_some(),
                "typeIs" | "kindIs" => self.type_is_predicate(args).is_some(),
                "hasKey" => self.has_key_predicate(args).is_some(),
                "hasPrefix" => self.range_key_prefix_predicate(args).is_some(),
                "contains" => self.contains_predicate(args).is_some(),
                "has" => self.helper_literal_membership_predicate(args).is_some(),
                "empty" => self.empty_predicate(args).is_some(),
                "coalesce" => self.coalesce_truthy_predicate(args).is_some(),
                "default" => self.default_truthy_predicate(args).is_some(),
                "dig" => self.dig_truthy_predicate(args).is_some(),
                "regexMatch" | "mustRegexMatch" => self.regex_match_predicate(args).is_some(),
                function if is_files_get_function(function) => {
                    self.files_get_printf_predicate(args).is_some()
                }
                _ => false,
            },
            TemplateExpr::Pipeline(stages) => {
                self.default_pipeline_truthy_predicate(stages).is_some()
            }
            _ => false,
        }
    }

    pub(crate) fn condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        if let TemplateExpr::VariableDefinition { value, .. }
        | TemplateExpr::Assignment { value, .. } = expr.deparen()
        {
            return self.condition_predicate_expr(value);
        }
        if let Some(predicate) = self.condition_predicate(expr) {
            return predicate;
        }
        if self.condition_has_unrepresentable_values_comparison_expr(expr) {
            return Predicate::True;
        }
        self.truthy_predicate(expr).unwrap_or(Predicate::True)
    }

    pub(crate) fn approximate_condition_predicate_expr(
        &self,
        expr: &TemplateExpr,
        marker: &str,
    ) -> Predicate {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Predicate::approximate(marker, self.resolved_values_paths_from_expr(expr));
        };
        if function != "or" {
            return Predicate::approximate_with_sound_subset(
                marker,
                self.resolved_values_paths_from_expr(expr),
                self.int_cast_comparison_sound_subset(expr),
            );
        }
        let alternatives = args
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                if self.condition_lowering_is_faithful(arg) {
                    self.condition_predicate_expr(arg)
                } else {
                    Predicate::approximate_with_sound_subset(
                        format!("{marker}:{index}"),
                        self.resolved_values_paths_from_expr(arg),
                        self.int_cast_comparison_sound_subset(arg),
                    )
                }
            })
            .collect();
        predicate_any(alternatives)
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(
            self.condition_predicate_expr(expr)
                .with_context_predicates(),
        )
    }

    fn condition_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        if let TemplateExpr::Pipeline(stages) = expr.deparen() {
            return self.default_pipeline_truthy_predicate(stages);
        }
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return self.truthy_predicate(expr);
        };
        match function.as_str() {
            "and" => self.and_predicate(args),
            "list" | "tuple" | "dict" => Some(bool_predicate(!args.is_empty())),
            "not" => self.not_predicate(args),
            "empty" => self.empty_predicate(args),
            "hasKey" => self.has_key_predicate(args),
            "hasPrefix" => self.range_key_prefix_predicate(args),
            "contains" => self.contains_predicate(args),
            "has" => self
                .helper_literal_membership_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            "or" => self.or_predicate(args),
            "eq" => self.value_comparison_predicate(args, false),
            "ne" => self.value_comparison_predicate(args, true),
            "gt" | "lt" => self.positive_len_predicate(function, args),
            "typeIs" | "kindIs" => self.type_is_predicate(args),
            "coalesce" => self.coalesce_truthy_predicate(args),
            "default" => self.default_truthy_predicate(args),
            "dig" => self
                .dig_truthy_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            "regexMatch" | "mustRegexMatch" => self.regex_match_predicate(args),
            function if is_files_get_function(function) => self
                .files_get_printf_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            _ => self.truthy_predicate(expr),
        }
    }

    /// `dig k1 … kn default subject` with literal keys, a FALSY literal
    /// default, and one values-backed map subject is truthy exactly when
    /// the dug path's value is truthy: a missing chain yields the falsy
    /// default, and a present chain yields the leaf value itself. Truthy
    /// or non-literal defaults abstain (absence would select a truthy
    /// fallback), as does a subject without a single map identity.
    fn dig_truthy_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let (subject, rest) = args.split_last()?;
        let (default, key_exprs) = rest.split_last()?;
        if key_exprs.is_empty() {
            return None;
        }
        let keys = key_exprs
            .iter()
            .map(literal_string)
            .collect::<Option<Vec<_>>>()?;
        let TemplateExpr::Literal(literal) = default.deparen() else {
            return None;
        };
        if literal_is_truthy(literal) {
            return None;
        }
        // The subject must be ONE values-backed map identity — the
        // whole-values root (`.Values` or `.Values.AsMap`) or a single
        // path. A choice of subjects would misstate the leaf condition.
        let base = match self.with_body_fragment_value_expr(subject)? {
            AbstractValue::ValuesPath(base) => base,
            _ => return None,
        };
        let path = keys.iter().fold(base, |path, key| {
            helm_schema_core::append_value_path(&path, key)
        });
        Some(Predicate::truthy_path(path))
    }

    /// `eq $key "literal"` (either operand order) where `$key` is a
    /// destructured range key: the predicate selects exactly the member
    /// with that key.
    fn range_key_equals_predicate(
        &self,
        left: &TemplateExpr,
        right: &TemplateExpr,
        negated: bool,
    ) -> Option<Predicate> {
        let (key_expr, literal) = match (literal_string(left), literal_string(right)) {
            (None, Some(literal)) => (left, literal),
            (Some(literal), None) => (right, literal),
            _ => return None,
        };
        let TemplateExpr::Variable(name) = key_expr.deparen() else {
            return None;
        };
        let binding = self
            .template_bindings
            .get(name)
            .or_else(|| self.template_bindings.get(name.trim_start_matches('$')))?;
        let AbstractValue::RangeKey(path) = binding else {
            return None;
        };
        let predicate = Predicate::from(Guard::RangeKeyEquals {
            path: path.clone(),
            key: literal.to_string(),
        });
        Some(if negated {
            predicate.negated()
        } else {
            predicate
        })
    }

    fn range_key_prefix_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [prefix, key] = args else {
            return None;
        };
        let prefix = literal_string(prefix)?;
        let TemplateExpr::Variable(name) = key.deparen() else {
            return None;
        };
        let binding = self
            .template_bindings
            .get(name)
            .or_else(|| self.template_bindings.get(name.trim_start_matches('$')))?;
        let AbstractValue::RangeKey(path) = binding else {
            return None;
        };
        Some(Predicate::from(Guard::RangeKeyPrefix {
            path: path.clone(),
            prefix: prefix.to_string(),
        }))
    }

    fn positive_len_predicate(&self, function: &str, args: &[TemplateExpr]) -> Option<Predicate> {
        let [left, right] = args else {
            return None;
        };
        let subject = match (function, left.deparen(), right.deparen()) {
            (
                "gt",
                TemplateExpr::Call {
                    function,
                    args: len_args,
                },
                TemplateExpr::Literal(Literal::Int(0)),
            ) if function == "len" => len_args.as_slice(),
            (
                "lt",
                TemplateExpr::Literal(Literal::Int(0)),
                TemplateExpr::Call {
                    function,
                    args: len_args,
                },
            ) if function == "len" => len_args.as_slice(),
            _ => return None,
        };
        let [subject] = subject else {
            return None;
        };
        self.single_truthy_predicate(subject)
    }

    fn default_truthy_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [fallback, primary] = args else {
            return None;
        };
        Some(predicate_any(vec![
            self.exact_truthy_predicate(primary)?,
            self.exact_truthy_predicate(fallback)?,
        ]))
    }

    fn default_pipeline_truthy_predicate(&self, stages: &[TemplateExpr]) -> Option<Predicate> {
        let [primary, stage] = stages else {
            return None;
        };
        let TemplateExpr::Call { function, args } = stage.deparen() else {
            return None;
        };
        let [fallback] = args.as_slice() else {
            return None;
        };
        if function != "default" {
            return None;
        }
        Some(predicate_any(vec![
            self.exact_truthy_predicate(primary)?,
            self.exact_truthy_predicate(fallback)?,
        ]))
    }

    fn exact_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        self.condition_lowering_is_faithful(expr)
            .then(|| self.condition_predicate_expr(expr))
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
        let mut predicates = args
            .iter()
            .filter_map(|arg| self.condition_predicate(arg))
            .collect::<Vec<_>>();
        if predicates.is_empty() {
            return None;
        }
        // Statically true conjuncts (`and $shouldContinue …` where the
        // local's reduction is `True`) carry nothing: dropping them keeps
        // the remaining conjunct in its exact single-predicate shape.
        predicates.retain(|predicate| !matches!(predicate, Predicate::True));
        Some(Predicate::all(predicates))
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
            TemplateExpr::Call { function, args }
                if function == "regexMatch" || function == "mustRegexMatch" =>
            {
                self.regex_match_predicate(args).map(|p| p.negated())
            }
            TemplateExpr::Call { function, args } if function == "has" => self
                .helper_literal_membership_predicate(args)
                .map(|predicate| predicate.negated())
                .or_else(|| {
                    self.single_truthy_predicate(arg)
                        .map(|predicate| predicate.negated())
                }),
            TemplateExpr::Call { function, args } if function == "and" => {
                self.and_predicate(args).map(|p| p.negated())
            }
            _ => {
                if let Some(predicate) = self.root_field_truthy_predicate(arg) {
                    return Some(predicate.negated());
                }
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
        // A literal key, or a variable statically bound to one string (an
        // unrolled traversal's `$elem`).
        let key = match key.deparen() {
            TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) => key.clone(),
            key => self.constant_scalar(key)?,
        };
        self.with_body_fragment_value_expr(map)
            .and_then(|value| value_has_key(&value, &key))
    }

    /// `regexMatch pattern subject` over a literal pattern and one
    /// values-backed subject is a pattern test on the path. `regexMatch`
    /// type-asserts a string subject, so the guard holding also implies
    /// string-ness; its NEGATION stays a raw predicate for fail lowering.
    fn regex_match_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [pattern, subject] = args else {
            return None;
        };
        let pattern = literal_string(pattern)?;
        let path = match self.with_body_fragment_value_expr(subject)? {
            AbstractValue::ValuesPath(path) if !path.is_empty() => path,
            _ => return None,
        };
        // A subject that reached this consumer through `tpl` carries its
        // rendered OUTPUT here, not the raw program: the pattern then
        // constrains the render, and a raw value carrying a template action
        // is admitted (redis-ha `masterGroupName: "{{ .Release.Name }}"`).
        // The string contract from `tpl`'s input assertion still stands.
        let templated = self.subject_is_derived_text(subject, &path);
        Some(Predicate::from(Guard::MatchesPattern {
            path,
            pattern: pattern.to_string(),
            templated,
        }))
    }

    fn contains_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [needle, subject] = args else {
            return None;
        };
        let needle = literal_string(needle)?;
        let path = match self.with_body_fragment_value_expr(subject)? {
            AbstractValue::ValuesPath(path) if !path.is_empty() => path,
            _ => return None,
        };
        Some(Predicate::from(Guard::MatchesPattern {
            path,
            pattern: escape_regex_literal(needle),
            templated: false,
        }))
    }

    /// Whether the subject expression's resolved `path` reached this site
    /// through a derived-text transform (`tpl`, a stringification) recorded
    /// on a bound local's output metadata.
    fn subject_is_derived_text(&self, subject: &TemplateExpr, path: &str) -> bool {
        let TemplateExpr::Variable(name) = subject.deparen() else {
            return false;
        };
        let name = name.trim_start_matches('$');
        self.template_output_meta
            .get(name)
            .and_then(|by_path| by_path.get(path))
            .is_some_and(|meta| meta.derived_text || meta.shape_erased)
    }

    /// The single string a statically known scalar expression denotes:
    /// folded literal members, unrolled iteration bindings, and constant
    /// `len`/`add1` results. Values-backed reads never qualify.
    fn constant_scalar(&self, expr: &TemplateExpr) -> Option<String> {
        let value = eval_expr(expr, &self.expression_eval_env()).value?;
        let AbstractValue::StringSet(strings) = value else {
            return None;
        };
        let mut strings = strings.iter();
        match (strings.next(), strings.next()) {
            (Some(text), None) => Some(text.clone()),
            _ => None,
        }
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
        let type_name = match args.first()?.deparen() {
            TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name)) => name,
            _ => return None,
        };
        let schema_type = type_is_schema_type(args.first());
        if type_name != "invalid" && schema_type.is_none() {
            return None;
        }
        let predicates = args
            .iter()
            .skip(1)
            .flat_map(|arg| self.paths_for_expr(arg))
            .map(|path| match &schema_type {
                Some(schema_type) => Predicate::from(Guard::TypeIs {
                    path,
                    schema_type: schema_type.clone(),
                }),
                None => invalid_kind_predicate(path),
            })
            .collect::<Vec<_>>();
        (!predicates.is_empty()).then(|| Predicate::all(predicates))
    }

    /// `eq (include "mode" .) "literal"`: when the called helper is a pure
    /// LITERAL DISPATCH invoked with a root-carrying context, the
    /// comparison selects exactly the arms returning that literal — the
    /// predicate is the any-of of their branch conditions, each conjoined
    /// with the negations of the arms before them (the chain is ordered
    /// and exclusive). Every arm header must decode faithfully, or the
    /// comparison abstains: a degraded arm would make those negations
    /// select states the helper never maps to the literal.
    fn helper_literal_dispatch_predicate(
        &self,
        left: &TemplateExpr,
        right: &TemplateExpr,
        negated: bool,
    ) -> Option<Predicate> {
        let (name, target) = match (helper_root_call(left), helper_root_call(right)) {
            (Some(name), None) => (name, literal_string(right)?),
            (None, Some(name)) => (name, literal_string(left)?),
            _ => return None,
        };
        // The dispatch helper reads `.Values.*` absolutely, so its
        // conditions keep their meaning only under a root dot.
        if !self
            .current_dot_binding
            .as_ref()
            .is_none_or(|dot| matches!(dot, AbstractValue::RootContext))
        {
            return None;
        }
        if HELPER_DISPATCH_DEPTH.with(std::cell::Cell::get) >= MAX_HELPER_DISPATCH_DEPTH {
            return None;
        }
        let arms = crate::helper_literal_dispatch::helper_literal_dispatch(
            self.fragment_context.analysis_db,
            name,
        )?;
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
        let predicate = self.literal_dispatch_arms_predicate(&arms, target);
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() - 1));
        let predicate = predicate?;
        Some(if negated {
            predicate.negated()
        } else {
            predicate
        })
    }

    fn helper_literal_membership_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [needle, haystack] = args else {
            return None;
        };
        let targets: Vec<String> = match haystack.deparen() {
            TemplateExpr::Call { function, args } if function == "list" && !args.is_empty() => args
                .iter()
                .map(literal_membership_string)
                .collect::<Option<Vec<_>>>()?,
            TemplateExpr::Variable(name) => {
                let AbstractValue::List(items) = self
                    .template_bindings
                    .get(name)
                    .or_else(|| self.template_bindings.get(name.trim_start_matches('$')))?
                else {
                    return None;
                };
                let mut targets = Vec::new();
                for item in items {
                    let strings = item.strings();
                    if strings.is_empty() {
                        return None;
                    }
                    targets.extend(strings);
                }
                targets
            }
            _ => return None,
        };
        if targets.is_empty() {
            return Some(Predicate::False);
        }

        if let TemplateExpr::Call { function, args } = needle.deparen()
            && function == "quote"
            && let [subject] = args.as_slice()
            && targets
                .iter()
                .all(|target| target.is_empty() || target == "\"\"")
            && targets.iter().any(|target| target == "\"\"")
        {
            let mut paths = self.paths_for_expr(subject).into_iter();
            let path = paths.next()?;
            if paths.next().is_some() {
                return None;
            }
            return Some(Predicate::Or(vec![
                Predicate::from(Guard::Absent { path: path.clone() }),
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: GuardValue::Null,
                }),
                Predicate::from(Guard::Eq {
                    path,
                    value: GuardValue::string(""),
                }),
            ]));
        }

        if helper_root_call(needle).is_none() {
            if !matches!(
                needle.deparen(),
                TemplateExpr::Field(_) | TemplateExpr::Selector { .. } | TemplateExpr::Variable(_)
            ) {
                return None;
            }
            let mut paths = self.paths_for_expr(needle).into_iter();
            let path = paths.next()?;
            if paths.next().is_some() {
                return None;
            }
            let predicates = targets
                .into_iter()
                .map(|target| {
                    Predicate::from(Guard::Eq {
                        path: path.clone(),
                        value: GuardValue::string(target),
                    })
                })
                .collect();
            return Some(predicate_any(predicates));
        }

        let name = helper_root_call(needle)?;
        if !self
            .current_dot_binding
            .as_ref()
            .is_none_or(|dot| matches!(dot, AbstractValue::RootContext))
            || HELPER_DISPATCH_DEPTH.with(std::cell::Cell::get) >= MAX_HELPER_DISPATCH_DEPTH
        {
            return None;
        }
        let arms = crate::helper_literal_dispatch::helper_literal_dispatch(
            self.fragment_context.analysis_db,
            name,
        )?;
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
        let predicates = targets
            .into_iter()
            .map(|target| self.literal_dispatch_arms_predicate(&arms, &target))
            .collect::<Option<Vec<_>>>();
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() - 1));
        let mut predicates = predicates?;
        predicates.retain(|predicate| !matches!(predicate, Predicate::False));
        match predicates.as_slice() {
            [] => Some(Predicate::False),
            [predicate] => Some(predicate.clone()),
            _ => Some(Predicate::Or(predicates)),
        }
    }

    fn literal_dispatch_arms_predicate(
        &self,
        arms: &[crate::helper_literal_dispatch::LiteralDispatchArm],
        target: &str,
    ) -> Option<Predicate> {
        let mut prior: Vec<Predicate> = Vec::new();
        let mut matching: Vec<Predicate> = Vec::new();
        for arm in arms {
            match &arm.header {
                Some(header) => {
                    if !self.condition_lowering_is_faithful(header.expr()) {
                        return None;
                    }
                    let condition = self.condition_predicate_expr(header.expr());
                    if arm.literal == target {
                        let mut conjuncts: Vec<Predicate> =
                            prior.iter().map(Predicate::negated).collect();
                        conjuncts.push(condition.clone());
                        matching.push(Predicate::all(conjuncts));
                    }
                    prior.push(condition);
                }
                None => {
                    if arm.literal == target {
                        matching.push(Predicate::all(
                            prior.iter().map(Predicate::negated).collect(),
                        ));
                    }
                }
            }
        }
        Some(match matching.len() {
            // No arm renders the literal: the dispatch is total, so the
            // comparison can never hold.
            0 => Predicate::False,
            1 => matching.remove(0),
            _ => Predicate::Or(matching),
        })
    }

    fn truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        if let TemplateExpr::Literal(literal) = expr.deparen() {
            return Some(bool_predicate(literal_is_truthy(literal)));
        }
        if let Some(predicate) = self.root_field_truthy_predicate(expr) {
            return Some(predicate);
        }
        if let TemplateExpr::Variable(name) = expr.deparen()
            && let Some(predicate) = self.template_truthy_reductions.get(name).or_else(|| {
                self.template_truthy_reductions
                    .get(name.trim_start_matches('$'))
            })
        {
            return Some(predicate.clone());
        }
        if let TemplateExpr::Variable(name) = expr.deparen()
            && let Some(predicate) = self.get_binding_truthy_predicate(name)
        {
            return Some(predicate);
        }
        let paths = self.paths_for_expr(expr);
        if paths.is_empty() {
            return None;
        }
        Some(Predicate::all(
            paths.into_iter().map(Predicate::truthy_path).collect(),
        ))
    }

    fn root_field_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let explicit_root = matches!(self.current_dot_binding, Some(AbstractValue::RootContext));
        if !self
            .current_dot_binding
            .as_ref()
            .is_none_or(|dot| matches!(dot, AbstractValue::RootContext))
        {
            return None;
        }
        let field = match expr.deparen() {
            TemplateExpr::Field(path) => path.as_slice(),
            TemplateExpr::Selector { operand, path } if matches!(operand.as_ref(), TemplateExpr::Variable(variable) if variable.is_empty()) => {
                path.as_slice()
            }
            _ => return None,
        };
        let [field] = field else {
            return None;
        };
        if let Some(predicate) = self.root_truthy_predicates.get(field) {
            return Some(predicate.clone());
        }
        if self.root_bindings.contains_key(field)
            || matches!(
                field.as_str(),
                "Capabilities"
                    | "Chart"
                    | "Files"
                    | "Release"
                    | "Subcharts"
                    | "Template"
                    | "Values"
            )
        {
            return None;
        }

        // Helm's explicit root context has a fixed set of built-in fields. A
        // custom field is absent until a tracked `set` mutation creates it.
        // An unresolved dot is not proof of root identity and must abstain.
        explicit_root.then_some(Predicate::False)
    }

    fn get_binding_truthy_predicate(&self, name: &str) -> Option<Predicate> {
        let binding = self.get_bindings.get(name.trim_start_matches('$'))?;
        let keys = self.range_domains.get(&binding.key_var)?;
        let predicates = keys
            .iter()
            .map(|key| {
                Predicate::truthy_path(helm_schema_core::append_value_path(&binding.base, key))
            })
            .collect::<Vec<_>>();
        match predicates.as_slice() {
            [] => None,
            [predicate] => Some(predicate.clone()),
            _ => Some(Predicate::Or(predicates)),
        }
    }

    /// `gt (int64 x) N` / `gt (int x) N` (and the flipped `lt N (…)`) with
    /// an integer literal bound and one values-backed subject admits a
    /// bounded sound strengthening: a RAW JSON integer above the bound
    /// always satisfies the coercing comparison, so a fail-arm keyed on the
    /// strengthened guard keeps firing there instead of abstaining
    /// wholesale (F86: redis `gt (int64 .Values.master.count) 0`).
    fn int_cast_comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        let [left, right] = args.as_slice() else {
            return Vec::new();
        };
        let (cast_expr, bound) = match (function.as_str(), left.deparen(), right.deparen()) {
            ("gt", cast, TemplateExpr::Literal(Literal::Int(bound))) => (cast, *bound),
            ("lt", TemplateExpr::Literal(Literal::Int(bound)), cast) => (cast, *bound),
            _ => return Vec::new(),
        };
        let TemplateExpr::Call { function, args } = cast_expr else {
            return Vec::new();
        };
        if !matches!(function.as_str(), "int" | "int64") || args.len() != 1 {
            return Vec::new();
        }
        let Some(Predicate::Guard(Guard::Truthy { path })) = self.single_truthy_predicate(&args[0])
        else {
            return Vec::new();
        };
        vec![Guard::IntGt { path, bound }]
    }

    fn single_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        if let TemplateExpr::Variable(name) = expr.deparen()
            && let Some(predicate) = self.template_truthy_reductions.get(name).or_else(|| {
                self.template_truthy_reductions
                    .get(name.trim_start_matches('$'))
            })
        {
            return Some(predicate.clone());
        }
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
        // `eq $key "name"` over a destructured range key selects exactly the
        // member with that key (F53: prometheus's serverFiles dispatch).
        if let Some(predicate) = self.range_key_equals_predicate(left, right, negated) {
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
            _ => {
                // Two statically known scalars compare as a constant:
                // `eq (len $secret.path) (add1 $index)` selects the last
                // element of an exactly unrolled iteration.
                if let (Some(left_value), Some(right_value)) =
                    (self.constant_scalar(left), self.constant_scalar(right))
                {
                    return Some(bool_predicate((left_value == right_value) != negated));
                }
                return self.helper_literal_dispatch_predicate(left, right, negated);
            }
        };
        let comparison_for = |path: String| {
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
        };
        if let TemplateExpr::Variable(name) = if guard_value_literal(left).is_some() {
            right.deparen()
        } else {
            left.deparen()
        } {
            let meta = self
                .template_output_meta
                .get(name)
                .or_else(|| self.template_output_meta.get(name.trim_start_matches('$')));
            // A binding qualified by lexical escape tokens is not the raw
            // value for every input (a replace/split chain rewrote some
            // strings, F74): an equality on it cannot lower to a raw-path
            // guard.
            if meta.is_some_and(|meta| {
                meta.values()
                    .any(|candidate| !candidate.lexical_escapes.is_empty())
            }) {
                return None;
            }
            if let Some(meta) = meta.filter(|meta| {
                meta.values()
                    .any(|candidate| !candidate.predicates.is_empty())
            }) {
                let mut alternatives = Vec::new();
                for path in &paths {
                    let comparison = comparison_for(path.clone());
                    match meta.get(path) {
                        Some(candidate) if !candidate.predicates.is_empty() => {
                            for branch in &candidate.predicates {
                                let mut conjuncts = branch.iter().cloned().collect::<Vec<_>>();
                                conjuncts.push(comparison.clone());
                                alternatives.push(Predicate::all(conjuncts));
                            }
                        }
                        _ => alternatives.push(comparison),
                    }
                }
                return Some(match alternatives.as_slice() {
                    [only] => only.clone(),
                    _ => Predicate::Or(alternatives),
                });
            }
        }
        let predicates = paths
            .iter()
            .cloned()
            .map(comparison_for)
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
        let described_sources = |expr: &TemplateExpr| -> Option<
            std::collections::BTreeMap<String, crate::helper_meta::HelperOutputMeta>,
        > {
            match expr.deparen() {
                TemplateExpr::Call { function, args }
                    if matches!(function.as_str(), "typeOf" | "kindOf") && args.len() == 1 =>
                {
                    // Selectors and bound locals (a range's value variable,
                    // a `$x := .Values.y` binding) both describe a single
                    // resolvable path.
                    let subject = args[0].deparen();
                    let subject = matches!(
                        subject,
                        TemplateExpr::Field(_)
                            | TemplateExpr::Selector { .. }
                            | TemplateExpr::Variable(_)
                    )
                    .then_some(subject)?;
                    let paths = self.paths_for_expr(subject);
                    if paths.is_empty() {
                        return None;
                    }
                    let local_meta = match subject {
                        TemplateExpr::Variable(name) => {
                            self.template_output_meta.get(name.trim_start_matches('$'))
                        }
                        _ => None,
                    };
                    Some(
                        paths
                            .into_iter()
                            .map(|path| {
                                let meta = local_meta
                                    .and_then(|meta| meta.get(&path))
                                    .cloned()
                                    .unwrap_or_default();
                                (path, meta)
                            })
                            .collect(),
                    )
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
        let (sources, type_name) = match (described_sources(left), described_sources(right)) {
            (Some(sources), None) => (sources, type_literal(right)?),
            (None, Some(sources)) => (sources, type_literal(left)?),
            _ => return None,
        };
        let schema_type = helm_schema_ast::go_type_schema_type(type_name);
        if type_name != "invalid" && schema_type.is_none() {
            return None;
        }
        let mut alternatives = Vec::new();
        for (path, meta) in sources {
            let type_predicate = match schema_type {
                Some(schema_type) => Predicate::from(Guard::TypeIs {
                    path,
                    schema_type: schema_type.to_string(),
                }),
                None => invalid_kind_predicate(path),
            };
            let type_predicate = if negated {
                type_predicate.negated()
            } else {
                type_predicate
            };
            if meta.predicates.is_empty() {
                alternatives.push(type_predicate);
            } else {
                alternatives.extend(meta.predicates.into_iter().map(|branch| {
                    let mut conjunction = branch.into_iter().collect::<Vec<_>>();
                    conjunction.push(type_predicate.clone());
                    Predicate::all(conjunction)
                }));
            }
        }
        match alternatives.as_slice() {
            [] => None,
            [predicate] => Some(predicate.clone()),
            _ => Some(Predicate::Or(alternatives)),
        }
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

fn invalid_kind_predicate(path: String) -> Predicate {
    Predicate::Or(vec![
        Predicate::from(Guard::Absent { path: path.clone() }),
        Predicate::from(Guard::Eq {
            path,
            value: GuardValue::Null,
        }),
    ])
}

fn predicate_any(predicates: Vec<Predicate>) -> Predicate {
    if predicates
        .iter()
        .any(|predicate| matches!(predicate, Predicate::True))
    {
        return Predicate::True;
    }
    let mut predicates = predicates
        .into_iter()
        .filter(|predicate| !matches!(predicate, Predicate::False))
        .collect::<Vec<_>>();
    match predicates.len() {
        0 => Predicate::False,
        1 => predicates.remove(0),
        _ => Predicate::Or(predicates),
    }
}

fn literal_is_truthy(literal: &Literal) -> bool {
    match literal {
        Literal::Bool(value) => *value,
        Literal::Int(value) => *value != 0,
        Literal::Float(value) => *value != 0.0,
        Literal::String(value) | Literal::RawString(value) => !value.is_empty(),
        Literal::Nil => false,
    }
}

fn escape_regex_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '\\'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
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
        AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => Some(
            Predicate::from(Guard::Absent {
                path: helm_schema_core::append_value_path(path, key),
            })
            .negated(),
        ),
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::List(_)
        | AbstractValue::SplitList { .. }
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
