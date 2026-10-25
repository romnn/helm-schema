use super::eval::{Executor, normal};
use super::{
    CallOutcome, Callee, CompletionInput, Execution, HelperReturn, Invocation, State, Value,
};
use helm_schema_core::GuardValue;

impl Executor<'_> {
    #[tracing::instrument(skip_all)]
    pub(super) fn helper(
        &self,
        invocation: Invocation,
        mut state: State,
        allow_summary: bool,
    ) -> CallOutcome {
        let Callee::Function(function) = &invocation.callee;
        let function = function.clone();
        let arguments: Vec<_> = invocation
            .operands
            .iter()
            .map(|operand| operand.value.clone())
            .collect();
        let influences = super::operations::inputs(&arguments, &state);
        let caller_program = state.program;
        if function != "tpl" && state.namespace_unknown {
            super::operations::invalidate(&mut state);
            return self.complete(CompletionInput::Executed {
                invocation,
                execution: Execution {
                    exits: normal(state, Value::influenced(influences)),
                },
                helper_return: None,
            });
        }
        let Some(GuardValue::String(name)) = arguments.first().and_then(Value::constant) else {
            state.namespace_unknown = true;
            super::operations::invalidate(&mut state);
            return self.complete(CompletionInput::Executed {
                invocation,
                execution: Execution {
                    exits: normal(state, Value::influenced(influences)),
                },
                helper_return: None,
            });
        };
        let argument = arguments.get(1).cloned().unwrap_or_else(Value::nil);
        let source = if function == "tpl" {
            Some(name.as_str())
        } else {
            self.db.parsed_helper_body(name).map(|body| body.source)
        };
        let Some(source) = source else {
            super::operations::invalidate(&mut state);
            return self.complete(CompletionInput::Executed {
                invocation,
                execution: Execution {
                    exits: normal(state, Value::influenced(influences)),
                },
                helper_return: None,
            });
        };
        if state.calls.len() >= 8
            || function != "tpl" && state.calls.iter().any(|active| active == name)
        {
            super::operations::invalidate(&mut state);
            return self.complete(CompletionInput::Executed {
                invocation,
                execution: Execution {
                    exits: normal(state, Value::influenced(influences)),
                },
                helper_return: None,
            });
        }
        if allow_summary {
            if let Some(plan) = super::shape_summary::certified_plan(source) {
                return self.complete(CompletionInput::Summarized {
                    plan,
                    invocation,
                    state,
                });
            }
        }
        if function == "tpl"
            && helm_schema_syntax::TemplatedDocument::parse(source)
                .actions()
                .iter()
                .any(|action| {
                    matches!(
                        action.kind,
                        helm_schema_syntax::ActionKind::RegionOpen {
                            kind: helm_schema_syntax::ControlKind::Define,
                            region: _,
                            region_end: _
                        }
                    )
                })
        {
            state.namespace_unknown = true;
        }
        let output = std::mem::take(&mut state.output);
        let loops = std::mem::take(&mut state.loops);
        state.enter_call(argument);
        state.calls.push(name.clone());
        let path = if function == "tpl" {
            None
        } else {
            self.db
                .parsed_helper_body(name)
                .map(|body| body.source_path)
        };
        let exits = self.body(source, path, state);
        let program = exits
            .first()
            .map_or(caller_program, |exit| exit.state.program);
        let wrapper = super::shape_summary::executed_wrapper_metadata(source, program);
        self.complete(CompletionInput::Executed {
            invocation,
            execution: Execution { exits },
            helper_return: Some(HelperReturn {
                program: caller_program,
                output,
                loops,
                wrapper,
            }),
        })
    }
}
