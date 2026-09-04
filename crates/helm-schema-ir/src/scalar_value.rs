//! Exact and partial scalar semantics for symbolic Helm evaluation.
//!
//! Scalar values retain their branch predicates and runtime transforms so
//! truthiness, equality, pattern matching, and semantic-version comparison
//! all consume one typed representation.

use std::collections::BTreeSet;

use helm_schema_core::PredicateMemo;

/// One exact scalar value carried by a dispatch arm.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarValue {
    /// A concrete Go-template scalar.
    Literal(helm_schema_core::GuardValue),
    /// A raw values-path identity before template rendering.
    Identity(helm_schema_core::ValuesPath),
    /// Text already rendered by a helper body.
    Rendered(Vec<ScalarRenderPart>),
    /// Output of `printf "%s"` over one raw scalar identity.
    ///
    /// Non-string inputs render Go's non-empty format-mismatch spelling, so
    /// only the empty-output preimage is exact without modeling that grammar.
    PrintfStringIdentity(helm_schema_core::ValuesPath),
    /// The number of segments produced by splitting one scalar value on a
    /// literal separator.
    SplitLength {
        value: Box<ScalarValue>,
        separator: String,
    },
}

/// One exact contribution to helper-rendered scalar text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScalarRenderPart {
    Text(String),
    Identity {
        path: helm_schema_core::ValuesPath,
        /// The text is Go's total scalar rendering of the raw value. When
        /// false, an earlier string consumer has already restricted the
        /// runtime input to strings.
        stringified: bool,
        lexical_escapes: BTreeSet<crate::helper_meta::LexicalEscape>,
    },
}

/// Known scalar alternatives for one evaluated value.
///
/// The arm conditions are mutually exclusive. [`Self::complete`] records
/// whether they also cover the whole input domain; helper output can retain
/// exact arms even when another branch remains unknown. Root-context fields
/// assigned across a complete `if`/`else` chain and evaluated helper output
/// both use this representation, so equality and truthiness consume the same
/// semantic fact.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValueDispatch {
    pub(crate) arms: Vec<(helm_schema_core::Predicate, ScalarValue)>,
    pub(crate) complete: bool,
}

impl ScalarValueDispatch {
    pub(crate) fn constant(value: helm_schema_core::GuardValue) -> Self {
        Self {
            arms: vec![(
                helm_schema_core::Predicate::True,
                ScalarValue::Literal(value),
            )],
            complete: true,
        }
    }

    pub(crate) fn identity(path: helm_schema_core::ValuesPath) -> Self {
        Self {
            arms: vec![(
                helm_schema_core::Predicate::True,
                ScalarValue::Identity(path),
            )],
            complete: true,
        }
    }

    /// Return the exact dispatch-arm conditions that select one raw identity.
    ///
    /// An incomplete dispatch may omit an unknown arm, so it cannot safely
    /// qualify a formatter capture.
    pub(crate) fn identity_selection_conditions(
        &self,
        path: &helm_schema_core::ValuesPath,
    ) -> Option<Vec<helm_schema_core::Predicate>> {
        if !self.complete {
            return None;
        }
        let conditions = self
            .arms
            .iter()
            .filter(
                |(_, value)| matches!(value, ScalarValue::Identity(identity) if identity == path),
            )
            .map(|(condition, _)| condition.clone())
            .collect::<Vec<_>>();
        (!conditions.is_empty()).then_some(conditions)
    }

    pub(crate) fn has_printf_string_identity(&self) -> bool {
        self.arms
            .iter()
            .any(|(_, value)| matches!(value, ScalarValue::PrintfStringIdentity(_)))
    }

    /// Render every known arm through Sprig's total `toString` conversion.
    ///
    /// The conversion changes runtime value semantics without changing
    /// provenance: equality now compares rendered text, while effects still
    /// attribute the operand to its source path.
    pub(crate) fn stringified(&self) -> Self {
        let mut complete = self.complete;
        let arms = self
            .arms
            .iter()
            .filter_map(|(condition, value)| {
                let value = value.stringified();
                complete &= value.is_some();
                value.map(|value| (condition.clone(), value))
            })
            .collect();
        Self { arms, complete }
    }

