use std::collections::BTreeSet;
use std::rc::Rc;

use super::eval::{Executor, normal};
use super::namespace::{TemplateLookup, TemplateResolution};
use super::{
    AbortReason, CallKey, CallOutcome, CompletionInput, Execution, Exit, Flow, HelperReturn,
    Invocation, Origin, ProgramWrapperMetadata, ScalarType, State, Value,
};
use crate::analysis_db::ParsedProgram;
use helm_schema_core::{GuardValue, ValuesPath};

struct HelperBranch {
    state: State,
    target: HelperTarget,
}

enum HelperTarget {
    Program(Rc<ParsedProgram>),
    Abort(AbortReason),
    Unknown,
}

impl Executor<'_> {
    #[tracing::instrument(skip_all, fields(function = invocation.callee.name(), helper = ?invocation.operands.first().and_then(|operand| operand.value.constant())))]
    pub(super) fn helper(
        &self,
        invocation: Invocation,
        mut state: State,
        allow_summary: bool,
    ) -> CallOutcome {
        let Some(function) = invocation.callee.function_name().map(str::to_string) else {
            let mut state = state;
            state.mark_invocation_incomplete();
            super::operations::invalidate(&mut state);
            return self.complete(CompletionInput::Executed {
                invocation,
                execution: Execution {
                    exits: normal(state, Value::unknown()),
                },
                helper_return: None,
            });
        };
        let arguments: Vec<_> = invocation
            .operands
            .iter()
            .map(|operand| operand.value.clone())
            .collect();
        let caller_program = state.program;
        let caller_namespace = Rc::clone(&state.namespace);
        let caller_output = std::mem::take(&mut state.output);
        let caller_loops = std::mem::take(&mut state.loops);
        let argument = arguments.get(1).cloned().unwrap_or_else(Value::nil);
        let mut branches = self.helper_branches(
            &function,
            arguments.first().and_then(Value::constant),
            state,
        );
        let plan = branches.first().and_then(|branch| match &branch.target {
            HelperTarget::Program(program)
                if allow_summary
                    && function != "tpl"
                    && branches.len() == 1
                    && !recursive_call(&branch.state, program) =>
            {
                super::shape_summary::certified_plan(program.source())
            }
            HelperTarget::Program(_) | HelperTarget::Abort(_) | HelperTarget::Unknown => None,
        });
        if let Some(plan) = plan
            && let Some(mut branch) = branches.pop()
        {
            branch.state.output = caller_output;
            branch.state.loops = caller_loops;
            return self.complete(CompletionInput::Summarized {
                plan,
                invocation,
                state: branch.state,
            });
        }
        let mut exits = super::ExitBuffer::default();
        let mut wrappers = Vec::new();
        for branch in branches {
            exits.extend(self.execute_helper_branch(branch, &argument, &arguments, &mut wrappers));
        }
        let wrapper = wrappers.first().cloned().flatten().filter(|first| {
            wrappers
                .iter()
                .all(|wrapper| wrapper.as_ref() == Some(first))
        });
        self.complete(CompletionInput::Executed {
            invocation,
            execution: Execution { exits: exits.exits },
            helper_return: Some(HelperReturn {
                program: caller_program,
                namespace: caller_namespace,
                output: caller_output,
                loops: caller_loops,
                wrapper,
            }),
        })
    }

    fn helper_branches(
        &self,
        function: &str,
        name: Option<&GuardValue>,
        state: State,
    ) -> Vec<HelperBranch> {
        match name {
            Some(GuardValue::String(name)) if function == "tpl" => {
                let source = self.db.parsed_source(name, None, "gotpl");
                let program = Rc::clone(&source.root);
                let build = state.namespace.overlay(&source, &state.heap.arena);
                build
                    .install(state)
                    .into_iter()
                    .map(|exit| HelperBranch {
                        state: exit.state,
                        target: match exit.flow {
                            Flow::Normal(_) => HelperTarget::Program(Rc::clone(&program)),
                            Flow::Abort(reason) => HelperTarget::Abort(reason),
                            Flow::Break(_) | Flow::Continue(_) => HelperTarget::Unknown,
                        },
                    })
                    .collect()
            }
            Some(GuardValue::String(name)) => {
                let namespace = Rc::clone(&state.namespace);
                namespace
                    .resolve(name, state)
                    .into_iter()
                    .map(|lookup| {
                        let TemplateLookup { state, resolution } = lookup;
                        HelperBranch {
                            state,
                            target: match resolution {
                                TemplateResolution::Found(program) => {
                                    HelperTarget::Program(program)
                                }
                                TemplateResolution::Missing => {
                                    HelperTarget::Abort(AbortReason::MissingTemplate)
                                }
                                TemplateResolution::Unknown => HelperTarget::Unknown,
                            },
                        }
                    })
                    .collect()
            }
            Some(
                GuardValue::Null | GuardValue::Bool(_) | GuardValue::Int(_) | GuardValue::Float(_),
            )
            | None => vec![HelperBranch {
                state,
                target: HelperTarget::Unknown,
            }],
        }
    }

    fn execute_helper_branch(
        &self,
        branch: HelperBranch,
        argument: &Value,
        arguments: &[Value],
        wrappers: &mut Vec<Option<ProgramWrapperMetadata>>,
    ) -> Vec<Exit> {
        let HelperBranch { mut state, target } = branch;
        let program = match &target {
            HelperTarget::Program(program) => Some(program.id()),
            HelperTarget::Abort(_) | HelperTarget::Unknown => None,
        };
        let call = CallKey {
            namespace: state.namespace.id(),
            program,
        };
        let recursive = state.calls.len() >= 8 || program.is_some() && state.calls.contains(&call);
        state.enter_call(argument.clone());
        state.calls.push(call);
        match target {
            HelperTarget::Abort(reason) => {
                wrappers.push(None);
                vec![Exit {
                    state,
                    flow: Flow::Abort(reason),
                }]
            }
            HelperTarget::Program(program) if !recursive && program.syntax().is_some() => {
                let returned = self.parsed_body(&program, state);
                for exit in &returned {
                    wrappers.push(super::shape_summary::executed_wrapper_metadata(
                        program.source(),
                        exit.state.program,
                    ));
                }
                returned
            }
            HelperTarget::Program(_) | HelperTarget::Unknown => {
                let influences = super::operations::inputs(arguments, &state);
                state.mark_invocation_incomplete();
                super::operations::invalidate(&mut state);
                let value = unknown_helper_text(influences);
                state.output.emit(state.heap.freeze(&value));
                wrappers.push(None);
                normal(state, value)
            }
        }
    }
}

fn recursive_call(state: &State, program: &ParsedProgram) -> bool {
    state.calls.len() >= 8
        || state.calls.contains(&CallKey {
            namespace: state.namespace.id(),
            program: Some(program.id()),
        })
}

fn unknown_helper_text(influences: BTreeSet<ValuesPath>) -> Value {
    Value {
        node: Rc::new(super::ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Scalar {
                go_type: ScalarType::String,
                content: None,
            },
            origin: Origin {
                identity: None,
                influences,
            },
        })),
    }
}
