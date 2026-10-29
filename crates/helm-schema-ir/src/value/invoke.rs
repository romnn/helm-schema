use super::eval::{Executor, normal};
use super::kinds::GoType;
use super::operations::TransferDemand;
use super::{
    AbortReason, ArgumentForm, Callee, Coverage, Datum, Exit, Flow, Invocation, Leaf, Normal,
    Operand, State, Value, ValueNode,
};
use super::{CallOutcome, CompletedExecution, CompletionInput, Execution};
use crate::function_semantics::{SuccessType, function_semantics};
use helm_schema_ast::TemplateExpr;
use helm_schema_core::{Guard, GuardValue, Predicate};
use std::rc::Rc;

struct Assembled {
    state: State,
    values: Vec<Operand>,
    pipe: Option<Value>,
}
struct ArgumentAssembly<'a> {
    next: &'a mut AssembledBuffer,
    exits: &'a mut super::ExitBuffer,
}
struct ValuePlan {
    value: Value,
    normal: Rc<super::FrozenTest>,
    failures: std::collections::BTreeMap<AbortReason, Rc<super::FrozenTest>>,
}

fn transfers_values(invocation: &Invocation) -> bool {
    let function = match &invocation.callee {
        Callee::Function(function) => function,
        Callee::Method {
            receiver: _,
            descriptor,
        } => {
            return descriptor.semantics().heap_effect
                == crate::function_semantics::HeapEffect::ReadOnly;
        }
    };
    if matches!(function.as_str(), "eq" | "ne") {
        return false;
    }
    let semantics = function_semantics(function);
    if semantics.heap_effect != crate::function_semantics::HeapEffect::ReadOnly
        || semantics.input_read == crate::function_semantics::InputRead::Keyed
    {
        return false;
    }
    matches!(function.as_str(), "default" | "coalesce" | "ternary")
        || matches!(
            semantics.success,
            SuccessType::Fixed(
                GoType::Nil
                    | GoType::Bool
                    | GoType::String
                    | GoType::Int
                    | GoType::Int64
                    | GoType::Uint64
                    | GoType::Float64
            )
        )
}