    /// Render one scalar argument through a literal `printf "%s"` format.
    ///
    /// Other formats abstain because their mismatch and padding languages
    /// need a separate typed preimage before they can drive control flow.
    pub(crate) fn printf_string(&self, format: &str) -> Option<Self> {
        if format != "%s" {
            return None;
        }
        let mut complete = self.complete;
        let arms = self
            .arms
            .iter()
            .filter_map(|(condition, value)| {
                let rendered = match value {
                    ScalarValue::Literal(value) => helm_schema_ast::render_printf_scalar_values(
                        format,
                        std::slice::from_ref(value),
                    )
                    .map(|value| ScalarValue::Literal(helm_schema_core::GuardValue::string(value))),
                    ScalarValue::Identity(path) => {
                        Some(ScalarValue::PrintfStringIdentity(path.clone()))
                    }
                    ScalarValue::Rendered(parts) => rendered_constant(parts).and_then(|value| {
                        helm_schema_ast::render_printf_scalar_values(
                            format,
                            &[helm_schema_core::GuardValue::string(value)],
                        )
                        .map(|value| {
                            ScalarValue::Literal(helm_schema_core::GuardValue::string(value))
                        })
                    }),
                    ScalarValue::PrintfStringIdentity(_) | ScalarValue::SplitLength { .. } => None,
                };
                complete &= rendered.is_some();
                rendered.map(|value| (condition.clone(), value))
            })
            .collect::<Vec<_>>();
        (!arms.is_empty()).then_some(Self { arms, complete })
    }

    /// Apply one exact `trimPrefix`/`trimSuffix` transform to every scalar
    /// arm whose successful result is known. Arms rejected by the transform's
    /// strict string contract are omitted and make the dispatch incomplete.
    pub(crate) fn trimmed(&self, token: &str, prefix: bool) -> Self {
        let mut complete = self.complete;
        let arms = self
            .arms
            .iter()
            .filter_map(|(condition, value)| {
                let value = value.trimmed(token, prefix);
                complete &= value.is_some();
                value.map(|value| (condition.clone(), value))
            })
            .collect();
        Self { arms, complete }
    }

    pub(crate) fn split_length(&self, separator: &str) -> Self {
        Self {
            arms: self
                .arms
                .iter()
                .map(|(condition, value)| {
                    (
                        condition.clone(),
                        ScalarValue::SplitLength {
                            value: Box::new(value.clone()),
                            separator: separator.to_string(),
                        },
                    )
                })
                .collect(),
            complete: self.complete,
        }
    }

