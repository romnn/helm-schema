use std::collections::BTreeMap;
use std::rc::Rc;

use super::{DecisionEdge, Frame, FrameBoundary, FrozenTest, Heap, LoopId, State, Value};

impl State {
    pub(super) fn new(argument: Value) -> Self {
        let heap = Heap::new();
        let frame = Frame::call(argument, heap.arena.call());
        Self {
            heap,
            frames: vec![frame],
            active: Vec::new(),
            output: super::TextBuffer::default(),
            output_widened: false,
            sites: Vec::new(),
            text_sites: Vec::new(),
            reads: Default::default(),
            calls: Vec::new(),
            program: 0,
            namespace_unknown: false,
            invocations: Vec::new(),
            invocation_records_complete: true,
            loops: Vec::new(),
        }
    }

    pub(super) fn bind(&mut self, name: String, value: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.bindings.insert(name, value);
        }
    }

    pub(super) fn assign(&mut self, name: &str, value: Value) -> bool {
        for frame in self.frames.iter_mut().rev() {
            if let Some(binding) = frame.bindings.get_mut(name) {
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
        self.frames.push(Frame {
            bindings: BTreeMap::new(),
            dot,
            boundary: FrameBoundary::Lexical,
        });
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

    pub(super) fn enter_call(&mut self, argument: Value) {
        self.frames
            .push(Frame::call(argument, self.heap.arena.call()));
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
        let forced = test.coverage == super::Coverage::Exact
            && if taken {
                test.when_true == helm_schema_core::Predicate::True
                    && test.when_false == helm_schema_core::Predicate::False
            } else {
                test.when_false == helm_schema_core::Predicate::True
                    && test.when_true == helm_schema_core::Predicate::False
            };
        if !forced {
            active.push(DecisionEdge { test, taken });
        }
        Self {
            heap: self.heap.fork(),
            frames: self.frames.clone(),
            active,
            output: self.output.clone(),
            output_widened: self.output_widened,
            sites: self.sites.clone(),
            text_sites: self.text_sites.clone(),
            reads: self.reads.clone(),
            calls: self.calls.clone(),
            program: self.program,
            namespace_unknown: self.namespace_unknown,
            invocations: self.invocations.clone(),
            invocation_records_complete: self.invocation_records_complete,
            loops: self.loops.clone(),
        }
    }
}

impl Frame {
    pub(super) fn call(argument: Value, call: u64) -> Self {
        Self {
            bindings: BTreeMap::from([("$".to_owned(), argument.clone())]),
            dot: Some(argument),
            boundary: FrameBoundary::Call(call),
        }
    }
}

impl super::Arena {
    fn call(&self) -> u64 {
        let call = self.next_call.get();
        self.next_call.set(call + 1);
        call
    }
}

impl State {
    #[tracing::instrument(skip_all)]
    pub(super) fn join_branches(decision: Rc<FrozenTest>, yes: &Self, no: &Self) -> Option<Self> {
        let State {
            heap: yes_heap,
            frames: yes_frames,
            active: yes_active,
            reads: yes_reads,
            calls: yes_calls,
            program: yes_program,
            namespace_unknown: yes_namespace,
            loops: yes_loops,
            output: _,
            output_widened: _,
            sites: _,
            text_sites: _,
            invocations: _,
            invocation_records_complete: _,
        } = yes;
        let State {
            heap: no_heap,
            frames: no_frames,
            active: no_active,
            reads: no_reads,
            calls: no_calls,
            program: no_program,
            namespace_unknown: no_namespace,
            loops: no_loops,
            output: _,
            output_widened: _,
            sites: _,
            text_sites: _,
            invocations: _,
            invocation_records_complete: _,
        } = no;
        if !Rc::ptr_eq(&yes_heap.arena, &no_heap.arena)
            || yes_program != no_program
            || yes_calls != no_calls
            || yes_loops != no_loops
            || yes_frames.len() != no_frames.len()
            || yes_active.len() != no_active.len()
        {
            return None;
        }
        let (yes_edge, yes_prefix) = yes_active.split_last()?;
        let (no_edge, no_prefix) = no_active.split_last()?;
        if yes_edge.test != decision
            || no_edge.test != decision
            || !yes_edge.taken
            || no_edge.taken
            || !yes_prefix
                .iter()
                .zip(no_prefix)
                .all(|(yes, no)| yes.test == no.test && yes.taken == no.taken)
        {
            return None;
        }
        for (yes, no) in yes_frames.iter().zip(no_frames) {
            let Frame {
                bindings: yes_bindings,
                dot: yes_dot,
                boundary: yes_boundary,
            } = yes;
            let Frame {
                bindings: no_bindings,
                dot: no_dot,
                boundary: no_boundary,
            } = no;
            if yes_boundary != no_boundary
                || !yes_bindings.keys().eq(no_bindings.keys())
                || yes_dot.is_some() != no_dot.is_some()
            {
                return None;
            }
        }
        let mut heap = yes_heap.clone();
        if let Err(err) = heap.join(Rc::clone(&decision), yes_heap, no_heap) {
            tracing::trace!(?err, "decline incompatible branch heap join");
            return None;
        }
        let mut widened_inputs = std::collections::BTreeSet::new();
        let frames = yes_frames
            .iter()
            .zip(no_frames)
            .map(|(yes, no)| {
                let Frame {
                    bindings: yes_bindings,
                    dot: yes_dot,
                    boundary,
                } = yes;
                let Frame {
                    bindings: no_bindings,
                    dot: no_dot,
                    boundary: _,
                } = no;
                let bindings = yes_bindings
                    .iter()
                    .filter_map(|(name, yes)| {
                        no_bindings.get(name).map(|no| {
                            (
                                name.clone(),
                                join_frame_value(
                                    Rc::clone(&decision),
                                    yes,
                                    no,
                                    yes_heap,
                                    no_heap,
                                    &mut widened_inputs,
                                ),
                            )
                        })
                    })
                    .collect();
                let dot = match (yes_dot, no_dot) {
                    (Some(yes), Some(no)) => Some(join_frame_value(
                        Rc::clone(&decision),
                        yes,
                        no,
                        yes_heap,
                        no_heap,
                        &mut widened_inputs,
                    )),
                    (None, None) => None,
                    (Some(_), None) | (None, Some(_)) => return None,
                };
                Some(Frame {
                    bindings,
                    dot,
                    boundary: *boundary,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let mut joined = State {
            heap,
            frames,
            active: yes_prefix.to_vec(),
            output: Default::default(),
            output_widened: false,
            sites: Vec::new(),
            text_sites: Vec::new(),
            reads: yes_reads.union(no_reads).cloned().collect(),
            calls: yes_calls.clone(),
            program: *yes_program,
            namespace_unknown: *yes_namespace || *no_namespace,
            invocations: Vec::new(),
            invocation_records_complete: true,
            loops: yes_loops.clone(),
        };
        let condition = joined.path_predicate();
        for path in widened_inputs {
            joined.reads.insert(super::RuntimeRead {
                path,
                condition: condition.clone(),
                coverage: super::ReadCoverage::UnknownRegion,
            });
        }
        for object in joined
            .heap
            .version
            .pages
            .values()
            .flat_map(|page| page.values())
        {
            if let super::ObjectData::Widened { inputs } = &object.data {
                for path in inputs {
                    joined.reads.insert(super::RuntimeRead {
                        path: path.clone(),
                        condition: condition.clone(),
                        coverage: super::ReadCoverage::UnknownRegion,
                    });
                }
            }
        }
        for (id, path) in joined.heap.arena.input_paths.borrow().iter() {
            if matches!(
                joined.heap.version.object(id).map(|object| &object.data),
                Some(
                    super::ObjectData::Widened { inputs: _ }
                        | super::ObjectData::Map {
                            fields: _,
                            remainder: super::Remainder::Open
                        }
                )
            ) {
                joined.reads.insert(super::RuntimeRead {
                    path: path.clone(),
                    condition: condition.clone(),
                    coverage: super::ReadCoverage::UnknownRegion,
                });
            }
        }
        joined.merge_observations(decision, yes, no);
        Some(joined)
    }
}

pub(super) fn value_join_fits(yes: &Value, no: &Value) -> bool {
    if Rc::ptr_eq(&yes.node, &no.node) {
        return true;
    }
    let mut pending = vec![yes, no];
    let mut leaves = 0;
    while let Some(value) = pending.pop() {
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
