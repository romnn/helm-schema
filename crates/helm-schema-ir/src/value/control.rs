use super::eval::{Executor, local_name, normal};
use super::write_footprint::widen_targets;
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
                for mut exit in evaluated {
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
        let mut results = results.exits;
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
        let RangeSummary {
            nonempty,
            mut member,
        } = summary;
        let uniform_path = uniform_member_path(program, bindings, state, value);
        if let Some(path) = &uniform_path {
            let mut path = path.clone();
            path.push_each_member();
            member = Value::input(path);
        }
        let local_targets =
            super::write_footprint::local_targets(program.source, program.syntax, &state.namespace);
        let mut exits = Vec::new();
        let mut state = if nonempty {
            state.clone()
        } else {
            let test = if let Some(path) = &uniform_path {
                let predicate =
                    Predicate::from(helm_schema_core::Guard::Range { path: path.clone() });
                state
                    .heap
                    .arena
                    .decision(predicate.clone(), predicate.negated(), Coverage::Exact)
            } else {
                state
                    .heap
                    .arena
                    .decision(Predicate::False, Predicate::False, Coverage::Partial)
            };
            exits.extend(self.range_else(program, boundaries, state.fork(Rc::clone(&test), false)));
            state.fork(test, true)
        };
        let id = state.loop_id(bindings.body.start as u64);
        let iteration = |mut state: State| {
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
                bind_range(&mut state, name, member.clone(), bindings.assignment);
            }
            if let Some(id) = id {
                state.loops.push(id);
            }
            state
        };
        // The first possible iteration can read named inputs before loop-carried
        // writes erase their provenance.
        // A writer requires separate summary execution; only widening reads
        // from this initial execution survive that replacement.
        let prior_sites = state.sites.clone();
        let first_exits = self.window(
            program,
            bindings.body,
            normal(iteration(state.clone()), Value::nil()),
        );
        let first_reads = first_exits
            .iter()
            .map(|exit| first_iteration_reads(&exit.state, &prior_sites))
            .fold(super::read_set::ReadSet::default(), |reads, next| {
                reads.union(&next)
            });
        let mut body_exits = if local_targets.as_ref().is_some_and(BTreeSet::is_empty) {
            // With no loop-carried writes, the initial execution already used
            // the summary's exact start state.
            first_exits
        } else {
            widen_targets(&mut state, local_targets.as_ref(), &value.inputs());
            self.window(
                program,
                bindings.body,
                normal(iteration(state), Value::nil()),
            )
        };
        for exit in &mut body_exits {
            exit.state.reads = exit.state.reads.union(&first_reads);
            if id.is_some() {
                exit.state.loops.pop();
            }
            exit.state.leave_scope();
            match &exit.flow {
                Flow::Break(target) | Flow::Continue(target) if Some(*target) == id => {}
                Flow::Normal(_) => {}
                Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => continue,
            }
            widen_targets(&mut exit.state, local_targets.as_ref(), &value.inputs());
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

fn first_iteration_reads(
    state: &State,
    prior_sites: &super::history::History<super::ExecutedSite>,
) -> super::read_set::ReadSet {
    let mut reads = state
        .reads
        .without_coverage(super::ReadCoverage::MemberHost { allows_nil: false })
        .without_coverage(super::ReadCoverage::MemberHost { allows_nil: true })
        .without_coverage(super::ReadCoverage::MapArgument { allows_nil: false })
        .without_coverage(super::ReadCoverage::MapArgument { allows_nil: true });
    for site in state
        .sites
        .since(prior_sites)
        .filter(|site| site.rendered && site.value.value.identity().is_none())
    {
        let condition = site.condition();
        for path in site.value.inputs() {
            if path.segments().next().is_some() {
                reads.insert(super::RuntimeRead {
                    coverage: super::ReadCoverage::SerializedInfluence,
                    path,
                    condition: condition.clone(),
                });
            }
        }
    }
    reads
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

fn uniform_member_path(
    program: &Program<'_>,
    bindings: &RangeBindings,
    state: &State,
    value: &Value,
) -> Option<helm_schema_core::ValuesPath> {
    let path = value.identity()?;
    if bindings.assignment
        || bindings.key.is_none()
            && !matches!(
                state.heap.freeze(&state.consumed(value)).go_type(),
                super::kinds::GoType::Map | super::kinds::GoType::Slice
            )
        || path.segments().any(|segment| segment.literal().is_none())
        || state
            .heap
            .arena
            .input_objects
            .borrow()
            .get(path)
            .is_some_and(|backing| state.heap.version.object(&backing.object).is_some())
        || !uniform_member_body(program, bindings.body)
    {
        return None;
    }
    Some(path.clone())
}

// This certificate excludes loop-carried writes and control that can skip later members.
fn uniform_member_body(program: &Program<'_>, body: Window) -> bool {
    for action in program
        .syntax
        .actions
        .iter()
        .filter(|action| action.span.start >= body.start && action.span.start < body.end)
    {
        if !matches!(
            action.kind,
            ActionKind::Assign | ActionKind::Output { .. } | ActionKind::TemplateComment
        ) {
            return false;
        }
        let Some(parsed) = program.syntax.expressions.at(action.span) else {
            return false;
        };
        let mut safe = true;
        for expression in parsed.expressions.iter() {
            expression.walk(|expr| match expr {
                TemplateExpr::Assignment { name: _, value: _ } | TemplateExpr::Unknown(_) => {
                    safe = false;
                }
                TemplateExpr::Call { function, args: _ } => {
                    safe &= crate::function_semantics::function_semantics(function).heap_effect
                        == crate::function_semantics::HeapEffect::ReadOnly;
                }
                TemplateExpr::Literal(_)
                | TemplateExpr::Field(_)
                | TemplateExpr::Variable(_)
                | TemplateExpr::Selector {
                    operand: _,
                    path: _,
                }
                | TemplateExpr::Pipeline(_)
                | TemplateExpr::Parenthesized(_)
                | TemplateExpr::VariableDefinition { name: _, value: _ } => {}
            });
        }
        if !safe {
            return false;
        }
    }
    true
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
                    if read.coverage.is_requirement() {
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
                .filter(|read| !read.coverage.is_requirement())
                .map(|read| super::RuntimeRead {
                    path: read.path.clone(),
                    coverage: read.coverage,
                    condition: Predicate::True,
                })
                .collect();
            for frame in &mut representative.state.frames {
                let frame = Rc::make_mut(frame);
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
            representative.state.active = vec![super::DecisionEdge { test, taken: true }].into();
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

pub(super) trait Reconvergent: Sized {
    type Context;
    type Joined;
    type Key: Ord;
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
    fn key(&self, context: &Self::Context) -> Option<Self::Key>;
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

pub(super) fn reconverge(exits: Vec<Exit>) -> Vec<Exit> {
    reconverge_continuations(exits, &())
        .into_iter()
        .map(|entry| entry.continuation)
        .collect()
}

#[tracing::instrument(skip_all, fields(inputs = exits.len(), kind = std::any::type_name::<T>(), scans = tracing::field::Empty))]
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
    let mut scans = 0_u64;
    let mut slots: Vec<Option<T>> = Vec::new();
    let mut pending = std::collections::BTreeMap::<T::Key, Vec<usize>>::new();
    for mut exit in exits {
        if let Some(key) = exit.key(context) {
            let candidates = pending.entry(key).or_default();
            let mut cursor = candidates.len();
            while let Some(position) = cursor.checked_sub(1) {
                cursor = position;
                let Some(index) = candidates.get(position).copied() else {
                    continue;
                };
                let Some(other) = slots.get(index).and_then(Option::as_ref) else {
                    continue;
                };
                scans += 1;
                let plan = exit
                    .state(context)
                    .heap
                    .arena
                    .join_paths(exit.active(context), other.active(context));
                if let Some(state) = exit.prepare_join(other, &plan, context)
                    && let Some(other) = slots.get_mut(index).and_then(Option::take)
                {
                    exit = exit.joined(other, state, plan.decision, plan.taken, context);
                    candidates.remove(position);
                    // A successful join may reconcile a previously declined storage shape.
                    cursor = candidates.len();
                }
            }
            candidates.push(slots.len());
        }
        slots.push(Some(exit));
    }
    tracing::Span::current().record("scans", scans);
    slots
        .into_iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            slot.map(|continuation| IndexedContinuation {
                index,
                continuation,
            })
        })
        .collect()
}

impl Reconvergent for Exit {
    type Context = ();
    type Joined = State;
    type Key = super::state::ContinuationKey;
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
        matches!(self.flow, Flow::Normal(_)).then(|| self.state.continuation_key())
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