impl Executor<'_> {
    #[tracing::instrument(skip_all, fields(function, site))]
    pub(super) fn invoke(
        &self,
        function: &str,
        arguments: &[TemplateExpr],
        pipe: Option<Value>,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        if let Some(receiver) = self.db.receiver_call(function) {
            return self.invoke_receiver(receiver, arguments, pipe.as_ref(), state, site);
        }
        self.invoke_callee(
            Callee::Function(function.to_owned()),
            arguments,
            pipe,
            state,
            site,
        )
    }

    pub(super) fn invoke_receiver(
        &self,
        receiver: crate::analysis_db::ReceiverCall,
        arguments: &[TemplateExpr],
        pipe: Option<&Value>,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        let crate::analysis_db::ReceiverCall { receiver, name } = receiver;
        let mut exits = super::ExitBuffer::default();
        for exit in self.expression(&receiver, state, site) {
            match exit.flow {
                Flow::Normal(normal) => {
                    let receiver = exit.state.consumed(&normal.value);
                    let descriptor = super::hosts::method(&receiver, &name);
                    exits.extend(self.invoke_callee(
                        Callee::Method {
                            receiver,
                            descriptor,
                        },
                        arguments,
                        pipe.cloned(),
                        exit.state,
                        site,
                    ));
                }
                flow => exits.push(Exit {
                    state: exit.state,
                    flow,
                }),
            }
        }
        exits.finish()
    }

    pub(super) fn invoke_callee(
        &self,
        callee: Callee,
        arguments: &[TemplateExpr],
        pipe: Option<Value>,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        let count = arguments.len() + usize::from(pipe.is_some());
        let semantics = callee.semantics();
        let function = callee.function_name();
        if (semantics.accepts_arity(count) == Some(false)
            && !(function == Some("template") && count == 1))
            || (matches!(function, Some("and" | "or")) && count == 0)
        {
            return vec![Exit {
                state,
                flow: Flow::Abort(AbortReason::Arity),
            }];
        }
        if let Some(function @ ("and" | "or")) = function {
            let exits = self.short_circuit(function, arguments, pipe.as_ref(), state, site);
            return self
                .complete(CompletionInput::Executed {
                    invocation: Invocation {
                        callee,
                        operands: Vec::new(),
                        site: site as u64,
                    },
                    execution: Execution { exits },
                    helper_return: None,
                })
                .into_exits();
        }
        let mut pending = vec![Assembled {
            state,
            values: Vec::new(),
            pipe,
        }];
        let mut exits = super::ExitBuffer::default();
        for (index, expression) in arguments.iter().enumerate() {
            let mut next = AssembledBuffer::default();
            for assembled in pending {
                self.assemble_argument(
                    expression,
                    assembled,
                    semantics.parameter(index),
                    site,
                    &mut ArgumentAssembly {
                        next: &mut next,
                        exits: &mut exits,
                    },
                );
            }
            pending = next.finish();
        }
        for assembled in pending {
            let validated = if let Some(pipe) = assembled.pipe {
                super::kinds::validate_parameter(
                    assembled.state,
                    pipe,
                    semantics.parameter(arguments.len()),
                    ArgumentForm::Evaluated,
                )
            } else {
                normal(assembled.state, Value::nil())
            };
            for exit in validated {
                match exit.flow {
                    Flow::Normal(normal) => {
                        let mut operands = assembled.values.clone();
                        if count > arguments.len() {
                            operands.push(Operand {
                                value: normal.value,
                                site: site as u64,
                                form: ArgumentForm::Evaluated,
                            });
                        }
                        for operand in &mut operands {
                            operand.value = exit.state.consumed(&operand.value);
                        }
                        exits.extend(
                            self.complete(CompletionInput::Invocation {
                                invocation: Invocation {
                                    callee: consumed_callee(&callee, &exit.state),
                                    operands,
                                    site: site as u64,
                                },
                                state: exit.state,
                            })
                            .into_exits(),
                        );
                    }
                    flow => exits.push(Exit {
                        state: exit.state,
                        flow,
                    }),
                }
            }
        }
        exits.finish()
    }
    fn assemble_argument(
        &self,
        expression: &TemplateExpr,
        assembled: Assembled,
        expected: Option<GoType>,
        site: usize,
        buffers: &mut ArgumentAssembly<'_>,
    ) {
        for exit in self.expression(expression, assembled.state, site) {
            match exit.flow {
                Flow::Normal(normal) => {
                    let value = match contextual_literal(expression, expected) {
                        Ok(Some(value)) => value,
                        Ok(None) => normal.value,
                        Err(reason) => {
                            buffers.exits.push(Exit {
                                state: exit.state,
                                flow: Flow::Abort(reason),
                            });
                            continue;
                        }
                    };
                    for validated in super::kinds::validate_parameter(
                        exit.state,
                        value,
                        expected,
                        argument_form(expression),
                    ) {
                        match validated.flow {
                            Flow::Normal(normal) => {
                                let mut values = assembled.values.clone();
                                values.push(Operand {
                                    value: normal.value,
                                    site: site as u64,
                                    form: argument_form(expression),
                                });
                                buffers.next.push(Assembled {
                                    state: validated.state,
                                    values,
                                    pipe: assembled.pipe.clone(),
                                });
                            }
                            Flow::Abort(reason) => buffers.exits.push(Exit {
                                state: validated.state,
                                flow: Flow::Abort(reason),
                            }),
                            Flow::Break(loop_id) => buffers.exits.push(Exit {
                                state: validated.state,
                                flow: Flow::Break(loop_id),
                            }),
                            Flow::Continue(loop_id) => buffers.exits.push(Exit {
                                state: validated.state,
                                flow: Flow::Continue(loop_id),
                            }),
                        }
                    }
                }
                flow => buffers.exits.push(Exit {
                    state: exit.state,
                    flow,
                }),
            }
        }
    }

    fn short_circuit(
        &self,
        function: &str,
        arguments: &[TemplateExpr],
        pipe: Option<&Value>,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        let mut pending = vec![state];
        let mut result = super::ExitBuffer::default();
        for (index, expression) in arguments.iter().enumerate() {
            let mut next = super::StateBuffer::default();
            for state in pending {
                for exit in self.expression(expression, state, site) {
                    match exit.flow {
                        Flow::Normal(normal) => {
                            if index + 1 == arguments.len() && pipe.is_none() {
                                result.extend(super::eval::normal(exit.state, normal.value));
                                continue;
                            }
                            let test = exit.state.test(&normal.value);
                            for taken in [true, false] {
                                if (taken && test.when_true == Predicate::False
                                    || !taken && test.when_false == Predicate::False)
                                    && test.coverage == Coverage::Exact
                                {
                                    continue;
                                }
                                let state = exit.state.fork(Rc::clone(&test), taken);
                                if state.path_is_infeasible() {
                                    continue;
                                }
                                if taken == (function == "or") {
                                    result.extend(super::eval::normal(state, normal.value.clone()));
                                } else {
                                    next.push(state);
                                }
                            }
                        }
                        flow => result.push(Exit {
                            state: exit.state,
                            flow,
                        }),
                    }
                }
            }
            pending = next.finish();
        }
        for state in pending {
            result.extend(normal(state, pipe.cloned().unwrap_or_else(Value::nil)));
        }
        result.finish()
    }
    #[tracing::instrument(skip_all)]
    pub(super) fn complete(&self, input: CompletionInput) -> CallOutcome {
        let completed = match input {
            CompletionInput::Invocation { invocation, state } => {
                let mut remaining = super::MAX_SCALAR_DISPATCH_STATES;
                if matches!(
                    invocation.callee.function_name(),
                    Some("include" | "template" | "tpl")
                ) {
                    return CallOutcome {
                        execution: Execution {
                            exits: self.helper_alternatives(invocation, state, &mut remaining),
                        },
                    };
                }
                let exits = if transfers_values(&invocation) {
                    let mut state = state;
                    // Each operand has one truth query across mutually exclusive value cases.
                    // Different operands and invocations retain independent fallback choices.
                    let mut queries = std::collections::BTreeMap::new();
                    let plan =
                        transfer_values(&invocation, &mut state, &mut remaining, &mut queries);
                    finish_value_plan(plan, state)
                } else if matches!(
                    invocation.callee.function_name(),
                    Some("eq" | "ne" | "list" | "tuple")
                ) {
                    Self::transfer_invocation(&invocation, state, &mut remaining)
                } else {
                    Self::transfer_alternatives(&invocation, state, &mut remaining)
                };
                CompletedExecution {
                    invocation,
                    execution: Execution { exits },
                    helper_return: None,
                }
            }
            CompletionInput::Executed {
                invocation,
                execution,
                helper_return,
            } => CompletedExecution {
                invocation,
                execution,
                helper_return,
            },
            CompletionInput::Summarized {
                plan,
                invocation,
                state,
            } => {
                let applicable = plan.applicable(invocation.operands.len());
                tracing::trace!(
                    applicable,
                    "source plan declines until its transfer is certified"
                );
                return self.helper(invocation, state, false);
            }
        };
        completed.finish()
    }

    fn helper_alternatives(
        &self,
        mut invocation: Invocation,
        state: State,
        remaining: &mut usize,
    ) -> Vec<Exit> {
        if *remaining > 0 {
            *remaining -= 1;
            if let Some(operand) = invocation.operands.first_mut() {
                operand.value = state.consumed(&operand.value);
                if let ValueNode::Select {
                    decision,
                    yes: _,
                    no: _,
                } = operand.value.node.as_ref()
                {
                    let decision = Rc::clone(decision);
                    let mut exits = super::ExitBuffer::default();
                    for taken in [true, false] {
                        let branch = state.fork(Rc::clone(&decision), taken);
                        if !branch.path_is_infeasible() {
                            exits.extend(self.helper_alternatives(
                                invocation.clone(),
                                branch,
                                remaining,
                            ));
                        }
                    }
                    return exits.finish();
                }
            }
        }
        self.helper(invocation, state, true).into_exits()
    }
    fn transfer_alternatives(
        invocation: &Invocation,
        state: State,
        remaining: &mut usize,
    ) -> Vec<Exit> {
        if *remaining == 0 {
            let mut state = state;
            let influences = super::operations::inputs(
                &invocation
                    .operands
                    .iter()
                    .map(|operand| operand.value.clone())
                    .collect::<Vec<_>>(),
                &state,
            );
            super::operations::invalidate(&mut state);
            return normal(state, Value::influenced(influences));
        }
        *remaining -= 1;
        let stride = if invocation.callee.function_name() == Some("dict") {
            2
        } else {
            1
        };
        for operand in invocation.operands.iter().step_by(stride) {
            let value = state.consumed(&operand.value);
            if let ValueNode::Select {
                decision,
                yes: _,
                no: _,
            } = value.node.as_ref()
            {
                let mut result = super::ExitBuffer::default();
                for taken in [true, false] {
                    let branch = state.fork(Rc::clone(decision), taken);
                    if !branch.path_is_infeasible() {
                        let mut selected = invocation.clone();
                        for operand in &mut selected.operands {
                            operand.value = branch.consumed(&operand.value);
                        }
                        result.extend(Self::transfer_alternatives(&selected, branch, remaining));
                    }
                }
                return result.finish();
            }
        }
        Self::transfer_invocation(invocation, state, remaining)
    }
    #[tracing::instrument(skip_all)]
    fn transfer_invocation(
        invocation: &Invocation,
        mut state: State,
        remaining: &mut usize,
    ) -> Vec<Exit> {
        state.record_invocation_reads(invocation);
        let function = match &invocation.callee {
            Callee::Function(function) => function,
            Callee::Method {
                receiver: _,
                descriptor: _,
            } => {
                let plan = host_value_plan(invocation, &mut state);
                return finish_value_plan(plan, state);
            }
        };
        let function = function.as_str();
        let arguments: Vec<Value> = invocation
            .operands
            .iter()
            .map(|operand| operand.value.clone())
            .collect();
        let influences = super::operations::inputs(&arguments, &state);
        let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
        match function {
            "hasKey" => {
                let Some(GuardValue::String(key)) = arguments.get(1).and_then(Value::constant)
                else {
                    return normal(state, Value::influenced(influences.clone()));
                };
                let presence = state.field_presence(&first, key);
                normal(state, presence)
            }
            "required" => super::kinds::required_subject(
                state,
                arguments.get(1).cloned().unwrap_or_else(Value::unknown),
            ),
            "eq" | "ne" => comparison_invocation(state, &arguments, function == "ne"),
            _ => match super::operations::transfer(invocation, &mut state) {
                Ok(value) => {
                    let value = match function_semantics(function).success {
                        SuccessType::Fixed(kind) => stamp_success(value, kind),
                        SuccessType::Selected => value,
                    };
                    normal(state, value)
                }
                Err(TransferDemand::Abort(reason)) => vec![Exit {
                    state,
                    flow: Flow::Abort(reason),
                }],
                Err(TransferDemand::Choice(decision)) => {
                    let mut exits = super::ExitBuffer::default();
                    for taken in [true, false] {
                        let branch = state.fork(Rc::clone(&decision), taken);
                        if !branch.path_is_infeasible() {
                            exits
                                .extend(Self::transfer_alternatives(invocation, branch, remaining));
                        }
                    }
                    exits.finish()
                }
            },
        }
    }
}
fn value_leaf(
    invocation: &Invocation,
    state: &mut State,
    queries: &mut std::collections::BTreeMap<usize, Option<Rc<super::FrozenTest>>>,
) -> Result<Value, TransferDemand> {
    state.record_invocation_reads(invocation);
    let Callee::Function(function) = &invocation.callee else {
        return Ok(Value::unknown());
    };
    let arguments: Vec<_> = invocation
        .operands
        .iter()
        .map(|operand| operand.value.clone())
        .collect();
    let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
    let value = match function.as_str() {
        "kindIs" | "typeIs" | "typeIsLike" | "kindOf" | "typeOf" => {
            super::kinds::descriptor(state, function, &arguments)
        }
        "fail" => return Err(TransferDemand::Abort(AbortReason::Fail)),
        "not" | "empty" => Value::select(
            state.test_with_unknown(&first, queries.entry(0).or_default()),
            Value::literal(GuardValue::Bool(false)),
            Value::literal(GuardValue::Bool(true)),
        ),
        "default" => {
            let given = arguments.get(1).cloned().unwrap_or_else(|| first.clone());
            Value::select(
                state.test_with_unknown(&given, queries.entry(1).or_default()),
                given,
                first,
            )
        }
        "coalesce" => {
            let mut result = Value::nil();
            for (index, argument) in arguments.iter().enumerate().rev() {
                result = Value::select(
                    state.test_with_unknown(argument, queries.entry(index).or_default()),
                    argument.clone(),
                    result,
                );
            }
            result
        }
        "ternary" => {
            let condition = arguments.get(2).cloned().unwrap_or_else(Value::unknown);
            Value::select(
                state.test_with_unknown(&condition, queries.entry(2).or_default()),
                first,
                arguments.get(1).cloned().unwrap_or_else(Value::unknown),
            )
        }
        _ => super::operations::transfer(invocation, state)?,
    };
    Ok(match function_semantics(function).success {
        SuccessType::Fixed(kind) => stamp_success(value, kind),
        SuccessType::Selected => value,
    })
}

