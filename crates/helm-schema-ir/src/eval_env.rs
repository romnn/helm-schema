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

/// One evaluated Helm value with the boundary it crossed to reach a binding.
///
/// `value` is provenance and shape, `mode` is the Go evaluation boundary, and
/// `metadata` holds the value facts that leaf owns. They travel as one payload
/// because a transform can keep a source path while changing the value, so a
/// consumer that reads one without the others reads a value that never existed.
/// The fields are private for exactly that reason: a consumer reaches them
/// through the leaf, never apart from it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct BindingValue {
    value: AbstractValue,
    mode: BindingEvaluationMode,
    metadata: BindingValueMetadata,
}

impl BindingValue {
    /// The leaf's provenance and shape, exactly as the binding recorded it.
    ///
    /// The transform program in [`Self::metadata`] is NOT folded in here:
    /// stamping it onto the value (`AbstractValue::with_output_meta`) turns a
    /// raw values path into an `OutputPath` that claims input identity, and
    /// every requirement lane then reads a derivation the chart never made
    /// (a `toString` local lost its stringified placement, a `typeIs` over
    /// `default .x .y` lost the fail validator on `y`). Consumers that need
    /// the program read it beside the value, through the metadata.
    pub(crate) fn value(&self) -> &AbstractValue {
        &self.value
    }

    pub(crate) fn mode(&self) -> BindingEvaluationMode {
        self.mode
    }

    pub(crate) fn metadata(&self) -> &BindingValueMetadata {
        &self.metadata
    }
}

/// How a leaf's selection is known.
///
/// A decision the interpreter could not decode still HAS branches: their
/// leaves are candidates whose selection condition is not provable. That is
/// distinct both from "selected exactly where this predicate holds" and from
/// "no value at all", and collapsing it into either is how an undecidable
/// guard used to delete the values it guards.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum LeafSelection {
    /// The leaf is selected exactly where the predicate holds.
    Proven(Predicate),
    /// The leaf is one of the candidates; no selection condition is provable.
    Unproven,
}

/// One reachable leaf of a binding, with the selection that reaches it.
///
/// A [`BindingValue`] is only reachable through this pair, so no consumer can
/// observe a leaf value without the selection, the structural source and the
/// evaluation boundary that produced it.
pub(crate) struct BindingLeaf<'a> {
    pub(crate) selection: LeafSelection,
    pub(crate) value: &'a BindingValue,
}

/// Every reachable leaf of a binding, in decision order, plus whether some
/// branch is unresolved (an `Unknown` node, or a decision whose truth is not
/// exact).
pub(crate) struct BindingLeaves<'a> {
    pub(crate) known: Vec<BindingLeaf<'a>>,
    pub(crate) has_unresolved: bool,
    /// a branch whose VALUE is unmodeled (`BindingNode::Unknown`), as
    /// distinct from a branch whose SELECTION is undecidable. Only the former
    /// means the candidate set is incomplete: two known values under an
    /// undecidable decision are still the only two values the chart can
    /// select.
    pub(crate) has_unknown_value: bool,
}

impl<'a> BindingLeaves<'a> {
    /// The proven leaves in stable `(condition, value)` order, without
    /// duplicates: two branches that select the same value under the same
    /// condition are one alternative.
    ///
    /// Strict consumers walk this lane only, so a leaf whose selection is not
    /// provable never becomes evidence for a requirement.
    pub(crate) fn proven(&self) -> Vec<(&Predicate, &'a BindingValue)> {
        let mut proven = Vec::new();
        for leaf in &self.known {
            if let LeafSelection::Proven(condition) = &leaf.selection {
                proven.push((condition, leaf.value));
            }
        }
        proven.sort_unstable();
        proven.dedup();
        proven
    }

    /// The value lane: every value this read can select, in decision order,
    /// joined as one choice, with `Unknown` appended iff some branch's VALUE
    /// is unmodelled.
    ///
    /// A leaf whose selection is not provable is still a value the chart
    /// produces, so deleting it narrows the read to a claim the chart never
    /// made; scoping an obligation by a selection nobody proved is the
    /// opposite error, which is why the strict lanes walk [`Self::proven`]
    /// instead. An undecidable DECISION over known values adds no third
    /// value: both branches are already candidates here.
    ///
    /// The join erases each leaf's selection, boundary and transform program
    /// by construction; a consumer that needs those walks `known`.
    pub(crate) fn joined_value(&self) -> Option<AbstractValue> {
        let mut values = Vec::with_capacity(self.known.len() + usize::from(self.has_unknown_value));
        for leaf in &self.known {
            values.push(leaf.value.value().clone());
        }
        if self.has_unknown_value {
            values.push(AbstractValue::Unknown);
        }
        AbstractValue::choice(values)
    }

    /// Whether any candidate crossed the Go evaluation boundary, including an
    /// unmodelled branch whose boundary cannot be named.
    pub(crate) fn has_evaluated_candidate(&self) -> bool {
        self.has_unknown_value
            || self
                .known
                .iter()
                .any(|leaf| leaf.value.mode() == BindingEvaluationMode::Evaluated)
    }
}

