use super::eval::{Executor, local_name, normal};
use super::{
    Coverage, Datum, Exit, Flow, Leaf, Normal, ObjectData, Remainder, State, Value, ValueNode,
};
use helm_schema_ast::{ParsedActions, TemplateExpr};
use helm_schema_core::{GuardValue, Predicate};
use helm_schema_syntax::{ActionKind, ControlKind, Span, TemplateAction, TemplatedDocument};
use std::rc::Rc;

struct Program<'a> {
    source: &'a str,
    path: Option<&'a str>,
    actions: Vec<TemplateAction>,
    expressions: ParsedActions,
    tree: tree_sitter::Tree,
}
struct Window {
    start: usize,
    end: usize,
}
struct RangeEntry {
    key: Value,
    value: Value,
}
impl Executor<'_> {
    pub(super) fn body(&self, source: &str, path: Option<&str>, mut state: State) -> Vec<Exit> {
        state.program = state.heap.arena.next_program.get();
        state.heap.arena.next_program.set(state.program + 1);
        let Some(tree) = helm_schema_syntax::parse_go_template(source) else {
            super::operations::invalidate(&mut state);
            state.output.emit(state.heap.freeze(&Value::unknown()));
            return normal(state, Value::unknown());
        };
        if tree.root_node().has_error() {
            super::operations::invalidate(&mut state);
            state.output.emit(state.heap.freeze(&Value::unknown()));
            return normal(state, Value::unknown());
        }
        let document = TemplatedDocument::parse_with_root(source, tree.root_node());
        let actions = document.actions().to_vec();
        let expressions = ParsedActions::lower(tree.root_node(), source, &actions);
        let program = Program {
            source,
            path,
            actions,
            expressions,
            tree,
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
    fn window(&self, program: &Program<'_>, window: Window, mut exits: Vec<Exit>) -> Vec<Exit> {
        let mut cursor = window.start;
        let mut index = program
            .actions
            .partition_point(|action| action.span.start < window.start);
        while let Some(action) = program
            .actions
            .get(index)
            .copied()
            .filter(|action| action.span.start < window.end)
        {
            let bytes = program
                .source
                .get(cursor..action.span.start)
                .unwrap_or_default();
            let bytes = if action.trim_left {
                bytes.trim_end_matches(char::is_whitespace)
            } else {
                bytes
            };
            for exit in &mut exits {
                if matches!(exit.flow, Flow::Normal(_)) {
                    exit.state.emit_bytes(
                        Span {
                            start: cursor,
                            end: action.span.start,
                        },
                        program.path,
                        bytes,
                    );
                }
            }
            cursor = action.span.end;
            let mut next = super::ExitBuffer::default();
            for mut exit in exits {
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
                        let rendered = matches!(action.kind, ActionKind::Output { .. });
                        let expressions = program
                            .expressions
                            .at(action.span)
                            .map(|parsed| parsed.expressions.as_ref())
                            .unwrap_or_default();
                        let mut action_exits = normal(exit.state, Value::nil());
                        for expression in expressions {
                            let mut evaluated = super::ExitBuffer::default();
                            for exit in action_exits {
                                match exit.flow {
                                    Flow::Normal(_) => evaluated.extend(self.expression(
                                        expression,
                                        exit.state,
                                        action.span.start,
                                    )),
                                    flow => evaluated.push(Exit {
                                        state: exit.state,
                                        flow,
                                    }),
                                }
                            }
                            action_exits = evaluated.finish();
                        }
                        for mut exit in action_exits {
                            if let Flow::Normal(value) = &exit.flow {
                                exit.state.observe(
                                    action.span,
                                    program.path,
                                    &value.value,
                                    rendered,
                                );
                            }
                            next.push(exit);
                        }
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
            exits = reconverge(next.finish());
            if let ActionKind::RegionOpen {
                region_end,
                region: _,
                kind: _,
            } = action.kind
            {
                cursor = region_end;
                index = program
                    .actions
                    .partition_point(|action| action.span.start < region_end);
            } else {
                index += 1;
            }
            if action.trim_right {
                let suffix = program.source.get(cursor..window.end).unwrap_or_default();
                cursor += suffix.len() - suffix.trim_start_matches(char::is_whitespace).len();
                index = program
                    .actions
                    .partition_point(|action| action.span.start < cursor);
            }
        }
        for exit in &mut exits {
            if matches!(exit.flow, Flow::Normal(_)) {
                exit.state.emit_bytes(
                    Span {
                        start: cursor,
                        end: window.end,
                    },
                    program.path,
                    program.source.get(cursor..window.end).unwrap_or_default(),
                );
            }
        }
        exits
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
        let boundaries: Vec<_> = program.actions.iter().copied().filter(|action| action.span.start > opener.span.start && action.span.start < end && matches!(action.kind, ActionKind::RegionBranch { region: candidate } | ActionKind::RegionEnd { region: candidate } if candidate == region)).collect();
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
                        if state.path_predicate().normalize_boolean() == Predicate::False {
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
    fn range(
        &self,
        program: &Program<'_>,
        opener: TemplateAction,
        boundaries: &[TemplateAction],
        mut state: State,
    ) -> Vec<Exit> {
        state.enter_scope(None);
        let node = node_at(program.tree.root_node(), opener.span.start, "range_action");
        let header =
            node.and_then(|node| helm_schema_ast::range_header_from_source(node, program.source));
        let expression = header.as_ref().map(|header| header.expr());
        let key_name = node.and_then(|node| {
            helm_schema_ast::range_destructured_key_variable(node, program.source)
        });
        let value_name = node
            .and_then(|node| {
                helm_schema_ast::range_destructured_value_variable(node, program.source)
            })
            .or_else(|| expression.and_then(helm_schema_ast::range_variable_name_expr));
        let assignment = node.is_some_and(helm_schema_ast::range_uses_assignment);
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
        let mut results = super::ExitBuffer::default();
        for mut exit in evaluated {
            let Flow::Normal(value) = exit.flow else {
                results.push(exit);
                continue;
            };
            exit.state
                .observe(opener.span, program.path, &value.value, false);
            for name in [key_name.as_deref(), value_name.as_deref()]
                .into_iter()
                .flatten()
            {
                bind_range(&mut exit.state, name, value.value.clone(), assignment);
            }
            let entries = entries(&exit.state, &value.value);
            if entries.as_ref().is_some_and(|entries| entries.len() > 32) {
                tracing::debug!(reason = ?super::BoundReason::LoopMembers, "widen range");
            }
            let Some(entries) = entries.filter(|entries| entries.len() <= 32) else {
                let test = exit.state.heap.arena.decision(
                    Predicate::False,
                    Predicate::False,
                    Coverage::Partial,
                );
                let zero = exit.state.fork(Rc::clone(&test), false);
                results.extend(self.range_else(program, boundaries, zero));
                let mut state = exit.state.fork(test, true);
                super::operations::invalidate(&mut state);
                state.enter_scope(Some(Value::influenced(value.value.inputs())));
                for name in [key_name.as_deref(), value_name.as_deref()]
                    .into_iter()
                    .flatten()
                {
                    bind_range(
                        &mut state,
                        name,
                        Value::influenced(value.value.inputs()),
                        assignment,
                    );
                }
                if let Some(id) = state.loop_id(opener.span.start as u64) {
                    state.loops.push(id);
                }
                let mut body_exits = self.window(
                    program,
                    Window {
                        start: body.start,
                        end: body.end,
                    },
                    normal(state, Value::nil()),
                );
                for exit in &mut body_exits {
                    exit.state.loops.pop();
                    exit.state.leave_scope();
                    super::operations::invalidate(&mut exit.state);
                    exit.flow = Flow::Normal(Normal {
                        value: Value::unknown(),
                    });
                    exit.state.output.emit(
                        exit.state
                            .heap
                            .freeze(&Value::influenced(value.value.inputs())),
                    );
                }
                results.extend(body_exits);
                continue;
            };
            if entries.is_empty() {
                results.extend(self.range_else(program, boundaries, exit.state));
                continue;
            }
            let Some(id) = exit.state.loop_id(opener.span.start as u64) else {
                results.extend(normal(exit.state, Value::unknown()));
                continue;
            };
            let mut pending = normal(exit.state, Value::nil());
            let mut breaks = super::ExitBuffer::default();
            for entry in entries {
                let mut next = super::ExitBuffer::default();
                for mut exit in pending {
                    if !matches!(exit.flow, Flow::Normal(_)) {
                        next.push(exit);
                        continue;
                    }
                    if let Some(name) = &key_name {
                        bind_range(&mut exit.state, name, entry.key.clone(), assignment);
                    }
                    if let Some(name) = &value_name {
                        bind_range(&mut exit.state, name, entry.value.clone(), assignment);
                    }
                    exit.state.enter_scope(Some(entry.value.clone()));
                    exit.state.loops.push(id);
                    let body_exits = self.window(
                        program,
                        Window {
                            start: body.start,
                            end: body.end,
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
                            Flow::Normal(_)
                            | Flow::Abort(_)
                            | Flow::Break(_)
                            | Flow::Continue(_) => next.push(exit),
                        }
                    }
                }
                pending = next.finish();
            }
            results.extend(pending);
            results.extend(breaks.finish());
        }
        let mut results = results.finish();
        for exit in &mut results {
            exit.state.leave_scope();
        }
        results
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
fn entries(state: &State, value: &Value) -> Option<Vec<RangeEntry>> {
    let value = state.consumed(value);
    match value.node.as_ref() {
        ValueNode::Leaf(Leaf {
            datum: Datum::Strings(id),
            origin: _,
        }) => {
            let ObjectData::Strings(slots) = &state.heap.version.object(id)?.data else {
                return None;
            };
            Some(
                slots
                    .iter()
                    .enumerate()
                    .map(|(index, _)| RangeEntry {
                        key: Value::literal(GuardValue::Int(index as i64)),
                        value: Value::leaf(Datum::Slot {
                            backing: id.clone(),
                            index,
                        }),
                    })
                    .collect(),
            )
        }

        ValueNode::Leaf(Leaf {
            datum: Datum::Sequence(Some(items)),
            origin: _,
        }) => Some(
            items
                .iter()
                .enumerate()
                .map(|(index, value)| RangeEntry {
                    key: Value::literal(GuardValue::Int(index as i64)),
                    value: value.clone(),
                })
                .collect(),
        ),
        ValueNode::Leaf(Leaf {
            datum:
                Datum::Sequence(None)
                | Datum::Map {
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
                .filter_map(|(key, field)| match field {
                    super::Field::Absent => None,
                    super::Field::Present(value) => Some(Some(RangeEntry {
                        key: Value::string(key),
                        value: state.consumed(value),
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
                Datum::Scalar {
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
    const MAX_EXECUTION_ALTERNATIVES: usize = 4096;
    if exits.len() <= MAX_EXECUTION_ALTERNATIVES {
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
            let mut reads = std::collections::BTreeSet::new();
            for exit in &group {
                for read in &exit.state.reads {
                    inputs.insert(read.path.clone());
                    reads.insert(super::RuntimeRead {
                        path: read.path.clone(),
                        coverage: read.coverage,
                        condition: Predicate::True,
                    });
                }
                if snapshots.insert(exit.state.heap.version.id.ordinal) {
                    inputs.extend(exit.state.heap.freeze(&Value::unknown()).inputs());
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
            for exit in group {
                representative.state.namespace_unknown |= exit.state.namespace_unknown;
            }
            representative.state.reads = reads;
            representative.state.invocation_records_complete = false;
            super::operations::invalidate(&mut representative.state);
            representative.state.reads = representative
                .state
                .reads
                .into_iter()
                .map(|read| super::RuntimeRead {
                    path: read.path,
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
        self.exits
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
    decision: u64,
    taken: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ReconvergenceKey {
    decision: u64,
    prefix: Vec<FrozenEdgeKey>,
    program: u64,
    frames: Vec<FrameShape>,
    calls: Vec<String>,
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
struct BranchKey {
    key: ReconvergenceKey,
    taken: bool,
    decision: Rc<super::FrozenTest>,
}
#[derive(Default)]
struct BranchSlots {
    yes: Vec<usize>,
    no: Vec<usize>,
}
fn branch_key(exit: &Exit) -> Option<BranchKey> {
    if !matches!(exit.flow, Flow::Normal(_)) {
        return None;
    }
    let (last, prefix) = exit.state.active.split_last()?;
    Some(BranchKey {
        key: ReconvergenceKey {
            decision: last.test.id,
            prefix: prefix
                .iter()
                .map(|edge| FrozenEdgeKey {
                    decision: edge.test.id,
                    taken: edge.taken,
                })
                .collect(),
            program: exit.state.program,
            frames: exit
                .state
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
            calls: exit.state.calls.clone(),
            loops: exit
                .state
                .loops
                .iter()
                .map(|super::LoopId { call, ordinal }| LoopShape {
                    call: *call,
                    ordinal: *ordinal,
                })
                .collect(),
        },
        taken: last.taken,
        decision: Rc::clone(&last.test),
    })
}
pub(super) fn reconverge(exits: Vec<Exit>) -> Vec<Exit> {
    if exits.len() < 2 {
        return exits;
    }
    let mut slots: Vec<Option<Exit>> = Vec::new();
    let mut pending = std::collections::BTreeMap::<ReconvergenceKey, BranchSlots>::new();
    for mut exit in exits {
        loop {
            let Some(branch) = branch_key(&exit) else {
                slots.push(Some(exit));
                break;
            };
            let counterpart = pending.get_mut(&branch.key).and_then(|slots| {
                if branch.taken {
                    slots.no.pop()
                } else {
                    slots.yes.pop()
                }
            });
            let Some(index) = counterpart else {
                let index = slots.len();
                slots.push(Some(exit));
                let entry = pending.entry(branch.key).or_default();
                if branch.taken {
                    entry.yes.push(index);
                } else {
                    entry.no.push(index);
                }
                break;
            };
            let Some(other) = slots.get_mut(index).and_then(Option::take) else {
                continue;
            };
            let joined = if branch.taken {
                State::join_branches(Rc::clone(&branch.decision), &exit.state, &other.state)
            } else {
                State::join_branches(Rc::clone(&branch.decision), &other.state, &exit.state)
            };
            let Some(mut state) = joined else {
                if let Some(slot) = slots.get_mut(index) {
                    *slot = Some(other);
                }
                let entry = pending.entry(branch.key.clone()).or_default();
                if branch.taken {
                    entry.no.push(index);
                } else {
                    entry.yes.push(index);
                }
                let index = slots.len();
                slots.push(Some(exit));
                let entry = pending.entry(branch.key).or_default();
                if branch.taken {
                    entry.yes.push(index);
                } else {
                    entry.no.push(index);
                }
                break;
            };
            let Flow::Normal(value) = &exit.flow else {
                slots.push(Some(exit));
                break;
            };
            let Flow::Normal(other_value) = &other.flow else {
                slots.push(Some(exit));
                slots.push(Some(other));
                break;
            };
            let mut widened = std::collections::BTreeSet::new();
            let value = if branch.taken {
                super::state::join_frame_value(
                    branch.decision,
                    &value.value,
                    &other_value.value,
                    &exit.state.heap,
                    &other.state.heap,
                    &mut widened,
                )
            } else {
                super::state::join_frame_value(
                    branch.decision,
                    &other_value.value,
                    &value.value,
                    &other.state.heap,
                    &exit.state.heap,
                    &mut widened,
                )
            };
            for path in widened {
                state.reads.insert(super::RuntimeRead {
                    path,
                    condition: state.path_predicate(),
                    coverage: super::ReadCoverage::UnknownRegion,
                });
            }
            exit = Exit {
                state,
                flow: Flow::Normal(Normal { value }),
            };
        }
    }
    slots.into_iter().flatten().collect()
}
