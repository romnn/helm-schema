use super::eval::{Executor, local_name, normal};
use super::{
    Coverage, Datum, Exit, Flow, Leaf, Normal, ObjectData, Remainder, State, Value, ValueNode,
};
use helm_schema_ast::TemplateExpr;
use helm_schema_core::{GuardValue, Predicate};
use helm_schema_syntax::{ActionKind, ControlKind, Span, TemplateAction, TextTrim, trim_text_span};
use std::collections::BTreeSet;
use std::rc::Rc;

struct Program<'a> {
    source: &'a str,
    path: Option<&'a str>,
    syntax: &'a crate::analysis_db::ParsedProgramSyntax,
}
#[derive(Clone, Copy)]
struct Window {
    start: usize,
    end: usize,
}
struct RangeEntry {
    key: Value,
    value: Value,
}
enum RangeInput {
    Exact(Vec<RangeEntry>),
    Summary(RangeSummary),
    Invalid,
}
struct RangeSummary {
    nonempty: bool,
    member: Value,
}
impl Executor<'_> {
    pub(super) fn body(&self, source: &str, path: Option<&str>, state: State) -> Vec<Exit> {
        let source = self.db.parsed_source(source, path, path.unwrap_or("gotpl"));
        self.parsed_body(&source.root, state)
    }

    pub(super) fn parsed_body(
        &self,
        parsed: &crate::analysis_db::ParsedProgram,
        state: State,
    ) -> Vec<Exit> {
        let program = state.heap.arena.program();
        self.execute_parsed_body(
            parsed.source(),
            parsed.source_path(),
            parsed.syntax(),
            state,
            program,
        )
    }

    pub(super) fn parsed_file_body(
        &self,
        source: &str,
        path: Option<&str>,
        syntax: Option<&crate::analysis_db::ParsedProgramSyntax>,
        state: State,
        occurrence: &super::session::FileOccurrence,
    ) -> Vec<Exit> {
        self.execute_parsed_body(source, path, syntax, state, occurrence.program)
    }

    fn execute_parsed_body(
        &self,
        source: &str,
        path: Option<&str>,
        syntax: Option<&crate::analysis_db::ParsedProgramSyntax>,
        mut state: State,
        program: u64,
    ) -> Vec<Exit> {
        state.program = program;
        let Some(syntax) = syntax.filter(|syntax| !syntax.tree.root_node().has_error()) else {
            state.invocation_records_complete = false;
            super::operations::invalidate(&mut state);
            state.output.emit(state.heap.freeze(&Value::unknown()));
            return normal(state, Value::unknown());
        };
        let program = Program {
            source,
            path,
            syntax,
        };
        self.window(
            &program,
            Window {
                start: 0,
                end: source.len(),
            },
            normal(state, Value::nil()),
        )
    }
    fn window(&self, program: &Program<'_>, window: Window, exits: Vec<Exit>) -> Vec<Exit> {
        let mut exits = super::window_exits::WindowExits::new(exits);
        let mut cursor = window.start;
        let mut index = program
            .syntax
            .actions
            .partition_point(|action| action.span.start < window.start);
        while let Some(action) = program
            .syntax
            .actions
            .get(index)
            .copied()
            .filter(|action| action.span.start < window.end)
        {
            let span = literal_span(program, index, Span::new(cursor, action.span.start));
            let bytes = program.source.get(span.start..span.end).unwrap_or_default();
            exits.emit_bytes(span, program.path, bytes);
            cursor = action.span.end;
            let mut next = exits.next_buffer();
            for chunk in exits.into_chunks() {
                let mut exit = match chunk {
                    super::window_exits::WindowChunk::Single(exit) => *exit,
                    super::window_exits::WindowChunk::Terminal(run) => {
                        next.forward(run);
                        continue;
                    }
                };
                if !matches!(exit.flow, Flow::Normal(_)) {
                    next.push(exit);
                    continue;
                }
                match action.kind {
                    ActionKind::Output {
                        expr_span: _,
                        render_indent: _,
                    }
                    | ActionKind::Assign => {
                        next.extend(self.output_action(program, action, exit.state));
                    }
                    ActionKind::RegionOpen {
                        region,
                        kind,
                        region_end,
                    } => {
                        next.extend(
                            self.region(program, action, region, kind, region_end, exit.state),
                        );
                    }
                    ActionKind::Break | ActionKind::Continue => {
                        if let Some(id) = exit.state.loops.last().copied() {
                            exit.flow = if matches!(action.kind, ActionKind::Break) {
                                Flow::Break(id)
                            } else {
                                Flow::Continue(id)
                            };
                        } else {
                            super::operations::invalidate(&mut exit.state);
                        }
                        next.push(exit);
                    }
                    ActionKind::TemplateComment => next.push(exit),
                    ActionKind::RegionBranch { region: _ }
                    | ActionKind::RegionEnd { region: _ }
                    | ActionKind::Error => {
                        super::operations::invalidate(&mut exit.state);
                        next.push(exit);
                    }
                }
            }
            exits = next.finish();
            if let ActionKind::RegionOpen {
                region_end,
                region: _,
                kind: _,
            } = action.kind
            {
                cursor = region_end;
                index = program
                    .syntax
                    .actions
                    .partition_point(|action| action.span.start < region_end);
            } else {
                index += 1;
            }
        }
        let span = literal_span(program, index, Span::new(cursor, window.end));
        exits.emit_bytes(
            span,
            program.path,
            program.source.get(span.start..span.end).unwrap_or_default(),
        );
        exits.into_exits()
    }
    fn output_action(
        &self,
        program: &Program<'_>,
        action: TemplateAction,
        state: State,
    ) -> Vec<Exit> {
        let rendered = matches!(action.kind, ActionKind::Output { .. });
        let expressions = program
            .syntax
            .expressions
            .at(action.span)
            .map(|parsed| parsed.expressions.as_ref())
            .unwrap_or_default();
        let mut action_exits = normal(state, Value::nil());
        for expression in expressions {
            let mut evaluated = super::ExitBuffer::default();
            for exit in action_exits {
                match exit.flow {
                    Flow::Normal(_) => {
                        evaluated.extend(self.expression(
                            expression,
                            exit.state,
                            action.span.start,
                        ));
                    }
                    flow => evaluated.push(Exit {
                        state: exit.state,
                        flow,
                    }),
                }
            }
            action_exits = evaluated.finish();
        }
        for exit in &mut action_exits {
            if let Flow::Normal(value) = &exit.flow {
                exit.state
                    .observe(action.span, program.path, &value.value, rendered);
            }
        }
        action_exits
    }
    fn region(
        &self,
        program: &Program<'_>,
        opener: TemplateAction,
        region: usize,
        kind: ControlKind,
        end: usize,
        mut state: State,
    ) -> Vec<Exit> {
        if kind == ControlKind::Define {
            return normal(state, Value::nil());
        }
        if kind == ControlKind::Block {
            let mut exits = if let Some(call) = program.syntax.block_calls.get(&opener.span.start) {
                self.expression(call, state, opener.span.start)
            } else {
                super::operations::invalidate(&mut state);
                normal(state, Value::unknown())
            };
            for exit in &mut exits {
                if let Flow::Normal(value) = &exit.flow {
                    exit.state
                        .observe(opener.span, program.path, &value.value, true);
                }
            }
            return exits;
        }
        let boundaries: Vec<_> = program.syntax.actions.iter().copied().filter(|action| action.span.start > opener.span.start && action.span.start < end && matches!(action.kind, ActionKind::RegionBranch { region: candidate } | ActionKind::RegionEnd { region: candidate } if candidate == region)).collect();
        if kind == ControlKind::Range {
            return self.range(program, opener, &boundaries, state);
        }
        state.enter_scope(None);
        let mut pending = vec![state];
        let mut results = super::ExitBuffer::default();
        let mut header = opener;
        for boundary in &boundaries {
            let mut next = super::StateBuffer::default();
            for state in pending {
                let expression = program
                    .syntax
                    .expressions
                    .at(header.span)
                    .and_then(|parsed| parsed.expressions.first());
                let evaluated = match expression {
                    Some(expression) => self.expression(expression, state, header.span.start),
                    None => normal(state, Value::literal(GuardValue::Bool(true))),
                };
                for mut exit in reconverge(evaluated) {
                    let Flow::Normal(value) = exit.flow else {
                        results.push(exit);
                        continue;
                    };
                    exit.state
                        .observe(header.span, program.path, &value.value, false);
                    let test = exit.state.test(&value.value);
                    for taken in [true, false] {
                        if test.coverage == Coverage::Exact
                            && (if taken {
                                &test.when_true
                            } else {
                                &test.when_false
                            }) == &Predicate::False
                        {
                            continue;
                        }
                        let mut state = exit.state.fork(Rc::clone(&test), taken);
                        if state.path_is_infeasible() {
                            continue;
                        }
                        if taken {
                            state.enter_scope(if kind == ControlKind::With {
                                Some(value.value.clone())
                            } else {
                                None
                            });
                            let mut branch = self.window(
                                program,
                                Window {
                                    start: header.span.end,
                                    end: boundary.span.start,
                                },
                                normal(state, Value::nil()),
                            );
                            for exit in &mut branch {
                                exit.state.leave_scope();
                            }
                            results.extend(branch);
                        } else {
                            next.push(state);
                        }
                    }
                }
            }
            pending = next.finish();
            header = *boundary;
            if matches!(boundary.kind, ActionKind::RegionEnd { .. }) {
                break;
            }
        }
        for state in pending {
            results.extend(normal(state, Value::nil()));
        }
        let mut results = results.finish();
        for exit in &mut results {
            exit.state.leave_scope();
        }
        reconverge(results)
    }
    #[tracing::instrument(skip_all)]
    fn range(
        &self,
        program: &Program<'_>,
        opener: TemplateAction,
        boundaries: &[TemplateAction],
        mut state: State,
    ) -> Vec<Exit> {
        state.enter_scope(None);
        let node = node_at(
            program.syntax.tree.root_node(),
            opener.span.start,
            "range_action",
        );
        let header =
            node.and_then(|node| helm_schema_ast::range_header_from_source(node, program.source));
        let expression = header.as_ref().map(helm_schema_ast::TemplateHeader::expr);
        let source = expression.map(|expression| match expression {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => value.as_ref(),
            other => other,
        });
        let evaluated = match source {
            Some(expression) => self.expression(expression, state, opener.span.start),
            None => normal(state, Value::unknown()),
        };
        let Some(first_boundary) = boundaries.first() else {
            return evaluated;
        };
        let body = Window {
            start: opener.span.end,
            end: first_boundary.span.start,
        };
        let bindings = range_bindings(node, expression, program.source, body);
        let mut results = super::ExitBuffer::default();
        for mut exit in evaluated {
            let Flow::Normal(value) = exit.flow else {
                results.push(exit);
                continue;
            };
            exit.state.record_container_read(&value.value);
            exit.state
                .observe(opener.span, program.path, &value.value, false);
            for name in [bindings.key.as_deref(), bindings.value.as_deref()]
                .into_iter()
                .flatten()
            {
                bind_range(
                    &mut exit.state,
                    name,
                    value.value.clone(),
                    bindings.assignment,
                );
            }
            let entries = match range_domain(&exit.state, &value.value, bindings.key.is_some()) {
                RangeInput::Exact(entries) => entries,
                RangeInput::Summary(summary) => {
                    results.extend(self.unknown_range(
                        program,
                        boundaries,
                        &exit.state,
                        &value.value,
                        &bindings,
                        summary,
                    ));
                    continue;
                }
                RangeInput::Invalid => {
                    exit.flow = Flow::Abort(super::AbortReason::Type);
                    results.push(exit);
                    continue;
                }
            };
            if entries.is_empty() {
                results.extend(self.range_else(program, boundaries, exit.state));
                continue;
            }
            let Some(id) = exit.state.loop_id(opener.span.start as u64) else {
                results.extend(normal(exit.state, Value::unknown()));
                continue;
            };
            results.extend(self.range_entries(program, &bindings, entries, id, exit.state));
        }
        let mut results = results.finish();
        for exit in &mut results {
            exit.state.leave_scope();
        }
        results
    }
    #[tracing::instrument(skip_all, fields(members = entries.len()))]
    fn range_entries(
        &self,
        program: &Program<'_>,
        bindings: &RangeBindings,
        entries: Vec<RangeEntry>,
        id: super::LoopId,
        state: State,
    ) -> Vec<Exit> {
        let mut pending = normal(state, Value::nil());
        let mut breaks = super::ExitBuffer::default();
        for entry in entries {
            let mut next = super::ExitBuffer::default();
            for mut exit in pending {
                if !matches!(exit.flow, Flow::Normal(_)) {
                    next.push(exit);
                    continue;
                }
                if let Some(name) = &bindings.key {
                    bind_range(
                        &mut exit.state,
                        name,
                        entry.key.clone(),
                        bindings.assignment,
                    );
                }
                if let Some(name) = &bindings.value {
                    bind_range(
                        &mut exit.state,
                        name,
                        entry.value.clone(),
                        bindings.assignment,
                    );
                }
                exit.state.enter_scope(Some(entry.value.clone()));
                exit.state.loops.push(id);
                let body_exits = self.window(
                    program,
                    Window {
                        start: bindings.body.start,
                        end: bindings.body.end,
                    },
                    normal(exit.state, Value::nil()),
                );
                for mut exit in body_exits {
                    exit.state.loops.pop();
                    exit.state.leave_scope();
                    match exit.flow {
                        Flow::Break(target) if target == id => {
                            exit.flow = Flow::Normal(Normal {
                                value: Value::nil(),
                            });
                            breaks.push(exit);
                        }
                        Flow::Continue(target) if target == id => {
                            exit.flow = Flow::Normal(Normal {
                                value: Value::nil(),
                            });
                            next.push(exit);
                        }
                        Flow::Normal(_) | Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => {
                            next.push(exit);
                        }
                    }
                }
            }
            pending = next.finish();
        }
        pending.extend(breaks.finish());
        pending
    }
    #[tracing::instrument(skip_all, fields(nonempty = summary.nonempty))]
    fn unknown_range(
        &self,
        program: &Program<'_>,
        boundaries: &[TemplateAction],
        state: &State,
        value: &Value,
        bindings: &RangeBindings,
        summary: RangeSummary,
    ) -> Vec<Exit> {
        let RangeSummary { nonempty, member } = summary;
        let mut exits = Vec::new();
        let mut state = if nonempty {
            state.clone()
        } else {
            let test =
                state
                    .heap
                    .arena
                    .decision(Predicate::False, Predicate::False, Coverage::Partial);
            exits.extend(self.range_else(program, boundaries, state.fork(Rc::clone(&test), false)));
            state.fork(test, true)
        };
        widen_loop_writes(program, bindings, value, &mut state);
        state.enter_scope(Some(member.clone()));
        if let Some(name) = &bindings.key {
            bind_range(
                &mut state,
                name,
                Value::influenced(value.inputs()),
                bindings.assignment,
            );
        }
        if let Some(name) = &bindings.value {
            bind_range(&mut state, name, member, bindings.assignment);
        }
        let id = state.loop_id(bindings.body.start as u64);
        if let Some(id) = id {
            state.loops.push(id);
        }
        let mut body_exits = self.window(
            program,
            Window {
                start: bindings.body.start,
                end: bindings.body.end,
            },
            normal(state, Value::nil()),
        );
        for exit in &mut body_exits {
            if id.is_some() {
                exit.state.loops.pop();
            }
            exit.state.leave_scope();
            match &exit.flow {
                Flow::Break(target) | Flow::Continue(target) if Some(*target) == id => {}
                Flow::Normal(_) => {}
                Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => continue,
            }
            // Entry widening covers every earlier iteration, so this can be the last one.
            exit.flow = Flow::Normal(Normal {
                value: Value::unknown(),
            });
            exit.state
                .output
                .emit(exit.state.heap.freeze(&Value::influenced(value.inputs())));
        }
        exits.extend(body_exits);
        exits
    }
    fn range_else(
        &self,
        program: &Program<'_>,
        boundaries: &[TemplateAction],
        state: State,
    ) -> Vec<Exit> {
        if let Some((branch, end)) = boundaries
            .split_first()
            .and_then(|(branch, rest)| rest.last().map(|end| (branch, end)))
        {
            self.window(
                program,
                Window {
                    start: branch.span.end,
                    end: end.span.start,
                },
                normal(state, Value::nil()),
            )
        } else {
            normal(state, Value::nil())
        }
    }
}
fn literal_span(program: &Program<'_>, next: usize, span: Span) -> Span {
    let previous = next
        .checked_sub(1)
        .and_then(|index| program.syntax.actions.get(index));
    let following = program.syntax.actions.get(next);
    trim_text_span(
        program.source,
        span,
        TextTrim {
            start: previous
                .is_some_and(|action| action.span.end == span.start && action.trim_right),
            end: following.is_some_and(|action| action.span.start == span.end && action.trim_left),
        },
    )
    .unwrap_or_else(|| Span::new(span.start, span.start))
}