fn constant_value_test(state: &State, value: bool) -> Rc<super::FrozenTest> {
    state.heap.arena.decision(
        if value {
            Predicate::True
        } else {
            Predicate::False
        },
        if value {
            Predicate::False
        } else {
            Predicate::True
        },
        Coverage::Exact,
    )
}

fn consumed_callee(callee: &Callee, state: &State) -> Callee {
    match callee {
        Callee::Function(name) => Callee::Function(name.clone()),
        Callee::Method {
            receiver,
            descriptor,
        } => Callee::Method {
            receiver: state.consumed(receiver),
            descriptor: descriptor.clone(),
        },
    }
}

fn host_value_plan(invocation: &Invocation, state: &mut State) -> ValuePlan {
    ValuePlan {
        value: super::hosts::transfer(invocation, state),
        normal: constant_value_test(state, true),
        failures: std::collections::BTreeMap::default(),
    }
}

fn transfer_values(
    invocation: &Invocation,
    state: &mut State,
    remaining: &mut usize,
    queries: &mut std::collections::BTreeMap<usize, Option<Rc<super::FrozenTest>>>,
) -> ValuePlan {
    if *remaining == 0 {
        state.record_invocation_reads(invocation);
        let mut values: Vec<_> = invocation
            .operands
            .iter()
            .map(|operand| operand.value.clone())
            .collect();
        match &invocation.callee {
            Callee::Function(_) => {}
            Callee::Method {
                receiver,
                descriptor: _,
            } => values.push(receiver.clone()),
        }
        return ValuePlan {
            value: Value::influenced(super::operations::inputs(&values, state)),
            normal: constant_value_test(state, true),
            failures: std::collections::BTreeMap::default(),
        };
    }
    *remaining -= 1;
    let receiver = match &invocation.callee {
        Callee::Function(_) => None,
        Callee::Method {
            receiver,
            descriptor: _,
        } => Some(receiver),
    };
    for value in receiver
        .into_iter()
        .chain(invocation.operands.iter().map(|operand| &operand.value))
    {
        let value = state.consumed(value);
        if let ValueNode::Select {
            decision,
            yes: _,
            no: _,
        } = value.node.as_ref()
        {
            return lift_value_choice(invocation, state, remaining, queries, decision);
        }
    }
    if matches!(
        &invocation.callee,
        Callee::Method {
            receiver: _,
            descriptor: _
        }
    ) {
        state.record_invocation_reads(invocation);
        return host_value_plan(invocation, state);
    }
    match value_leaf(invocation, state, queries) {
        Ok(value) => ValuePlan {
            value,
            normal: constant_value_test(state, true),
            failures: std::collections::BTreeMap::default(),
        },
        Err(TransferDemand::Choice(decision)) => {
            lift_value_choice(invocation, state, remaining, queries, &decision)
        }
        Err(TransferDemand::Abort(reason)) => ValuePlan {
            value: Value::unknown(),
            normal: constant_value_test(state, false),
            failures: std::collections::BTreeMap::from([(
                reason,
                constant_value_test(state, true),
            )]),
        },
    }
}

