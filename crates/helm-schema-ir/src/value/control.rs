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
    first: Option<RangeEntry>,
    member: Value,
}
/// One execution of a range body, split by where each exit continues.
struct LoopPass {
    backedge: Vec<Exit>,
    breaks: Vec<Exit>,
    other: Vec<Exit>,
}

struct RangeExits {
    completed: Vec<Exit>,
    backedge: Vec<Exit>,
    continuation: Option<Rc<super::RepeatedOutput>>,
}

impl RangeExits {
    fn into_exits(self, source: &Value) -> impl Iterator<Item = Exit> {
        let Self {
            completed,
            backedge,
            continuation,
        } = self;
        completed
            .into_iter()
            .chain(backedge.into_iter().map(move |mut exit| {
                if matches!(exit.flow, Flow::Normal(_)) {
                    // A finished range statement references no object.
                    exit.flow = Flow::Normal(Normal {
                        value: super::heap::unknown_member(&[], BTreeSet::new()),
                    });
                    if let Some(continuation) = &continuation {
                        exit.state.output.repeat(Rc::clone(continuation));
                    } else {
                        let unknown = super::heap::unknown_member(&[], source.inputs());
                        exit.state.output.emit(exit.state.heap.freeze(&unknown));
                    }
                }
                exit
            }))
    }
}

impl LoopPass {
    fn covers_repetition(&self, entry: &State, scope: super::loops::CarriedScope<'_>) -> bool {
        self.repeats(|exit| super::loops::covers(entry, &exit.state, scope))
    }

    fn repeats(&self, covers: impl Fn(&Exit) -> bool) -> bool {
        // On a successful render a member can be skipped only by leaving the loop.
        // A stable backedge gives the next member the same input and bindings.
        self.breaks.is_empty()
            && self
                .other
                .iter()
                .all(|exit| matches!(exit.flow, Flow::Abort(_)))
            && self.backedge.iter().chain(&self.other).all(covers)
    }

    fn repeats_under_action_free_members(
        &self,
        entry: &State,
        scope: super::loops::CarriedScope<'_>,
        member: &helm_schema_core::ValuesPath,
    ) -> bool {
        self.repeats(|exit| {
            let mut covered = entry.clone();
            let super::path::Divergence {
                common: _,
                left: _,
                right,
            } = entry.active.diverge(&exit.state.active);
            for link in right {
                if link.kind() != super::path::LinkKind::Assume {
                    continue;
                }
                let edge = link.edge();
                if !edge.taken
                    || !super::text_actions::is_action_free_member_input(
                        &edge.test.when_true,
                        member,
                    )
                {
                    return false;
                }
                covered
                    .active
                    .push(edge.clone(), super::path::LinkKind::Assume);
            }
            super::loops::covers(&covered, &exit.state, scope)
        })
    }

    fn always_aborts(&self, entry: &State, scope: super::loops::CarriedScope<'_>) -> bool {
        self.backedge.is_empty() && !self.other.is_empty() && self.covers_repetition(entry, scope)
    }

    fn output_continuation(
        &self,
        entry: &State,
        source: &Value,
    ) -> Option<Rc<super::RepeatedOutput>> {
        let outputs = |exits: &[Exit]| {
            exits
                .iter()
                .map(|exit| exit.state.output.suffix_after(&entry.output))
                .collect::<Option<Vec<_>>>()
        };
        Some(Rc::new(super::RepeatedOutput {
            bodies: outputs(&self.backedge)?,
            breaks: outputs(&self.breaks)?,
            inputs: source.inputs(),
        }))
    }

    fn member_demands<'a>(
        &'a self,
        prior: &'a super::history::History<super::Consumption>,
        member: &'a helm_schema_core::ValuesPath,
    ) -> impl Iterator<Item = &'a super::Consumption> {
        self.backedge
            .iter()
            .chain(&self.breaks)
            .chain(&self.other)
            .flat_map(|exit| exit.state.consumptions.since(prior))
            .filter(move |fact| {
                (&fact.path == member || fact.path.is_descendant_of(member))
                    && matches!(fact.domain, super::ConsumptionDomain::Known { .. })
            })
    }
}