fn range_domain(state: &State, value: &Value, two_variables: bool) -> RangeInput {
    use super::kinds::GoType;

    let value = state.consumed(value);
    match state.heap.freeze(&value).go_type() {
        GoType::Nil => return RangeInput::Exact(Vec::new()),
        GoType::Bool | GoType::String | GoType::Float64 => return RangeInput::Invalid,
        GoType::Int | GoType::Int64 | GoType::Uint64 => {
            return integer_entries(&value, two_variables);
        }
        GoType::Map | GoType::Slice | GoType::Unknown => {}
    }
    match entries(state, &value) {
        Some(entries) if entries.len() <= 32 => RangeInput::Exact(entries),
        Some(entries) => {
            tracing::debug!(reason = ?super::BoundReason::LoopMembers, "widen range");
            let member = summarized_member(state, &entries, value.inputs());
            RangeInput::Summary(RangeSummary {
                nonempty: true,
                member,
            })
        }
        None => RangeInput::Summary(RangeSummary {
            nonempty: false,
            member: Value::influenced(value.inputs()),
        }),
    }
}

// A final iteration may use any entry; only their common scalar type survives the summary.
fn summarized_member(
    state: &State,
    entries: &[RangeEntry],
    inputs: BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    let mut common = None;
    for entry in entries {
        let value = state.consumed(&entry.value);
        let ValueNode::Leaf(Leaf {
            datum:
                Datum::Scalar {
                    go_type,
                    content: _,
                },
            origin: _,
        }) = value.node.as_ref()
        else {
            return Value::influenced(inputs);
        };
        if common.is_some_and(|common| common != *go_type) {
            return Value::influenced(inputs);
        }
        common = Some(*go_type);
    }
    match common {
        Some(kind) => Value::with_influences(Value::scalar(kind, None), inputs),
        None => Value::influenced(inputs),
    }
}