    pub(crate) fn condition_matches_pattern_with_memo(
        &self,
        pattern: &str,
        memo: &PredicateMemo,
    ) -> TruthCondition {
        let mut matches = Vec::new();
        let mut mismatches = Vec::new();
        let mut fully_classified = true;
        for (arm_condition, value) in &self.arms {
            let value_condition = value.condition_matches_pattern(pattern, memo);
            fully_classified &= value_condition.predicate().is_some();
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_true(),
                memo,
            ) {
                matches.push(condition);
            }
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_false_with_memo(memo),
                memo,
            ) {
                mismatches.push(condition);
            }
        }
        TruthCondition::from_subsets_with_memo(
            any_predicates_with_memo(matches, memo),
            any_predicates_with_memo(mismatches, memo),
            self.complete && fully_classified,
            memo,
        )
    }

    /// Evaluate a semantic-version constraint per scalar arm.
    ///
    /// Concrete strings use the full constraint parser. Input identities
    /// require an exact regex preimage; constraints without one remain
    /// unknown for that arm.
    pub(crate) fn condition_matches_semver_with_memo(
        &self,
        constraint: &str,
        memo: &PredicateMemo,
    ) -> TruthCondition {
        let mut matches = Vec::new();
        let mut mismatches = Vec::new();
        let mut fully_classified = true;
        for (arm_condition, value) in &self.arms {
            let value_condition = value.condition_matches_semver(constraint, memo);
            fully_classified &= value_condition.predicate().is_some();
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_true(),
                memo,
            ) {
                matches.push(condition);
            }
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_false_with_memo(memo),
                memo,
            ) {
                mismatches.push(condition);
            }
        }
        TruthCondition::from_subsets_with_memo(
            any_predicates_with_memo(matches, memo),
            any_predicates_with_memo(mismatches, memo),
            self.complete && fully_classified,
            memo,
        )
    }

    pub(crate) fn constant_value(&self) -> Option<helm_schema_core::GuardValue> {
        if !self.complete {
            return None;
        }
        let [(predicate, value)] = self.arms.as_slice() else {
            return None;
        };
        (predicate == &helm_schema_core::Predicate::True)
            .then(|| value.constant_value())
            .flatten()
    }

    pub(crate) fn condition_equals_with_memo(
        &self,
        target: &helm_schema_core::GuardValue,
        memo: &PredicateMemo,
    ) -> TruthCondition {
        let mut matches = Vec::new();
        let mut mismatches = Vec::new();
        let mut fully_classified = true;
        for (arm_condition, value) in &self.arms {
            let value_condition = value.condition_equals(target, memo);
            fully_classified &= value_condition.predicate().is_some();
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_true(),
                memo,
            ) {
                matches.push(condition);
            }
            if let Some(condition) = conjoin_predicates_with_memo(
                arm_condition.clone(),
                value_condition.when_false_with_memo(memo),
                memo,
            ) {
                mismatches.push(condition);
            }
        }
        TruthCondition::from_subsets_with_memo(
            any_predicates_with_memo(matches, memo),
            any_predicates_with_memo(mismatches, memo),
            self.complete && fully_classified,
            memo,
        )
    }

    pub(crate) fn truth_condition(&self) -> TruthCondition {
        self.truth_condition_with_memo(&PredicateMemo::new())
    }

    pub(crate) fn truth_condition_with_memo(&self, memo: &PredicateMemo) -> TruthCondition {
        let mut truthy = Vec::new();
        let mut falsy = Vec::new();
        let mut fully_classified = true;
        for (arm_condition, value) in &self.arms {
            let Some(value_condition) = value.truth_condition() else {
                fully_classified = false;
                continue;
            };
            if let Some(condition) =
                conjoin_predicates_with_memo(arm_condition.clone(), value_condition.clone(), memo)
            {
                truthy.push(condition);
            }
            if let Some(condition) =
                conjoin_predicates_with_memo(arm_condition.clone(), value_condition.negated(), memo)
            {
                falsy.push(condition);
            }
        }
        TruthCondition::from_subsets_with_memo(
            any_predicates_with_memo(truthy, memo),
            any_predicates_with_memo(falsy, memo),
            self.complete && fully_classified,
            memo,
        )
    }

    pub(crate) fn select_default_with_memo(
        primary: &Self,
        fallback: &Self,
        memo: &PredicateMemo,
    ) -> Option<ScalarValueDispatch> {
        let primary_truth = primary.truth_condition_with_memo(memo);
        let truthy = primary_truth.when_true();
        let falsy = primary_truth.when_false_with_memo(memo);
        let mut arms = Vec::new();
        for (condition, value) in &primary.arms {
            if let Some(condition) =
                conjoin_predicates_with_memo(condition.clone(), truthy.clone(), memo)
            {
                arms.push((condition, value.clone()));
            }
        }
        for (condition, value) in &fallback.arms {
            if let Some(condition) =
                conjoin_predicates_with_memo(condition.clone(), falsy.clone(), memo)
            {
                arms.push((condition, value.clone()));
            }
        }
        if arms.is_empty() {
            return None;
        }
        let complete = match primary_truth.predicate() {
            Some(predicate) if predicate == &helm_schema_core::Predicate::True => primary.complete,
            Some(predicate) if predicate == &helm_schema_core::Predicate::False => {
                fallback.complete
            }
            Some(_) => primary.complete && fallback.complete,
            None => false,
        };
        Some(Self { arms, complete })
    }

    pub(crate) fn select_ternary_with_memo(
        condition: &TruthCondition,
        when_true: &Self,
        when_false: &Self,
        memo: &PredicateMemo,
    ) -> Option<Self> {
        let truthy = condition.when_true();
        let falsy = condition.when_false_with_memo(memo);
        let mut arms = Vec::new();
        for (arm_condition, value) in &when_true.arms {
            if let Some(condition) =
                conjoin_predicates_with_memo(arm_condition.clone(), truthy.clone(), memo)
            {
                arms.push((condition, value.clone()));
            }
        }
        for (arm_condition, value) in &when_false.arms {
            if let Some(condition) =
                conjoin_predicates_with_memo(arm_condition.clone(), falsy.clone(), memo)
            {
                arms.push((condition, value.clone()));
            }
        }
        if arms.is_empty() {
            return None;
        }
        let complete = match condition.predicate() {
            Some(predicate) if predicate == &helm_schema_core::Predicate::True => {
                when_true.complete
            }
            Some(predicate) if predicate == &helm_schema_core::Predicate::False => {
                when_false.complete
            }
            Some(_) => when_true.complete && when_false.complete,
            None => false,
        };
        Some(Self { arms, complete })
    }
}

