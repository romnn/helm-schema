use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::MemberHostConversion;
use crate::function_semantics::ArgumentEvaluationMode;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition, conjoin_predicates_with_memo};
use helm_schema_core::{Predicate, PredicateMemo};

/// How a template value reached a dot or variable binding.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum BindingEvaluationMode {
    #[default]
    Direct,
    Evaluated,
}

/// Reusable value facts owned by one binding leaf.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct BindingValueMetadata {
    pub(crate) default_paths: BTreeSet<helm_schema_core::ValuesPath>,
    pub(crate) output_meta: BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>,
}

/// One proven binding value with its evaluation boundary.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct LocalBindingAlternative {
    pub(crate) condition: Predicate,
    pub(crate) value: AbstractValue,
    pub(crate) mode: BindingEvaluationMode,
    pub(crate) metadata: BindingValueMetadata,
}

/// One control decision shared by the bindings selected at that activation.
#[derive(Debug)]
pub(crate) struct BindingDecision {
    truth: TruthCondition,
}

impl BindingDecision {
    pub(crate) fn new(truth: TruthCondition) -> Rc<Self> {
        Rc::new(Self { truth })
    }
}

/// A local binding whose decision leaves own both value and evaluation boundary.
#[derive(Clone, Debug)]
pub(crate) struct LocalBinding(Rc<BindingNode>);

#[derive(Debug)]
enum BindingNode {
    Value {
        value: AbstractValue,
        mode: BindingEvaluationMode,
        metadata: BindingValueMetadata,
    },
    Select {
        decision: Rc<BindingDecision>,
        when_true: LocalBinding,
        when_false: LocalBinding,
    },
    Unknown,
}

impl PartialEq for LocalBinding {
    fn eq(&self, other: &Self) -> bool {
        if Rc::ptr_eq(&self.0, &other.0) {
            return true;
        }
        match (self.0.as_ref(), other.0.as_ref()) {
            (
                BindingNode::Value {
                    value: left_value,
                    mode: left_mode,
                    metadata: left_metadata,
                },
                BindingNode::Value {
                    value: right_value,
                    mode: right_mode,
                    metadata: right_metadata,
                },
            ) => {
                left_value == right_value
                    && left_mode == right_mode
                    && left_metadata == right_metadata
            }
            (
                BindingNode::Select {
                    decision: left_decision,
                    when_true: left_true,
                    when_false: left_false,
                },
                BindingNode::Select {
                    decision: right_decision,
                    when_true: right_true,
                    when_false: right_false,
                },
            ) => {
                Rc::ptr_eq(left_decision, right_decision)
                    && left_true == right_true
                    && left_false == right_false
            }
            (BindingNode::Unknown, BindingNode::Unknown) => true,
            _ => false,
        }
    }
}

impl Eq for LocalBinding {}

pub(crate) struct LocalBindingProjection {
    pub(crate) alternatives: BTreeSet<LocalBindingAlternative>,
    pub(crate) has_unresolved: bool,
}

impl LocalBinding {
    pub(crate) fn direct(value: AbstractValue) -> Self {
        Self::new(value, BindingEvaluationMode::Direct)
    }

    pub(crate) fn evaluated(value: AbstractValue) -> Self {
        Self::new(value, BindingEvaluationMode::Evaluated)
    }

    pub(crate) fn new(value: AbstractValue, mode: BindingEvaluationMode) -> Self {
        Self::new_with_metadata(value, mode, BindingValueMetadata::default())
    }

    pub(crate) fn new_with_metadata(
        value: AbstractValue,
        mode: BindingEvaluationMode,
        metadata: BindingValueMetadata,
    ) -> Self {
        Self(Rc::new(BindingNode::Value {
            value,
            mode,
            metadata,
        }))
    }

    pub(crate) fn unknown() -> Self {
        Self(Rc::new(BindingNode::Unknown))
    }

    pub(crate) fn select(decision: Rc<BindingDecision>, when_true: Self, when_false: Self) -> Self {
        if when_true == when_false {
            return when_true;
        }
        match decision.truth.predicate() {
            Some(predicate) if predicate == &Predicate::True => when_true,
            Some(predicate) if predicate == &Predicate::False => when_false,
            _ => Self(Rc::new(BindingNode::Select {
                decision,
                when_true,
                when_false,
            })),
        }
    }

    pub(crate) fn unresolved_with_candidate(
        decision: Rc<BindingDecision>,
        candidate: Self,
    ) -> Self {
        Self::select(decision, candidate, Self::unknown())
    }

