//! Private runtime foundations and laws, isolated from the active executor.

mod algebra;
mod heap;
mod laws;
mod state;

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Predicate, ValuesPath};

use crate::value::scalar_value::ScalarContent;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Value {
    node: Rc<ValueNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ValueNode {
    Leaf(Leaf),
    Select {
        decision: Rc<FrozenTest>,
        yes: Value,
        no: Value,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Leaf {
    datum: Datum,
    inputs: BTreeSet<ValuesPath>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Datum {
    Scalar {
        go_type: ScalarType,
        content: Option<ScalarContent>,
    },
    Map {
        view: MapType,
        target: Option<ObjectId>,
    },
    Sequence(Option<Rc<Vec<Value>>>),
    Slot {
        backing: ObjectId,
        index: usize,
    },
    Unknown(AliasSet),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarType {
    Nil,
    Bool,
    String,
    Int,
    Int64,
    Uint8,
    Uint64,
    Float64,
    Complex128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MapType {
    Interface,
    String,
    Values,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AliasSet {
    Known(BTreeSet<ObjectId>),
    RenderGraph,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FrozenTest {
    id: u64,
    when_true: Predicate,
    when_false: Predicate,
    coverage: Coverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coverage {
    Exact,
    Partial,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DecisionEdge {
    test: Rc<FrozenTest>,
    taken: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ObjectId {
    ordinal: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SnapshotId {
    ordinal: u64,
}

/// Forks share only the allocator; immutable versions own every semantic fact.
#[derive(Debug, Default)]
struct Arena {
    next_object: Cell<u64>,
    next_snapshot: Cell<u64>,
    next_decision: Cell<u64>,
    next_call: Cell<u64>,
}

#[derive(Clone, Debug)]
struct Heap {
    version: Rc<HeapVersion>,
    arena: Rc<Arena>,
}

#[derive(Clone, Debug)]
struct HeapVersion {
    id: SnapshotId,
    pages: BTreeMap<u64, Rc<ObjectPage>>,
}

type ObjectPage = BTreeMap<ObjectId, Rc<ObjectVersion>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObjectVersion {
    revision: SnapshotId,
    multiplicity: Multiplicity,
    data: ObjectData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Multiplicity {
    Singleton,
    Summary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ObjectData {
    Map {
        fields: BTreeMap<String, Field>,
        remainder: Remainder,
    },
    Strings(Vec<Value>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Field {
    Absent,
    Present(Value),
    Select {
        decision: Rc<FrozenTest>,
        yes: Box<Field>,
        no: Box<Field>,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Remainder {
    Closed,
    Open,
}

#[derive(Clone, Debug)]
struct FrozenValue {
    heap: Rc<HeapVersion>,
    value: Value,
}

#[derive(Clone, Debug)]
struct FrozenField {
    heap: Rc<HeapVersion>,
    field: Field,
}

#[derive(Clone, Debug)]
struct State {
    heap: Heap,
    frames: Vec<Frame>,
    active: Vec<DecisionEdge>,
}

#[derive(Clone, Debug)]
struct Frame {
    bindings: BTreeMap<String, Value>,
    dot: Option<Value>,
    boundary: FrameBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameBoundary {
    Call(u64),
    Lexical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoopId {
    call: u64,
    ordinal: u64,
}

#[derive(Debug)]
struct Exit {
    state: State,
    flow: Flow,
}

#[derive(Debug)]
enum Flow {
    Normal(Normal),
    Abort(AbortReason),
    Break(LoopId),
    Continue(LoopId),
}

#[derive(Debug)]
struct Normal {
    value: Value,
    output: Vec<FrozenValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbortReason {
    Arity,
    Type,
    Required,
    Fail,
}

#[derive(Debug)]
struct CallOutcome {
    exits: Vec<Exit>,
    completeness: Completeness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Completeness {
    Complete,
    Truncated,
}

#[derive(Debug)]
struct Invocation {
    callee: Callee,
    operands: Vec<Operand>,
    site: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum Callee {
    Function(String),
    Program(u64),
}

#[derive(Debug)]
struct Operand {
    value: Value,
    site: u64,
    form: ArgumentForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArgumentForm {
    DirectLiteral,
    Evaluated,
}