fn lift_value_choice(
    invocation: &Invocation,
    state: &mut State,
    remaining: &mut usize,
    queries: &mut std::collections::BTreeMap<usize, Option<Rc<super::FrozenTest>>>,
    decision: &Rc<super::FrozenTest>,
) -> ValuePlan {
    let depth = state.active.len();
    let [yes, no] = [true, false].map(|taken| {
        std::rc::Rc::make_mut(&mut state.active).push(super::DecisionEdge {
            test: Rc::clone(decision),
            taken,
        });
        let plan = if state.path_is_infeasible() {
            ValuePlan {
                value: Value::unknown(),
                normal: constant_value_test(state, false),
                failures: std::collections::BTreeMap::default(),
            }
        } else {
            let mut invocation = invocation.clone();
            invocation.callee = consumed_callee(&invocation.callee, state);
            for operand in &mut invocation.operands {
                operand.value = state.consumed(&operand.value);
            }
            transfer_values(&invocation, state, remaining, queries)
        };
        std::rc::Rc::make_mut(&mut state.active).truncate(depth);
        plan
    });
    join_value_plans(state, decision, yes, no)
}

fn join_value_plans(
    state: &mut State,
    decision: &Rc<super::FrozenTest>,
    yes: ValuePlan,
    no: ValuePlan,
) -> ValuePlan {
    let ValuePlan {
        value: yes_value,
        normal: yes_normal,
        failures: yes_failures,
    } = yes;
    let ValuePlan {
        value: no_value,
        normal: no_normal,
        failures: no_failures,
    } = no;
    let normal = state
        .heap
        .arena
        .choose_test(Rc::clone(decision), yes_normal, no_normal);
    let reasons: std::collections::BTreeSet<_> = yes_failures
        .keys()
        .chain(no_failures.keys())
        .copied()
        .collect();
    let failures = reasons
        .into_iter()
        .map(|reason| {
            let yes = yes_failures
                .get(&reason)
                .cloned()
                .unwrap_or_else(|| constant_value_test(state, false));
            let no = no_failures
                .get(&reason)
                .cloned()
                .unwrap_or_else(|| constant_value_test(state, false));
            (
                reason,
                state.heap.arena.choose_test(Rc::clone(decision), yes, no),
            )
        })
        .collect();
    let value = state.join_local_values(Rc::clone(decision), &yes_value, &no_value);
    ValuePlan {
        value,
        normal,
        failures,
    }
}

fn finish_value_plan(plan: ValuePlan, state: State) -> Vec<Exit> {
    let ValuePlan {
        value,
        normal: success,
        failures,
    } = plan;
    let known = state.heap.arena.runtime_path(&state.active);
    if success
        .constant_outcome()
        .or_else(|| known.implies_test(&success))
        == Some(true)
    {
        return normal(state, value);
    }
    if failures.len() == 1
        && let Some((&reason, failed)) = failures.first_key_value()
        && failed
            .constant_outcome()
            .or_else(|| known.implies_test(failed))
            == Some(true)
    {
        return vec![Exit {
            state,
            flow: Flow::Abort(reason),
        }];
    }
    let mut exits = Vec::new();
    if success.constant_outcome() != Some(false) {
        let branch = state.fork(success, true);
        if !branch.path_is_infeasible() {
            exits.extend(normal(branch, value));
        }
    }
    for (reason, failed) in failures {
        if failed.constant_outcome() == Some(false) {
            continue;
        }
        let branch = state.fork(failed, true);
        if !branch.path_is_infeasible() {
            exits.push(Exit {
                state: branch,
                flow: Flow::Abort(reason),
            });
        }
    }
    exits
}

struct ComparisonBudget {
    remaining: usize,
    truncated: bool,
}

struct ComparisonCase {
    active: Rc<Vec<super::DecisionEdge>>,
    first: Value,
}

struct ComparisonExit {
    active: Rc<Vec<super::DecisionEdge>>,
    flow: Flow,
}

fn comparison_invocation(mut state: State, arguments: &[Value], negated: bool) -> Vec<Exit> {
    if arguments.len() < 2 {
        return vec![Exit {
            state,
            flow: Flow::Abort(AbortReason::Arity),
        }];
    }
    let arguments: Vec<_> = arguments
        .iter()
        .map(|value| state.consumed(value))
        .collect();
    let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
    let mut budget = ComparisonBudget {
        remaining: super::MAX_SCALAR_DISPATCH_STATES,
        truncated: false,
    };
    // Keep hypothetical loader paths local; the caller's original decisions survive every return.
    let caller_active = std::mem::take(&mut state.active);
    let accepted = comparison_success(&mut state, &first, &arguments, negated, &mut budget);
    state.active = caller_active;
    let Some(accepted) = accepted else {
        tracing::debug!(reason = ?super::BoundReason::ExecutionAlternatives, "widen comparison dispatch");
        let value = unknown_comparison_arguments(&state, &arguments);
        return normal(state, value);
    };
    if accepted == Predicate::False {
        return super::kinds::validate_type_predicate(state, Value::nil(), accepted);
    }
    let value = comparison_result(&state, &arguments, negated, &mut budget);
    super::kinds::validate_type_predicate(state, value, accepted)
}

#[tracing::instrument(skip_all)]
fn comparison_success(
    state: &mut State,
    first: &Value,
    arguments: &[Value],
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Option<Predicate> {
    if let Some(accepted) = binary_comparison_acceptance(state, first, arguments) {
        return Some(accepted);
    }
    comparison_success_explored(state, first, arguments, negated, budget)
}

fn binary_comparison_acceptance(
    state: &State,
    first: &Value,
    arguments: &[Value],
) -> Option<Predicate> {
    let [_, right] = arguments else {
        return None;
    };
    let left_type = state.heap.freeze(first).go_type();
    let right_type = state.heap.freeze(right).go_type();
    if left_type == GoType::Unknown && right_type == GoType::Unknown {
        return None;
    }
    if matches!(
        first.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _
        })
    ) && matches!(
        right.node.as_ref(),
        ValueNode::Leaf(Leaf {
            datum: _,
            origin: _
        })
    ) {
        // A typed counterpart fixes the raw input's compatible loader domain.
        return Some(comparison_acceptance(state, first, right).unwrap_or(Predicate::True));
    }
    if left_type == GoType::Nil || right_type == GoType::Nil {
        return Some(Predicate::True);
    }
    // Uniform scalar alternatives cannot fail comparison; container nilness still needs dispatch.
    if !matches!(left_type, GoType::Map | GoType::Slice | GoType::Unknown)
        && !matches!(right_type, GoType::Map | GoType::Slice | GoType::Unknown)
        && (left_type == right_type || integer_type(left_type) && integer_type(right_type))
    {
        return Some(Predicate::True);
    }
    None
}

