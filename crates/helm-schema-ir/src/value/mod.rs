mod algebra;
mod control;
mod eval;
mod heap;
mod helpers;
mod invoke;
pub(crate) mod kinds;
mod operations;
mod output;
mod query;
pub(crate) mod scalar_value;
mod session;
mod shape_summary;
mod state;
pub(crate) use session::execute_document;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use helm_schema_core::{Predicate, ValuesPath};

use crate::value::scalar_value::ScalarContent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Value {
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
    origin: Origin,
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
    Strings(ObjectId),
    Slot {
        backing: ObjectId,
        index: usize,
    },
    Unknown(UnknownValue),
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
    next_program: Cell<u64>,
    next_invocation: Cell<u64>,
    aggregate_summary: Cell<Option<u64>>,
    input_objects: RefCell<BTreeMap<ValuesPath, ObjectId>>,
    input_paths: RefCell<BTreeMap<ObjectId, ValuesPath>>,
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
    Opaque {
        reachable: AliasSet,
    },
    Widened {
        inputs: BTreeSet<ValuesPath>,
    },
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
    Input,
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenValue {
    heap: Rc<HeapVersion>,
    value: Value,
}

#[derive(Clone, Debug)]
struct FrozenField {
    heap: Rc<HeapVersion>,
    field: Field,
}

#[derive(Clone, Debug)]
pub(crate) struct State {
    heap: Heap,
    frames: Vec<Frame>,
    active: Vec<DecisionEdge>,
    output: TextBuffer,
    output_widened: bool,
    sites: Vec<ExecutedSite>,
    text_sites: Vec<ExecutedText>,
    reads: BTreeSet<RuntimeRead>,
    calls: Vec<String>,
    program: u64,
    namespace_unknown: bool,
    invocations: Vec<ExecutedInvocationRecord>,
    invocation_records_complete: bool,
    loops: Vec<LoopId>,
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

#[derive(Clone, Debug)]
struct Exit {
    state: State,
    flow: Flow,
}

#[derive(Clone, Debug)]
enum Flow {
    Normal(Normal),
    Abort(AbortReason),
    Break(LoopId),
    Continue(LoopId),
}

#[derive(Clone, Debug)]
struct Normal {
    value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbortReason {
    Arity,
    Type,
    Required,
    Fail,
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
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Default)]
pub(crate) struct TextBuffer {
    parts: Vec<TextPart>,
}
#[derive(Clone, Debug)]
enum TextPart {
    Bytes(String),
    Frozen(FrozenValue),
    Unknown(BTreeSet<ValuesPath>),
    Select {
        decision: Rc<FrozenTest>,
        yes: Rc<TextBuffer>,
        no: Rc<TextBuffer>,
    },
}
#[derive(Clone, Debug)]
pub(crate) struct ExecutedSite {
    program: u64,
    coverage_exact: bool,
    ordinal: usize,
    span: helm_schema_syntax::Span,
    source_path: Option<String>,
    condition: Predicate,
    value: FrozenValue,
    rendered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutedText {
    program: u64,
    coverage_exact: bool,
    ordinal: usize,
    bytes: String,
    span: helm_schema_syntax::Span,
    source_path: Option<String>,
    condition: Predicate,
}
#[derive(Debug)]
pub(crate) struct DocumentExecution {
    exits: Vec<Exit>,
    observed_facts: crate::observed_facts::ObservedFacts,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct RuntimeRead {
    coverage: ReadCoverage,
    path: ValuesPath,
    condition: Predicate,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Origin {
    identity: Option<ValuesPath>,
    influences: BTreeSet<ValuesPath>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct UnknownValue {
    types: TypeCoverage,
    aliases: AliasSet,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeCoverage {
    Unresolved,
    Scalar(ScalarType),
    Map(MapType),
    Sequence,
}

#[derive(Clone, Debug)]
struct CertifiedSourcePlan {
    proof: ShapeProof,
    plan: SourceSummaryPlan,
}
#[derive(Clone, Debug)]
struct ShapeProof {
    complete: bool,
}
#[derive(Clone, Debug)]
enum SourceSummaryPlan {
    NilScrub(NilScrubPlan),
    ParsedMapListMerge(ParsedMapListMergePlan),
}
#[derive(Clone, Debug)]
struct NilScrubPlan {
    operand: usize,
}
#[derive(Clone, Debug)]
struct ParsedMapListMergePlan {
    operands: Vec<usize>,
    renderer: TypedYamlMatchProof,
    overwrite: bool,
}
#[derive(Clone, Debug)]
struct TypedYamlMatchProof {
    proof: ShapeProof,
}

/// Known scalar alternatives for one evaluated value.
///
/// The arm conditions are mutually exclusive. [`Self::complete`] records
/// whether they also cover the whole input domain; helper output can retain
/// exact arms even when another branch remains unknown. Root-context fields
/// assigned across a complete `if`/`else` chain and evaluated helper output
/// both use this representation, so equality and truthiness consume the same
/// semantic fact.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValueDispatch {
    arms: Vec<(helm_schema_core::Predicate, ScalarValue)>,
    complete: bool,
}

/// One sealed leaf carries independent runtime type and optional legacy content.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValue {
    go_type: kinds::GoType,
    content: Option<ScalarContent>,
}

/// Borrowed legacy content; type-only alternatives never become a transport projection.
#[derive(Clone, Copy)]
pub(crate) struct ContentView<'a> {
    dispatch: &'a ScalarValueDispatch,
    pub(crate) complete: bool,
}

impl<'a> ContentView<'a> {
    pub(crate) fn arms(self) -> impl Iterator<Item = (&'a Predicate, &'a ScalarContent)> {
        self.dispatch.arms.iter().filter_map(|(condition, value)| {
            value.content.as_ref().map(|content| (condition, content))
        })
    }

    pub(crate) fn single(self) -> Option<(&'a Predicate, &'a ScalarContent)> {
        let mut arms = self.arms();
        let first = arms.next()?;
        arms.next().is_none().then_some(first)
    }
}

impl ScalarValueDispatch {
    pub(crate) fn content(&self) -> Option<ContentView<'_>> {
        if !self.arms.is_empty() && self.arms.iter().all(|(_, value)| value.content.is_none()) {
            return None;
        }
        Some(ContentView {
            dispatch: self,
            complete: self.complete && self.arms.iter().all(|(_, value)| value.content.is_some()),
        })
    }

    pub(crate) fn bind_content(arms: Vec<(Predicate, ScalarContent)>, complete: bool) -> Self {
        Self {
            arms: arms
                .into_iter()
                .map(|(condition, content)| (condition, ScalarValue::untyped(content)))
                .collect(),
            complete,
        }
    }
}

pub(crate) const MAX_SCALAR_DISPATCH_STATES: usize = 128;

#[cfg(test)]
#[path = "../tests/runtime_execution.rs"]
mod tests;

#[derive(Clone, Debug)]
pub(crate) struct ValueAlternative {
    condition: Predicate,
    value: FrozenValue,
    coverage_exact: bool,
}

#[derive(Clone, Copy, Debug)]
enum BoundReason {
    ObjectClasses,
    ObjectFields,
    StringSlots,
    OutputTerms,
    ExecutionAlternatives,
    LoopMembers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ReadCoverage {
    ExactPath,
    UnknownRegion,
}

#[derive(Default)]
struct ExitBuffer {
    exits: Vec<Exit>,
    summarized: bool,
}

struct Execution {
    exits: Vec<Exit>,
}
struct CallOutcome {
    execution: Execution,
}
enum CompletionInput {
    Invocation {
        invocation: Invocation,
        state: State,
    },
    Executed {
        invocation: Invocation,
        execution: Execution,
        helper_return: Option<HelperReturn>,
    },
    Summarized {
        plan: CertifiedSourcePlan,
        invocation: Invocation,
        state: State,
    },
}
struct HelperReturn {
    program: u64,
    output: TextBuffer,
    loops: Vec<LoopId>,
    wrapper: Option<ProgramWrapperMetadata>,
}
struct CompletedExecution {
    invocation: Invocation,
    execution: Execution,
    helper_return: Option<HelperReturn>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutedInvocationRecord {
    invocation: u64,
    program: u64,
    site: u64,
    function: String,
    wrapper: Option<ProgramWrapperMetadata>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProgramWrapperMetadata {
    family: Vec<u64>,
    sentinels: Vec<WrapperSentinelSite>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct WrapperSentinelSite {
    key: String,
    lookup: u64,
    tpl: u64,
    guard: u64,
}

#[derive(Default)]
struct StateBuffer {
    exits: ExitBuffer,
}
