use super::eval::{Executor, normal};
use super::kinds::GoType;
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
impl Executor<'_> {
    #[tracing::instrument(skip_all)]
    pub(super) fn invoke(
        &self,
        function: &str,
        arguments: &[TemplateExpr],
        pipe: Option<Value>,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        let count = arguments.len() + usize::from(pipe.is_some());
        let semantics = function_semantics(function);
        if (semantics.accepts_arity(count) == Some(false)
            && !(function == "template" && count == 1))
            || (matches!(function, "and" | "or") && count == 0)
        {
            return vec![Exit {
                state,
                flow: Flow::Abort(AbortReason::Arity),
            }];
        }
        if matches!(function, "and" | "or") {
            let exits = self.short_circuit(function, arguments, pipe, state, site);
            return self
                .complete(CompletionInput::Executed {
                    invocation: Invocation {
                        callee: Callee::Function(function.to_owned()),
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
                for exit in self.expression(expression, assembled.state, site) {
                    match exit.flow {
                        Flow::Normal(mut normal) => {
                            if let TemplateExpr::Literal(helm_schema_ast::Literal::Int(integer)) =
                                expression
                            {
                                let kind = match semantics.parameter(index) {
                                    Some(GoType::Int64) => Some(super::ScalarType::Int64),
                                    Some(GoType::Float64) => Some(super::ScalarType::Float64),
                                    Some(GoType::Uint64) if *integer >= 0 => {
                                        Some(super::ScalarType::Uint64)
                                    }
                                    Some(
                                        GoType::Nil
                                        | GoType::Bool
                                        | GoType::String
                                        | GoType::Int
                                        | GoType::Uint64
                                        | GoType::Map
                                        | GoType::Slice
                                        | GoType::Unknown,
                                    )
                                    | None => None,
                                };
                                if let Some(kind) = kind {
                                    normal.value = Value::scalar(
                                        kind,
                                        Some(super::scalar_value::ScalarContent::Literal(
                                            GuardValue::Int(*integer),
                                        )),
                                    );
                                }
                            }
                            if let TemplateExpr::Literal(helm_schema_ast::Literal::Float(number)) =
                                expression
                            {
                                let kind = match semantics.parameter(index) {
                                    Some(GoType::Int) => Some(super::ScalarType::Int),
                                    Some(GoType::Int64) => Some(super::ScalarType::Int64),
                                    Some(GoType::Uint64) => Some(super::ScalarType::Uint64),
                                    Some(
                                        GoType::Nil
                                        | GoType::Bool
                                        | GoType::String
                                        | GoType::Float64
                                        | GoType::Map
                                        | GoType::Slice
                                        | GoType::Unknown,
                                    )
                                    | None => None,
                                };
                                if let Some(kind) = kind {
                                    let content = if number.fract() == 0.0
                                        && number.abs() <= 9_007_199_254_740_991.0
                                    {
                                        Some(super::scalar_value::ScalarContent::Literal(
                                            GuardValue::Int(*number as i64),
                                        ))
                                    } else {
                                        None
                                    };
                                    normal.value = Value::scalar(kind, content);
                                }
                            }
                            if !valid_parameter(
                                &exit.state,
                                &normal.value,
                                semantics.parameter(index),
                            ) {
                                exits.push(Exit {
                                    state: exit.state,
                                    flow: Flow::Abort(AbortReason::Type),
                                });
                                continue;
                            }
                            let mut values = assembled.values.clone();
                            values.push(Operand {
                                value: normal.value,
                                site: site as u64,
                                form: if matches!(expression, TemplateExpr::Literal(_)) {
                                    ArgumentForm::DirectLiteral
                                } else {
                                    ArgumentForm::Evaluated
                                },
                            });
                            next.push(Assembled {
                                state: exit.state,
                                values,
                                pipe: assembled.pipe.clone(),
                            });
                        }
                        flow => exits.push(Exit {
                            state: exit.state,
                            flow,
                        }),
                    }
                }
            }
            pending = next.finish();
        }
        for mut assembled in pending {
            if let Some(pipe) = &assembled.pipe {
                if !valid_parameter(&assembled.state, pipe, semantics.parameter(arguments.len())) {
                    exits.push(Exit {
                        state: assembled.state,
                        flow: Flow::Abort(AbortReason::Type),
                    });
                    continue;
                }
                assembled.values.push(Operand {
                    value: pipe.clone(),
                    site: site as u64,
                    form: ArgumentForm::Evaluated,
                });
            }
            for operand in &mut assembled.values {
                tracing::trace!(site = operand.site, form = ?operand.form, "consume operand");
                operand.value = assembled.state.consumed(&operand.value);
            }
            exits.extend(
                self.complete(CompletionInput::Invocation {
                    invocation: Invocation {
                        callee: Callee::Function(function.to_owned()),
                        operands: assembled.values,
                        site: site as u64,
                    },
                    state: assembled.state,
                })
                .into_exits(),
            );
        }
        exits.finish()
    }
    fn short_circuit(
        &self,
        function: &str,
        arguments: &[TemplateExpr],
        pipe: Option<Value>,
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
                                if state.path_predicate().normalize_boolean() == Predicate::False {
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
            result.extend(normal(state, pipe.clone().unwrap_or_else(Value::nil)));
        }
        result.finish()
    }
    #[tracing::instrument(skip_all)]
    pub(super) fn complete(&self, input: CompletionInput) -> CallOutcome {
        let mut completed = match input {
            CompletionInput::Invocation { invocation, state } => {
                let Callee::Function(function) = &invocation.callee;
                if matches!(function.as_str(), "include" | "template" | "tpl") {
                    return self.helper(invocation, state, true);
                }
                let exits = self.transfer_invocation(&invocation, state);
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
        let Some(first) = completed.execution.exits.first() else {
            return CallOutcome {
                execution: completed.execution,
            };
        };
        let invocation = first.state.heap.arena.next_invocation.get();
        first.state.heap.arena.next_invocation.set(invocation + 1);
        let Callee::Function(function) = &completed.invocation.callee;
        for exit in &mut completed.execution.exits {
            let mut wrapper = None;
            if let Some(helper) = &completed.helper_return {
                exit.state.program = helper.program;
                exit.state.leave_call();
                exit.state.calls.pop();
                exit.state.loops = helper.loops.clone();
                let returned = exit.state.output.as_value();
                exit.state.output = helper.output.clone();
                if let Flow::Normal(normal) = &mut exit.flow {
                    normal.value = returned;
                }
                wrapper = helper.wrapper.clone();
            }
            if exit.state.invocations.len() < 128 {
                exit.state
                    .invocations
                    .push(super::ExecutedInvocationRecord {
                        invocation,
                        program: exit.state.program,
                        site: completed.invocation.site,
                        function: function.clone(),
                        wrapper,
                    });
            } else {
                exit.state.invocation_records_complete = false;
            }
        }
        CallOutcome {
            execution: completed.execution,
        }
    }
    #[tracing::instrument(skip_all)]
    fn transfer_invocation(&self, invocation: &Invocation, mut state: State) -> Vec<Exit> {
        let Callee::Function(function) = &invocation.callee;
        let function = function.as_str();
        let arguments: Vec<Value> = invocation
            .operands
            .iter()
            .map(|operand| operand.value.clone())
            .collect();
        let influences = super::operations::inputs(&arguments, &state);
        let first = arguments.first().cloned().unwrap_or_else(Value::unknown);
        let last = arguments.last().cloned().unwrap_or_else(Value::unknown);
        match function {
            "hasKey" => {
                let Some(GuardValue::String(key)) = arguments.get(1).and_then(Value::constant)
                else {
                    return normal(state, Value::influenced(influences.clone()));
                };
                let presence = state.field_presence(&first, key);
                normal(state, presence)
            }
            "fail" => vec![Exit {
                state,
                flow: Flow::Abort(AbortReason::Fail),
            }],
            "required" => {
                let subject = arguments.get(1).cloned().unwrap_or_else(Value::unknown);
                match subject.constant() {
                    Some(GuardValue::Null) => vec![Exit {
                        state,
                        flow: Flow::Abort(AbortReason::Required),
                    }],
                    Some(GuardValue::String(text)) if text.is_empty() => vec![Exit {
                        state,
                        flow: Flow::Abort(AbortReason::Required),
                    }],
                    Some(
                        GuardValue::Bool(_)
                        | GuardValue::Int(_)
                        | GuardValue::Float(_)
                        | GuardValue::String(_),
                    ) => normal(state, subject),
                    None => {
                        if let Some(path) = subject.identity() {
                            let empty = Predicate::Or(vec![
                                Predicate::from(Guard::Eq {
                                    path: path.clone(),
                                    value: GuardValue::Null,
                                }),
                                Predicate::from(Guard::Eq {
                                    path: path.clone(),
                                    value: GuardValue::String(String::new()),
                                }),
                            ]);
                            let test = state.heap.arena.decision(
                                empty.clone(),
                                empty.negated(),
                                Coverage::Exact,
                            );
                            return vec![
                                Exit {
                                    state: state.fork(Rc::clone(&test), true),
                                    flow: Flow::Abort(AbortReason::Required),
                                },
                                Exit {
                                    state: state.fork(test, false),
                                    flow: Flow::Normal(Normal { value: subject }),
                                },
                            ];
                        }
                        normal(state, subject)
                    }
                }
            }
            "default" => choose_truth(state, last, first),
            "coalesce" => {
                let mut pending = vec![state];
                let mut exits = super::ExitBuffer::default();
                for value in arguments {
                    let mut next = super::StateBuffer::default();
                    for state in pending {
                        for exit in choose_truth(state, value.clone(), Value::nil()) {
                            match exit.flow {
                                Flow::Normal(ref normal)
                                    if normal.value.constant() == Some(&GuardValue::Null) =>
                                {
                                    next.push(exit.state);
                                }
                                Flow::Normal(_)
                                | Flow::Abort(_)
                                | Flow::Break(_)
                                | Flow::Continue(_) => exits.push(exit),
                            }
                        }
                    }
                    pending = next.finish();
                }
                for state in pending {
                    exits.extend(normal(state, Value::nil()));
                }
                exits.finish()
            }
            "ternary" => {
                let condition = arguments.get(2).cloned().unwrap_or_else(Value::unknown);
                let test = state.test(&condition);
                branches(
                    state,
                    test,
                    first,
                    arguments.get(1).cloned().unwrap_or_else(Value::unknown),
                )
            }
            "eq" | "ne" => {
                if function == "eq" && arguments.len() > 2 {
                    let result = first
                        .constant()
                        .and_then(|first| {
                            arguments
                                .iter()
                                .skip(1)
                                .map(|value| value.constant().map(|value| value == first))
                                .collect::<Option<Vec<_>>>()
                        })
                        .map(|results| results.into_iter().any(|equal| equal));
                    return normal(
                        state,
                        result.map_or_else(
                            || Value::influenced(influences.clone()),
                            |equal| Value::literal(GuardValue::Bool(equal)),
                        ),
                    );
                }
                let right = arguments.get(1).cloned().unwrap_or_else(Value::unknown);
                let mut leaves = 0;
                let result = scalar_equality(&state, &first, &right, function == "ne", &mut leaves)
                    .unwrap_or_else(|| Value::influenced(influences.clone()));
                normal(state, result)
            }
            _ => match super::operations::transfer(invocation, &mut state) {
                Ok(value) => {
                    let value = match function_semantics(function).success {
                        SuccessType::Fixed(kind) => stamp_success(value, kind),
                        SuccessType::Selected => value,
                    };
                    normal(state, value)
                }
                Err(reason) => vec![Exit {
                    state,
                    flow: Flow::Abort(reason),
                }],
            },
        }
    }
}
fn scalar_equality(
    state: &State,
    left: &Value,
    right: &Value,
    negated: bool,
    leaves: &mut usize,
) -> Option<Value> {
    match (left.node.as_ref(), right.node.as_ref()) {
        (
            ValueNode::Select { decision, yes, no },
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            })
            | ValueNode::Select {
                decision: _,
                yes: _,
                no: _,
            },
        ) => Some(Value::select(
            Rc::clone(decision),
            scalar_equality(state, yes, &right.under(decision, true), negated, leaves)?,
            scalar_equality(state, no, &right.under(decision, false), negated, leaves)?,
        )),
        (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Select { decision, yes, no },
        ) => Some(Value::select(
            Rc::clone(decision),
            scalar_equality(state, left, yes, negated, leaves)?,
            scalar_equality(state, left, no, negated, leaves)?,
        )),
        (
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
            ValueNode::Leaf(Leaf {
                datum: _,
                origin: _,
            }),
        ) => {
            *leaves += 1;
            if *leaves > super::MAX_SCALAR_DISPATCH_STATES {
                tracing::debug!(reason = "scalar_fanout", "widen scalar equality");
                return None;
            }
            if let (Some(left), Some(right)) = (left.constant(), right.constant()) {
                return Some(Value::literal(GuardValue::Bool((left == right) != negated)));
            }
            let pair = left
                .identity()
                .zip(right.constant())
                .or_else(|| right.identity().zip(left.constant()));
            if let Some((path, literal)) = pair {
                let predicate = Predicate::from(Guard::Eq {
                    path: path.clone(),
                    value: literal.clone(),
                });
                let predicate = if negated {
                    predicate.negated()
                } else {
                    predicate
                };
                let decision = state.heap.arena.decision(
                    predicate.clone(),
                    predicate.negated(),
                    Coverage::Exact,
                );
                Some(Value::select(
                    decision,
                    Value::literal(GuardValue::Bool(true)),
                    Value::literal(GuardValue::Bool(false)),
                ))
            } else {
                Some(Value::influenced(
                    state
                        .heap
                        .freeze(left)
                        .inputs()
                        .union(&state.heap.freeze(right).inputs())
                        .cloned()
                        .collect(),
                ))
            }
        }
    }
}

fn choose_truth(state: State, value: Value, fallback: Value) -> Vec<Exit> {
    let test = state.test(&value);
    branches(state, test, value, fallback)
}
fn branches(state: State, test: Rc<super::FrozenTest>, yes: Value, no: Value) -> Vec<Exit> {
    let mut exits = Vec::new();
    for (taken, value) in [(true, yes), (false, no)] {
        if test.coverage == Coverage::Exact
            && (if taken {
                &test.when_true
            } else {
                &test.when_false
            }) == &Predicate::False
        {
            continue;
        }
        let state = state.fork(Rc::clone(&test), taken);
        if state.path_predicate().normalize_boolean() != Predicate::False {
            exits.extend(normal(state, value));
        }
    }
    exits
}
fn valid_parameter(state: &State, value: &Value, expected: Option<GoType>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    if expected == GoType::Unknown {
        return true;
    }
    let value = state.consumed(value);
    let actual = state.heap.freeze(&value).go_type();
    actual == GoType::Unknown
        || actual == expected
        || actual == GoType::Nil && matches!(expected, GoType::Map | GoType::Slice)
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
    let types = match kind {
        GoType::Nil => super::TypeCoverage::Scalar(super::ScalarType::Nil),
        GoType::Bool => super::TypeCoverage::Scalar(super::ScalarType::Bool),
        GoType::String => super::TypeCoverage::Scalar(super::ScalarType::String),
        GoType::Int => super::TypeCoverage::Scalar(super::ScalarType::Int),
        GoType::Int64 => super::TypeCoverage::Scalar(super::ScalarType::Int64),
        GoType::Uint64 => super::TypeCoverage::Scalar(super::ScalarType::Uint64),
        GoType::Float64 => super::TypeCoverage::Scalar(super::ScalarType::Float64),
        GoType::Map => super::TypeCoverage::Map(super::MapType::Interface),
        GoType::Slice => super::TypeCoverage::Sequence,
        GoType::Unknown => return value,
    };
    Value {
        node: Rc::new(ValueNode::Leaf(Leaf {
            datum: Datum::Unknown(super::UnknownValue {
                types,
                aliases: unknown.aliases.clone(),
            }),
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
                        value: Value::leaf(Datum::Sequence(Some(Rc::new(values)))),
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
            .all(|exit| exit.state.invocation_records_complete)
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
        self.values
    }
}