fn integer_entries(value: &Value, two_variables: bool) -> RangeInput {
    if two_variables {
        return RangeInput::Invalid;
    }
    let ValueNode::Leaf(Leaf {
        datum: Datum::Scalar {
            go_type,
            content: _,
        },
        origin: _,
    }) = value.node.as_ref()
    else {
        return RangeInput::Summary(RangeSummary {
            nonempty: false,
            member: Value::influenced(value.inputs()),
        });
    };
    let Some(GuardValue::Int(count)) = value.constant() else {
        return RangeInput::Summary(RangeSummary {
            nonempty: false,
            member: Value::with_influences(Value::scalar(*go_type, None), value.inputs()),
        });
    };
    if *count > 32 {
        tracing::debug!(reason = ?super::BoundReason::LoopMembers, "widen range");
        return RangeInput::Summary(RangeSummary {
            nonempty: true,
            member: Value::with_influences(Value::scalar(*go_type, None), value.inputs()),
        });
    }
    RangeInput::Exact(
        (0..*count)
            .map(|index| {
                let value = Value::scalar(
                    *go_type,
                    Some(super::scalar_value::ScalarContent::Literal(
                        GuardValue::Int(index),
                    )),
                );
                RangeEntry {
                    key: value.clone(),
                    value,
                }
            })
            .collect(),
    )
}