impl ScalarValue {
    fn stringified(&self) -> Option<Self> {
        let parts = match self {
            Self::Literal(value) => {
                let text = match value {
                    helm_schema_core::GuardValue::String(text)
                    | helm_schema_core::GuardValue::Float(text) => text.clone(),
                    helm_schema_core::GuardValue::Bool(value) => value.to_string(),
                    helm_schema_core::GuardValue::Int(value) => value.to_string(),
                    helm_schema_core::GuardValue::Null => "<nil>".to_string(),
                };
                vec![ScalarRenderPart::Text(text)]
            }
            Self::Identity(path) => vec![ScalarRenderPart::Identity {
                path: path.clone(),
                stringified: true,
                lexical_escapes: BTreeSet::new(),
            }],
            Self::Rendered(parts) => parts.clone(),
            Self::PrintfStringIdentity(path) => {
                return Some(Self::PrintfStringIdentity(path.clone()));
            }
            Self::SplitLength { .. } => return None,
        };
        Some(Self::Rendered(parts))
    }

    fn trimmed(&self, token: &str, prefix: bool) -> Option<Self> {
        let trim = |text: &str| {
            if prefix {
                text.strip_prefix(token).unwrap_or(text)
            } else {
                text.strip_suffix(token).unwrap_or(text)
            }
            .to_string()
        };
        match self {
            Self::Literal(helm_schema_core::GuardValue::String(text)) => Some(Self::Literal(
                helm_schema_core::GuardValue::string(trim(text)),
            )),
            Self::Literal(_) | Self::PrintfStringIdentity(_) | Self::SplitLength { .. } => None,
            Self::Identity(path) => {
                let escape = if prefix {
                    crate::helper_meta::LexicalEscape::TrimPrefix(token.to_string())
                } else {
                    crate::helper_meta::LexicalEscape::TrimSuffix(token.to_string())
                };
                Some(Self::Rendered(vec![ScalarRenderPart::Identity {
                    path: path.clone(),
                    stringified: false,
                    lexical_escapes: BTreeSet::from([escape]),
                }]))
            }
            Self::Rendered(parts) => {
                if let Some(text) = rendered_constant(parts) {
                    return Some(Self::Rendered(vec![ScalarRenderPart::Text(trim(&text))]));
                }
                let [
                    ScalarRenderPart::Identity {
                        path,
                        stringified,
                        lexical_escapes,
                    },
                ] = parts.as_slice()
                else {
                    return None;
                };
                let mut lexical_escapes = lexical_escapes.clone();
                lexical_escapes.insert(if prefix {
                    crate::helper_meta::LexicalEscape::TrimPrefix(token.to_string())
                } else {
                    crate::helper_meta::LexicalEscape::TrimSuffix(token.to_string())
                });
                Some(Self::Rendered(vec![ScalarRenderPart::Identity {
                    path: path.clone(),
                    stringified: *stringified,
                    lexical_escapes,
                }]))
            }
        }
    }

    pub(crate) fn rendered_parts(&self) -> Option<Vec<ScalarRenderPart>> {
        let parts = match self {
            Self::Literal(value) => {
                let text = match value {
                    helm_schema_core::GuardValue::String(text)
                    | helm_schema_core::GuardValue::Float(text) => text.clone(),
                    helm_schema_core::GuardValue::Bool(value) => value.to_string(),
                    helm_schema_core::GuardValue::Int(value) => value.to_string(),
                    helm_schema_core::GuardValue::Null => return None,
                };
                vec![ScalarRenderPart::Text(text)]
            }
            Self::Identity(path) => vec![ScalarRenderPart::Identity {
                path: path.clone(),
                stringified: true,
                lexical_escapes: BTreeSet::new(),
            }],
            Self::Rendered(parts) => parts.clone(),
            Self::PrintfStringIdentity(_) | Self::SplitLength { .. } => return None,
        };
        Some(parts)
    }

    fn constant_value(&self) -> Option<helm_schema_core::GuardValue> {
        match self {
            Self::Literal(value) => Some(value.clone()),
            Self::Identity(_) | Self::PrintfStringIdentity(_) | Self::SplitLength { .. } => None,
            Self::Rendered(parts) => {
                rendered_constant(parts).map(helm_schema_core::GuardValue::string)
            }
        }
    }

