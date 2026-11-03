use std::collections::BTreeMap;
use std::rc::Rc;

use super::{DecisionEdge, Frame, FrameBoundary, FrozenTest, Heap, LoopId, State, Value};

pub(super) struct ContinuationJoin {
    pub(super) decision: Rc<FrozenTest>,
    pub(super) active: Vec<DecisionEdge>,
    pub(super) taken: bool,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct FrameLayout {
    parent: Option<u64>,
    boundary: FrameBoundary,
    names: Vec<String>,
    has_dot: bool,
}

impl super::Arena {
    fn frame_layout(&self, layout: FrameLayout) -> u64 {
        let mut layouts = self.frames.borrow_mut();
        let next = layouts.len() as u64;
        *layouts.entry(layout).or_insert(next)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ContinuationKey {
    arena: *const super::Arena,
    program: u64,
    frame: Option<u64>,
    calls: Vec<super::CallKey>,
    namespace: super::namespace::NamespaceId,
    loops: Vec<LoopId>,
}

impl State {
    pub(super) fn with_heap(argument: Value, heap: Heap) -> Self {
        let frame = Frame::call(argument, heap.arena.call(), None, &heap.arena);
        let namespace = super::namespace::Namespace::unknown(&heap.arena);
        Self {
            heap,
            frames: vec![frame],
            active: Rc::default(),
            output: super::TextBuffer::default(),
            output_widened: false,
            observation_count: super::output::BranchCounts::default(),
            invocation_count: super::output::BranchCounts::default(),
            sites: super::history::History::default(),
            text_sites: super::history::History::default(),
            reads: super::read_set::ReadSet::default(),
            consumptions: super::history::History::default(),
            calls: Vec::new(),
            program: 0,
            namespace,
            invocations: super::history::History::default(),
            invocation_records_complete: true,
            loops: Vec::new(),
        }
    }

    pub(super) fn mark_invocation_incomplete(&mut self) {
        self.invocation_records_complete = false;
    }

    pub(super) fn join_local_values(
        &mut self,
        decision: Rc<FrozenTest>,
        yes: &Value,
        no: &Value,
    ) -> Value {
        let mut widened = std::collections::BTreeSet::new();
        let value = join_frame_value(decision, yes, no, &self.heap, &self.heap, &mut widened);
        for path in widened {
            self.reads.insert(super::RuntimeRead {
                path,
                condition: self.path_predicate(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
        value
    }

    pub(super) fn continuation_key(&self) -> ContinuationKey {
        ContinuationKey {
            arena: Rc::as_ptr(&self.heap.arena),
            program: self.program,
            frame: self.frames.last().map(|frame| frame.continuation),
            calls: self.calls.clone(),
            namespace: self.namespace.id(),
            loops: self.loops.clone(),
        }
    }

    pub(super) fn bind(&mut self, name: String, value: Value) {
        let parent = self
            .frames
            .iter()
            .rev()
            .nth(1)
            .map(|frame| frame.continuation);
        if let Some(frame) = self.frames.last_mut() {
            let frame = Rc::make_mut(frame);
            if frame.bindings.insert(name, value).is_none() {
                frame.continuation = self.heap.arena.frame_layout(FrameLayout {
                    parent,
                    boundary: frame.boundary,
                    names: frame.bindings.keys().cloned().collect(),
                    has_dot: frame.dot.is_some(),
                });
            }
        }
    }

    pub(super) fn assign(&mut self, name: &str, value: Value) -> bool {
        for frame in self.frames.iter_mut().rev() {
            if frame.bindings.contains_key(name)
                && let Some(binding) = Rc::make_mut(frame).bindings.get_mut(name)
            {
                *binding = value;
                return true;
            }
            if matches!(frame.boundary, FrameBoundary::Call(_)) {
                break;
            }
        }
        false
    }

    pub(super) fn read_local(&self, name: &str) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(value) = frame.bindings.get(name) {
                return Some(value);
            }
            if matches!(frame.boundary, FrameBoundary::Call(_)) {
                break;
            }
        }
        None
    }

    pub(super) fn read_root(&self) -> Option<&Value> {
        self.read_local("$")
    }

    pub(super) fn dot(&self) -> Option<&Value> {
        for frame in self.frames.iter().rev() {
            if let Some(dot) = &frame.dot {
                return Some(dot);
            }
            if matches!(frame.boundary, FrameBoundary::Call(_)) {
                break;
            }
        }
        None
    }

    pub(super) fn enter_scope(&mut self, dot: Option<Value>) {
        let continuation = self.heap.arena.frame_layout(FrameLayout {
            parent: self.frames.last().map(|frame| frame.continuation),
            boundary: FrameBoundary::Lexical,
            names: Vec::new(),
            has_dot: dot.is_some(),
        });
        self.frames.push(Rc::new(Frame {
            continuation,
            bindings: BTreeMap::new(),
            dot,
            boundary: FrameBoundary::Lexical,
        }));
    }

    pub(super) fn leave_scope(&mut self) -> bool {
        if self
            .frames
            .last()
            .is_some_and(|frame| frame.boundary == FrameBoundary::Lexical)
        {
            self.frames.pop();
            true
        } else {
            false
        }
    }

    pub(super) fn enter_call(&mut self, argument: &Value) {
        let argument = self.evaluated_member_entry(argument);
        self.frames.push(Frame::call(
            argument,
            self.heap.arena.call(),
            self.frames.last().map(|frame| frame.continuation),
            &self.heap.arena,
        ));
    }

    pub(super) fn enter_file_call(
        &mut self,
        argument: &Value,
        occurrence: &super::session::FileOccurrence,
    ) {
        let argument = self.evaluated_member_entry(argument);
        self.frames.push(Frame::call(
            argument,
            occurrence.call,
            self.frames.last().map(|frame| frame.continuation),
            &self.heap.arena,
        ));
    }

    pub(super) fn leave_call(&mut self) -> bool {
        let Some(boundary) = self
            .frames
            .iter()
            .rposition(|frame| matches!(frame.boundary, FrameBoundary::Call(_)))
        else {
            return false;
        };
        if boundary == 0 {
            return false;
        }
        self.frames.truncate(boundary);
        true
    }

    pub(super) fn loop_id(&self, ordinal: u64) -> Option<LoopId> {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame.boundary {
                FrameBoundary::Call(call) => Some(LoopId { call, ordinal }),
                FrameBoundary::Lexical => None,
            })
    }

    #[tracing::instrument(skip_all)]
    pub(super) fn fork(&self, test: Rc<FrozenTest>, taken: bool) -> Self {
        let mut active = self.active.clone();
        let forced = test.constant_outcome() == Some(taken);
        if !forced {
            Rc::make_mut(&mut active).push(DecisionEdge { test, taken });
        }
        let mut output = self.output.clone();
        let runtime = self.heap.arena.runtime_path(&active);
        output.restrict_budget(&runtime, &self.heap.arena);
        let output_widened = self.output_widened
            && (output.is_widened_under(&runtime, &self.heap.arena)
                || self
                    .observation_count
                    .overflow_under(&runtime, &self.heap.arena));
        Self {
            heap: self.heap.fork(),
            frames: self.frames.clone(),
            active,
            output,
            output_widened,
            observation_count: self.observation_count.clone(),
            invocation_count: self.invocation_count.clone(),
            sites: self.sites.clone(),
            text_sites: self.text_sites.clone(),
            reads: self.reads.clone(),
            consumptions: self.consumptions.clone(),
            calls: self.calls.clone(),
            program: self.program,
            namespace: Rc::clone(&self.namespace),
            invocations: self.invocations.clone(),
            invocation_records_complete: self.invocation_records_complete,
            loops: self.loops.clone(),
        }
    }
}

impl Frame {
    pub(super) fn call(
        argument: Value,
        call: u64,
        parent: Option<u64>,
        arena: &super::Arena,
    ) -> Rc<Self> {
        let boundary = FrameBoundary::Call(call);
        let continuation = arena.frame_layout(FrameLayout {
            parent,
            boundary,
            names: vec!["$".to_owned()],
            has_dot: true,
        });
        Rc::new(Self {
            continuation,
            bindings: BTreeMap::from([("$".to_owned(), argument.clone())]),
            dot: Some(argument),
            boundary,
        })
    }
}

impl super::Arena {
    pub(super) fn program(&self) -> u64 {
        let program = self.next_program.get();
        self.next_program.set(program + 1);
        program
    }

    pub(super) fn call(&self) -> u64 {
        let call = self.next_call.get();
        self.next_call.set(call + 1);
        call
    }
}

impl State {
    #[tracing::instrument(skip_all)]
    pub(super) fn join_branches(
        plan: &ContinuationJoin,
        left: &Self,
        right: &Self,
    ) -> Option<Self> {
        let (yes, no) = if plan.taken {
            (left, right)
        } else {
            (right, left)
        };
        Self::join_guarded(&plan.decision, plan.active.clone(), yes, no)
    }

    fn join_guarded(
        decision: &Rc<FrozenTest>,
        active: Vec<DecisionEdge>,
        yes: &Self,
        no: &Self,
    ) -> Option<Self> {
        let State {
            heap: yes_heap,
            frames: yes_frames,
            active: _,
            reads: yes_reads,
            consumptions: yes_consumptions,
            calls: yes_calls,
            program: yes_program,
            namespace: yes_namespace,
            loops: yes_loops,
            output: _,
            output_widened: _,
            observation_count: _,
            invocation_count: _,
            sites: _,
            text_sites: _,
            invocations: _,
            invocation_records_complete: _,
        } = yes;
        let State {
            heap: no_heap,
            frames: no_frames,
            active: _,
            reads: no_reads,
            consumptions: no_consumptions,
            calls: no_calls,
            program: no_program,
            namespace: no_namespace,
            loops: no_loops,
            output: _,
            output_widened: _,
            observation_count: _,
            invocation_count: _,
            sites: _,
            text_sites: _,
            invocations: _,
            invocation_records_complete: _,
        } = no;
        if !Rc::ptr_eq(&yes_heap.arena, &no_heap.arena)
            || yes_program != no_program
            || yes_namespace.id() != no_namespace.id()
            || yes_calls != no_calls
            || yes_loops != no_loops
            || yes_frames.len() != no_frames.len()
        {
            return None;
        }

        if yes_frames.last().map(|frame| frame.continuation)
            != no_frames.last().map(|frame| frame.continuation)
        {
            return None;
        }
        let mut heap = yes_heap.clone();
        if let Err(err) = heap.join(decision, yes_heap, no_heap) {
            tracing::trace!(?err, "decline incompatible branch heap join");
            return None;
        }
        let mut widened_inputs = std::collections::BTreeSet::new();
        let frames = join_frames(
            decision,
            yes_frames,
            no_frames,
            yes_heap,
            no_heap,
            &mut widened_inputs,
        )?;
        let mut joined = State {
            heap,
            frames,
            active: active.into(),
            output: super::TextBuffer::default(),
            output_widened: false,
            observation_count: super::output::BranchCounts::default(),
            invocation_count: super::output::BranchCounts::default(),
            sites: super::history::History::default(),
            text_sites: super::history::History::default(),
            reads: yes_reads.union(no_reads),
            consumptions: yes_consumptions.union_by(no_consumptions, |left, right| left == right),
            calls: yes_calls.clone(),
            program: *yes_program,
            namespace: Rc::clone(yes_namespace),
            invocations: super::history::History::default(),
            invocation_records_complete: true,
            loops: yes_loops.clone(),
        };
        joined.observe_join_coverage(widened_inputs);
        joined.merge_observations(decision, yes, no);
        Some(joined)
    }
    fn observe_join_coverage(
        &mut self,
        widened_inputs: std::collections::BTreeSet<helm_schema_core::ValuesPath>,
    ) {
        let condition = self.path_predicate();
        for path in widened_inputs {
            self.reads.insert(super::RuntimeRead {
                path,
                condition: condition.clone(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
        self.reads
            .observe_heap(Rc::clone(&self.heap.version), condition);
    }
}

pub(super) fn value_join_fits(yes: &Value, no: &Value) -> bool {
    value_join_fits_with_budget(
        yes,
        no,
        &mut super::heap_query::QueryBudget::new(usize::MAX),
    )
}

pub(super) fn value_join_fits_with_budget(
    yes: &Value,
    no: &Value,
    budget: &mut super::heap_query::QueryBudget,
) -> bool {
    if Rc::ptr_eq(&yes.node, &no.node) {
        return true;
    }
    let mut pending = vec![yes, no];
    let mut leaves = 0;
    while let Some(value) = pending.pop() {
        if !budget.charge(4) {
            return false;
        }
        match value.node.as_ref() {
            super::ValueNode::Leaf(super::Leaf {
                datum: _,
                origin: _,
            }) => {
                leaves += 1;
                if leaves > 128 {
                    return false;
                }
            }
            super::ValueNode::Select {
                decision: _,
                yes,
                no,
            } => {
                pending.push(yes);
                pending.push(no);
            }
        }
    }
    true
}
pub(super) fn join_frame_value(
    decision: Rc<FrozenTest>,
    yes: &Value,
    no: &Value,
    yes_heap: &Heap,
    no_heap: &Heap,
    widened: &mut std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> Value {
    if Rc::ptr_eq(&yes.node, &no.node) {
        return yes.clone();
    }
    if value_join_fits(yes, no) {
        return Value::select(decision, yes.clone(), no.clone());
    }
    let mut inputs = yes_heap.freeze(yes).inputs();
    inputs.extend(no_heap.freeze(no).inputs());
    widened.extend(inputs.iter().cloned());
    tracing::debug!(reason = "value_fanout", "widen joined binding");
    Value::influenced(inputs)
}

fn join_frames(
    decision: &Rc<FrozenTest>,
    yes_frames: &[Rc<Frame>],
    no_frames: &[Rc<Frame>],
    yes_heap: &super::Heap,
    no_heap: &super::Heap,
    widened_inputs: &mut std::collections::BTreeSet<helm_schema_core::ValuesPath>,
) -> Option<Vec<Rc<Frame>>> {
    yes_frames
        .iter()
        .zip(no_frames)
        .map(|(yes, no)| {
            if Rc::ptr_eq(yes, no) {
                return Some(Rc::clone(yes));
            }
            let Frame {
                continuation,
                bindings: yes_bindings,
                dot: yes_dot,
                boundary,
            } = yes.as_ref();
            let Frame {
                continuation: _,
                bindings: no_bindings,
                dot: no_dot,
                boundary: _,
            } = no.as_ref();
            let bindings = yes_bindings
                .iter()
                .filter_map(|(name, yes)| {
                    no_bindings.get(name).map(|no| {
                        (
                            name.clone(),
                            join_frame_value(
                                Rc::clone(decision),
                                yes,
                                no,
                                yes_heap,
                                no_heap,
                                widened_inputs,
                            ),
                        )
                    })
                })
                .collect();
            let dot = match (yes_dot, no_dot) {
                (Some(yes), Some(no)) => Some(join_frame_value(
                    Rc::clone(decision),
                    yes,
                    no,
                    yes_heap,
                    no_heap,
                    widened_inputs,
                )),
                (None, None) => None,
                (Some(_), None) | (None, Some(_)) => return None,
            };
            Some(Rc::new(Frame {
                continuation: *continuation,
                bindings,
                dot,
                boundary: *boundary,
            }))
        })
        .collect::<Option<Vec<_>>>()
}