fn comparison_success_explored(
    state: &mut State,
    first: &Value,
    arguments: &[Value],
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Option<Predicate> {
    let cases = first_comparison_cases(state, first, budget);
    if budget.truncated {
        return None;
    }
    let exits = comparison_walk(state, cases, arguments, negated, budget)?;
    if exits
        .iter()
        .all(|exit| super::output::flow_may_continue(&exit.flow, &exit.active))
    {
        return Some(Predicate::True);
    }
    let mut success = Vec::new();
    for exit in exits {
        if super::output::flow_may_continue(&exit.flow, &exit.active) {
            state.active = exit.active;
            success.push(state.path_predicate());
        }
    }
    Some(Predicate::Or(success).normalize_boolean())
}

fn first_comparison_cases(
    state: &mut State,
    first: &Value,
    budget: &mut ComparisonBudget,
) -> Vec<ComparisonCase> {
    if budget.remaining == 0 {
        budget.truncated = true;
        return Vec::new();
    }
    budget.remaining -= 1;
    let first = state.consumed(first);
    if let ValueNode::Select {
        decision,
        yes: _,
        no: _,
    } = first.node.as_ref()
    {
        let mut cases = Vec::new();
        let depth = state.active.len();
        for taken in [true, false] {
            comparison_push(state, Rc::clone(decision), taken);
            if !state.path_is_infeasible() {
                cases.extend(first_comparison_cases(
                    state,
                    &first.under(decision, taken),
                    budget,
                ));
            }
            std::rc::Rc::make_mut(&mut state.active).truncate(depth);
        }
        return cases;
    }
    // Overlapping domains describe feasible loaders; none is published as a proven raw type.
    if let Some(alternatives) = super::kinds::input_type_alternatives(&first) {
        let mut cases = Vec::new();
        for alternative in alternatives {
            if budget.remaining == 0 {
                budget.truncated = true;
                break;
            }
            budget.remaining -= 1;
            let decision = state.heap.arena.decision(
                alternative.condition.clone(),
                alternative.condition.negated(),
                Coverage::Exact,
            );
            let mut active = state.active.clone();
            if decision.constant_outcome() != Some(true) {
                Rc::make_mut(&mut active).push(super::DecisionEdge {
                    test: decision,
                    taken: true,
                });
            }
            cases.push(ComparisonCase {
                active,
                first: alternative.value,
            });
        }
        cases
    } else {
        vec![ComparisonCase {
            active: state.active.clone(),
            first,
        }]
    }
}

fn comparison_walk(
    state: &mut State,
    cases: Vec<ComparisonCase>,
    arguments: &[Value],
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Option<Vec<ComparisonExit>> {
    let mut result = Vec::new();
    for case in cases {
        let mut pending = vec![case.active];
        for (index, right) in arguments.iter().enumerate().skip(1) {
            let mut next = Vec::new();
            for active in pending {
                state.active = active;
                for compared in compare_pair_inner(state, &case.first, right, negated, budget) {
                    if budget.truncated {
                        return None;
                    }
                    let Flow::Normal(normal_value) = compared.flow else {
                        result.push(compared);
                        continue;
                    };
                    state.active = compared.active;
                    let value = normal_value.value;
                    if index + 1 == arguments.len() {
                        result.push(comparison_normal(state, value));
                        continue;
                    }
                    let test = state.test(&value);
                    let depth = state.active.len();
                    for taken in [true, false] {
                        comparison_push(state, Rc::clone(&test), taken);
                        if !state.path_is_infeasible() {
                            if taken {
                                result.push(comparison_normal(
                                    state,
                                    Value::literal(GuardValue::Bool(true)),
                                ));
                            } else {
                                next.push(state.active.clone());
                            }
                        }
                        std::rc::Rc::make_mut(&mut state.active).truncate(depth);
                    }
                }
            }
            pending = comparison_reconverge(
                state,
                next.into_iter()
                    .map(|active| ComparisonExit {
                        active,
                        flow: Flow::Normal(Normal {
                            value: Value::unknown(),
                        }),
                    })
                    .collect(),
            )
            .into_iter()
            .map(|exit| exit.active)
            .collect();
        }
    }
    Some(comparison_reconverge(state, result))
}

fn comparison_push(state: &mut State, test: Rc<super::FrozenTest>, taken: bool) {
    if test.constant_outcome() != Some(taken) {
        std::rc::Rc::make_mut(&mut state.active).push(super::DecisionEdge { test, taken });
    }
}

fn comparison_normal(state: &State, value: Value) -> ComparisonExit {
    ComparisonExit {
        active: state.active.clone(),
        flow: Flow::Normal(Normal { value }),
    }
}

fn comparison_reconverge(state: &State, exits: Vec<ComparisonExit>) -> Vec<ComparisonExit> {
    super::control::reconverge_continuations(exits, state)
        .into_iter()
        .map(|entry| entry.continuation)
        .collect()
}

impl super::control::Reconvergent for ComparisonExit {
    type Context = State;
    type Joined = Vec<super::DecisionEdge>;
    type Key = ();

    fn state<'a>(&'a self, context: &'a State) -> &'a State {
        context
    }
    fn active<'a>(&'a self, _context: &'a State) -> &'a [super::DecisionEdge] {
        &self.active
    }
    fn key(&self, _context: &State) -> Option<Self::Key> {
        matches!(self.flow, Flow::Normal(_)).then_some(())
    }
    fn prepare_join(
        &self,
        _other: &Self,
        plan: &super::state::ContinuationJoin,
        _context: &State,
    ) -> Option<Self::Joined> {
        Some(plan.active.clone())
    }
    fn joined(
        self,
        other: Self,
        active: Self::Joined,
        decision: Rc<super::FrozenTest>,
        taken: bool,
        context: &State,
    ) -> Self {
        let (yes, no) = if taken {
            (&self, &other)
        } else {
            (&other, &self)
        };
        let (Flow::Normal(yes), Flow::Normal(no)) = (&yes.flow, &no.flow) else {
            return self;
        };
        let mut widened = std::collections::BTreeSet::new();
        let value = super::state::join_frame_value(
            decision,
            &yes.value,
            &no.value,
            &context.heap,
            &context.heap,
            &mut widened,
        );
        Self {
            active: active.into(),
            flow: Flow::Normal(Normal { value }),
        }
    }
}

fn comparison_result(
    state: &State,
    arguments: &[Value],
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Value {
    let Some(first) = arguments.first() else {
        return unknown_comparison_arguments(state, arguments);
    };
    let mut result = Value::literal(GuardValue::Bool(false));
    for right in arguments.iter().skip(1) {
        let pair = comparison_preimage(state, first, right, negated, budget);
        if budget.truncated {
            return unknown_comparison_arguments(state, arguments);
        }
        let decision = state.test(&result);
        // Every normal execution returns the disjunction, regardless of skipped incompatible pairs.
        result = Value::select(decision, Value::literal(GuardValue::Bool(true)), pair);
    }
    result
}

fn comparison_preimage(
    state: &State,
    left: &Value,
    right: &Value,
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Value {
    if budget.remaining == 0 {
        budget.truncated = true;
        return unknown_comparison(state, left, right);
    }
    budget.remaining -= 1;
    let left = state.consumed(left);
    let right = state.consumed(right);
    if let ValueNode::Select { decision, yes, no } = left.node.as_ref() {
        return Value::select(
            Rc::clone(decision),
            comparison_preimage(state, yes, &right.under(decision, true), negated, budget),
            comparison_preimage(state, no, &right.under(decision, false), negated, budget),
        );
    }
    if let ValueNode::Select { decision, yes, no } = right.node.as_ref() {
        return Value::select(
            Rc::clone(decision),
            comparison_preimage(state, &left.under(decision, true), yes, negated, budget),
            comparison_preimage(state, &left.under(decision, false), no, negated, budget),
        );
    }
    comparison_value(state, &left, &right, negated)
}

fn unknown_comparison_arguments(state: &State, arguments: &[Value]) -> Value {
    stamp_success(
        Value::influenced(super::operations::inputs(arguments, state)),
        GoType::Bool,
    )
}

fn compare_pair_inner(
    state: &mut State,
    left: &Value,
    right: &Value,
    negated: bool,
    budget: &mut ComparisonBudget,
) -> Vec<ComparisonExit> {
    let left = state.consumed(left);
    let right = state.consumed(right);
    if budget.remaining == 0 {
        budget.truncated = true;
        let value = unknown_comparison(state, &left, &right);
        return vec![comparison_normal(state, value)];
    }
    budget.remaining -= 1;
    let selected = comparison_selector(&left, &right);
    if let Some(decision) = selected {
        let mut exits = Vec::new();
        let depth = state.active.len();
        for taken in [true, false] {
            comparison_push(state, Rc::clone(&decision), taken);
            if !state.path_is_infeasible() {
                exits.extend(compare_pair_inner(
                    state,
                    &left.under(&decision, taken),
                    &right.under(&decision, taken),
                    negated,
                    budget,
                ));
            }
            std::rc::Rc::make_mut(&mut state.active).truncate(depth);
        }
        return comparison_reconverge(state, exits);
    }
    let accepted = comparison_acceptance(state, &left, &right);
    let value = if accepted == Some(Predicate::False) {
        Value::nil()
    } else {
        comparison_value(state, &left, &right, negated)
    };
    let Some(accepted) = accepted else {
        return vec![comparison_normal(state, value)];
    };
    let decision = state
        .heap
        .arena
        .decision(accepted.clone(), accepted.negated(), Coverage::Exact);
    let known = state.heap.arena.runtime_path(&state.active);
    let outcome = decision
        .constant_outcome()
        .or_else(|| known.implies_test(&decision));
    let mut exits = Vec::new();
    let depth = state.active.len();
    for taken in [true, false] {
        if outcome.is_some_and(|outcome| outcome != taken) {
            continue;
        }
        if outcome.is_none() {
            comparison_push(state, Rc::clone(&decision), taken);
        }
        if !state.path_is_infeasible() {
            exits.push(ComparisonExit {
                active: state.active.clone(),
                flow: if taken {
                    Flow::Normal(Normal {
                        value: value.clone(),
                    })
                } else {
                    Flow::Abort(AbortReason::Type)
                },
            });
        }
        std::rc::Rc::make_mut(&mut state.active).truncate(depth);
    }
    exits
}

fn comparison_selector(left: &Value, right: &Value) -> Option<Rc<super::FrozenTest>> {
    match (left.node.as_ref(), right.node.as_ref()) {
        (
            ValueNode::Select {
                decision,
                yes: _,
                no: _,
            },
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            },
        )
        | (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Select {
                decision,
                yes: _,
                no: _,
            },
        ) => Some(Rc::clone(decision)),
        (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
        ) => None,
    }
}

fn comparison_acceptance(state: &State, left: &Value, right: &Value) -> Option<Predicate> {
    let left_type = state.heap.freeze(left).go_type();
    let right_type = state.heap.freeze(right).go_type();
    if left_type == GoType::Nil || right_type == GoType::Nil {
        return Some(Predicate::True);
    }
    if left_type != GoType::Unknown && right_type != GoType::Unknown {
        if matches!(left_type, GoType::Map | GoType::Slice)
            || matches!(right_type, GoType::Map | GoType::Slice)
        {
            return Some(
                if left_type != right_type
                    || comparison_nilness(state, left) == Some(false)
                        && comparison_nilness(state, right) == Some(false)
                {
                    Predicate::False
                } else {
                    Predicate::True
                },
            );
        }
        return Some(
            if left_type == right_type || integer_type(left_type) && integer_type(right_type) {
                Predicate::True
            } else {
                Predicate::False
            },
        );
    }
    if let (Some(left_path), Some(right_path)) = (left.identity(), right.identity())
        && left_type == GoType::Unknown
        && right_type == GoType::Unknown
    {
        let mut accepted = vec![
            super::kinds::input_nil_predicate(left_path),
            super::kinds::input_nil_predicate(right_path),
        ];
        for kind in [GoType::Bool, GoType::String, GoType::Float64] {
            accepted.push(Predicate::all(vec![
                compatible_input_predicate(left_path, kind)?,
                compatible_input_predicate(right_path, kind)?,
            ]));
        }
        return Some(Predicate::Or(accepted).normalize_boolean());
    }
    let (path, other, other_type) = if left_type == GoType::Unknown {
        (left.identity()?, right, right_type)
    } else {
        (right.identity()?, left, left_type)
    };
    let mut accepted = vec![super::kinds::input_nil_predicate(path)];
    if !matches!(other_type, GoType::Map | GoType::Slice)
        || comparison_nilness(state, other) != Some(false)
    {
        accepted.push(compatible_input_predicate(path, other_type)?);
    }
    Some(Predicate::Or(accepted).normalize_boolean())
}

fn comparison_nilness(state: &State, value: &Value) -> Option<bool> {
    super::kinds::nilness(value).or_else(|| {
        (value.identity().is_some()
            && matches!(
                state.heap.freeze(value).go_type(),
                GoType::Map | GoType::Slice
            ))
        .then_some(false)
    })
}

fn comparison_value(state: &State, left: &Value, right: &Value, negated: bool) -> Value {
    let left_type = state.heap.freeze(left).go_type();
    let right_type = state.heap.freeze(right).go_type();
    let left_nil = comparison_nilness(state, left);
    let right_nil = comparison_nilness(state, right);
    let literal = if left_type == GoType::Nil {
        right_nil
    } else if right_type == GoType::Nil {
        left_nil
    } else if matches!(left_type, GoType::Map | GoType::Slice)
        || matches!(right_type, GoType::Map | GoType::Slice)
    {
        match (left_nil, right_nil) {
            (Some(left), Some(right)) => Some(left == right),
            (Some(false), None) | (None, Some(false)) => Some(false),
            (Some(true) | None, None) | (None, Some(true)) => None,
        }
    } else {
        None
    };
    if let Some(equal) = literal {
        return Value::literal(GuardValue::Bool(equal != negated));
    }
    if left.identity().is_some() && left.identity() == right.identity() {
        return Value::literal(GuardValue::Bool(!negated));
    }
    for (value, other_nil) in [(left, right_nil), (right, left_nil)] {
        if other_nil == Some(true)
            && let Some(path) = value.identity()
        {
            return equality_predicate(state, super::kinds::input_nil_predicate(path), negated);
        }
    }
    if left_type == GoType::Unknown
        && right_type == GoType::Unknown
        && let (Some(left_path), Some(right_path)) = (left.identity(), right.identity())
    {
        let nil_right = super::kinds::input_nil_predicate(right_path);
        let left_nil_value = equality_predicate(state, nil_right.clone(), negated);
        let right_nil_value = select_predicate(
            state,
            &nil_right,
            Value::literal(GuardValue::Bool(negated)),
            unknown_comparison(state, left, right),
        );
        return select_predicate(
            state,
            &super::kinds::input_nil_predicate(left_path),
            left_nil_value,
            right_nil_value,
        );
    }
    scalar_equality(state, left, right, negated)
}

fn unknown_comparison(state: &State, left: &Value, right: &Value) -> Value {
    stamp_success(
        Value::influenced(super::operations::inputs(
            &[left.clone(), right.clone()],
            state,
        )),
        GoType::Bool,
    )
}

fn equality_predicate(state: &State, predicate: Predicate, negated: bool) -> Value {
    let predicate = if negated {
        predicate.negated()
    } else {
        predicate
    };
    select_predicate(
        state,
        &predicate,
        Value::literal(GuardValue::Bool(true)),
        Value::literal(GuardValue::Bool(false)),
    )
}

fn select_predicate(state: &State, predicate: &Predicate, yes: Value, no: Value) -> Value {
    let decision =
        state
            .heap
            .arena
            .decision(predicate.clone(), predicate.negated(), Coverage::Exact);
    Value::select(decision, yes, no)
}

fn compatible_input_predicate(
    path: &helm_schema_core::ValuesPath,
    kind: GoType,
) -> Option<Predicate> {
    let kind = if integer_type(kind) {
        GoType::Int64
    } else {
        kind
    };
    super::kinds::input_type_domain(path, kind).map(|domain| domain.possible)
}

fn integer_type(kind: GoType) -> bool {
    matches!(kind, GoType::Int | GoType::Int64 | GoType::Uint64)
}

fn scalar_equality(state: &State, left: &Value, right: &Value, negated: bool) -> Value {
    if let (Some(left_literal), Some(right_literal)) = (left.constant(), right.constant()) {
        let equal = if let (GuardValue::Float(left_number), GuardValue::Float(right_number)) =
            (left_literal, right_literal)
        {
            match (left_number.parse::<f64>(), right_number.parse::<f64>()) {
                (Ok(left), Ok(right)) => {
                    left.partial_cmp(&right) == Some(std::cmp::Ordering::Equal)
                }
                (Err(_), Ok(_) | Err(_)) | (Ok(_), Err(_)) => {
                    return unknown_comparison(state, left, right);
                }
            }
        } else {
            left_literal == right_literal
        };
        return Value::literal(GuardValue::Bool(equal != negated));
    }
    let pair = left
        .identity()
        .zip(right.constant())
        .or_else(|| right.identity().zip(left.constant()));
    if let Some((path, literal)) = pair {
        let predicate = if *literal == GuardValue::Null {
            super::kinds::input_nil_predicate(path)
        } else {
            Predicate::from(Guard::Eq {
                path: path.clone(),
                value: literal.clone(),
            })
        };
        equality_predicate(state, predicate, negated)
    } else {
        unknown_comparison(state, left, right)
    }
}

fn argument_form(expression: &TemplateExpr) -> ArgumentForm {
    match expression {
        TemplateExpr::Literal(_) => ArgumentForm::DirectLiteral,
        TemplateExpr::Field(path) | TemplateExpr::Selector { operand: _, path }
            if !path.is_empty() =>
        {
            ArgumentForm::DirectLookup
        }
        TemplateExpr::Field(_)
        | TemplateExpr::Selector {
            operand: _,
            path: _,
        }
        | TemplateExpr::Variable(_)
        | TemplateExpr::Pipeline(_)
        | TemplateExpr::Parenthesized(_)
        | TemplateExpr::VariableDefinition { name: _, value: _ }
        | TemplateExpr::Assignment { name: _, value: _ }
        | TemplateExpr::Call {
            function: _,
            args: _,
        }
        | TemplateExpr::Unknown(_) => ArgumentForm::Evaluated,
    }
}

fn contextual_literal(
    expression: &TemplateExpr,
    expected: Option<GoType>,
) -> Result<Option<Value>, super::AbortReason> {
    use super::ScalarType;
    use helm_schema_ast::Literal;
    let kind = match expected {
        Some(GoType::Int) => ScalarType::Int,
        Some(GoType::Int64) => ScalarType::Int64,
        Some(GoType::Uint64) => ScalarType::Uint64,
        Some(GoType::Float64) => ScalarType::Float64,
        Some(
            GoType::Nil
            | GoType::Bool
            | GoType::String
            | GoType::Map
            | GoType::Slice
            | GoType::Unknown,
        )
        | None => return Ok(None),
    };
    let content = match expression {
        TemplateExpr::Literal(Literal::Int(integer)) => {
            if kind == ScalarType::Uint64 && *integer < 0 {
                return Err(AbortReason::Type);
            }
            if kind == ScalarType::Float64 {
                return Ok(GuardValue::float(
                    integer
                        .to_string()
                        .parse::<f64>()
                        .map_err(|_| AbortReason::Type)?,
                )
                .map(|value| {
                    Value::scalar(
                        kind,
                        Some(super::scalar_value::ScalarContent::Literal(value)),
                    )
                }));
            }
            GuardValue::Int(*integer)
        }
        TemplateExpr::Literal(Literal::Float(number)) => {
            if kind == ScalarType::Float64 {
                return Ok(None);
            }
            if number.fract() != 0.0 || kind == ScalarType::Uint64 && *number < 0.0 {
                return Err(AbortReason::Type);
            }
            if number.abs() > 9_007_199_254_740_991.0 {
                return Ok(Some(Value::unknown()));
            }
            GuardValue::Int(
                format!("{number:.0}")
                    .parse::<i64>()
                    .map_err(|_| AbortReason::Type)?,
            )
        }
        _ => return Ok(None),
    };
    Ok(Some(Value::scalar(
        kind,
        Some(super::scalar_value::ScalarContent::Literal(content)),
    )))
}

fn stamp_success(value: Value, kind: GoType) -> Value {
    let ValueNode::Leaf(Leaf {
        datum: Datum::Unknown(unknown),
        origin,
    }) = value.node.as_ref()
    else {
        return value;
    };
    if origin.identity.is_some() {
        return value;
    }
    let datum = match kind {
        GoType::Nil => Datum::Scalar {
            go_type: super::ScalarType::Nil,
            content: None,
        },
        GoType::Bool => Datum::Scalar {
            go_type: super::ScalarType::Bool,
            content: None,
        },
        GoType::String => Datum::Scalar {
            go_type: super::ScalarType::String,
            content: None,
        },
        GoType::Int => Datum::Scalar {
            go_type: super::ScalarType::Int,
            content: None,
        },
        GoType::Int64 => Datum::Scalar {
            go_type: super::ScalarType::Int64,
            content: None,
        },
        GoType::Uint64 => Datum::Scalar {
            go_type: super::ScalarType::Uint64,
            content: None,
        },
        GoType::Float64 => Datum::Scalar {
            go_type: super::ScalarType::Float64,
            content: None,
        },
        GoType::Map | GoType::Slice => Datum::Unknown(super::UnknownValue {
            types: if kind == GoType::Map {
                super::TypeCoverage::Map(super::MapType::Interface)
            } else {
                super::TypeCoverage::Sequence
            },
            aliases: unknown.aliases.clone(),
        }),
        GoType::Unknown => return value,
    };
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum,
            origin: origin.clone(),
        })),
    }
}