fn input_member_pass(
    entry: &mut State,
    collection: helm_schema_core::ValuesPath,
    source: &Value,
    bindings: &RangeBindings,
    enter: &Rc<super::FrozenTest>,
    run: impl Fn(State, &Value) -> LoopPass,
) -> Option<LoopPass> {
    use helm_schema_core::Guard;

    let rebound = bindings.rebound();
    let scope = super::loops::CarriedScope {
        frames: entry.frames.len(),
        rebound: &rebound,
    };
    let mut member = collection.clone();
    member.push_each_member();
    if entry
        .heap
        .version
        .member_input_may_be_written(&member, super::heap_snapshot::MemberRead::Slot)
    {
        return None;
    }
    let pass = run(
        entry.fork(Rc::clone(enter), true),
        &Value::input(member.clone()),
    );
    let stable = pass.covers_repetition(entry, scope);
    if stable && bindings.key.is_some() {
        return Some(pass);
    }
    let prior = entry.consumptions.clone();
    let mut demands = pass.member_demands(&prior, &member).peekable();
    demands.peek()?;
    let mut restrictions = Vec::new();
    if !stable {
        restrictions.push(
            if pass.repeats_under_action_free_members(entry, scope, &member) {
                Guard::MatchesValue {
                    path: collection.clone(),
                    pattern: helm_schema_core::ValuePattern::TemplateActionFree,
                }
            } else {
                Guard::AtMostOneMember {
                    path: collection.clone(),
                }
            },
        );
    }
    if bindings.key.is_none()
        && entry.heap.freeze(&entry.consumed(source)).go_type() == super::kinds::GoType::Unknown
    {
        let index = Value::scalar(
            super::ScalarType::Int64,
            Some(super::scalar_value::ScalarContent::Literal(
                GuardValue::Int(0),
            )),
        );
        let first = run(entry.clone(), &index);
        // A collection demand can constrain a positive count only when its first iteration aborts.
        if !first.always_aborts(entry, scope) {
            restrictions.push(Guard::NotTypeIs {
                path: collection,
                schema_type: "integer".to_owned(),
            });
        }
    }
    for fact in demands {
        let mut fact = fact.clone();
        fact.restrict_to(&restrictions);
        entry.consumptions.push(fact);
    }
    None
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
        let entry = state.active.clone();
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
        settled(&entry, reconverge(results))
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
            exit.state.record_requirement(
                &value.value,
                &Predicate::True,
                super::query::RequirementKind::Range {
                    allow_integer: bindings.key.is_none(),
                },
            );
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
            let entries = match range_domain(&mut exit.state, &value.value, bindings.key.is_some())
            {
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
    #[tracing::instrument(skip_all, fields(exact_first = summary.first.is_some()))]
    fn unknown_range(
        &self,
        program: &Program<'_>,
        boundaries: &[TemplateAction],
        state: &State,
        value: &Value,
        bindings: &RangeBindings,
        summary: RangeSummary,
    ) -> Vec<Exit> {
        let RangeSummary { first, member } = summary;
        let id = state.loop_id(bindings.body.start as u64);
        let iteration = |mut state: State, entry_value: &Value, entry: Option<&RangeEntry>| {
            state.enter_scope(Some(entry_value.clone()));
            if let Some(name) = &bindings.key {
                // A key is a string or an index: it references nothing.
                let key = entry.map_or_else(
                    || super::heap::unknown_member(&[], value.inputs()),
                    |entry| entry.key.clone(),
                );
                bind_range(&mut state, name, key, bindings.assignment);
            }
            if let Some(name) = &bindings.value {
                bind_range(&mut state, name, entry_value.clone(), bindings.assignment);
            }
            if let Some(id) = id {
                state.loops.push(id);
            }
            state
        };
        let input = if first.is_none() && id.is_some() && !bindings.assignment {
            member_collection(state, value)
        } else {
            None
        };
        let mut exits = Vec::new();
        let mut entry_state = state.clone();
        let members = input.and_then(|collection| {
            let nonempty = Predicate::from(helm_schema_core::Guard::Range {
                path: collection.clone(),
            });
            let enter =
                state
                    .heap
                    .arena
                    .decision(nonempty.clone(), nonempty.negated(), Coverage::Exact);
            let pass = input_member_pass(
                &mut entry_state,
                collection,
                value,
                bindings,
                &enter,
                |entry, member| {
                    self.loop_pass(program, bindings.body, iteration(entry, member, None), id)
                },
            )?;
            exits.extend(self.range_else(program, boundaries, state.fork(enter, false)));
            Some(pass)
        });
        let range = if let Some(pass) = members {
            let continuation = pass.output_continuation(state, value);
            let LoopPass {
                mut backedge,
                breaks,
                mut other,
            } = pass;
            for exit in backedge.iter_mut().chain(&mut other) {
                exit.state.retain_rendered_uses_since(state);
            }
            other.extend(breaks);
            RangeExits {
                completed: other,
                backedge,
                continuation,
            }
        } else {
            if first.is_none() {
                let test = state.heap.arena.decision(
                    Predicate::False,
                    Predicate::False,
                    Coverage::Partial,
                );
                exits.extend(self.range_else(
                    program,
                    boundaries,
                    entry_state.fork(Rc::clone(&test), false),
                ));
                entry_state = entry_state.fork(test, true);
            }
            self.iterate_members(
                program,
                bindings,
                &|state, entry| {
                    iteration(state, entry.map_or(&member, |entry| &entry.value), entry)
                },
                &entry_state,
                value,
                first.as_ref(),
            )
        };
        exits.extend(range.into_exits(value));
        exits
    }

    /// Runs the members of a range from `state`: the first iteration from the exact entry
    /// state, then the Rest post-fixpoint when that iteration changed a carried component.
    /// Returns the exits that leave the loop's control (aborts, outer loops) and the states that
    /// continue after it.
    fn iterate_members(
        &self,
        program: &Program<'_>,
        bindings: &RangeBindings,
        iteration: &dyn Fn(State, Option<&RangeEntry>) -> State,
        state: &State,
        source: &Value,
        first_entry: Option<&RangeEntry>,
    ) -> RangeExits {
        // ADR 0072 §6: the first iteration runs from the entry state with its exact member
        // when known. A pass using the summary member can stand for later iterations if its
        // backedges stay inside the entry state.
        // Each iteration declares its binders afresh, so their values never carry over.
        let id = state.loop_id(bindings.body.start as u64);
        let rebound = bindings.rebound();
        let scope = super::loops::CarriedScope {
            frames: state.frames.len(),
            rebound: &rebound,
        };
        let mut first = self.loop_pass(
            program,
            bindings.body,
            iteration(state.clone(), first_entry),
            id,
        );
        for exit in first
            .backedge
            .iter_mut()
            .chain(&mut first.breaks)
            .chain(&mut first.other)
        {
            exit.state.retain_rendered_uses_since(state);
        }
        let stable = first_entry.is_none()
            && first
                .backedge
                .iter()
                .all(|exit| super::loops::covers(state, &exit.state, scope));
        let continuation = if stable && !first.backedge.is_empty() {
            first.output_continuation(state, source)
        } else {
            None
        };
        let LoopPass {
            backedge,
            breaks,
            mut other,
        } = first;
        other.extend(breaks);
        if backedge.is_empty() || stable {
            return RangeExits {
                completed: other,
                backedge,
                continuation,
            };
        }
        let mut leaving = Vec::new();
        let starts = if first_entry.is_some() {
            // A summarized known collection has more than one member.
            backedge
                .into_iter()
                .map(|exit| super::loops::Carried::new(exit.state))
                .collect()
        } else {
            let more =
                state
                    .heap
                    .arena
                    .decision(Predicate::False, Predicate::False, Coverage::Partial);
            backedge
                .into_iter()
                .map(|exit| {
                    leaving.push(Exit {
                        state: exit.state.fork(Rc::clone(&more), false),
                        flow: exit.flow,
                    });
                    super::loops::Carried::new(exit.state.fork(Rc::clone(&more), true))
                })
                .collect()
        };
        let (finished, repeating) = self.rest_fixpoint(
            program,
            bindings.body,
            &|state| iteration(state, None),
            id,
            starts,
            scope,
        );
        other.extend(finished);
        leaving.extend(repeating);
        RangeExits {
            completed: other,
            backedge: leaving,
            continuation: None,
        }
    }

    /// Runs the Rest passes from the states after the first iteration until one pass stays
    /// inside the states it started from (a post-fixpoint over every carried component).
    /// Earlier proved observations and terminal exits survive the carried-state joins.
    fn rest_fixpoint(
        &self,
        program: &Program<'_>,
        body: Window,
        iteration: &dyn Fn(State) -> State,
        id: Option<super::LoopId>,
        mut starts: Vec<super::loops::Carried>,
        scope: super::loops::CarriedScope<'_>,
    ) -> (Vec<Exit>, Vec<Exit>) {
        let mut passes = 0;
        let mut leaving = Vec::new();
        loop {
            passes += 1;
            let mut results = starts
                .iter()
                .map(|start| self.loop_pass(program, body, iteration(start.state.clone()), id))
                .collect::<Vec<_>>();
            let stable = starts.iter().zip(&results).all(|(start, pass)| {
                pass.backedge
                    .iter()
                    .all(|exit| start.covers(&exit.state, scope))
            });
            for pass in &mut results {
                leaving.append(&mut pass.breaks);
                leaving.append(&mut pass.other);
            }
            if stable {
                tracing::debug!(passes, "range fixpoint");
                return (
                    leaving,
                    results.into_iter().flat_map(|pass| pass.backedge).collect(),
                );
            }
            // Up to the pass bound a changed field of an exact map widens alone; past it every
            // uncovered object widens whole, and past twice the bound every uncovered location
            // becomes the top summary. Each top round tops a carried location not topped before,
            // so the transfer terminates within the loop's own carried locations.
            let widening = if passes < super::loops::MAX_LOOP_PASSES {
                super::loops::Widening::Fields
            } else if passes < 2 * super::loops::MAX_LOOP_PASSES {
                super::loops::Widening::Objects
            } else {
                super::loops::Widening::Top
            };
            if passes == super::loops::MAX_LOOP_PASSES {
                let _bound =
                    tracing::debug_span!(target: "helm_schema_ir::counters", "loop_fixpoint_bound")
                        .entered();
            }
            if passes == 2 * super::loops::MAX_LOOP_PASSES {
                let _bound =
                    tracing::debug_span!(target: "helm_schema_ir::counters", "loop_fixpoint_top")
                        .entered();
            }
            for (start, pass) in starts.iter_mut().zip(&results) {
                for exit in &pass.backedge {
                    start.widen(&exit.state, scope, widening);
                }
            }
        }
    }

    /// One execution of a range body: backedge (normal end or `continue`), this loop's
    /// `break`s, and every other exit (aborts, outer loops).
    fn loop_pass(
        &self,
        program: &Program<'_>,
        body: Window,
        state: State,
        id: Option<super::LoopId>,
    ) -> LoopPass {
        let mut pass = LoopPass {
            backedge: Vec::new(),
            breaks: Vec::new(),
            other: Vec::new(),
        };
        for mut exit in self.window(program, body, normal(state, Value::nil())) {
            if id.is_some() {
                exit.state.loops.pop();
            }
            exit.state.leave_scope();
            match &exit.flow {
                Flow::Break(target) if Some(*target) == id => {
                    exit.flow = Flow::Normal(Normal {
                        value: Value::nil(),
                    });
                    pass.breaks.push(exit);
                }
                Flow::Continue(target) if Some(*target) == id => {
                    exit.flow = Flow::Normal(Normal {
                        value: Value::nil(),
                    });
                    pass.backedge.push(exit);
                }
                Flow::Normal(_) => pass.backedge.push(exit),
                Flow::Abort(_) | Flow::Break(_) | Flow::Continue(_) => pass.other.push(exit),
            }
        }
        pass.backedge = reconverge(pass.backedge);
        pass
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

fn member_collection(state: &State, value: &Value) -> Option<helm_schema_core::ValuesPath> {
    let value = state.consumed(value);
    let path = value.identity()?.clone();
    if !matches!(
        state.heap.freeze(&value).go_type(),
        super::kinds::GoType::Map | super::kinds::GoType::Slice | super::kinds::GoType::Unknown
    ) || path.segments().any(|segment| segment.literal().is_none())
        || state
            .heap
            .arena
            .input_objects
            .borrow()
            .get(&path)
            .is_some_and(|backing| state.heap.version.has_allocated_object(&backing.object))
    {
        return None;
    }
    Some(path)
}

fn range_domain(state: &mut State, value: &Value, two_variables: bool) -> RangeInput {
    use super::kinds::GoType;

    let value = state.consumed_contents(value);
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
            let mut entries = entries.into_iter();
            let first = entries.next();
            let member = summarized_member(state, entries.as_slice(), value.inputs());
            RangeInput::Summary(RangeSummary { first, member })
        }
        // A member references only what the collection reaches when the range begins.
        None => RangeInput::Summary(RangeSummary {
            first: None,
            member: Value::with_influences(
                Value::leaf(Datum::Unknown(super::UnknownValue {
                    types: super::TypeCoverage::Unresolved,
                    aliases: super::operations::member_aliases(&value, state),
                })),
                value.inputs(),
            ),
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
            return super::heap::unknown_member(&entry_values(entries), inputs);
        };
        if common.is_some_and(|common| common != *go_type) {
            return super::heap::unknown_member(&entry_values(entries), inputs);
        }
        common = Some(*go_type);
    }
    match common {
        Some(kind) => Value::with_influences(Value::scalar(kind, None), inputs),
        None => super::heap::unknown_member(&entry_values(entries), inputs),
    }
}

fn entry_values(entries: &[RangeEntry]) -> Vec<Value> {
    entries.iter().map(|entry| entry.value.clone()).collect()
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
            first: None,
            member: Value::influenced(value.inputs()),
        });
    };
    let Some(GuardValue::Int(count)) = value.constant() else {
        return RangeInput::Summary(RangeSummary {
            first: None,
            member: Value::with_influences(Value::scalar(*go_type, None), value.inputs()),
        });
    };
    let entry = |index| {
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
    };
    if *count > 32 {
        tracing::debug!(reason = ?super::BoundReason::LoopMembers, "widen range");
        return RangeInput::Summary(RangeSummary {
            first: Some(entry(0)),
            member: Value::with_influences(Value::scalar(*go_type, None), value.inputs()),
        });
    }
    RangeInput::Exact((0..*count).map(entry).collect())
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
            let storage = state.heap.version.object(id)?;
            let ObjectData::Map {
                fields,
                remainder: Remainder::Closed,
            } = &storage.data
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