    fn condition_equals(
        &self,
        target: &helm_schema_core::GuardValue,
        memo: &PredicateMemo,
    ) -> TruthCondition {
        use helm_schema_core::{Guard, GuardValue, Predicate};

        match self {
            Self::Literal(value) => TruthCondition::exact_with_memo(
                if value == target {
                    Predicate::True
                } else {
                    Predicate::False
                },
                memo,
            ),
            Self::Identity(path) => TruthCondition::exact_with_memo(
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: target.clone(),
                }),
                memo,
            ),
            Self::Rendered(parts) => {
                if let Some(value) = rendered_constant(parts) {
                    return TruthCondition::exact_with_memo(
                        if target == &GuardValue::string(value) {
                            Predicate::True
                        } else {
                            Predicate::False
                        },
                        memo,
                    );
                }
                let [
                    ScalarRenderPart::Identity {
                        path,
                        stringified,
                        lexical_escapes,
                    },
                ] = parts.as_slice()
                else {
                    return TruthCondition::Unknown;
                };
                let GuardValue::String(target) = target else {
                    return TruthCondition::exact_with_memo(Predicate::False, memo);
                };
                rendered_identity_equals(path, *stringified, lexical_escapes, target, memo)
            }
            Self::PrintfStringIdentity(path) => {
                let GuardValue::String(target) = target else {
                    return TruthCondition::exact_with_memo(Predicate::False, memo);
                };
                let predicate = Predicate::from(Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: format!("^{}$", crate::escape_regex_literal(target)),
                    templated: false,
                });
                if target.is_empty() {
                    TruthCondition::exact_with_memo(predicate, memo)
                } else {
                    TruthCondition::from_subsets_with_memo(predicate, Predicate::False, false, memo)
                }
            }
            Self::SplitLength { value, separator } => {
                let GuardValue::Int(length) = target else {
                    return TruthCondition::exact_with_memo(Predicate::False, memo);
                };
                let Ok(length) = usize::try_from(*length) else {
                    return TruthCondition::exact_with_memo(Predicate::False, memo);
                };
                let Some(pattern) = split_length_pattern(separator, length) else {
                    return TruthCondition::Unknown;
                };
                value.condition_matches_pattern(&pattern, memo)
            }
        }
    }

    fn truth_condition(&self) -> Option<helm_schema_core::Predicate> {
        match self {
            Self::Literal(value) => Some(bool_predicate(
                crate::value_path_context::guard_value_is_truthy(value),
            )),
            Self::Identity(path) => Some(helm_schema_core::Predicate::from(
                helm_schema_core::Guard::Truthy { path: path.clone() },
            )),
            Self::Rendered(parts) => rendered_constant(parts).map(|value| {
                if value.is_empty() {
                    helm_schema_core::Predicate::False
                } else {
                    helm_schema_core::Predicate::True
                }
            }),
            Self::PrintfStringIdentity(path) => Some(
                helm_schema_core::Predicate::from(helm_schema_core::Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: "^$".to_string(),
                    templated: false,
                })
                .negated(),
            ),
            Self::SplitLength { .. } => Some(helm_schema_core::Predicate::True),
        }
    }

    fn condition_matches_pattern(&self, pattern: &str, memo: &PredicateMemo) -> TruthCondition {
        use helm_schema_core::{Guard, GuardValue, Predicate};

        let Ok(regex) = regex::Regex::new(pattern) else {
            return TruthCondition::Unknown;
        };
        match self {
            Self::Literal(GuardValue::String(value)) => TruthCondition::exact_with_memo(
                if regex.is_match(value) {
                    Predicate::True
                } else {
                    Predicate::False
                },
                memo,
            ),
            Self::Literal(_) | Self::PrintfStringIdentity(_) | Self::SplitLength { .. } => {
                TruthCondition::Unknown
            }
            Self::Identity(path) => TruthCondition::exact_with_memo(
                Predicate::from(Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: pattern.to_string(),
                    templated: false,
                }),
                memo,
            ),
            Self::Rendered(parts) => {
                if let Some(value) = rendered_constant(parts) {
                    return TruthCondition::exact_with_memo(
                        if regex.is_match(&value) {
                            Predicate::True
                        } else {
                            Predicate::False
                        },
                        memo,
                    );
                }
                let [
                    ScalarRenderPart::Identity {
                        path,
                        stringified,
                        lexical_escapes,
                    },
                ] = parts.as_slice()
                else {
                    return TruthCondition::Unknown;
                };
                let raw_match = Predicate::from(Guard::MatchesPattern {
                    path: path.clone(),
                    pattern: pattern.to_string(),
                    templated: false,
                });
                if !lexical_escapes.is_empty() {
                    // Escape-qualified provenance guarantees identity only
                    // where none of its transform tokens occurs. The wider
                    // parser-acceptance preimage deliberately admits every
                    // token-bearing input, so using it as Boolean truth
                    // would invert that widening into false rejections.
                    let unchanged = Predicate::all(
                        lexical_escapes
                            .iter()
                            .map(|escape| {
                                let token = match escape {
                                    crate::helper_meta::LexicalEscape::Contains(token)
                                    | crate::helper_meta::LexicalEscape::TrimPrefix(token)
                                    | crate::helper_meta::LexicalEscape::TrimSuffix(token)
                                    | crate::helper_meta::LexicalEscape::CutAtToken(token) => token,
                                };
                                Predicate::from(Guard::MatchesPattern {
                                    path: path.clone(),
                                    pattern: crate::escape_regex_literal(token),
                                    templated: false,
                                })
                                .negated()
                            })
                            .collect(),
                    );
                    return TruthCondition::from_subsets_with_memo(
                        Predicate::all(vec![unchanged.clone(), raw_match.clone()]),
                        Predicate::all(vec![unchanged, raw_match.negated()]),
                        false,
                        memo,
                    );
                }
                let predicate = raw_match;
                // A total stringification renders every raw kind, so the
                // raw-level pattern guard (which asserts `type: string`)
                // cannot be the exact truth: `toString 3` matches
                // `^[0-9]+$` while the guard rejects the integer. Only a
                // string-restricted rendering keeps the guard exact.
                if *stringified {
                    let raw_string_mismatch = Predicate::from(Guard::NotMatchesPattern {
                        path: path.clone(),
                        pattern: pattern.to_string(),
                    });
                    TruthCondition::from_subsets_with_memo(
                        predicate,
                        raw_string_mismatch,
                        false,
                        memo,
                    )
                } else {
                    TruthCondition::exact_with_memo(predicate, memo)
                }
            }
        }
    }

    fn condition_matches_semver(&self, constraint: &str, memo: &PredicateMemo) -> TruthCondition {
        use helm_schema_core::{GuardValue, Predicate};

        let constant = match self {
            Self::Literal(GuardValue::String(value)) => Some(value.clone()),
            Self::Rendered(parts) => rendered_constant(parts),
            Self::Literal(_)
            | Self::Identity(_)
            | Self::PrintfStringIdentity(_)
            | Self::SplitLength { .. } => None,
        };
        if let Some(value) = constant {
            return match helm_schema_ast::semver_constraint_matches_version(constraint, &value) {
                Some(matches) => TruthCondition::exact_with_memo(
                    if matches {
                        Predicate::True
                    } else {
                        Predicate::False
                    },
                    memo,
                ),
                None => TruthCondition::Unknown,
            };
        }
        let Some(pattern) = helm_schema_ast::semver_constraint_match_pattern(constraint) else {
            return TruthCondition::Unknown;
        };
        self.condition_matches_pattern(&pattern, memo)
    }
}