fn summarize_assembled(assembled: Vec<Assembled>) -> Vec<Assembled> {
    let count = assembled
        .first()
        .map_or(0, |assembled| assembled.values.len());
    let has_pipe = assembled
        .first()
        .is_some_and(|assembled| assembled.pipe.is_some());
    super::control::summarize_exits(
        assembled
            .into_iter()
            .map(|assembled| {
                let values = assembled
                    .values
                    .into_iter()
                    .map(
                        |Operand {
                             value,
                             site: _,
                             form: _,
                         }| value,
                    )
                    .chain(assembled.pipe)
                    .collect();
                Exit {
                    state: assembled.state,
                    flow: Flow::Normal(Normal {
                        value: Value::sequence(values),
                    }),
                }
            })
            .collect(),
    )
    .into_iter()
    .map(|exit| {
        let value = match exit.flow {
            Flow::Normal(normal) => normal.value,
            Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => Value::unknown(),
        };
        Assembled {
            state: exit.state,
            values: vec![
                Operand {
                    value: value.clone(),
                    site: 0,
                    form: ArgumentForm::Evaluated
                };
                count
            ],
            pipe: has_pipe.then_some(value),
        }
    })
    .collect()
}

impl CallOutcome {
    pub(super) fn into_exits(self) -> Vec<Exit> {
        self.execution.exits
    }
}
impl super::DocumentExecution {
    pub(crate) fn invocations(&self) -> impl Iterator<Item = &super::ExecutedInvocationRecord> {
        self.exits.iter().flat_map(|exit| &exit.state.invocations)
    }
    pub(crate) fn invocation_records_complete(&self) -> bool {
        self.exits
            .iter()
            .all(|exit| exit.state.invocations_complete())
    }
}
impl super::ExecutedInvocationRecord {
    pub(crate) fn invocation(&self) -> u64 {
        self.invocation
    }
    pub(crate) fn program(&self) -> u64 {
        self.program
    }
    pub(crate) fn site(&self) -> u64 {
        self.site
    }
    pub(crate) fn function(&self) -> &str {
        &self.function
    }
    pub(crate) fn wrapper(&self) -> Option<&super::ProgramWrapperMetadata> {
        self.wrapper.as_ref()
    }
}

