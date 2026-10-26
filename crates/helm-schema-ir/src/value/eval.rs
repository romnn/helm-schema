use super::scalar_value::ScalarContent;
use super::{Exit, Flow, Normal, ScalarType, State, Value};
use helm_schema_ast::{Literal, TemplateExpr};
use helm_schema_core::GuardValue;

pub(super) struct Executor<'a> {
    pub(super) db: &'a crate::analysis_db::IrAnalysisDb,
}
impl Executor<'_> {
    pub(super) fn expression(
        &self,
        expression: &TemplateExpr,
        state: State,
        site: usize,
    ) -> Vec<Exit> {
        let exits = match expression {
            TemplateExpr::Literal(literal) => normal(
                state,
                match literal {
                    Literal::String(text) | Literal::RawString(text) => Value::string(text),
                    Literal::UnresolvedString(_) => Value::scalar(ScalarType::String, None),
                    Literal::Int(value) => Value::literal(GuardValue::Int(*value)),
                    Literal::Float(value) => Value::scalar(
                        ScalarType::Float64,
                        GuardValue::float(*value).map(ScalarContent::Literal),
                    ),
                    Literal::Bool(value) => Value::literal(GuardValue::Bool(*value)),
                    Literal::Nil => Value::nil(),
                },
            ),
            TemplateExpr::Field(path) => {
                let value = state.dot().cloned().unwrap_or_else(Value::unknown);
                self.selector(state, value, path, site)
            }
            TemplateExpr::Selector { operand, path } => {
                let mut result = super::ExitBuffer::default();
                for exit in self.expression(operand, state, site) {
                    match exit.flow {
                        Flow::Normal(normal) => {
                            result.extend(self.selector(exit.state, normal.value, path, site));
                        }
                        flow => result.push(Exit {
                            state: exit.state,
                            flow,
                        }),
                    }
                }
                result.finish()
            }
            TemplateExpr::Variable(name) => {
                let name = if name.is_empty() { "$" } else { name.as_str() };
                let value = state
                    .read_local(name)
                    .cloned()
                    .unwrap_or_else(Value::unknown);
                normal(state, value)
            }
            TemplateExpr::Parenthesized(expression) => self.expression(expression, state, site),
            TemplateExpr::VariableDefinition { name, value }
            | TemplateExpr::Assignment { name, value } => self
                .expression(value, state, site)
                .into_iter()
                .map(|mut exit| {
                    if let Flow::Normal(normal) = &exit.flow {
                        let value = normal.value.clone();
                        let name = local_name(name);
                        if matches!(expression, TemplateExpr::VariableDefinition { .. }) {
                            exit.state.bind(name.to_owned(), value);
                        } else if !exit.state.assign(name, value) {
                            super::operations::invalidate(&mut exit.state);
                        }
                    }
                    exit
                })
                .collect(),
            TemplateExpr::Call { function, args } => self.invoke(function, args, None, state, site),
            TemplateExpr::Pipeline(stages) => self.pipeline(stages, state, site),
            TemplateExpr::Unknown(_) => {
                let mut state = state;
                super::operations::invalidate(&mut state);
                normal(state, Value::unknown())
            }
        };
        super::control::bound_exits(exits)
    }
    fn pipeline(&self, stages: &[TemplateExpr], state: State, site: usize) -> Vec<Exit> {
        let Some((first, stages)) = stages.split_first() else {
            return normal(state, Value::unknown());
        };
        let mut exits = self.expression(first, state, site);
        for stage in stages {
            let mut next = super::ExitBuffer::default();
            for exit in exits {
                match exit.flow {
                    Flow::Normal(normal) => match stage {
                        TemplateExpr::Call { function, args } => next.extend(self.invoke(
                            function,
                            args,
                            Some(normal.value),
                            exit.state,
                            site,
                        )),
                        TemplateExpr::Field(_)
                        | TemplateExpr::Selector {
                            operand: _,
                            path: _,
                        } => {
                            if let Some(receiver) =
                                crate::analysis_db::ReceiverCall::from_expression(stage)
                            {
                                next.extend(self.invoke_receiver(
                                    receiver,
                                    &[],
                                    Some(&normal.value),
                                    exit.state,
                                    site,
                                ));
                            } else {
                                let mut state = exit.state;
                                super::operations::invalidate(&mut state);
                                next.extend(normal_exit(state, Value::unknown()));
                            }
                        }
                        TemplateExpr::Literal(_)
                        | TemplateExpr::Variable(_)
                        | TemplateExpr::Pipeline(_)
                        | TemplateExpr::Parenthesized(_)
                        | TemplateExpr::VariableDefinition { name: _, value: _ }
                        | TemplateExpr::Assignment { name: _, value: _ }
                        | TemplateExpr::Unknown(_) => {
                            let mut state = exit.state;
                            super::operations::invalidate(&mut state);
                            next.extend(normal_exit(state, Value::unknown()));
                        }
                    },
                    flow => next.push(Exit {
                        state: exit.state,
                        flow,
                    }),
                }
            }
            exits = next.finish();
        }
        exits
    }
    fn selector(&self, state: State, value: Value, path: &[String], site: usize) -> Vec<Exit> {
        let live = state.selector_live_domain(&value);
        let mut exits = normal(state, value);
        for key in path {
            let mut next = super::ExitBuffer::default();
            for exit in exits {
                let Exit { mut state, flow } = exit;
                match flow {
                    Flow::Normal(Normal { value }) => {
                        let candidate = crate::function_semantics::Method::resolve(
                            crate::function_semantics::MethodReceiver::Values,
                            key,
                        );
                        let receiver = (!candidate.is_foreign()).then(|| state.consumed(&value));
                        let descriptor = receiver
                            .as_ref()
                            .map_or(candidate, |receiver| super::hosts::method(receiver, key));
                        if descriptor.is_foreign() {
                            state.record_selector_host(&value, &live);
                            let value = state.selector_field(&value, key);
                            next.extend(normal(state, value));
                        } else {
                            next.extend(self.invoke_callee(
                                super::Callee::Method {
                                    receiver: receiver.unwrap_or(value),
                                    descriptor,
                                },
                                &[],
                                None,
                                state,
                                site,
                            ));
                        }
                    }
                    flow => next.push(Exit { state, flow }),
                }
            }
            exits = next.finish();
        }
        for exit in &mut exits {
            if let Flow::Normal(Normal { value }) = &exit.flow {
                exit.state
                    .record_value_read(value, super::ReadCoverage::ExactPath);
            }
        }
        exits
    }
}
pub(super) fn normal(state: State, value: Value) -> Vec<Exit> {
    normal_exit(state, value)
}
fn normal_exit(state: State, value: Value) -> Vec<Exit> {
    vec![Exit {
        state,
        flow: Flow::Normal(Normal { value }),
    }]
}
pub(super) fn local_name(name: &str) -> &str {
    if name == "$" {
        "$"
    } else {
        name.trim_start_matches('$')
    }
}