fn split_length_pattern(separator: &str, length: usize) -> Option<String> {
    if separator.is_empty() || length == 0 {
        return None;
    }
    let mut characters = separator.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    let separator = crate::escape_regex_literal(separator);
    let segment = format!(
        "[^{}]*",
        crate::escape_regex_literal(&character.to_string())
    );
    Some(format!(
        "^{segment}(?:{separator}{segment}){{{}}}$",
        length - 1
    ))
}

fn rendered_identity_equals(
    path: &helm_schema_core::ValuesPath,
    stringified: bool,
    lexical_escapes: &BTreeSet<crate::helper_meta::LexicalEscape>,
    target: &str,
    memo: &PredicateMemo,
) -> TruthCondition {
    use crate::helper_meta::LexicalEscape;
    use helm_schema_core::{Guard, GuardValue, Predicate};

    let string_pattern = crate::helper_meta::pattern_with_lexical_escapes(
        &format!("^{}$", crate::escape_regex_literal(target)),
        lexical_escapes,
    );
    let mut matches = vec![Predicate::from(Guard::MatchesPattern {
        path: path.clone(),
        pattern: string_pattern,
        templated: false,
    })];
    if !stringified {
        return TruthCondition::exact_with_memo(matches.remove(0), memo);
    }

    let mut candidate_texts = BTreeSet::from([target.to_string()]);
    let mut exact = true;
    for escape in lexical_escapes {
        let current = candidate_texts.iter().cloned().collect::<Vec<_>>();
        match escape {
            LexicalEscape::TrimPrefix(token) => {
                candidate_texts.extend(current.into_iter().map(|text| format!("{token}{text}")));
            }
            LexicalEscape::TrimSuffix(token) => {
                candidate_texts.extend(current.into_iter().map(|text| format!("{text}{token}")));
            }
            LexicalEscape::Contains(_) | LexicalEscape::CutAtToken(_) => exact = false,
        }
    }
    for text in &candidate_texts {
        let preimage = crate::value_path_context::stringified_equality_preimage(text);
        let covered_number = preimage
            .iter()
            .any(|value| matches!(value, GuardValue::Int(_) | GuardValue::Float(_)));
        if text.parse::<f64>().is_ok() && !covered_number {
            exact = false;
        }
        if text.starts_with('[') || text.starts_with("map[") {
            exact = false;
        }
        matches.extend(preimage.into_iter().filter_map(|value| {
            (!matches!(value, GuardValue::String(_))).then(|| {
                Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value,
                })
            })
        }));
    }
    let when_true = any_predicates_with_memo(matches, memo);
    let when_false = if exact {
        when_true.negated()
    } else {
        Predicate::False
    };
    TruthCondition::from_subsets_with_memo(when_true, when_false, exact, memo)
}

