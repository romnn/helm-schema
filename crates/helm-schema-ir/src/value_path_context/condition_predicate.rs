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

/// The typed guard value of a plain scalar literal. Floats abstain: their
/// file-vs-`--set` numeric channels compare differently, so an equality
/// target would overstate what the analysis knows.
fn literal_guard_value(expr: &TemplateExpr) -> Option<GuardValue> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(GuardValue::string(value.clone()))
        }
        TemplateExpr::Literal(Literal::Bool(value)) => Some(GuardValue::Bool(*value)),
        TemplateExpr::Literal(Literal::Int(value)) => Some(GuardValue::Int(*value)),
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
                "semverCompare" => self.semver_capabilities_predicate(args).is_some(),
                "gt" | "lt" | "ge" | "le" => self.positive_len_predicate(function, args).is_some(),
                "typeIs" | "kindIs" => self.type_is_predicate(args).is_some(),
                "hasKey" => self.has_key_predicate(args).is_some(),
                "hasPrefix" => {
                    self.range_key_prefix_predicate(args).is_some()
                        || self.string_affix_predicate(args, false).is_some()
                }
                "hasSuffix" => self.string_affix_predicate(args, true).is_some(),
                "contains" => self.contains_predicate(args).is_some(),
                "has" => self.helper_literal_membership_predicate(args).is_some(),
                "empty" => self.empty_predicate(args).is_some(),
                "coalesce" => self.coalesce_truthy_predicate(args).is_some(),
                "default" => self.default_truthy_predicate(args).is_some(),
                "dig" => self.dig_truthy_predicate(args).is_some(),
                "toString" => args.len() == 1 && self.tostring_truthy_predicate(&args[0]).is_some(),
                "regexMatch" | "mustRegexMatch" => self.regex_match_predicate(args).is_some(),
                "include" => self.include_truthy_predicate(expr).is_some(),
                function if is_files_get_function(function) => {
                    self.files_get_printf_predicate(args).is_some()
                }
                _ => false,
            },
            TemplateExpr::Pipeline(stages) => {
                self.default_pipeline_truthy_predicate(stages).is_some()
                    || self.tostring_pipeline_truthy_predicate(stages).is_some()
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
        self.approximate_condition_parts(expr, marker, false)
    }

    /// One node of an approximately-lowered condition, decomposed as far as
    /// the Boolean structure allows.
    ///
    /// `and`/`or` decompose per argument so exact conjuncts survive beside
    /// undecodable siblings, recursing into nested connectives (cilium's
    /// `or (and (ge …) (le …)) (and (ge …) (le …))` cluster-id window):
    /// a fail-arm consumer can then strengthen the residue soundly (drop
    /// approximate disjuncts, or negate only the decodable conjuncts)
    /// instead of abstaining on one opaque blob. `not` distributes by
    /// De Morgan — each negated leaf either decodes exactly and negates,
    /// or carries the region-flipped comparison subset (cilium's
    /// `not (and (ge (int …) 0) (le (int …) 4294967295))` baseID domain).
    fn approximate_condition_parts(
        &self,
        expr: &TemplateExpr,
        marker: &str,
        negated: bool,
    ) -> Predicate {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            let leaf = Predicate::approximate(marker, self.resolved_values_paths_from_expr(expr));
            // The stand-in covers the unknown condition in BOTH polarities:
            // an Approximate marker is never negated downstream, so the
            // negated leaf keeps the same abstention.
            return leaf;
        };
        match function.as_str() {
            // Under negation, De Morgan swaps the connective.
            "and" | "or" => {
                let parts: Vec<Predicate> = args
                    .iter()
                    .enumerate()
                    .map(|(index, arg)| {
                        if !negated && self.condition_lowering_is_faithful(arg) {
                            self.condition_predicate_expr(arg)
                        } else if negated
                            && self.condition_lowering_is_faithful(arg)
                            && let Some(exact) = self.condition_predicate(arg)
                        {
                            exact.negated()
                        } else {
                            self.approximate_condition_parts(
                                arg,
                                &format!("{marker}:{index}"),
                                negated,
                            )
                        }
                    })
                    .collect();
                if (function == "and") != negated {
                    Predicate::all(parts)
                } else {
                    predicate_any(parts)
                }
            }
            "not" if args.len() == 1 => {
                // The whole negated atom may decode as its own subset —
                // negated literal membership rides the full
                // `not (list … | has X)` shape — before De Morgan descends.
                if !negated {
                    let whole = self.comparison_sound_subset(expr);
                    if !whole.is_empty() {
                        return Predicate::approximate_with_sound_subset(
                            marker,
                            self.resolved_values_paths_from_expr(expr),
                            whole,
                        );
                    }
                }
                self.approximate_condition_parts(&args[0], &format!("{marker}:!"), !negated)
            }
            _ => Predicate::approximate_with_sound_subset(
                marker,
                self.resolved_values_paths_from_expr(expr),
                if negated {
                    self.negated_comparison_sound_subset(expr)
                } else {
                    self.comparison_sound_subset(expr)
                },
            ),
        }
    }

    pub(crate) fn with_condition_predicate_expr(&self, expr: &TemplateExpr) -> Predicate {
        Predicate::all(
            self.condition_predicate_expr(expr)
                .with_context_predicates(),
        )
    }

    fn condition_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        if let TemplateExpr::Pipeline(stages) = expr.deparen() {
            return self
                .default_pipeline_truthy_predicate(stages)
                .or_else(|| self.tostring_pipeline_truthy_predicate(stages));
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
            "hasPrefix" => self
                .range_key_prefix_predicate(args)
                .or_else(|| self.string_affix_predicate(args, false)),
            "hasSuffix" => self.string_affix_predicate(args, true),
            "contains" => self.contains_predicate(args),
            "has" => self
                .helper_literal_membership_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            "or" => self.or_predicate(args),
            "eq" => self.value_comparison_predicate(args, false),
            "ne" => self.value_comparison_predicate(args, true),
            "semverCompare" => self.semver_capabilities_predicate(args),
            "gt" | "lt" | "ge" | "le" => self.positive_len_predicate(function, args),
            "typeIs" | "kindIs" => self.type_is_predicate(args),
            "coalesce" => self.coalesce_truthy_predicate(args),
            "default" => self.default_truthy_predicate(args),
            "dig" => self
                .dig_truthy_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            "toString" if args.len() == 1 => self
                .tostring_truthy_predicate(&args[0])
                .or_else(|| self.truthy_predicate(expr)),
            "regexMatch" | "mustRegexMatch" => self.regex_match_predicate(args),
            "include" => self
                .include_truthy_predicate(expr)
                .or_else(|| self.truthy_predicate(expr)),
            function if is_files_get_function(function) => self
                .files_get_printf_predicate(args)
                .or_else(|| self.truthy_predicate(expr)),
            _ => self.truthy_predicate(expr),
        }
    }

    /// Truthiness of a total stringification (`toString X` /
    /// `X | toString`) tests the RENDERED text against the empty string,
    /// never the raw value's own Helm truthiness: `"false"`, `"0"`, and
    /// `"<nil>"` are all truthy strings (cilium's removed-option guards
    /// rely on exactly that to abort on explicitly-disabled options).
    ///
    /// Two subjects decode exactly:
    ///
    /// - a literal-key `dig` with an EMPTY-STRING literal default: a
    ///   missing chain renders the empty default (falsy), so the guard
    ///   holds exactly when the leaf is present with any value other than
    ///   the empty string — explicit null renders `"<nil>"`, which is
    ///   truthy;
    /// - a direct selector: an absent or null subject renders `"<nil>"`
    ///   (truthy), so only the raw empty string is falsy.
    fn tostring_truthy_predicate(&self, subject: &TemplateExpr) -> Option<Predicate> {
        if let TemplateExpr::Call { function, args } = subject.deparen()
            && function == "dig"
        {
            let (map, rest) = args.split_last()?;
            let (default, key_exprs) = rest.split_last()?;
            if key_exprs.is_empty() {
                return None;
            }
            let keys = key_exprs
                .iter()
                .map(literal_string)
                .collect::<Option<Vec<_>>>()?;
            // Only the empty-string default keeps "missing renders falsy"
            // exact: any other falsy literal (`false`, `0`) stringifies to
            // truthy text, flipping the absent case.
            if !literal_string(default)?.is_empty() {
                return None;
            }
            let base = match self.with_body_fragment_value_expr(map)? {
                AbstractValue::ValuesPath(base) => base,
                _ => return None,
            };
            let (leaf_key, parent_keys) = keys.split_last()?;
            let parent = parent_keys.iter().fold(base, |path, key| {
                helm_schema_core::append_value_path(&path, key)
            });
            let path = helm_schema_core::append_value_path(&parent, leaf_key);
            // `HasKey` (not `¬Absent`): dig OBSERVES a present nil member,
            // which renders as truthy "<nil>".
            return Some(Predicate::all(vec![
                Predicate::from(Guard::HasKey {
                    path: parent,
                    key: (*leaf_key).to_string(),
                }),
                Predicate::from(Guard::NotEq {
                    path,
                    value: GuardValue::string(""),
                }),
            ]));
        }
        if matches!(
            subject.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            let path = self.single_resolved_values_path_expr(subject)?;
            return Some(Predicate::from(Guard::NotEq {
                path,
                value: GuardValue::string(""),
            }));
        }
        None
    }

    /// The `X | toString` pipeline form of [`Self::tostring_truthy_predicate`].
    fn tostring_pipeline_truthy_predicate(&self, stages: &[TemplateExpr]) -> Option<Predicate> {
        let [primary, stage] = stages else {
            return None;
        };
        let TemplateExpr::Call { function, args } = stage.deparen() else {
            return None;
        };
        (function == "toString" && args.is_empty())
            .then(|| self.tostring_truthy_predicate(primary))
            .flatten()
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
        let (subject, bound) = match (function, left.deparen(), right.deparen()) {
            ("gt", subject, TemplateExpr::Literal(Literal::Int(bound))) => (subject, *bound),
            ("lt", TemplateExpr::Literal(Literal::Int(bound)), subject) => (subject, *bound),
            // The inclusive forms normalize to the strict bound below them.
            ("ge", subject, TemplateExpr::Literal(Literal::Int(bound)))
            | ("le", TemplateExpr::Literal(Literal::Int(bound)), subject) => {
                (subject, bound.checked_sub(1)?)
            }
            _ => return None,
        };
        if bound == 0
            && let TemplateExpr::Call {
                function,
                args: len_args,
            } = subject
            && function == "len"
            && let [len_subject] = len_args.as_slice()
        {
            return self.single_truthy_predicate(len_subject);
        }
        // `gt (keys X | len) N` is an exact member-count bound: `keys`
        // aborts rendering on non-maps, so the body runs exactly for
        // mappings with more than N members (external-secrets'
        // `gt (keys . | len) 1` securityContext gate).
        if bound >= 0
            && let Some(map_expr) = keys_len_subject(subject)
            && let Some(path) = self.single_resolved_values_path_expr(map_expr)
        {
            return Some(Predicate::from(Guard::MinMembers {
                path,
                bound: bound.checked_add(1)?,
            }));
        }
        None
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
                // De Morgan over EXACT per-disjunct decodes: when every
                // disjunct lowers faithfully, each negates precisely and
                // the conjunction stays flat for guard extraction —
                // cilium's `not (or (eq … "Cluster") (eq … "Local"))`
                // traffic-policy gate needs the equality enum, not the
                // truthiness weakening the fallback lowers to. A truthy
                // stand-in for an undecodable disjunct must never be
                // negated, hence the faithfulness gate.
                if args
                    .iter()
                    .all(|arg| self.condition_lowering_is_faithful(arg))
                    && let Some(negated) = args
                        .iter()
                        .map(|arg| {
                            self.condition_predicate(arg)
                                .map(|predicate| predicate.negated())
                        })
                        .collect::<Option<Vec<_>>>()
                {
                    return Some(Predicate::all(negated));
                }
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
            // `not (toString X)` / `not (X | toString)` negates the
            // RENDERING test; the raw-truthiness fallback below would fire
            // on raw false, which stringifies truthy. An undecodable
            // stringification abstains for the same reason.
            TemplateExpr::Call { function, args } if function == "toString" && args.len() == 1 => {
                self.tostring_truthy_predicate(&args[0])
                    .map(|predicate| predicate.negated())
            }
            TemplateExpr::Pipeline(stages)
                if matches!(
                    stages.last().map(TemplateExpr::deparen),
                    Some(TemplateExpr::Call { function, args })
                        if function == "toString" && args.is_empty()
                ) =>
            {
                self.tostring_pipeline_truthy_predicate(stages)
                    .map(|predicate| predicate.negated())
            }
            // A local's negation lowers through its stored truthy
            // reduction — the same trust the positive Variable lowering
            // extends (`not $stateful` selecting airflow's Deployment
            // arm). Only approximation-free reductions qualify: negating
            // an approximate marker would fire in states the marker never
            // described. The path fallback below would mint a truthy
            // stand-in over the flowing paths, which negation must not do.
            TemplateExpr::Variable(name)
                if self
                    .variable_truthy_reduction(name)
                    .is_some_and(|reduction| !reduction.contains_approximation()) =>
            {
                self.variable_truthy_reduction(name).map(Predicate::negated)
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
                // A grouped selector chain reads the receiver AND the leaf
                // (`($config.http).tls.enabled` yields `…http` beside
                // `…http.tls.enabled`). On an ancestor CHAIN the leaf's
                // truthiness is the exact conjunction — a truthy leaf makes
                // every ancestor a nonempty container — so the negation is
                // the leaf's falsiness. Unrelated path sets still abstain.
                if let Some(leaf) = ancestor_chain_leaf(&paths) {
                    return Some(Predicate::truthy_path(leaf).negated());
                }
                None
            }
        }
    }

    fn variable_truthy_reduction(&self, name: &str) -> Option<&Predicate> {
        self.template_truthy_reductions.get(name).or_else(|| {
            self.template_truthy_reductions
                .get(name.trim_start_matches('$'))
        })
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
        if let Some(predicate) = self.type_descriptor_regex_predicate(pattern, subject) {
            return Some(predicate);
        }
        let path = match self.with_body_fragment_value_expr(subject)? {
            AbstractValue::ValuesPath(path) if !path.is_empty() => path,
            // The subject is a destructured range KEY: the pattern applies
            // per key of the ranged collection (traefik's uppercase
            // `ingressRoute` gate).
            AbstractValue::RangeKey(collection) if !collection.is_empty() => {
                return Some(Predicate::from(Guard::RangeKeyMatches {
                    path: collection,
                    pattern: pattern.to_string(),
                }));
            }
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

    /// `hasPrefix`/`hasSuffix` over a literal affix and a values-path
    /// subject is an anchored literal pattern test on the value's rendered
    /// text (datadog's `hasPrefix "unix:" .` OTLP endpoint terminal, where
    /// the dot is the caller's bound endpoint scalar).
    fn string_affix_predicate(&self, args: &[TemplateExpr], suffix: bool) -> Option<Predicate> {
        let [affix, subject] = args else {
            return None;
        };
        let affix = literal_string(affix)?;
        let path = match self.with_body_fragment_value_expr(subject)? {
            AbstractValue::ValuesPath(path) if !path.is_empty() => path,
            _ => return None,
        };
        let templated = self.subject_is_derived_text(subject, &path);
        let pattern = if suffix {
            format!("{}$", escape_regex_literal(affix))
        } else {
            format!("^{}", escape_regex_literal(affix))
        };
        Some(Predicate::from(Guard::MatchesPattern {
            path,
            pattern,
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
                // An exactly decodable pipeline disjunct (`… | toString`)
                // must not collapse to raw truthiness: the RENDERING's
                // truthiness differs from the value's (cilium's
                // removed-option guards abort on a truthy "false").
                if matches!(arg.deparen(), TemplateExpr::Pipeline(_))
                    && let Some(exact) = self.condition_predicate(arg)
                {
                    alternatives.push(exact);
                    continue;
                }
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
        let predicate = self.literal_dispatch_arms_predicate(&arms, &|arm| arm.literal == target);
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() - 1));
        let predicate = predicate?;
        Some(if negated {
            predicate.negated()
        } else {
            predicate
        })
    }

    /// A bare `include "name" .` used AS a condition is truthy exactly when
    /// the called helper emits non-empty text. For a pure literal dispatch
    /// under a root context, the truthy states are the arms with non-empty
    /// output, each conjoined with its prior arms' negations (redis'
    /// `if (include "redis.createConfigmap" .)` document gate reduces to
    /// `empty .Values.existingConfigmap`). An arm whose trimmed literal is
    /// empty but which still collected whitespace abstains: its render
    /// truthiness would depend on the chain's trim markers.
    fn include_truthy_predicate(&self, expr: &TemplateExpr) -> Option<Predicate> {
        let name = helper_root_call(expr)?;
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
        if arms
            .iter()
            .any(|arm| arm.literal.is_empty() && !arm.raw_empty)
        {
            return None;
        }
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() + 1));
        let predicate = self.literal_dispatch_arms_predicate(&arms, &|arm| !arm.literal.is_empty());
        HELPER_DISPATCH_DEPTH.with(|depth| depth.set(depth.get() - 1));
        predicate
    }

    fn helper_literal_membership_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [needle, haystack] = args else {
            return None;
        };
        // `has X (list L1 L2 …)` over a direct selector with typed scalar
        // literals is EXACTLY "X equals one of the literals" (Sprig `has`
        // is deep equality per item). Typed targets keep non-string
        // literals precise: airflow probes explicit Boolean flags with
        // `has .Values.…enabled (list true false)`.
        if let TemplateExpr::Call {
            function,
            args: list_args,
        } = haystack.deparen()
            && matches!(function.as_str(), "list" | "tuple")
            && !list_args.is_empty()
            && matches!(
                needle.deparen(),
                TemplateExpr::Field(_) | TemplateExpr::Selector { .. } | TemplateExpr::Variable(_)
            )
            && let Some(values) = list_args
                .iter()
                .map(literal_guard_value)
                .collect::<Option<Vec<_>>>()
        {
            let mut paths = self.paths_for_expr(needle).into_iter();
            let path = paths.next()?;
            if paths.next().is_some() {
                return None;
            }
            return Some(predicate_any(
                values
                    .into_iter()
                    .map(|value| {
                        Predicate::from(Guard::Eq {
                            path: path.clone(),
                            value,
                        })
                    })
                    .collect(),
            ));
        }
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
            .map(|target| self.literal_dispatch_arms_predicate(&arms, &|arm| arm.literal == target))
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
        select: &dyn Fn(&crate::helper_literal_dispatch::LiteralDispatchArm) -> bool,
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
                    if select(arm) {
                        let mut conjuncts: Vec<Predicate> =
                            prior.iter().map(Predicate::negated).collect();
                        conjuncts.push(condition.clone());
                        matching.push(Predicate::all(conjuncts));
                    }
                    prior.push(condition);
                }
                None => {
                    if select(arm) {
                        matching.push(Predicate::all(
                            prior.iter().map(Predicate::negated).collect(),
                        ));
                    }
                }
            }
        }
        Some(match matching.len() {
            // No arm is selected: the dispatch is total, so the condition
            // can never hold.
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

    fn root_field_dispatch_predicate(
        &self,
        left: &TemplateExpr,
        right: &TemplateExpr,
        negated: bool,
    ) -> Option<Predicate> {
        let (field, value) = match (self.root_dispatch_field(left), guard_value_literal(right)) {
            (Some(field), Some(value)) => (field, value),
            _ => match (self.root_dispatch_field(right), guard_value_literal(left)) {
                (Some(field), Some(value)) => (field, value),
                _ => return None,
            },
        };
        let dispatch = self.root_value_dispatches.get(field)?;
        let selected = predicate_any(
            dispatch
                .arms
                .iter()
                .filter(|(_, literal)| *literal == value)
                .map(|(condition, _)| condition.clone())
                .collect(),
        );
        Some(if negated {
            selected.negated()
        } else {
            selected
        })
    }

    /// A single-segment root-context field (`.mode`) under a root (or
    /// unresolved) dot that carries a joined value dispatch.
    fn root_dispatch_field<'expr>(&self, expr: &'expr TemplateExpr) -> Option<&'expr str> {
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
        self.root_value_dispatches
            .contains_key(field)
            .then_some(field.as_str())
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

    /// Sound subsets of a comparison's NEGATION: the int-cast lanes
    /// region-flip exactly (¬(x > N) ⇔ x < N+1 over the coerced value,
    /// ¬(x == N) ⇔ x ≠ N, ¬(x ≠ N) ⇔ x == N); other comparison families
    /// abstain under negation.
    fn negated_comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let subset = self.int_cast_comparison_region_subset(expr, true);
        if !subset.is_empty() {
            return subset;
        }
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        match function.as_str() {
            "eq" => self.int_cast_not_equal_subset(args),
            "ne" => self.int_cast_equality_region(args),
            _ => Vec::new(),
        }
    }

    /// Sound positive strengthenings of otherwise-undecodable comparison
    /// conditions, tried in order of exactness.
    fn comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let subset = self.semver_comparison_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        let subset = self.int_cast_comparison_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        let subset = self.len_comparison_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        let subset = self.int_cast_inequality_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        let subset = self.int_cast_equality_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        let subset = self.include_dispatch_semver_sound_subset(expr);
        if !subset.is_empty() {
            return subset;
        }
        self.negated_membership_sound_subset(expr)
    }

    /// `eq (include "helper" .) "LIT"` over a pure literal dispatch whose
    /// non-else arms are `semverCompare "<C" (PATH | default
    /// .Capabilities.KubeVersion.Version)` upper bounds: selecting the
    /// ELSE arm's literal requires every bound to fail, and a PATH
    /// matching every flipped `>=C` constraint certainly fails them all —
    /// a sound subset usable in fail position (oauth2-proxy's
    /// `capabilities.ingress.apiVersion` gate on the legacy extraPaths
    /// abort). The capability-default lane stays out of the subset: with
    /// PATH unset the selection is cluster-dependent.
    fn include_dispatch_semver_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        if function != "eq" {
            return Vec::new();
        }
        let [left, right] = args.as_slice() else {
            return Vec::new();
        };
        let (name, target) = match (helper_root_call(left), helper_root_call(right)) {
            (Some(name), None) => (name, literal_string(right)),
            (None, Some(name)) => (name, literal_string(left)),
            _ => return Vec::new(),
        };
        let Some(target) = target else {
            return Vec::new();
        };
        if !self
            .current_dot_binding
            .as_ref()
            .is_none_or(|dot| matches!(dot, AbstractValue::RootContext))
        {
            return Vec::new();
        }
        if HELPER_DISPATCH_DEPTH.with(std::cell::Cell::get) >= MAX_HELPER_DISPATCH_DEPTH {
            return Vec::new();
        }
        let Some(arms) = crate::helper_literal_dispatch::helper_literal_dispatch(
            self.fragment_context.analysis_db,
            name,
        ) else {
            return Vec::new();
        };
        let Some((else_arm, bounded_arms)) = arms.split_last() else {
            return Vec::new();
        };
        if else_arm.header.is_some()
            || else_arm.literal != target
            || bounded_arms.iter().any(|arm| arm.literal == target)
        {
            return Vec::new();
        }
        let mut subject_path: Option<String> = None;
        let mut guards = Vec::new();
        for arm in bounded_arms {
            let Some(header) = &arm.header else {
                return Vec::new();
            };
            let TemplateExpr::Call { function, args } = header.expr().deparen() else {
                return Vec::new();
            };
            if function != "semverCompare" || args.len() != 2 {
                return Vec::new();
            }
            let TemplateExpr::Literal(Literal::String(constraint)) = args[0].deparen() else {
                return Vec::new();
            };
            // `<X-0` opts prereleases into Masterminds' matching; the
            // flipped subset pattern only ever matches RELEASE versions,
            // and a release ≥ X certainly fails `<X-0` too, so the
            // prerelease marker drops out of the flipped bound.
            let Some(flipped) = constraint
                .strip_prefix('<')
                .filter(|rest| !rest.starts_with('='))
                .map(|rest| format!(">={}", rest.strip_suffix("-0").unwrap_or(rest)))
            else {
                return Vec::new();
            };
            let Some(path) = self.single_resolved_values_path_expr(&args[1]) else {
                return Vec::new();
            };
            if subject_path.get_or_insert_with(|| path.clone()) != &path {
                return Vec::new();
            }
            let Some(pattern) = helm_schema_ast::semver_constraint_match_pattern(&flipped) else {
                return Vec::new();
            };
            guards.push(Guard::MatchesPattern {
                path,
                pattern,
                templated: false,
            });
        }
        guards
    }

    /// `gt (len .Values.x) N` admits a bounded string strengthening: a
    /// string of more than N CHARACTERS has more than N bytes, so it always
    /// satisfies Go's byte-length comparison (cilium's 32-character
    /// `cluster.name` bound). Long collections also satisfy the guard but
    /// have no pattern encoding, so they stay outside the subset. The
    /// inclusive comparators normalize by shifting the bound (traefik's
    /// `ge (len .Values.hub.token) 65` license gates).
    fn len_comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        let form = match (function.as_str(), args.as_slice()) {
            ("gt" | "ge", [left, right]) => match right.deparen() {
                TemplateExpr::Literal(Literal::Int(bound)) => {
                    Some((function, left.deparen(), *bound))
                }
                _ => None,
            },
            ("lt" | "le", [left, right]) => match left.deparen() {
                TemplateExpr::Literal(Literal::Int(bound)) => {
                    Some((function, right.deparen(), *bound))
                }
                _ => None,
            },
            _ => None,
        };
        let Some((function, len_expr, bound)) = form else {
            return Vec::new();
        };
        let bound = match function.as_str() {
            "gt" | "lt" => bound,
            // `ge (len x) N` ⇔ `gt (len x) (N-1)`; overflow abstains.
            _ => match bound.checked_sub(1) {
                Some(bound) => bound,
                None => return Vec::new(),
            },
        };
        let TemplateExpr::Call { function, args } = len_expr else {
            return Vec::new();
        };
        if function != "len" || args.len() != 1 {
            return Vec::new();
        }
        let (Some(path), Ok(bound)) = (
            self.single_resolved_values_path_expr(&args[0]),
            usize::try_from(bound),
        ) else {
            return Vec::new();
        };
        vec![Guard::MatchesPattern {
            path,
            pattern: format!("^[\\s\\S]{{{},}}$", bound + 1),
            templated: false,
        }]
    }

    /// `ne (int x) L` admits a raw-integer strengthening: an integer input
    /// is its own coercion, so a raw JSON integer different from the
    /// literal always satisfies the comparison (cilium's 255-or-511
    /// `maxConnectedClusters` check). Strings and other kinds coerce and
    /// stay outside the subset. The cast may sit behind a bound local.
    fn int_cast_inequality_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        if function != "ne" {
            return Vec::new();
        }
        self.int_cast_not_equal_subset(args)
    }

    /// The "coerced value differs from N" claim shared by `ne (int X) N`
    /// and the negation of `eq (int X) N`: a raw JSON integer other than
    /// the literal certainly satisfies it.
    fn int_cast_not_equal_subset(&self, args: &[TemplateExpr]) -> Vec<Guard> {
        let [left, right] = args else {
            return Vec::new();
        };
        let (cast_expr, literal) = match (left.deparen(), right.deparen()) {
            (cast, TemplateExpr::Literal(Literal::Int(literal))) => (cast, *literal),
            (TemplateExpr::Literal(Literal::Int(literal)), cast) => (cast, *literal),
            _ => return Vec::new(),
        };
        let Some(source) = self.int_cast_operand(cast_expr) else {
            return Vec::new();
        };
        let mut guards = vec![
            Guard::TypeIs {
                path: source.path.clone(),
                schema_type: "integer".to_string(),
            },
            Guard::NotEq {
                path: source.path.clone(),
                value: helm_schema_core::GuardValue::Int(literal),
            },
        ];
        // A literal `default` substitutes for a raw 0 (numerically empty)
        // BEFORE the comparison: when the fallback equals the literal, a
        // raw 0 no longer satisfies `ne`, so exclude it from the claim.
        if literal != 0 && source.default_int == Some(literal) {
            guards.push(Guard::NotEq {
                path: source.path,
                value: helm_schema_core::GuardValue::Int(0),
            });
        }
        guards
    }

    /// `eq (int X) N` certainly holds for a RAW integer equal to N, so a
    /// fail arm keyed on the [`Guard::IntGt`]`{N-1}` ∧ [`Guard::IntLt`]
    /// `{N+1}` region pair keeps firing there (kyverno's `eq (int .) 0`
    /// replicas terminal through `{{ template … }}`). Coercible
    /// non-integers — booleans, fractional floats, parseable strings —
    /// also satisfy the equality; they stay a sound abstention.
    fn int_cast_equality_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        if function != "eq" {
            return Vec::new();
        }
        self.int_cast_equality_region(args)
    }

    fn int_cast_equality_region(&self, args: &[TemplateExpr]) -> Vec<Guard> {
        let [left, right] = args else {
            return Vec::new();
        };
        let (cast_expr, literal) = match (left.deparen(), right.deparen()) {
            (cast, TemplateExpr::Literal(Literal::Int(literal))) => (cast, *literal),
            (TemplateExpr::Literal(Literal::Int(literal)), cast) => (cast, *literal),
            _ => return Vec::new(),
        };
        let (Some(below), Some(above)) = (literal.checked_sub(1), literal.checked_add(1)) else {
            return Vec::new();
        };
        let Some(source) = self.int_cast_operand(cast_expr) else {
            return Vec::new();
        };
        let mut guards = vec![
            Guard::IntGt {
                path: source.path.clone(),
                bound: below,
            },
            Guard::IntLt {
                path: source.path.clone(),
                bound: above,
            },
        ];
        // A literal `default` substitutes for a raw 0 (numerically empty)
        // BEFORE the comparison: when the fallback misses the literal, a
        // raw 0 no longer satisfies the equality, so exclude it.
        if literal == 0 && source.default_int.is_some_and(|fallback| fallback != 0) {
            guards.push(Guard::NotEq {
                path: source.path,
                value: helm_schema_core::GuardValue::Int(0),
            });
        }
        guards
    }

    /// `not (list "a" "b" | has .Values.x)` is EXACTLY "x differs from
    /// every listed literal" (Sprig `has` is deep equality against each
    /// item), so the negated membership lowers to the NotEq conjunction
    /// (cilium's internal-or-external `kvstoreMode` check).
    fn negated_membership_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        if function != "not" || args.len() != 1 {
            return Vec::new();
        }
        let (subject, list_args) = match args[0].deparen() {
            TemplateExpr::Call { function, args } if function == "has" && args.len() == 2 => {
                (&args[0], &args[1])
            }
            TemplateExpr::Pipeline(stages) => match stages.as_slice() {
                [list, has] => match (list.deparen(), has.deparen()) {
                    (
                        list @ TemplateExpr::Call { function, .. },
                        TemplateExpr::Call {
                            function: has_name,
                            args: has_args,
                        },
                    ) if matches!(function.as_str(), "list" | "tuple")
                        && has_name == "has"
                        && has_args.len() == 1 =>
                    {
                        (&has_args[0], list)
                    }
                    _ => return Vec::new(),
                },
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        let TemplateExpr::Call {
            function: list_name,
            args: literals,
        } = list_args.deparen()
        else {
            return Vec::new();
        };
        if !matches!(list_name.as_str(), "list" | "tuple") || literals.is_empty() {
            return Vec::new();
        }
        let Some(path) = self.single_resolved_values_path_expr(subject) else {
            return Vec::new();
        };
        let mut guards = Vec::new();
        for literal in literals {
            let value = match literal.deparen() {
                TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text)) => {
                    helm_schema_core::GuardValue::string(text.clone())
                }
                TemplateExpr::Literal(Literal::Int(value)) => {
                    helm_schema_core::GuardValue::Int(*value)
                }
                _ => return Vec::new(),
            };
            guards.push(Guard::NotEq {
                path: path.clone(),
                value,
            });
        }
        guards
    }

    /// `semverCompare "<constraint>" .Values.path` with a literal bounded
    /// comparator and a direct values-backed version selector admits an
    /// EXACT strengthening: the comparator lowers to a pattern matching
    /// precisely the version strings that satisfy it, so a fail-arm keyed
    /// on the pattern fires exactly where the guard held (airflow
    /// `semverCompare "<3.0.0" .Values.airflowVersion`). Only direct
    /// selectors qualify — a transformed operand no longer carries the
    /// path's own text, and a fallback chain selects other sources.
    fn semver_comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        if function != "semverCompare" || args.len() != 2 {
            return Vec::new();
        }
        let TemplateExpr::Literal(Literal::String(constraint)) = args[0].deparen() else {
            return Vec::new();
        };
        if !matches!(
            args[1].deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            return Vec::new();
        }
        let Some(path) = self.single_resolved_values_path_expr(&args[1]) else {
            return Vec::new();
        };
        let Some(pattern) = helm_schema_ast::semver_constraint_match_pattern(constraint) else {
            return Vec::new();
        };
        vec![Guard::MatchesPattern {
            path,
            pattern,
            templated: false,
        }]
    }

    /// `gt (int64 x) N` / `gt (int x) N` and the mirrored below-bound forms
    /// (`lt (int x) N`, either operand order flipped) with an integer
    /// literal bound admit a bounded sound strengthening: a RAW JSON
    /// integer beyond the bound always satisfies the coercing comparison,
    /// so a fail-arm keyed on the strengthened guard keeps firing there
    /// instead of abstaining wholesale (redis `gt (int64 .Values.
    /// master.count) 0`, jenkins' `$replicas` domain). The cast may sit
    /// behind a bound local.
    ///
    /// The inclusive comparators normalize into the strict guards with a
    /// shifted bound — `ge (int x) N` ⇔ `gt (int x) (N-1)` over int64 —
    /// abstaining when the shift overflows (cilium's `ge (int
    /// .Values.cluster.id) 128` ENI window).
    fn int_cast_comparison_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        self.int_cast_comparison_region_subset(expr, false)
    }

    fn int_cast_comparison_region_subset(&self, expr: &TemplateExpr, negated: bool) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        let [left, right] = args.as_slice() else {
            return Vec::new();
        };
        let form = match (function.as_str(), left.deparen(), right.deparen()) {
            ("gt", cast, TemplateExpr::Literal(Literal::Int(bound))) => Some((cast, *bound, true)),
            ("lt", TemplateExpr::Literal(Literal::Int(bound)), cast) => Some((cast, *bound, true)),
            ("lt", cast, TemplateExpr::Literal(Literal::Int(bound))) => Some((cast, *bound, false)),
            ("gt", TemplateExpr::Literal(Literal::Int(bound)), cast) => Some((cast, *bound, false)),
            ("ge", cast, TemplateExpr::Literal(Literal::Int(bound)))
            | ("le", TemplateExpr::Literal(Literal::Int(bound)), cast) => {
                bound.checked_sub(1).map(|bound| (cast, bound, true))
            }
            ("le", cast, TemplateExpr::Literal(Literal::Int(bound)))
            | ("ge", TemplateExpr::Literal(Literal::Int(bound)), cast) => {
                bound.checked_add(1).map(|bound| (cast, bound, false))
            }
            _ => None,
        };
        let Some((cast_expr, bound, greater)) = form else {
            return Vec::new();
        };
        // Negation flips the region: ¬(x > N) ⇔ x < N+1 over int64. The
        // flipped guard feeds the same zero/fallback reasoning below — the
        // claim is still plain region membership of the coerced value.
        let (bound, greater) = if negated {
            let flipped = if greater {
                bound.checked_add(1)
            } else {
                bound.checked_sub(1)
            };
            match flipped {
                Some(bound) => (bound, !greater),
                None => return Vec::new(),
            }
        } else {
            (bound, greater)
        };
        let Some(source) = self.int_cast_operand(cast_expr) else {
            return Vec::new();
        };
        let mut guards = vec![if greater {
            Guard::IntGt {
                path: source.path.clone(),
                bound,
            }
        } else {
            Guard::IntLt {
                path: source.path.clone(),
                bound,
            }
        }];
        // A literal `default` substitutes for a raw 0 (numerically empty)
        // BEFORE the comparison: when 0 itself would satisfy the claim but
        // the substituted fallback does not, exclude 0 from the claim.
        let zero_claims = if greater { 0 > bound } else { 0 < bound };
        let fallback_escapes = source.default_int.is_some_and(|fallback| {
            !(if greater {
                fallback > bound
            } else {
                fallback < bound
            })
        });
        if zero_claims && fallback_escapes {
            guards.push(Guard::NotEq {
                path: source.path,
                value: helm_schema_core::GuardValue::Int(0),
            });
        }
        guards
    }

    /// The int-cast provenance behind a comparison operand: the inline
    /// `int X` / `int (default L X)` call over one direct values selector,
    /// or a local bound to exactly that shape. Any other subject transform
    /// breaks the "a raw JSON integer at the path reaches the comparison
    /// unchanged" argument the raw-integer subsets rely on.
    /// `semverCompare "<constraint>" SUBJECT` where SUBJECT is the policy
    /// Kubernetes version, optionally shadowed by a values-path override
    /// (`default .Capabilities.KubeVersion.X .Values.kubeTargetVersionOverride`,
    /// directly or through a bound local). The constraint's exact version
    /// language comes from the semver pattern encoder; the policy arm
    /// evaluates it against the configured version, and the override arm
    /// tests the raw override text — both exact, so the negated form stays
    /// faithful.
    fn semver_capabilities_predicate(&self, args: &[TemplateExpr]) -> Option<Predicate> {
        let [constraint_expr, subject] = args else {
            return None;
        };
        let TemplateExpr::Literal(Literal::String(constraint) | Literal::RawString(constraint)) =
            constraint_expr.deparen()
        else {
            return None;
        };
        let source = self.kube_version_subject(subject)?;
        let policy = self.fragment_context.analysis_db.kubernetes_version()?;
        let pattern = helm_schema_ast::semver_constraint_match_pattern(constraint)?;
        let regex = regex::Regex::new(&pattern).ok()?;
        let policy_matches = regex.is_match(policy);
        Some(match source.override_path {
            None => bool_predicate(policy_matches),
            Some(path) => {
                let truthy = Predicate::truthy_path(path.clone());
                let matches = Predicate::from(Guard::MatchesPattern {
                    path,
                    pattern,
                    templated: false,
                });
                if policy_matches {
                    // A falsy override renders the (satisfying) policy
                    // version; a truthy override must satisfy on its own.
                    predicate_any(vec![
                        truthy.negated(),
                        Predicate::all(vec![truthy, matches]),
                    ])
                } else {
                    Predicate::all(vec![truthy, matches])
                }
            }
        })
    }

    fn kube_version_subject(
        &self,
        expr: &TemplateExpr,
    ) -> Option<crate::symbolic_local_state::KubeVersionSource> {
        if let TemplateExpr::Variable(name) = expr.deparen() {
            return self
                .kube_version_bindings
                .get(name.trim_start_matches('$'))
                .cloned();
        }
        self.kube_version_operand(expr)
    }

    /// The Kubernetes-version identity of an expression: a bare
    /// `.Capabilities.KubeVersion.Version|GitVersion` selector, or a
    /// `default` of that fallback with a DIRECT values-path override (a
    /// transformed override no longer carries the path's raw text).
    pub(crate) fn kube_version_operand(
        &self,
        expr: &TemplateExpr,
    ) -> Option<crate::symbolic_local_state::KubeVersionSource> {
        let expr = expr.deparen();
        if capabilities_kube_version_selector(expr) {
            return Some(crate::symbolic_local_state::KubeVersionSource {
                override_path: None,
            });
        }
        let TemplateExpr::Call { function, args } = expr else {
            return None;
        };
        if function != "default" {
            return None;
        }
        let [fallback, subject] = args.as_slice() else {
            return None;
        };
        if !capabilities_kube_version_selector(fallback.deparen()) {
            return None;
        }
        if !matches!(
            subject.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            return None;
        }
        let path = self.single_resolved_values_path_expr(subject)?;
        Some(crate::symbolic_local_state::KubeVersionSource {
            override_path: Some(path),
        })
    }

    pub(crate) fn int_cast_operand(
        &self,
        expr: &TemplateExpr,
    ) -> Option<crate::symbolic_local_state::IntCastSource> {
        if let TemplateExpr::Variable(name) = expr.deparen() {
            return self
                .int_cast_bindings
                .get(name)
                .or_else(|| self.int_cast_bindings.get(name.trim_start_matches('$')))
                .cloned();
        }
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return None;
        };
        if !matches!(function.as_str(), "int" | "int64") || args.len() != 1 {
            return None;
        }
        let (subject, default_int) = match args[0].deparen() {
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let TemplateExpr::Literal(Literal::Int(fallback)) = args[0].deparen() else {
                    return None;
                };
                (&args[1], Some(*fallback))
            }
            _ => (&args[0], None),
        };
        if !matches!(
            subject.deparen(),
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
        ) {
            return None;
        }
        let path = self.single_resolved_values_path_expr(subject)?;
        Some(crate::symbolic_local_state::IntCastSource { path, default_int })
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
        // member with that key (prometheus's serverFiles dispatch).
        if let Some(predicate) = self.range_key_equals_predicate(left, right, negated) {
            return Some(predicate);
        }
        // A root-context field assigned across a COMPLETE if/else chain
        // compares through its joined value dispatch: the equality selects
        // exactly the arms assigning the compared literal (vault's
        // `eq .mode "ha"` / `ne .mode "external"` gates). The arm conditions
        // are mutually exclusive and total, so the negated form is the
        // exact complement.
        if let Some(predicate) = self.root_field_dispatch_predicate(left, right, negated) {
            return Some(predicate);
        }
        // Only a DIRECT selector operand claims a value equality: seeing
        // through a call (`eq (typeOf .Values.x) "string"`) would compare
        // the call's OUTPUT, not the path's value. Two call shapes are
        // admitted: literal-key `index` navigation — its output IS the raw
        // member value (nil when absent), so the equality binds the member
        // path exactly (cilium's `ne (index .Values.extraConfig
        // "allow-unsafe-policy-skb-usage") "true"` gate) — and `toString`
        // over a selector, whose output is the `%v` rendering the preimage
        // projection below decodes exactly (cilium validate.yaml's
        // `eq (toString .Values.kubeProxyReplacement) "disabled"`).
        fn tostring_selector(expr: &TemplateExpr) -> bool {
            tostring_wrapped_subject(expr).is_some_and(|subject| {
                matches!(
                    subject.deparen(),
                    TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
                )
            })
        }
        let direct_selector = |expr: &TemplateExpr| match expr.deparen() {
            TemplateExpr::Field(_) | TemplateExpr::Selector { .. } => true,
            TemplateExpr::Variable(name) => !self
                .typeof_bindings
                .contains_key(name.trim_start_matches('$')),
            TemplateExpr::Call { function, args } if function == "index" => {
                args.len() >= 2
                    && matches!(
                        args[0].deparen(),
                        TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
                    )
                    && args[1..].iter().all(|key| {
                        matches!(
                            key.deparen(),
                            TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_))
                        )
                    })
            }
            expr => tostring_selector(expr),
        };
        // A `toString`-wrapped selector compares the rendering of the inner
        // path's value; resolve the paths from the operand itself.
        fn tostring_operand(expr: &TemplateExpr) -> &TemplateExpr {
            tostring_wrapped_subject(expr).unwrap_or(expr)
        }
        // Literal-key `index` navigation compares the MEMBER value only; the
        // evaluator's influence set also carries the parent map, which is
        // not the compared value.
        let subject_paths = |expr: &TemplateExpr| {
            let expr = tostring_operand(expr);
            if let TemplateExpr::Call { function, args } = expr.deparen()
                && function == "index"
                && let Some(base) = self.single_resolved_values_path_expr(&args[0])
            {
                let mut path = base;
                for key in &args[1..] {
                    match key.deparen() {
                        TemplateExpr::Literal(Literal::String(key) | Literal::RawString(key)) => {
                            path = helm_schema_core::append_value_path(&path, key);
                        }
                        _ => return self.paths_for_expr(expr),
                    }
                }
                return std::collections::BTreeSet::from([path]);
            }
            self.paths_for_expr(expr)
        };
        let (value, paths) = match (guard_value_literal(left), guard_value_literal(right)) {
            (Some(value), None) if direct_selector(right) => (value, subject_paths(right)),
            (None, Some(value)) if direct_selector(left) => (value, subject_paths(left)),
            _ => {
                // Two statically known scalars compare as a constant:
                // `eq (len $secret.path) (add1 $index)` selects the last
                // element of an exactly unrolled iteration.
                if let (Some(left_value), Some(right_value)) =
                    (self.constant_scalar(left), self.constant_scalar(right))
                {
                    return Some(bool_predicate((left_value == right_value) != negated));
                }
                // `eq (default D X) V` over a literal fallback D compares X
                // with its Helm-falsy states substituted by D: with V == D
                // the guard also holds for every falsy X; a truthy V ≠ D
                // binds X == V exactly (a falsy X renders D ≠ V); a falsy
                // V ≠ D never holds (any X equal to V would itself be falsy
                // and render D instead). oauth2-proxy's
                // `eq (default "" .Values.…clientType) "standalone"` caller
                // gate rides this shape.
                let defaulted = match (guard_value_literal(left), guard_value_literal(right)) {
                    (Some(value), None) => default_call_operand(right)
                        .map(|(fallback, subject)| (value, fallback, subject)),
                    (None, Some(value)) => default_call_operand(left)
                        .map(|(fallback, subject)| (value, fallback, subject)),
                    _ => None,
                };
                if let Some((value, fallback, subject)) = defaulted
                    && direct_selector(subject)
                {
                    let paths = subject_paths(subject);
                    if !paths.is_empty() {
                        let predicate = if value == fallback {
                            Predicate::all(
                                paths
                                    .iter()
                                    .map(|path| {
                                        predicate_any(vec![
                                            Predicate::from(Guard::Eq {
                                                path: path.clone(),
                                                value: value.clone(),
                                            }),
                                            Predicate::truthy_path(path.clone()).negated(),
                                        ])
                                    })
                                    .collect(),
                            )
                        } else if guard_value_is_truthy(&value) {
                            Predicate::all(
                                paths
                                    .iter()
                                    .map(|path| {
                                        Predicate::from(Guard::Eq {
                                            path: path.clone(),
                                            value: value.clone(),
                                        })
                                    })
                                    .collect(),
                            )
                        } else {
                            bool_predicate(false)
                        };
                        return Some(if negated {
                            predicate.negated()
                        } else {
                            predicate
                        });
                    }
                }
                return self.helper_literal_dispatch_predicate(left, right, negated);
            }
        };
        let subject = if guard_value_literal(left).is_some() {
            right
        } else {
            left
        };
        let subject_meta_for = |path: &str| {
            let TemplateExpr::Variable(name) = subject.deparen() else {
                return None;
            };
            self.template_output_meta
                .get(name)
                .or_else(|| self.template_output_meta.get(name.trim_start_matches('$')))
                .and_then(|by_path| by_path.get(path))
        };
        let comparison_for = |path: String| {
            // Equality over a total stringification compares the DERIVED
            // text, so a string literal binds the raw path through its
            // `toString` preimage: every raw value that renders the same
            // text stays inside the equality (cilium's
            // `ne $kubeProxyReplacement "true"` chain must keep a raw
            // Boolean `true` beside the string spelling). Serialized or
            // include-derived text has a different (or unknowable) image
            // and keeps the literal alone.
            let stringified = tostring_selector(subject)
                || subject_meta_for(&path).is_some_and(|meta| meta.stringified);
            let candidates = match &value {
                GuardValue::String(text) if stringified => {
                    let mut candidates = stringified_equality_preimage(text);
                    // A recorded coalesce rescue substitutes the fallback
                    // exactly while the stringification renders Helm-empty,
                    // so an equality against the fallback literal also
                    // admits the empty spellings (cilium's
                    // `ne $kubeProxyReplacement "false"` chain keeps ""
                    // and null renderable).
                    if let Some(rescue) =
                        subject_meta_for(&path).and_then(|meta| meta.empty_rescue.as_ref())
                        && rescue.fallback == *text
                    {
                        for spelling in &rescue.spellings {
                            if !candidates.contains(spelling) {
                                candidates.push(spelling.clone());
                            }
                        }
                    }
                    candidates
                }
                other => vec![other.clone()],
            };
            let comparisons = candidates
                .into_iter()
                .map(|candidate| {
                    if negated {
                        Predicate::from(Guard::NotEq {
                            path: path.clone(),
                            value: candidate,
                        })
                    } else {
                        Predicate::from(Guard::Eq {
                            path: path.clone(),
                            value: candidate,
                        })
                    }
                })
                .collect::<Vec<_>>();
            // `ne` holds only when the raw value misses EVERY preimage
            // member; `eq` when it hits any one of them.
            if negated {
                Predicate::all(comparisons)
            } else {
                predicate_any(comparisons)
            }
        };
        if let TemplateExpr::Variable(name) = subject.deparen() {
            let meta = self
                .template_output_meta
                .get(name)
                .or_else(|| self.template_output_meta.get(name.trim_start_matches('$')));
            // A binding qualified by lexical escape tokens is not the raw
            // value for every input (a replace/split chain rewrote some
            // strings): an equality on it cannot lower to a raw-path
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

    /// The values paths (with any binding-time meta) a `typeOf`/`kindOf`
    /// descriptor expression describes: a direct call over a selector or
    /// bound local, or a local bound to such a call
    /// (`$tp := typeOf .Values.x`).
    fn type_descriptor_sources(
        &self,
        expr: &TemplateExpr,
    ) -> Option<std::collections::BTreeMap<String, crate::helper_meta::HelperOutputMeta>> {
        match expr.deparen() {
            TemplateExpr::Call { function, args }
                if helm_schema_ast::type_descriptor_call_subject(function, args).is_some() =>
            {
                // Selectors and bound locals (a range's value variable,
                // a `$x := .Values.y` binding) both describe a single
                // resolvable path.
                let subject =
                    helm_schema_ast::type_descriptor_call_subject(function, args)?.deparen();
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
    }

    /// `regexMatch pat (typeOf x)` tests the FINITE set of type spellings a
    /// chart value can print, so the pattern lowers to the type alternatives
    /// whose every spelling matches (sealed-secrets' `regexMatch "64$"
    /// (typeOf .Values.pdb.minAvailable)` emits the field only for numeric
    /// kinds). A kind with mixed spelling verdicts is provenance-dependent
    /// (file-decoded `float64` vs `--set` `int64`), so it abstains rather
    /// than guessing either way.
    fn type_descriptor_regex_predicate(
        &self,
        pattern: &str,
        subject: &TemplateExpr,
    ) -> Option<Predicate> {
        let sources = self.type_descriptor_sources(subject)?;
        let regex = regex::Regex::new(pattern).ok()?;
        let mut matched_types = Vec::new();
        for schema_type in ["array", "boolean", "integer", "number", "object", "string"] {
            let spellings = helm_schema_ast::go_type_descriptor_spellings(schema_type);
            let matches = spellings
                .iter()
                .filter(|spelling| regex.is_match(spelling))
                .count();
            if matches == spellings.len() {
                matched_types.push(schema_type);
            } else if matches != 0 {
                return None;
            }
        }
        // `typeOf nil` prints `<nil>` and `kindOf nil` prints `invalid`.
        let null_spellings = ["<nil>", "invalid"];
        let null_matches = null_spellings
            .iter()
            .filter(|spelling| regex.is_match(spelling))
            .count();
        if null_matches != 0 && null_matches != null_spellings.len() {
            return None;
        }
        if matched_types.is_empty() && null_matches == 0 {
            return None;
        }
        let mut alternatives = Vec::new();
        for (path, meta) in sources {
            let mut kind_alternatives: Vec<Predicate> = matched_types
                .iter()
                .map(|schema_type| {
                    Predicate::from(Guard::TypeIs {
                        path: path.clone(),
                        schema_type: (*schema_type).to_string(),
                    })
                })
                .collect();
            if null_matches != 0 {
                kind_alternatives.push(invalid_kind_predicate(path.clone()));
            }
            let type_predicate = predicate_any(kind_alternatives);
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
        fn type_literal(expr: &TemplateExpr) -> Option<&str> {
            match expr.deparen() {
                TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name)) => {
                    Some(name.as_str())
                }
                _ => None,
            }
        }
        let (sources, type_name) = match (
            self.type_descriptor_sources(left),
            self.type_descriptor_sources(right),
        ) {
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

/// The raw values whose Go `toString` rendering equals `text`, for an
/// equality whose subject is a total stringification of the path.
///
/// The string itself always renders itself. Beyond it, `%v` prints Booleans
/// as `true`/`false`, nil as `<nil>`, and numbers in fixed decimal notation —
/// but float64 switches to exponent form at 1e6 (`1e+06`), so only spellings
/// below that magnitude cover draft-07's single number kind exactly (an
/// `enum` integer already accepts the numerically equal float). Larger or
/// non-canonical spellings keep the string alone rather than claim a
/// preimage that splits the int and float channels.
fn stringified_equality_preimage(text: &str) -> Vec<GuardValue> {
    let mut values = vec![GuardValue::string(text)];
    match text {
        "true" => values.push(GuardValue::Bool(true)),
        "false" => values.push(GuardValue::Bool(false)),
        "<nil>" => values.push(GuardValue::Null),
        _ => {
            if let Ok(value) = text.parse::<i64>()
                && value.to_string() == text
                && (-1_000_000..1_000_000).contains(&value)
            {
                values.push(GuardValue::Int(value));
            }
        }
    }
    values
}

pub(crate) fn predicate_any(predicates: Vec<Predicate>) -> Predicate {
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
            // Alternatives must AGREE — including definitely-empty
            // `default list`/`default dict` fallbacks. Skipping the empty
            // alternatives here is tempting for range-item bindings (nats'
            // jsonpatch members ride `.patch | default list`), but the
            // helper walker records member probes at TRUNCATED absolute
            // paths with no range identity, and a decode here feeds those
            // captures into document-level terminal clauses that reject
            // valid documents. The abstention is load-bearing until fail
            // captures carry member identities through helper ranges.
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
        // The merged map contains the key exactly when some layer does.
        AbstractValue::MergedLayers(layers) => {
            let mut resolved = layers
                .iter()
                .map(|layer| value_has_key(layer, key))
                .collect::<Option<Vec<_>>>()?;
            resolved.sort();
            resolved.dedup();
            match resolved.as_slice() {
                [predicate] => Some(predicate.clone()),
                _ => Some(predicate_any(resolved)),
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
        | AbstractValue::KeysList(_)
        | AbstractValue::OutputPath(_, _)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::List(_)
        | AbstractValue::SplitList { .. }
        | AbstractValue::SplitSegment { .. }
        | AbstractValue::Widened(_) => None,
    }
}

/// The deepest path of a set forming one ancestor CHAIN (every path an
/// ancestor of the next); `None` for unrelated paths.
fn ancestor_chain_leaf(paths: &std::collections::BTreeSet<String>) -> Option<&str> {
    let mut ordered: Vec<&String> = paths.iter().collect();
    ordered.sort_by_key(|path| helm_schema_core::split_value_path(path).len());
    for pair in ordered.windows(2) {
        if !helm_schema_core::values_path_is_descendant(pair[1], pair[0]) {
            return None;
        }
    }
    ordered.last().map(|path| path.as_str())
}

fn bool_predicate(value: bool) -> Predicate {
    if value {
        Predicate::True
    } else {
        Predicate::False
    }
}

/// The mapping whose member count `expr` computes: `len (keys M)` or the
/// pipeline form `keys M | len`.
fn keys_len_subject(expr: &TemplateExpr) -> Option<&TemplateExpr> {
    fn keys_map(candidate: &TemplateExpr) -> Option<&TemplateExpr> {
        match candidate.deparen() {
            TemplateExpr::Call { function, args } if function == "keys" => match args.as_slice() {
                [map_expr] => Some(map_expr),
                _ => None,
            },
            _ => None,
        }
    }
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "len" => match args.as_slice() {
            [inner] => keys_map(inner),
            _ => None,
        },
        TemplateExpr::Pipeline(stages) => match stages.as_slice() {
            [first, last] => {
                let piped_len = matches!(
                    last.deparen(),
                    TemplateExpr::Call { function, args } if function == "len" && args.is_empty()
                );
                piped_len.then(|| keys_map(first)).flatten()
            }
            _ => None,
        },
        _ => None,
    }
}

/// The subject of a total stringification — `toString X` or the two-stage
/// pipeline `X | toString` (vault's `.Values.server.ha.enabled | toString`
/// redundancy-zone gates spell it this way).
fn tostring_wrapped_subject(expr: &TemplateExpr) -> Option<&TemplateExpr> {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "toString" && args.len() == 1 => {
            Some(&args[0])
        }
        TemplateExpr::Pipeline(stages) => {
            let [subject, tail] = stages.as_slice() else {
                return None;
            };
            let TemplateExpr::Call { function, args } = tail.deparen() else {
                return None;
            };
            (function == "toString" && args.is_empty()).then_some(subject)
        }
        _ => None,
    }
}

/// The `(fallback, subject)` of a literal-fallback `default` call —
/// `default D X` or the two-stage pipeline `X | default D`.
fn default_call_operand(expr: &TemplateExpr) -> Option<(GuardValue, &TemplateExpr)> {
    match expr.deparen() {
        TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
            Some((guard_value_literal(&args[0])?, &args[1]))
        }
        TemplateExpr::Pipeline(stages) => {
            let [subject, tail] = stages.as_slice() else {
                return None;
            };
            let TemplateExpr::Call { function, args } = tail.deparen() else {
                return None;
            };
            if function != "default" || args.len() != 1 {
                return None;
            }
            Some((guard_value_literal(&args[0])?, subject))
        }
        _ => None,
    }
}

/// Helm truthiness of a literal guard value (sprig `empty` complement):
/// nil, `""`, `0`, `0.0`, and `false` are falsy.
pub(crate) fn guard_value_is_truthy(value: &GuardValue) -> bool {
    match value {
        GuardValue::String(text) => !text.is_empty(),
        GuardValue::Bool(value) => *value,
        GuardValue::Int(value) => *value != 0,
        GuardValue::Float(text) => text.parse::<f64>().is_ok_and(|value| value != 0.0),
        GuardValue::Null => false,
    }
}

/// A `.Capabilities.KubeVersion.Version` / `.GitVersion` selector (also the
/// `$.`-rooted spelling).
fn capabilities_kube_version_selector(expr: &TemplateExpr) -> bool {
    let path = match expr {
        TemplateExpr::Field(path) => path.as_slice(),
        TemplateExpr::Selector { operand, path } if matches!(operand.as_ref(), TemplateExpr::Variable(variable) if variable.is_empty()) => {
            path.as_slice()
        }
        _ => return false,
    };
    matches!(
        path,
        [first, second, third]
            if first == "Capabilities"
                && second == "KubeVersion"
                && (third == "Version" || third == "GitVersion")
    )
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