fn entries(state: &State, value: &Value) -> Option<Vec<RangeEntry>> {
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(slice),
            origin: _,
        }) => Some(
            (0..slice.length)
                .map(|index| RangeEntry {
                    key: range_index(index),
                    value: Value::leaf(Datum::Slot {
                        backing: slice.backing.clone(),
                        index: slice.start + index,
                    }),
                })
                .collect(),
        ),
        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(sequence),
            origin: _,
        }) => Some(
            sequence
                .items()?
                .iter()
                .enumerate()
                .map(|(index, value)| RangeEntry {
                    key: range_index(index),
                    value: value.clone(),
                })
                .collect(),
        ),
        ValueNode::Leaf(Leaf {
            datum: Datum::Map {
                target: None,
                view: _,
            },
            origin: _,
        }) => Some(Vec::new()),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Map {
                    target: Some(id),
                    view: _,
                },
            origin: _,
        }) => {
            let ObjectData::Map {
                fields,
                remainder: Remainder::Closed,
            } = &state.heap.version.object(id)?.data
            else {
                return None;
            };
            fields
                .iter()
                .filter_map(|(key, field)| match state.consumed_field(field) {
                    super::Field::Absent => None,
                    super::Field::Present(value) => Some(Some(RangeEntry {
                        key: Value::string(key),
                        value: state.consumed(&value),
                    })),
                    super::Field::Unknown
                    | super::Field::Select {
                        decision: _,
                        yes: _,
                        no: _,
                    } => Some(None),
                })
                .collect()
        }
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Text(_)
                | Datum::Scalar {
                    go_type: _,
                    content: _,
                }
                | Datum::Slot {
                    backing: _,
                    index: _,
                }
                | Datum::Unknown(_),
            origin: _,
        })
        | ValueNode::Select {
            decision: _,
            yes: _,
            no: _,
        } => None,
    }
}
fn widen_loop_writes(
    program: &Program<'_>,
    bindings: &RangeBindings,
    source: &Value,
    state: &mut State,
) {
    use crate::function_semantics::{HeapEffect, function_semantics};

    let actions = program
        .syntax
        .expressions
        .starting_in(Span::new(bindings.body.start, bindings.body.end));
    let rebound = loop_rebindings(program, bindings, actions);
    let mut targets = Vec::new();
    let mut unknown = false;
    let mut malformed = false;
    // This scans both arms and unreachable suffixes without evaluating an expression again.
    for action in actions {
        if matches!(
            action.action.kind,
            ActionKind::RegionOpen {
                region: _,
                kind: ControlKind::Block,
                region_end: _
            }
        ) {
            unknown = true;
        }
        for expression in action.expressions.iter() {
            expression.walk(|expression| match expression {
                TemplateExpr::Call { function, args } => {
                    if let Some(operands) = loop_mutation_operands(function, args, state, &rebound)
                    {
                        targets.extend(operands);
                    } else {
                        unknown = true;
                    }
                }
                TemplateExpr::Unknown(_) => malformed = true,
                TemplateExpr::Pipeline(stages) => {
                    unknown |= stages.iter().skip(1).any(|stage| {
                        matches!(stage.deparen(), TemplateExpr::Call { function, args: _ }
                            if function_semantics(function).heap_effect != HeapEffect::ReadOnly)
                    });
                }
                TemplateExpr::Literal(_)
                | TemplateExpr::Field(_)
                | TemplateExpr::Selector {
                    operand: _,
                    path: _,
                }
                | TemplateExpr::Variable(_)
                | TemplateExpr::Parenthesized(_)
                | TemplateExpr::VariableDefinition { name: _, value: _ }
                | TemplateExpr::Assignment { name: _, value: _ } => {}
            });
        }
    }
    if malformed || unknown {
        super::operations::invalidate(state);
    } else {
        super::operations::widen_operands(&targets, state);
    }
    for name in rebound {
        if let Some(previous) = state.read_local(&name) {
            let mut inputs = source.inputs();
            inputs.extend(previous.inputs());
            state.assign(&name, Value::influenced(inputs));
        }
    }
}