fn rendered_constant(parts: &[ScalarRenderPart]) -> Option<String> {
    let mut rendered = String::new();
    for part in parts {
        match part {
            ScalarRenderPart::Text(text) => rendered.push_str(text),
            ScalarRenderPart::Identity { .. } => return None,
        }
    }
    Some(rendered)
}

pub(crate) fn any_predicates_with_memo(
    predicates: Vec<helm_schema_core::Predicate>,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(helm_schema_core::Predicate::Or(predicates))
}

pub(crate) fn conjoin_predicates_with_memo(
    left: helm_schema_core::Predicate,
    right: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> Option<helm_schema_core::Predicate> {
    let predicate = memo.normalize(helm_schema_core::Predicate::And(vec![left, right]));
    (predicate != helm_schema_core::Predicate::False).then_some(predicate)
}

fn normalized_factored(
    predicate: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(predicate)
}

fn factored_negated(
    predicate: helm_schema_core::Predicate,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    memo.normalize(helm_schema_core::Predicate::Not(Box::new(predicate)))
}

pub(crate) const fn bool_predicate(value: bool) -> helm_schema_core::Predicate {
    if value {
        helm_schema_core::Predicate::True
    } else {
        helm_schema_core::Predicate::False
    }
}

/// Sound knowledge about the states in which a value is Helm-truthy or
/// Helm-falsy.
///
/// Partial conditions retain independently proven subsets of both polarities.
/// Consumers that require a complement still use [`Self::predicate`], which
/// exposes only an exact condition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum TruthCondition {
    /// Neither polarity has a known non-empty subset.
    #[default]
    Unknown,
    /// Sound subsets of the truthy and falsy domains, which need not be
    /// exhaustive.
    Partial {
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
    },
    /// The predicate is exact in both positive and negative polarity.
    Exact(helm_schema_core::Predicate),
}

impl TruthCondition {
    pub(crate) fn exact(predicate: helm_schema_core::Predicate) -> Self {
        Self::exact_with_memo(predicate, &PredicateMemo::new())
    }

    pub(crate) fn exact_with_memo(
        predicate: helm_schema_core::Predicate,
        memo: &PredicateMemo,
    ) -> Self {
        if predicate.contains_approximation() {
            Self::from_predicate_with_memo(predicate, memo)
        } else {
            Self::Exact(normalized_factored(predicate, memo))
        }
    }

    pub(crate) fn from_predicate(predicate: helm_schema_core::Predicate) -> Self {
        Self::from_predicate_with_memo(predicate, &PredicateMemo::new())
    }

