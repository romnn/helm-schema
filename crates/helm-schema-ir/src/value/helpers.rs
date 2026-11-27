use std::collections::BTreeSet;
use std::rc::Rc;

use super::eval::{Executor, normal};
use super::namespace::{TemplateLookup, TemplateResolution};
use super::{
    AbortReason, CallKey, CallOutcome, Execution, Exit, Flow, HelperReturn, Invocation, Origin,
    ScalarType, State, Value,
};
use crate::analysis_db::ParsedProgram;
use helm_schema_core::{GuardValue, Predicate, ValuesPath};

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
    pub(super) fn helper(&self, invocation: Invocation, mut state: State) -> CallOutcome {
        let Some(function) = invocation.callee.function_name().map(str::to_string) else {
            let mut state = state;
            state.mark_invocation_incomplete();
            super::operations::invalidate(&mut state);
            return Self::complete_executed(
                invocation,
                Execution {
                    exits: normal(state, Value::unknown()),
                },
                None,
            );
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
        let branches = self.helper_branches(
            &function,
            arguments.first().and_then(Value::constant),
            state,
        );
        let mut exits = super::ExitBuffer::default();
        for branch in branches {
            exits.extend(self.execute_helper_branch(&function, branch, &argument, &arguments));
        }
        Self::complete_executed(
            invocation,
            Execution { exits: exits.exits },
            Some(HelperReturn {
                program: caller_program,
                namespace: caller_namespace,
                output: caller_output,
                loops: caller_loops,
            }),
        )
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
        function: &str,
        branch: HelperBranch,
        argument: &Value,
        arguments: &[Value],
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
        state.enter_call(argument);
        state.calls.push(call);
        match target {
            HelperTarget::Abort(reason) => {
                if reason == AbortReason::MissingTemplate {
                    state.record_failure(&Predicate::True);
                }
                vec![Exit {
                    state,
                    flow: Flow::Abort(reason),
                }]
            }
            HelperTarget::Program(program) if !recursive && program.syntax().is_some() => {
                // An execution summary can replace an empty prefix with one placeholder.
                if program
                    .syntax()
                    .and_then(|syntax| syntax.text_term_bound)
                    .is_some_and(|bound| bound < super::output::OUTPUT_EVENT_LIMIT)
                {
                    state.output.counter = None;
                }
                self.parsed_body(&program, state)
            }
            HelperTarget::Program(_) | HelperTarget::Unknown => {
                let influences = super::operations::inputs(arguments, &state);
                state.mark_invocation_incomplete();
                let read_only_inputs = match &target {
                    HelperTarget::Program(program) => program.syntax().is_some_and(|syntax| {
                        let targets = super::write_footprint::local_targets(
                            program.source(),
                            syntax,
                            &state.namespace,
                        );
                        super::stage0_diagnostics::certificate(
                            program.source(),
                            &state.namespace,
                            targets.as_ref(),
                        );
                        targets.is_some()
                    }),
                    HelperTarget::Abort(_) | HelperTarget::Unknown => false,
                };
                let program = matches!(target, HelperTarget::Unknown)
                    .then(|| arguments.first())
                    .flatten()
                    .filter(|_| function == "tpl");
                // An unknown program is analyzed under the assumption that it writes nothing:
                // facts below are claimed only where it holds no template action (ADR 0072 §2,
                // §5). Where that cannot be stated, its effect is still an unknown write, to
                // what its data argument reaches: Helm executes it against that context alone,
                // in a cloned template namespace (`engine.go` `tpl`, `include`).
                let assumed = program.is_some_and(|program| state.assume_action_free(program));
                let states = if !assumed && !read_only_inputs {
                    super::operations::opaque_call(argument, arguments, state)
                } else {
                    vec![state]
                };
                states
                    .into_iter()
                    .flat_map(|mut state| {
                        let value = unknown_helper_text(influences.clone());
                        state.output.emit(state.heap.freeze(&value));
                        normal(state, value)
                    })
                    .collect()
            }
        }
    }
}

fn unknown_helper_text(influences: BTreeSet<ValuesPath>) -> Value {
    Value {
        node: Rc::new(super::ValueNode::Leaf(super::Leaf {
            datum: super::Datum::Scalar {
                go_type: ScalarType::String,
                content: None,
            },
            origin: Origin {
                derivation: None,
                identity: None,
                influences,
            },
        })),
    }
}

#[cfg(test)]
#[path = "../tests/runtime_helper_text_budget.rs"]
pub(super) mod tests;