fn loop_rebindings(
    program: &Program<'_>,
    bindings: &RangeBindings,
    actions: &[helm_schema_ast::ParsedAction],
) -> BTreeSet<String> {
    let mut rebound: BTreeSet<String> = [bindings.key.as_deref(), bindings.value.as_deref()]
        .into_iter()
        .flatten()
        .map(|name| local_name(name).to_owned())
        .collect();
    for action in actions {
        if matches!(
            action.action.kind,
            ActionKind::RegionOpen {
                region: _,
                kind: ControlKind::Range,
                region_end: _
            }
        ) {
            let node = node_at(
                program.syntax.tree.root_node(),
                action.action.span.start,
                "range_action",
            );
            let header = node
                .and_then(|node| helm_schema_ast::range_header_from_source(node, program.source));
            let nested = range_bindings(
                node,
                header.as_ref().map(helm_schema_ast::TemplateHeader::expr),
                program.source,
                bindings.body,
            );
            rebound.extend(
                [nested.key, nested.value]
                    .into_iter()
                    .flatten()
                    .map(|name| local_name(&name).to_owned()),
            );
        }
        for expression in action.expressions.iter() {
            expression.walk(|expression| {
                if let TemplateExpr::Assignment { name, value: _ }
                | TemplateExpr::VariableDefinition { name, value: _ } = expression
                {
                    rebound.insert(local_name(name).to_owned());
                }
            });
        }
    }
    rebound
}