    pub(crate) fn projection(&self, memo: &PredicateMemo) -> LocalBindingProjection {
        fn visit(
            binding: &LocalBinding,
            condition: Predicate,
            memo: &PredicateMemo,
            projection: &mut LocalBindingProjection,
        ) {
            match binding.0.as_ref() {
                BindingNode::Value {
                    value,
                    mode,
                    metadata,
                } => {
                    projection.alternatives.insert(LocalBindingAlternative {
                        condition,
                        value: value.clone(),
                        mode: *mode,
                        metadata: metadata.clone(),
                    });
                }
                BindingNode::Select {
                    decision,
                    when_true,
                    when_false,
                } => {
                    let when_true_condition = conjoin_predicates_with_memo(
                        condition.clone(),
                        decision.truth.when_true(),
                        memo,
                    );
                    let when_false_condition = conjoin_predicates_with_memo(
                        condition,
                        decision.truth.when_false_with_memo(memo),
                        memo,
                    );
                    if let Some(condition) = when_true_condition {
                        visit(when_true, condition, memo, projection);
                    }
                    if let Some(condition) = when_false_condition {
                        visit(when_false, condition, memo, projection);
                    }
                    if decision.truth.predicate().is_none() {
                        projection.has_unresolved = true;
                    }
                }
                BindingNode::Unknown => projection.has_unresolved = true,
            }
        }

        let mut projection = LocalBindingProjection {
            alternatives: BTreeSet::new(),
            has_unresolved: false,
        };
        visit(self, Predicate::True, memo, &mut projection);
        projection
    }

    pub(crate) fn value(&self) -> Option<AbstractValue> {
        fn collect(binding: &LocalBinding, values: &mut Vec<AbstractValue>) {
            match binding.0.as_ref() {
                BindingNode::Value { value, .. } => values.push(value.clone()),
                BindingNode::Select {
                    when_true,
                    when_false,
                    ..
                } => {
                    collect(when_true, values);
                    collect(when_false, values);
                }
                BindingNode::Unknown => values.push(AbstractValue::Unknown),
            }
        }

        let mut values = Vec::new();
        collect(self, &mut values);
        AbstractValue::choice(values)
    }

    pub(crate) fn paths(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        self.value()
            .map_or_else(BTreeSet::new, |value| value.paths())
    }

    pub(crate) fn with_overlay_entries(self, entries: BTreeMap<String, AbstractValue>) -> Self {
        self.map_values(|value| value.with_overlay_entries(entries.clone()))
    }

    pub(crate) fn map_values(self, mut map: impl FnMut(AbstractValue) -> AbstractValue) -> Self {
        fn map_node(
            binding: LocalBinding,
            map: &mut impl FnMut(AbstractValue) -> AbstractValue,
        ) -> LocalBinding {
            match binding.0.as_ref() {
                BindingNode::Value {
                    value,
                    mode,
                    metadata,
                } => LocalBinding::new_with_metadata(map(value.clone()), *mode, metadata.clone()),
                BindingNode::Select {
                    decision,
                    when_true,
                    when_false,
                } => LocalBinding::select(
                    Rc::clone(decision),
                    map_node(when_true.clone(), map),
                    map_node(when_false.clone(), map),
                ),
                BindingNode::Unknown => LocalBinding::unknown(),
            }
        }

        map_node(self, &mut map)
    }

    pub(crate) fn join_unconditioned(bindings: Vec<&Self>) -> Self {
        let Some((first, rest)) = bindings.split_first() else {
            return Self::unknown();
        };
        if rest.iter().all(|binding| *binding == *first) {
            return (*first).clone();
        }
        let mut bindings = bindings.into_iter().rev();
        let Some(last) = bindings.next() else {
            return Self::unknown();
        };
        let mut joined = last.clone();
        for binding in bindings {
            joined = Self::select(
                BindingDecision::new(TruthCondition::Unknown),
                binding.clone(),
                joined,
            );
        }
        joined
    }

    pub(crate) fn has_evaluated_value(&self) -> bool {
        match self.0.as_ref() {
            BindingNode::Value { mode, .. } => *mode == BindingEvaluationMode::Evaluated,
            BindingNode::Select {
                when_true,
                when_false,
                ..
            } => when_true.has_evaluated_value() || when_false.has_evaluated_value(),
            BindingNode::Unknown => true,
        }
    }
}