/// The selection of one branch of a decision, or `None` when the branch is
/// provably unreachable under the enclosing selection.
///
/// A decision that proves nothing about a branch (`Predicate::False`, which
/// is what an unknown or one-sided truth reports) leaves that branch's leaves
/// as candidates instead of deleting them: the value still reaches the
/// consumer, only its selection condition cannot be named.
fn branch_selection(
    selection: &LeafSelection,
    branch: Predicate,
    memo: &PredicateMemo,
) -> Option<LeafSelection> {
    if branch == Predicate::False {
        return Some(LeafSelection::Unproven);
    }
    match selection {
        LeafSelection::Proven(condition) => {
            conjoin_predicates_with_memo(condition.clone(), branch, memo).map(LeafSelection::Proven)
        }
        LeafSelection::Unproven => Some(LeafSelection::Unproven),
    }
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
    /// Boxed because a leaf payload carries a whole `AbstractValue`: inline
    /// it would set the size of every `Select` and `Unknown` node too.
    Value(Box<BindingValue>),
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
            (BindingNode::Value(left), BindingNode::Value(right)) => left == right,
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
        Self(Rc::new(BindingNode::Value(Box::new(BindingValue {
            value,
            mode,
            metadata,
        }))))
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

    /// Every leaf this binding can select, in decision order.
    ///
    /// A leaf is reported whether or not its selection is provable, so an
    /// undecidable decision widens the result instead of deleting the values
    /// it guards. Only a branch the enclosing selection proves unreachable is
    /// dropped.
    pub(crate) fn leaves(&self, memo: &PredicateMemo) -> BindingLeaves<'_> {
        fn visit<'a>(
            binding: &'a LocalBinding,
            selection: LeafSelection,
            memo: &PredicateMemo,
            leaves: &mut BindingLeaves<'a>,
        ) {
            match binding.0.as_ref() {
                BindingNode::Value(value) => leaves.known.push(BindingLeaf { selection, value }),
                BindingNode::Select {
                    decision,
                    when_true,
                    when_false,
                } => {
                    if let Some(selection) =
                        branch_selection(&selection, decision.truth.when_true(), memo)
                    {
                        visit(when_true, selection, memo, leaves);
                    }
                    if let Some(selection) = branch_selection(
                        &selection,
                        decision.truth.when_false_with_memo(memo),
                        memo,
                    ) {
                        visit(when_false, selection, memo, leaves);
                    }
                    if decision.truth.predicate().is_none() {
                        leaves.has_unresolved = true;
                    }
                }
                BindingNode::Unknown => {
                    leaves.has_unresolved = true;
                    leaves.has_unknown_value = true;
                }
            }
        }

        let mut leaves = BindingLeaves {
            known: Vec::new(),
            has_unresolved: false,
            has_unknown_value: false,
        };
        visit(
            self,
            LeafSelection::Proven(Predicate::True),
            memo,
            &mut leaves,
        );
        leaves
    }

    /// The condition-erased join of every reachable leaf: the same value
    /// lane a read of this binding reports, so a context built from bindings
    /// and a read of one binding cannot disagree.
    pub(crate) fn value(&self, memo: &PredicateMemo) -> Option<AbstractValue> {
        self.leaves(memo).joined_value()
    }

    pub(crate) fn paths(&self, memo: &PredicateMemo) -> BTreeSet<helm_schema_core::ValuesPath> {
        self.value(memo)
            .map_or_else(BTreeSet::new, |value| value.paths())
    }

    pub(crate) fn with_overlay_entries(&self, entries: &BTreeMap<String, AbstractValue>) -> Self {
        self.map_values(|value| value.with_overlay_entries(entries.clone()))
    }

    pub(crate) fn map_values(&self, mut map: impl FnMut(AbstractValue) -> AbstractValue) -> Self {
        fn map_node(
            binding: &LocalBinding,
            map: &mut impl FnMut(AbstractValue) -> AbstractValue,
        ) -> LocalBinding {
            match binding.0.as_ref() {
                BindingNode::Value(leaf) => LocalBinding::new_with_metadata(
                    map(leaf.value.clone()),
                    leaf.mode,
                    leaf.metadata.clone(),
                ),
                BindingNode::Select {
                    decision,
                    when_true,
                    when_false,
                } => LocalBinding::select(
                    Rc::clone(decision),
                    map_node(when_true, map),
                    map_node(when_false, map),
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
        predicate_memo: &Rc<PredicateMemo>,
    ) -> Self {
        Self {
            dot: current_dot.cloned(),
            dot_binding_mode,
            root_fields: locals
                .iter()
                .filter_map(|(name, binding)| {
                    binding
                        .value(predicate_memo)
                        .map(|value| (name.clone(), value))
                })
                .collect(),
            locals: locals.clone(),
            allow_field_root_lookup: false,
            predicate_memo: Rc::clone(predicate_memo),
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
                .insert(name.clone(), value.with_overlay_entries(entries));
            applied = true;
        }
        applied
    }
}