fn loop_mutation_operands(
    function: &str,
    args: &[TemplateExpr],
    state: &State,
    rebound: &BTreeSet<String>,
) -> Option<Vec<Value>> {
    use crate::function_semantics::{HeapEffect, function_semantics};
    match function_semantics(function).heap_effect {
        HeapEffect::ReadOnly => return Some(Vec::new()),
        HeapEffect::Unknown => return None,
        HeapEffect::Operands => {}
    }
    let operands = if matches!(function, "set" | "unset") {
        args.get(..1)?
    } else {
        args
    };
    if operands.is_empty() {
        return None;
    }
    operands
        .iter()
        .map(|operand| match operand.deparen() {
            TemplateExpr::Literal(_) => Some(Value::nil()),
            TemplateExpr::Variable(name) => {
                let name = if name.is_empty() { "$" } else { name };
                (!rebound.contains(name))
                    .then(|| state.read_local(name).cloned())
                    .flatten()
            }
            TemplateExpr::Field(_)
            | TemplateExpr::Selector {
                operand: _,
                path: _,
            }
            | TemplateExpr::Call {
                function: _,
                args: _,
            }
            | TemplateExpr::Pipeline(_)
            | TemplateExpr::Parenthesized(_)
            | TemplateExpr::VariableDefinition { name: _, value: _ }
            | TemplateExpr::Assignment { name: _, value: _ }
            | TemplateExpr::Unknown(_) => None,
        })
        .collect()
}

fn bind_range(state: &mut State, name: &str, value: Value, assignment: bool) {
    let name = local_name(name);
    if assignment {
        state.assign(name, value);
    } else {
        state.bind(name.to_owned(), value);
    }
}
fn node_at<'a>(
    node: tree_sitter::Node<'a>,
    start: usize,
    kind: &str,
) -> Option<tree_sitter::Node<'a>> {
    if node.start_byte() == start && node.kind() == kind {
        return Some(node);
    }
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .filter(|node| node.start_byte() <= start && node.end_byte() > start)
        .find_map(|node| node_at(node, start, kind))
}

pub(super) fn bound_exits(exits: Vec<Exit>) -> Vec<Exit> {
    if exits.len() <= super::MAX_EXECUTION_ALTERNATIVES {
        return exits;
    }
    tracing::debug!(
        reason = ?super::BoundReason::ExecutionAlternatives,
        alternatives = exits.len(),
        "widen execution"
    );
    summarize_exits(exits)
}

pub(super) fn summarize_exits(exits: Vec<Exit>) -> Vec<Exit> {
    let mut groups: Vec<Vec<Exit>> = Vec::new();
    for exit in exits {
        if let Some(group) = groups.iter_mut().find(|group| {
            group
                .first()
                .is_some_and(|first| same_flow(&first.flow, &exit.flow))
        }) {
            group.push(exit);
        } else {
            groups.push(vec![exit]);
        }
    }
    groups
        .into_iter()
        .filter_map(|mut group| {
            let mut inputs = std::collections::BTreeSet::new();
            let mut snapshots = std::collections::BTreeSet::new();
            let mut reads = super::read_set::ReadSet::default();
            for exit in &group {
                for read in &exit.state.reads {
                    if read.coverage == super::ReadCoverage::MemberHost {
                        continue;
                    }
                    inputs.insert(read.path.clone());
                    reads.insert(super::RuntimeRead {
                        path: read.path.clone(),
                        coverage: read.coverage,
                        condition: Predicate::True,
                    });
                }
                if snapshots.insert(exit.state.heap.version.id.ordinal) {
                    inputs.extend(exit.state.heap.freeze(&Value::unknown()).reachable_inputs());
                }
                for frame in &exit.state.frames {
                    for value in frame.bindings.values().chain(frame.dot.iter()) {
                        inputs.extend(value.inputs());
                    }
                }
                if let Flow::Normal(normal) = &exit.flow {
                    inputs.extend(normal.value.inputs());
                }
            }
            let mut representative = group.pop()?;
            if group
                .iter()
                .any(|exit| exit.state.namespace.id() != representative.state.namespace.id())
            {
                representative.state.namespace =
                    super::namespace::Namespace::unknown(&representative.state.heap.arena);
            }
            representative.state.reads = reads;
            representative.state.invocation_records_complete = false;
            super::operations::invalidate(&mut representative.state);
            representative.state.reads = representative
                .state
                .reads
                .iter()
                .filter(|read| read.coverage != super::ReadCoverage::MemberHost)
                .map(|read| super::RuntimeRead {
                    path: read.path.clone(),
                    coverage: read.coverage,
                    condition: Predicate::True,
                })
                .collect();
            for frame in &mut representative.state.frames {
                for value in frame.bindings.values_mut() {
                    *value = Value::influenced(inputs.clone());
                }
                if let Some(dot) = &mut frame.dot {
                    *dot = Value::influenced(inputs.clone());
                }
            }
            let test = representative.state.heap.arena.decision(
                Predicate::False,
                Predicate::False,
                Coverage::Partial,
            );
            representative.state.active = vec![super::DecisionEdge { test, taken: true }];
            representative.state.sites.clear();
            representative.state.text_sites.clear();
            representative.state.observation_count = super::output::BranchCounts::default();
            representative.state.output = super::TextBuffer::default();
            representative.state.output.emit(
                representative
                    .state
                    .heap
                    .freeze(&Value::influenced(inputs.clone())),
            );
            if let Flow::Normal(normal) = &mut representative.flow {
                normal.value = Value::influenced(inputs);
            }
            Some(representative)
        })
        .collect()
}
fn same_flow(left: &Flow, right: &Flow) -> bool {
    match (left, right) {
        (Flow::Normal(_), Flow::Normal(_)) | (Flow::Abort(_), Flow::Abort(_)) => true,
        (Flow::Break(left), Flow::Break(right)) | (Flow::Continue(left), Flow::Continue(right)) => {
            left == right
        }
        (Flow::Normal(_), Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_))
        | (Flow::Abort(_), Flow::Normal(_) | Flow::Break(_) | Flow::Continue(_))
        | (Flow::Break(_), Flow::Normal(_) | Flow::Abort(_) | Flow::Continue(_))
        | (Flow::Continue(_), Flow::Normal(_) | Flow::Abort(_) | Flow::Break(_)) => false,
    }
}