/// Abstract interpreter environment for Helm expression evaluation.
#[derive(Clone, Debug, Default)]
pub(crate) struct EvalEnv {
    pub(crate) dot: Option<AbstractValue>,
    pub(crate) dot_binding_mode: BindingEvaluationMode,
    pub(crate) root_fields: HashMap<String, AbstractValue>,
    pub(crate) root_truthy_predicates: HashMap<String, Predicate>,
    pub(crate) root_value_dispatches: HashMap<String, ScalarValueDispatch>,
    /// The root-field semantic maps describe the current dot rather than
    /// only the global root. Bound helpers set this for their call argument
    /// frame; nested `with`/`range` frames clear it.
    pub(crate) root_field_semantics_on_current_dot: bool,
    pub(crate) locals: HashMap<String, LocalBinding>,
    /// Exact scalar values carried by locals, including branch-dependent
    /// reassignments. This is separate from `locals`: an abstract fragment
    /// identifies where a value came from, while a scalar dispatch identifies
    /// the runtime value that equality and truthiness consume.
    pub(crate) local_scalar_dispatches: HashMap<String, ScalarValueDispatch>,
    pub(crate) local_default_paths: HashMap<String, BTreeSet<helm_schema_core::ValuesPath>>,
    pub(crate) local_output_meta:
        HashMap<String, BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Structural conditions under which a local is truthy, as the fragment
    /// interpreter reduced them. A boolean flag carries no values-path
    /// identity, so short-circuit operand truthiness has no other way to
    /// decode it.
    pub(crate) local_truthy_reductions: HashMap<String, Predicate>,
    pub(crate) member_host_conversions: BTreeSet<MemberHostConversion>,
    pub(crate) active_predicates: Vec<Predicate>,
    pub(crate) bound_values: BoundValueContext,
    pub(crate) allow_field_root_lookup: bool,
    pub(crate) skip_helper_call_args: bool,
    pub(crate) predicate_memo: Rc<PredicateMemo>,
}

impl EvalEnv {
    /// Returns the parameter evaluation mode from AST shape and current-dot provenance.
    #[must_use]
    pub(crate) fn argument_evaluation_mode(&self, expr: &TemplateExpr) -> ArgumentEvaluationMode {
        match expr {
            TemplateExpr::Field(path)
                if self.dot_binding_mode == BindingEvaluationMode::Evaluated =>
            {
                if path.is_empty() {
                    ArgumentEvaluationMode::Evaluated
                } else {
                    ArgumentEvaluationMode::GroupedReceiverLookup {
                        selected_segments: path.len(),
                    }
                }
            }
            TemplateExpr::Field(_) => ArgumentEvaluationMode::DirectLookup,
            TemplateExpr::Selector { operand, path }
                if matches!(operand.as_ref(), TemplateExpr::Parenthesized(_)) =>
            {
                ArgumentEvaluationMode::GroupedReceiverLookup {
                    selected_segments: path.len(),
                }
            }
            TemplateExpr::Variable(_) | TemplateExpr::Selector { .. } => {
                ArgumentEvaluationMode::DirectLookup
            }
            TemplateExpr::Literal(_)
            | TemplateExpr::Call { .. }
            | TemplateExpr::Pipeline(_)
            | TemplateExpr::Parenthesized(_)
            | TemplateExpr::VariableDefinition { .. }
            | TemplateExpr::Assignment { .. }
            | TemplateExpr::Unknown(_) => ArgumentEvaluationMode::Evaluated,
        }
    }

    /// Projects root-context aliases through the current call's exact root fields.
    pub(crate) fn value_at_path(
        &self,
        value: &AbstractValue,
        path: &[String],
    ) -> Option<AbstractValue> {
        if matches!(value, AbstractValue::RootContext)
            && let Some((head, tail)) = path.split_first()
            && head != "Values"
        {
            return self.root_fields.get(head)?.apply_to_path(tail);
        }
        value.apply_to_path(path)
    }

    pub(crate) fn from_helper_context(
        bindings: Option<&HashMap<String, AbstractValue>>,
        current_dot: Option<&AbstractValue>,
        dot_binding_mode: BindingEvaluationMode,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            dot_binding_mode,
            root_fields: bindings.cloned().unwrap_or_default(),
            allow_field_root_lookup: true,
            ..Self::default()
        }
    }

    pub(crate) fn from_fragment_context(
        locals: &HashMap<String, LocalBinding>,
        current_dot: Option<&AbstractValue>,
        dot_binding_mode: BindingEvaluationMode,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            dot_binding_mode,
            root_fields: locals
                .iter()
                .filter_map(|(name, binding)| binding.value().map(|value| (name.clone(), value)))
                .collect(),
            locals: locals.clone(),
            allow_field_root_lookup: false,
            ..Self::default()
        }
    }

    pub(crate) fn without_helper_call_args(mut self) -> Self {
        self.skip_helper_call_args = true;
        self
    }

    pub(crate) fn with_predicate_memo(mut self, predicate_memo: Rc<PredicateMemo>) -> Self {
        self.predicate_memo = predicate_memo;
        self
    }

    pub(crate) fn apply_local_set_mutations(
        &mut self,
        mutations: &BTreeMap<String, BTreeMap<String, AbstractValue>>,
    ) -> bool {
        let mut applied = false;
        for (name, entries) in mutations {
            let Some(value) = self.locals.remove(name) else {
                continue;
            };
            self.locals
                .insert(name.clone(), value.with_overlay_entries(entries.clone()));
            applied = true;
        }
        applied
    }
}