#[derive(Default)]
struct AssembledBuffer {
    values: Vec<Assembled>,
    summarized: bool,
}
impl AssembledBuffer {
    fn push(&mut self, value: Assembled) {
        self.values.push(value);
        if self.summarized || self.values.len() > 4096 {
            self.summarized = true;
            self.values = summarize_assembled(std::mem::take(&mut self.values));
        }
    }
    fn finish(self) -> Vec<Assembled> {
        super::control::reconverge_continuations(self.values, &())
            .into_iter()
            .map(|entry| entry.continuation)
            .collect()
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct AssemblyKey {
    continuation: super::state::ContinuationKey,
    pipe: bool,
    operands: Vec<OperandKey>,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct OperandKey {
    site: u64,
    form: ArgumentForm,
}

impl super::control::Reconvergent for Assembled {
    type Context = ();
    type Joined = State;
    type Key = AssemblyKey;
    fn state<'a>(&'a self, _context: &'a ()) -> &'a State {
        &self.state
    }

    fn prepare_join(
        &self,
        other: &Self,
        plan: &super::state::ContinuationJoin,
        _context: &(),
    ) -> Option<State> {
        State::join_branches(plan, &self.state, &other.state)
    }
    fn key(&self, _context: &()) -> Option<Self::Key> {
        Some(AssemblyKey {
            continuation: self.state.continuation_key(),
            pipe: self.pipe.is_some(),
            operands: self
                .values
                .iter()
                .map(
                    |Operand {
                         value: _,
                         site,
                         form,
                     }| OperandKey {
                        site: *site,
                        form: *form,
                    },
                )
                .collect(),
        })
    }

    fn joined(
        self,
        other: Self,
        mut state: State,
        decision: Rc<super::FrozenTest>,
        taken: bool,
        _context: &(),
    ) -> Self {
        let (yes, no) = if taken {
            (&self, &other)
        } else {
            (&other, &self)
        };
        let mut widened = std::collections::BTreeSet::new();
        let mut join_value = |yes_value: &Value, no_value: &Value| {
            super::state::join_frame_value(
                Rc::clone(&decision),
                yes_value,
                no_value,
                &yes.state.heap,
                &no.state.heap,
                &mut widened,
            )
        };
        let values = yes
            .values
            .iter()
            .zip(&no.values)
            .map(|(yes, no)| {
                let Operand { value, site, form } = yes;
                Operand {
                    value: join_value(value, &no.value),
                    site: *site,
                    form: *form,
                }
            })
            .collect();
        let pipe = match (&yes.pipe, &no.pipe) {
            (Some(yes), Some(no)) => Some(join_value(yes, no)),
            (None, None) => None,
            (Some(_), None) | (None, Some(_)) => return self,
        };
        for path in widened {
            state.reads.insert(super::RuntimeRead {
                path,
                condition: state.path_predicate(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
        Self {
            state,
            values,
            pipe,
        }
    }
}

#[cfg(test)]
#[path = "../tests/runtime_comparison_work.rs"]
mod comparison_work_tests;

impl CompletedExecution {
    fn finish(mut self) -> CallOutcome {
        let Some(first) = self.execution.exits.first() else {
            return CallOutcome {
                execution: self.execution,
            };
        };
        let invocation = first.state.heap.arena.next_invocation.get();
        first.state.heap.arena.next_invocation.set(invocation + 1);
        let function = self.invocation.callee.name();
        for exit in &mut self.execution.exits {
            let mut wrapper = None;
            if let Some(helper) = &self.helper_return {
                exit.state.program = helper.program;
                exit.state.namespace = Rc::clone(&helper.namespace);
                exit.state.leave_call();
                exit.state.calls.pop();
                exit.state.loops = helper.loops.clone();
                let returned = if function == "tpl" {
                    exit.state.output.stripped_value()
                } else {
                    super::text::capture(&exit.state.output, false)
                };
                exit.state.output = helper.output.clone();
                if let Flow::Normal(normal) = &mut exit.flow {
                    normal.value = returned;
                }
                wrapper = helper.wrapper.clone();
            }
            for slot in exit.state.reserve_invocation() {
                exit.state
                    .invocations
                    .push(super::ExecutedInvocationRecord {
                        condition: slot.condition,
                        ordinal: slot.ordinal,
                        invocation,
                        program: exit.state.program,
                        site: self.invocation.site,
                        function: function.to_owned(),
                        wrapper: wrapper.clone(),
                    });
            }
        }
        if self.helper_return.is_some() {
            self.execution.exits = super::control::reconverge(self.execution.exits);
        }
        CallOutcome {
            execution: self.execution,
        }
    }
}