    pub(crate) fn from_predicate_with_memo(
        predicate: helm_schema_core::Predicate,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        if !predicate.contains_approximation() {
            return Self::exact_with_memo(predicate, memo);
        }
        match predicate.kind() {
            helm_schema_core::PredicateKind::Approximate { sound_subset, .. } => {
                let when_true = sound_subset.cloned().unwrap_or(Predicate::False);
                Self::from_subsets_with_memo(when_true, Predicate::False, false, memo)
            }
            helm_schema_core::PredicateKind::Not(inner) => {
                Self::from_predicate_with_memo(inner.clone(), memo).negated_with_memo(memo)
            }
            helm_schema_core::PredicateKind::And(predicates) => Self::all_with_memo(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| Self::from_predicate_with_memo(predicate, memo)),
                memo,
            ),
            helm_schema_core::PredicateKind::Or(predicates) => Self::any_with_memo(
                predicates
                    .iter()
                    .cloned()
                    .map(|predicate| Self::from_predicate_with_memo(predicate, memo)),
                memo,
            ),
            _ => Self::Exact(predicate),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_subsets(
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
        complete: bool,
    ) -> Self {
        Self::from_subsets_with_memo(when_true, when_false, complete, &PredicateMemo::new())
    }

    pub(crate) fn from_subsets_with_memo(
        when_true: helm_schema_core::Predicate,
        when_false: helm_schema_core::Predicate,
        complete: bool,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let when_true = normalized_factored(when_true, memo);
        let when_false = normalized_factored(when_false, memo);
        let disjoint = memo.exactly_implies(
            &Predicate::all(vec![when_true.clone(), when_false.clone()]),
            &Predicate::False,
        );
        let exhaustive = memo.exactly_implies(
            &Predicate::True,
            &Predicate::Or(vec![when_true.clone(), when_false.clone()]),
        );
        if complete && disjoint && exhaustive {
            return Self::Exact(when_true);
        }
        if when_true == Predicate::True {
            return Self::Exact(Predicate::True);
        }
        if when_false == Predicate::True {
            return Self::Exact(Predicate::False);
        }
        if when_true == Predicate::False && when_false == Predicate::False {
            return Self::Unknown;
        }
        Self::Partial {
            when_true,
            when_false,
        }
    }

    pub(crate) fn predicate(&self) -> Option<&helm_schema_core::Predicate> {
        match self {
            Self::Unknown | Self::Partial { .. } => None,
            Self::Exact(predicate) => Some(predicate),
        }
    }

    pub(crate) fn when_true(&self) -> helm_schema_core::Predicate {
        match self {
            Self::Unknown => helm_schema_core::Predicate::False,
            Self::Partial { when_true, .. } | Self::Exact(when_true) => when_true.clone(),
        }
    }

    pub(crate) fn when_false(&self) -> helm_schema_core::Predicate {
        self.when_false_with_memo(&PredicateMemo::new())
    }

    pub(crate) fn when_false_with_memo(&self, memo: &PredicateMemo) -> helm_schema_core::Predicate {
        match self {
            Self::Unknown => helm_schema_core::Predicate::False,
            Self::Partial { when_false, .. } => when_false.clone(),
            Self::Exact(when_true) => factored_negated(when_true.clone(), memo),
        }
    }

    pub(crate) fn negated_with_memo(&self, memo: &PredicateMemo) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Partial {
                when_true,
                when_false,
            } => Self::from_subsets_with_memo(when_false.clone(), when_true.clone(), false, memo),
            Self::Exact(predicate) => Self::Exact(factored_negated(predicate.clone(), memo)),
        }
    }

    pub(crate) fn all_with_memo(
        conditions: impl IntoIterator<Item = Self>,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let conditions = conditions.into_iter().collect::<Vec<_>>();
        if conditions.is_empty() {
            return Self::Exact(Predicate::True);
        }
        if conditions
            .iter()
            .any(|condition| condition.predicate() == Some(&Predicate::False))
        {
            return Self::Exact(Predicate::False);
        }
        let complete = conditions
            .iter()
            .all(|condition| condition.predicate().is_some());
        let when_true = conjoin_many(conditions.iter().map(Self::when_true), memo);
        let when_false = any_predicates_with_memo(
            conditions
                .iter()
                .map(|condition| condition.when_false_with_memo(memo))
                .collect(),
            memo,
        );
        Self::from_subsets_with_memo(when_true, when_false, complete, memo)
    }

    pub(crate) fn any_with_memo(
        conditions: impl IntoIterator<Item = Self>,
        memo: &PredicateMemo,
    ) -> Self {
        use helm_schema_core::Predicate;

        let conditions = conditions.into_iter().collect::<Vec<_>>();
        if conditions.is_empty() {
            return Self::Exact(Predicate::False);
        }
        if conditions
            .iter()
            .any(|condition| condition.predicate() == Some(&Predicate::True))
        {
            return Self::Exact(Predicate::True);
        }
        let complete = conditions
            .iter()
            .all(|condition| condition.predicate().is_some());
        let when_true =
            any_predicates_with_memo(conditions.iter().map(Self::when_true).collect(), memo);
        let when_false = conjoin_many(
            conditions
                .iter()
                .map(|condition| condition.when_false_with_memo(memo)),
            memo,
        );
        Self::from_subsets_with_memo(when_true, when_false, complete, memo)
    }
}

fn conjoin_many(
    predicates: impl IntoIterator<Item = helm_schema_core::Predicate>,
    memo: &PredicateMemo,
) -> helm_schema_core::Predicate {
    let mut condition = helm_schema_core::Predicate::True;
    for predicate in predicates {
        let Some(joined) = conjoin_predicates_with_memo(condition, predicate, memo) else {
            return helm_schema_core::Predicate::False;
        };
        condition = joined;
    }
    condition
}