/// What a group of exits summarized into one depends on and may reference.
struct GroupDependencies {
    inputs: std::collections::BTreeSet<helm_schema_core::ValuesPath>,
    referenced: Vec<Value>,
    reads: super::read_set::ReadSet,
}

fn group_dependencies(group: &[Exit]) -> GroupDependencies {
    let mut inputs = std::collections::BTreeSet::new();
    let mut referenced = Vec::new();
    let mut snapshots = std::collections::BTreeSet::new();
    let mut reads = super::read_set::ReadSet::default();
    for exit in group {
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
                referenced.push(value.clone());
            }
        }
        if let Flow::Normal(normal) = &exit.flow {
            inputs.extend(normal.value.inputs());
            referenced.push(normal.value.clone());
        }
    }
    GroupDependencies {
        inputs,
        referenced,
        reads,
    }
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
            let GroupDependencies {
                inputs,
                referenced,
                reads,
            } = group_dependencies(&group);
            let mut representative = group.pop()?;
            // The summary widens only the continuation (ADR 0072 §10): every member's recorded
            // consumptions, reads, sites and text sites survive under their own conditions.
            for exit in &group {
                representative.state.retain_loop_facts(&exit.state);
            }
            let history = representative.state.reads.clone();
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
            let widened: super::read_set::ReadSet = representative
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
            representative.state.reads = widened.union(&history);
            // Each summary references only what some member's values referenced.
            let summary = super::heap::unknown_member(&referenced, inputs.clone());
            for frame in &mut representative.state.frames {
                let frame = Rc::make_mut(frame);
                for value in frame.bindings.values_mut() {
                    *value = summary.clone();
                }
                if let Some(dot) = &mut frame.dot {
                    *dot = summary.clone();
                }
            }
            let test = representative.state.heap.arena.decision(
                Predicate::False,
                Predicate::False,
                Coverage::Partial,
            );
            representative.state.active =
                super::path::Path::from_edges([super::DecisionEdge { test, taken: true }]);
            // Recorded sites keep their placements' facts only where the output stays exact.
            representative.state.output_widened = true;
            representative.state.observation_count = super::output::BranchCounts::default();
            representative.state.output.clear();
            representative.state.output.emit(
                representative
                    .state
                    .heap
                    .freeze(&Value::influenced(inputs.clone())),
            );
            if let Flow::Normal(normal) = &mut representative.flow {
                normal.value = summary;
            }
            Some(representative)
        })
        .collect()
}
fn settled(entry: &super::path::Path, mut exits: Vec<Exit>) -> Vec<Exit> {
    settle_exits(entry, &mut exits);
    exits
}

/// Re-kinds as survival the branch links a region's continuations all took (ADR 0072 §2).
///
/// An abort ends the render on its edge's whole runtime region, however partially the edge's
/// predicate describes that region, so only the flow decides which exits continue.
pub(super) fn settle_exits(entry: &super::path::Path, exits: &mut [Exit]) {
    let mut paths = exits
        .iter_mut()
        .filter(|exit| !matches!(exit.flow, Flow::Abort(_)))
        .map(|exit| &mut exit.state.active)
        .collect::<Vec<_>>();
    super::path::settle_survival(entry, &mut paths);
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
        // A buffered continuation carries no value of its own: it references no object.
        self.exits.push(Exit {
            state,
            flow: Flow::Normal(Normal {
                value: super::heap::unknown_member(&[], std::collections::BTreeSet::new()),
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
    fn active<'a>(&'a self, context: &'a Self::Context) -> &'a super::path::Path {
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
impl RangeBindings {
    fn rebound(&self) -> BTreeSet<String> {
        [&self.key, &self.value]
            .into_iter()
            .flatten()
            .filter(|_| !self.assignment)
            .map(|name| local_name(name).to_owned())
            .collect()
    }
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