impl super::ExitBuffer {
    pub(super) fn push(&mut self, exit: Exit) {
        self.exits.push(exit);
        if self.summarized {
            self.exits = summarize_exits(std::mem::take(&mut self.exits));
        } else if self.exits.len() > 4096 {
            self.summarized = true;
            self.exits = bound_exits(std::mem::take(&mut self.exits));
        }
    }
    pub(super) fn extend(&mut self, exits: impl IntoIterator<Item = Exit>) {
        for exit in exits {
            self.push(exit);
        }
    }
    pub(super) fn finish(self) -> Vec<Exit> {
        reconverge(self.exits)
    }
}
impl super::StateBuffer {
    pub(super) fn push(&mut self, state: State) {
        self.exits.push(Exit {
            state,
            flow: Flow::Normal(Normal {
                value: Value::unknown(),
            }),
        });
    }
    pub(super) fn finish(self) -> Vec<State> {
        self.exits
            .finish()
            .into_iter()
            .map(|exit| exit.state)
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FrozenEdgeKey {
    parent: u64,
    decision: u64,
    taken: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ContinuationKey {
    arena: *const super::Arena,
    context: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PathKey {
    continuation: ContinuationKey,
    prefix: u64,
    snapshot: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ContinuationContext {
    program: u64,
    frames: Vec<FrameShape>,
    calls: Vec<super::CallKey>,
    namespace: super::namespace::NamespaceId,
    loops: Vec<LoopShape>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct FrameShape {
    call: Option<u64>,
    names: Vec<String>,
    has_dot: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LoopShape {
    call: u64,
    ordinal: u64,
}
#[derive(Debug, Default)]
pub(super) struct FrontierIndex {
    prefixes: std::collections::BTreeMap<FrozenEdgeKey, u64>,
    contexts: std::collections::BTreeMap<ContinuationContext, u64>,
    declined: std::collections::BTreeSet<DeclinedJoin>,
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DeclinedJoin {
    left: PathKey,
    right: PathKey,
}
impl FrontierIndex {
    fn prefix(&mut self, parent: u64, edge: &super::DecisionEdge) -> u64 {
        let key = FrozenEdgeKey {
            parent,
            decision: edge.test.id,
            taken: edge.taken,
        };
        let next = self.prefixes.len() as u64 + 1;
        *self.prefixes.entry(key).or_insert(next)
    }

    fn path(&mut self, state: &State, active: &[super::DecisionEdge]) -> PathKey {
        let shape = ContinuationContext {
            program: state.program,
            frames: state
                .frames
                .iter()
                .map(|frame| {
                    let super::Frame {
                        bindings,
                        dot,
                        boundary,
                    } = frame;
                    FrameShape {
                        call: match boundary {
                            super::FrameBoundary::Call(call) => Some(*call),
                            super::FrameBoundary::Lexical => None,
                        },
                        names: bindings.keys().cloned().collect(),
                        has_dot: dot.is_some(),
                    }
                })
                .collect(),
            calls: state.calls.clone(),
            namespace: state.namespace.id(),
            loops: state
                .loops
                .iter()
                .map(|super::LoopId { call, ordinal }| LoopShape {
                    call: *call,
                    ordinal: *ordinal,
                })
                .collect(),
        };
        let next = self.contexts.len() as u64;
        let context = *self.contexts.entry(shape).or_insert(next);
        let mut prefix = 0;
        for edge in active {
            prefix = self.prefix(prefix, edge);
        }
        PathKey {
            continuation: ContinuationKey {
                arena: Rc::as_ptr(&state.heap.arena),
                context,
            },
            prefix,
            snapshot: state.heap.version.id.ordinal,
        }
    }
}
pub(super) trait Reconvergent: Sized {
    type Context;
    type Joined;
    fn state<'a>(&'a self, context: &'a Self::Context) -> &'a State;
    fn active<'a>(&'a self, context: &'a Self::Context) -> &'a [super::DecisionEdge] {
        &self.state(context).active
    }
    fn prepare_join(
        &self,
        other: &Self,
        plan: &super::state::ContinuationJoin,
        context: &Self::Context,
    ) -> Option<Self::Joined>;
    fn is_normal(&self) -> bool;
    fn compatible(&self, other: &Self) -> bool;
    fn joined(
        self,
        other: Self,
        state: Self::Joined,
        decision: Rc<super::FrozenTest>,
        taken: bool,
        context: &Self::Context,
    ) -> Self;
}

pub(super) struct IndexedContinuation<T> {
    pub(super) index: usize,
    pub(super) continuation: T,
}

struct FrontierSlot<T> {
    continuation: T,
    path: Option<PathKey>,
}

struct PreparedContinuation<T> {
    index: usize,
    plan: super::state::ContinuationJoin,
    state: T,
}

pub(super) fn reconverge(exits: Vec<Exit>) -> Vec<Exit> {
    reconverge_continuations(exits, &())
        .into_iter()
        .map(|entry| entry.continuation)
        .collect()
}

#[tracing::instrument(skip_all)]
pub(super) fn reconverge_continuations<T: Reconvergent>(
    exits: Vec<T>,
    context: &T::Context,
) -> Vec<IndexedContinuation<T>> {
    if exits.len() < 2 {
        return exits
            .into_iter()
            .enumerate()
            .map(|(index, continuation)| IndexedContinuation {
                index,
                continuation,
            })
            .collect();
    }
    let mut slots: Vec<Option<FrontierSlot<T>>> = Vec::new();
    let mut pending = std::collections::BTreeMap::<ContinuationKey, Vec<usize>>::new();
    for mut exit in exits {
        loop {
            let arena = &exit.state(context).heap.arena;
            let path = exit.is_normal().then(|| {
                arena
                    .frontier
                    .borrow_mut()
                    .path(exit.state(context), exit.active(context))
            });
            let mut counterpart = None;
            if let Some(path) = path
                && let Some(candidates) = pending.get_mut(&path.continuation)
            {
                candidates.retain(|index| slots.get(*index).is_some_and(Option::is_some));
                for &index in candidates.iter().rev() {
                    let Some(FrontierSlot {
                        continuation: other,
                        path: Some(other_path),
                    }) = slots.get(index).and_then(Option::as_ref)
                    else {
                        continue;
                    };
                    if !exit.compatible(other) {
                        continue;
                    }
                    let declined = DeclinedJoin {
                        left: path,
                        right: *other_path,
                    };
                    if arena.frontier.borrow().declined.contains(&declined) {
                        continue;
                    }
                    let plan = arena.join_paths(exit.active(context), other.active(context));
                    if let Some(state) = exit.prepare_join(other, &plan, context) {
                        counterpart = Some(PreparedContinuation { index, plan, state });
                        break;
                    }
                    arena.frontier.borrow_mut().declined.insert(declined);
                }
            }
            if let Some(PreparedContinuation { index, plan, state }) = counterpart
                && let Some(FrontierSlot {
                    continuation: other,
                    path: _,
                }) = slots.get_mut(index).and_then(Option::take)
            {
                exit = exit.joined(other, state, plan.decision, plan.taken, context);
            } else {
                let index = slots.len();
                if let Some(path) = path {
                    pending.entry(path.continuation).or_default().push(index);
                }
                slots.push(Some(FrontierSlot {
                    continuation: exit,
                    path,
                }));
                break;
            }
        }
    }
    slots
        .into_iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            slot.map(
                |FrontierSlot {
                     continuation,
                     path: _,
                 }| {
                    IndexedContinuation {
                        index,
                        continuation,
                    }
                },
            )
        })
        .collect()
}

impl Reconvergent for Exit {
    type Context = ();
    type Joined = State;
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
    fn is_normal(&self) -> bool {
        matches!(self.flow, Flow::Normal(_))
    }
    fn compatible(&self, other: &Self) -> bool {
        self.is_normal() && other.is_normal()
    }
    fn joined(
        self,
        other: Self,
        mut state: State,
        decision: Rc<super::FrozenTest>,
        taken: bool,
        _context: &(),
    ) -> Self {
        let (Flow::Normal(value), Flow::Normal(other_value)) = (&self.flow, &other.flow) else {
            return self;
        };
        let mut widened = std::collections::BTreeSet::new();
        let (yes, no) = if taken {
            (&self, &other)
        } else {
            (&other, &self)
        };
        let (yes_value, no_value) = if taken {
            (&value.value, &other_value.value)
        } else {
            (&other_value.value, &value.value)
        };
        let value = super::state::join_frame_value(
            decision,
            yes_value,
            no_value,
            &yes.state.heap,
            &no.state.heap,
            &mut widened,
        );
        for path in widened {
            state.reads.insert(super::RuntimeRead {
                path,
                condition: state.path_predicate(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
        Exit {
            state,
            flow: Flow::Normal(Normal { value }),
        }
    }
}

fn range_index(index: usize) -> Value {
    i64::try_from(index).map_or_else(
        |_| Value::unknown(),
        |index| Value::literal(GuardValue::Int(index)),
    )
}

struct RangeBindings {
    key: Option<String>,
    value: Option<String>,
    assignment: bool,
    body: Window,
}

fn range_bindings(
    node: Option<tree_sitter::Node<'_>>,
    expression: Option<&TemplateExpr>,
    source: &str,
    body: Window,
) -> RangeBindings {
    let key_name =
        node.and_then(|node| helm_schema_ast::range_destructured_key_variable(node, source));
    let value_name = node
        .and_then(|node| helm_schema_ast::range_destructured_value_variable(node, source))
        .or_else(|| expression.and_then(helm_schema_ast::range_variable_name_expr));
    let assignment = node.is_some_and(helm_schema_ast::range_uses_assignment);
    RangeBindings {
        key: key_name,
        value: value_name,
        assignment,
        body,
    }
}
