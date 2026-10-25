use std::collections::BTreeMap;
use std::rc::Rc;

use helm_schema_core::Predicate;

use super::{
    CallOutcome, Completeness, Coverage, DecisionEdge, Exit, Flow, Frame, FrameBoundary,
    FrozenTest, Heap, LoopId, State, Value,
};

impl State {
    pub(super) fn new(argument: Value) -> Self {
        let heap = Heap::new();
        let frame = Frame::call(argument, heap.arena.call());
        Self {
            heap,
            frames: vec![frame],
            active: Vec::new(),
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

    pub(super) fn fork(&self, test: Rc<FrozenTest>, taken: bool) -> Self {
        let mut active = self.active.clone();
        active.push(DecisionEdge { test, taken });
        Self {
            heap: self.heap.fork(),
            frames: self.frames.clone(),
            active,
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

impl CallOutcome {
    /// Partial predicates cannot turn an unknown execution into a universal rejection.
    pub(super) fn definitely_aborts(&self) -> bool {
        if self.completeness != Completeness::Complete || self.exits.is_empty() {
            return false;
        }
        let mut paths = Vec::new();
        let all_abort = self.exits.iter().all(|Exit { state, flow }| {
            paths.push(Predicate::all(
                state
                    .active
                    .iter()
                    .map(|DecisionEdge { test, taken }| {
                        if *taken {
                            test.when_true.clone()
                        } else {
                            test.when_false.clone()
                        }
                    })
                    .collect(),
            ));
            state
                .active
                .iter()
                .all(|DecisionEdge { test, taken: _ }| test.coverage == Coverage::Exact)
                && matches!(flow, Flow::Abort(_))
        });
        all_abort && Predicate::Or(paths).normalize_boolean() == Predicate::True
    }
}

impl super::Arena {
    fn call(&self) -> u64 {
        let call = self.next_call.get();
        self.next_call.set(call + 1);
        call
    }
}
