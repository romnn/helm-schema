mod algebra;
mod control;
mod eval;
mod guard;
mod heap;
mod helpers;
mod history;
mod hosts;
mod invoke;
pub(crate) mod kinds;
mod namespace;
mod operations;
pub(crate) mod output;
mod query;
mod quoted;
mod read_set;
pub(crate) mod scalar_value;
mod session;
mod shape_summary;
mod site_index;
mod state;
mod text;
mod window_exits;
mod write_footprint;
pub(crate) mod yaml;
pub(crate) use session::project_documents;
pub(crate) use site_index::Observation;
#[cfg(test)]
pub(crate) use tests::execute_documents;

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ahash::AHashMap;
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
    Text(text::FrozenText),
    Scalar {
        go_type: ScalarType,
        content: Option<ScalarContent>,
    },
    Map {
        view: MapType,
        target: Option<ObjectId>,
    },
    Sequence(InterfaceSequence),
    Strings(StringSlice),
    Slot {
        backing: ObjectId,
        index: usize,
    },
    Unknown(UnknownValue),
}

#[derive(Clone, Debug)]
enum SequenceStorage {
    Interfaces {
        values: Vec<Value>,
    },
    Bytes {
        content: Option<Vec<u8>>,
        values: std::cell::OnceCell<Vec<Value>>,
    },
}

// Materializing scalar elements does not change immutable byte identity.
impl PartialEq for SequenceStorage {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Interfaces { values: left }, Self::Interfaces { values: right }) => {
                left == right
            }
            (
                Self::Bytes {
                    content: left,
                    values: _,
                },
                Self::Bytes {
                    content: right,
                    values: _,
                },
            ) => left == right,
            (
                Self::Interfaces { values: _ },
                Self::Bytes {
                    content: _,
                    values: _,
                },
            )
            | (
                Self::Bytes {
                    content: _,
                    values: _,
                },
                Self::Interfaces { values: _ },
            ) => false,
        }
    }
}

impl Eq for SequenceStorage {}

impl SequenceStorage {
    fn get(&self, range: std::ops::Range<usize>) -> Option<&[Value]> {
        match self {
            Self::Interfaces { values } => values.get(range),
            Self::Bytes { content, values } => values
                .get_or_init(|| {
                    content
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .map(|byte| {
                            Value::scalar(
                                ScalarType::Uint8,
                                Some(ScalarContent::Literal(helm_schema_core::GuardValue::Int(
                                    i64::from(*byte),
                                ))),
                            )
                        })
                        .collect()
                })
                .get(range),
        }
    }

    fn iter(&self) -> std::slice::Iter<'_, Value> {
        match self {
            Self::Interfaces { values } => values.iter(),
            // Byte elements carry neither input identity nor heap aliases.
            Self::Bytes {
                content: _,
                values: _,
            } => [].iter(),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Interfaces { values } => values.len(),
            Self::Bytes { content, values: _ } => content.as_ref().map_or(0, Vec::len),
        }
    }

    fn is_bytes(&self) -> bool {
        matches!(
            self,
            Self::Bytes {
                content: _,
                values: _
            }
        )
    }

    fn is_nil(&self) -> bool {
        matches!(
            self,
            Self::Bytes {
                content: None,
                values: _
            }
        )
    }

    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes { content, values: _ } => Some(content.as_deref().unwrap_or(&[])),
            Self::Interfaces { values: _ } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct InterfaceSequence {
    storage: Option<Rc<SequenceStorage>>,
    start: usize,
    length: usize,
    capacity: Option<usize>,
}

impl InterfaceSequence {
    fn items(&self) -> Option<&[Value]> {
        let Some(storage) = &self.storage else {
            return Some(&[]);
        };
        storage.get(self.start..self.start.checked_add(self.length)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StringSlice {
    backing: ObjectId,
    start: usize,
    length: usize,
    capacity: Option<usize>,
}

impl Value {
    fn sequence(items: Vec<Value>) -> Self {
        let length = items.len();
        Self::leaf(Datum::Sequence(InterfaceSequence {
            storage: Some(Rc::new(SequenceStorage::Interfaces { values: items })),
            start: 0,
            length,
            capacity: Some(length),
        }))
    }

    fn byte_sequence(items: Vec<u8>, capacity: Option<usize>) -> Self {
        let length = items.len();
        Self::leaf(Datum::Sequence(InterfaceSequence {
            storage: Some(Rc::new(SequenceStorage::Bytes {
                content: Some(items),
                values: std::cell::OnceCell::new(),
            })),
            start: 0,
            length,
            capacity,
        }))
    }

    fn nil_byte_sequence() -> Self {
        Self::leaf(Datum::Sequence(InterfaceSequence {
            storage: Some(Rc::new(SequenceStorage::Bytes {
                content: None,
                values: std::cell::OnceCell::new(),
            })),
            start: 0,
            length: 0,
            capacity: Some(0),
        }))
    }

    fn nil_sequence() -> Self {
        Self::leaf(Datum::Sequence(InterfaceSequence {
            storage: None,
            start: 0,
            length: 0,
            capacity: Some(0),
        }))
    }

    fn strings(backing: ObjectId, length: usize) -> Self {
        Self::leaf(Datum::Strings(StringSlice {
            backing,
            start: 0,
            length,
            capacity: Some(length),
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarType {
    /// A skipped selector entry; nilable parameters convert it to their zero value.
    Invalid,
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
    Files,
    ApiVersions,
    Capabilities,
    KubeVersion,
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
    runtime: Option<guard::RuntimeGuard>,
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

#[derive(Clone, Debug)]
struct InputBacking {
    object: ObjectId,
    map_type: Rc<FrozenTest>,
}

/// Forks share only the allocator; immutable versions own every semantic fact.
#[derive(Debug, Default)]
struct Arena {
    guards: RefCell<guard::DecisionStore>,
    frames: RefCell<BTreeMap<state::FrameLayout, u64>>,
    exact_decisions: RefCell<AHashMap<guard::ExactDecisionKey, std::rc::Weak<FrozenTest>>>,
    empty_snapshot: std::cell::OnceCell<Rc<HeapVersion>>,
    next_object: Cell<u64>,
    next_snapshot: Cell<u64>,
    next_decision: Cell<u64>,
    next_call: Cell<u64>,
    next_program: Cell<u64>,
    next_namespace: Cell<u64>,
    next_invocation: Cell<u64>,
    aggregate_summary: Cell<Option<u64>>,
    input_objects: RefCell<BTreeMap<ValuesPath, InputBacking>>,
    input_paths: RefCell<BTreeMap<ObjectId, ValuesPath>>,
    /// The analysis's Boolean normalization cache; results equal uncached normalization.
    predicate_memo: Rc<helm_schema_core::PredicateMemo>,
    exported_paths: RefCell<AHashMap<*const Vec<DecisionEdge>, query::PathPredicateCache>>,
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
        inputs: Rc<BTreeSet<ValuesPath>>,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Remainder {
    Closed,
    Open,
    Input,
    Unknown { inputs: Rc<BTreeSet<ValuesPath>> },
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenValue {
    heap: Rc<HeapVersion>,
    value: Value,
}

#[cfg(test)]
#[path = "../tests/runtime_frozen_helpers.rs"]
mod frozen_test_helpers;

#[derive(Clone, Debug)]
pub(crate) struct State {
    heap: Heap,
    frames: Vec<Rc<Frame>>,
    active: Rc<Vec<DecisionEdge>>,
    output: TextBuffer,
    output_widened: bool,
    observation_count: output::BranchCounts,
    invocation_count: output::BranchCounts,
    sites: history::History<ExecutedSite>,
    text_sites: history::History<ExecutedText>,
    reads: read_set::ReadSet,
    calls: Vec<CallKey>,
    program: u64,
    namespace: Rc<namespace::Namespace>,
    invocations: history::History<ExecutedInvocationRecord>,
    invocation_records_complete: bool,
    loops: Vec<LoopId>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CallKey {
    namespace: namespace::NamespaceId,
    program: Option<crate::analysis_db::ParsedProgramId>,
}

#[derive(Clone, Debug)]
struct Frame {
    continuation: u64,
    bindings: BTreeMap<String, Value>,
    dot: Option<Value>,
    boundary: FrameBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FrameBoundary {
    Call(u64),
    Lexical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AbortReason {
    Arity,
    Type,
    Required,
    Fail,
    MissingTemplate,
    TemplateParse,
}

#[derive(Clone, Debug)]
struct Invocation {
    callee: Callee,
    operands: Vec<Operand>,
    site: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Callee {
    Function(String),
    Method {
        receiver: Value,
        descriptor: crate::function_semantics::Method,
    },
}

impl Callee {
    fn semantics(&self) -> crate::function_semantics::FunctionSemantics {
        match self {
            Self::Function(name) => crate::function_semantics::function_semantics(name),
            Self::Method {
                receiver: _,
                descriptor,
            } => descriptor.semantics(),
        }
    }

    fn function_name(&self) -> Option<&str> {
        match self {
            Self::Function(name) => Some(name),
            Self::Method {
                receiver: _,
                descriptor: _,
            } => None,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Function(name) => name,
            Self::Method {
                receiver: _,
                descriptor,
            } => descriptor.name(),
        }
    }
}

#[derive(Clone, Debug)]
struct Operand {
    value: Value,
    site: u64,
    form: ArgumentForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ArgumentForm {
    DirectLiteral,
    DirectLookup,
    Evaluated,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TextBuffer {
    parts: Vec<OutputNode>,
    counts: output::BranchCounts,
    arena: Option<Rc<Arena>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObservationId {
    pub(crate) program: u64,
    pub(crate) ordinal: usize,
    pub(crate) span: helm_schema_syntax::Span,
}

#[derive(Clone, Debug)]
struct OutputNode {
    occurrences: Vec<ObservationId>,
    part: TextPart,
}

impl From<TextPart> for OutputNode {
    fn from(part: TextPart) -> Self {
        Self {
            occurrences: Vec::new(),
            part,
        }
    }
}

impl OutputNode {
    fn content(&self) -> &TextPart {
        let Self {
            occurrences: _,
            part,
        } = self;
        part
    }
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
    /// Certainty at evaluation, unaffected by later output or control flow.
    coverage_exact: bool,
    /// Whether final file text still preserves this emission's original bytes.
    rendered_unchanged: bool,
    ordinal: usize,
    span: helm_schema_syntax::Span,
    source_path: Option<String>,
    condition: output::RuntimeCondition,
    value: FrozenValue,
    plain_scalar_placement: helm_schema_core::PlainScalarPlacement,
    rendered: bool,
}
#[derive(Clone, Debug, Eq)]
pub(crate) struct ExecutedText {
    program: u64,
    coverage_exact: bool,
    rendered_unchanged: bool,
    ordinal: usize,
    bytes: String,
    span: helm_schema_syntax::Span,
    source_path: Option<String>,
    condition: output::RuntimeCondition,
}

pub(crate) struct RenderedContainerUse {
    pub(crate) condition: Predicate,
    pub(crate) path: ValuesPath,
    pub(crate) kind: crate::ValueKind,
}
#[derive(Debug)]
pub(crate) struct DocumentExecution {
    exits: Vec<Exit>,
    root_programs: BTreeSet<u64>,
    site_index: site_index::SiteIndex,
    observed_facts: crate::observed_facts::ObservedFacts,
    placement: yaml::FilePlacement,
}

#[derive(Clone, Copy)]
enum FileOutput {
    Manifest,
    Text,
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
    InputNonMap,
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

/// Guarded scalar content projected by the executed-output bridge.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScalarValueDispatch {
    arms: Vec<(Predicate, ScalarContent)>,
}

/// Borrowed contents of an executed scalar projection.
#[derive(Clone, Copy)]
pub(crate) struct ContentView<'a> {
    dispatch: &'a ScalarValueDispatch,
}

impl<'a> ContentView<'a> {
    pub(crate) fn arms(self) -> impl Iterator<Item = (&'a Predicate, &'a ScalarContent)> {
        let Self { dispatch } = self;
        let ScalarValueDispatch { arms } = dispatch;
        arms.iter().map(|(condition, content)| (condition, content))
    }
}

impl ScalarValueDispatch {
    pub(crate) fn content(&self) -> Option<ContentView<'_>> {
        let Self { arms } = self;
        (!arms.is_empty()).then_some(ContentView { dispatch: self })
    }

    pub(crate) fn bind_content(arms: Vec<(Predicate, ScalarContent)>, _complete: bool) -> Self {
        // Frozen-value coverage owns completeness; this projection carries known content.
        Self { arms }
    }
}

pub(crate) const MAX_SCALAR_DISPATCH_STATES: usize = 128;

#[cfg(test)]
#[path = "../tests/runtime_execution.rs"]
pub(crate) mod tests;

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
    BudgetPredicate,
    BudgetNodes,
    ExecutionAlternatives,
    LoopMembers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ReadCoverage {
    ExactPath,
    UnknownRegion,
    /// An executed fallback accepts an empty or nil input.
    DefaultFallback,
    /// An executed root rewrite installs a fallback merge result.
    RootMergeFallback,
    /// An opaque installed values map leaves its input key domain unresolved.
    OpaqueValuesRoot,
    /// A derived rendered value retains only its input influences.
    SerializedInfluence,
    /// An input receiver of an executed, operation-compatible member access.
    MemberHost {
        allows_nil: bool,
    },
    MapArgument {
        allows_nil: bool,
    },
}

impl ReadCoverage {
    fn is_requirement(self) -> bool {
        matches!(self, Self::MemberHost { .. } | Self::MapArgument { .. })
    }
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
    namespace: Rc<namespace::Namespace>,
    output: TextBuffer,
    loops: Vec<LoopId>,
    wrapper: Option<ProgramWrapperMetadata>,
}
struct CompletedExecution {
    invocation: Invocation,
    execution: Execution,
    helper_return: Option<HelperReturn>,
    values_fallback_source: Option<crate::ValuesDefaultSource>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutedInvocationRecord {
    ordinal: usize,
    invocation: u64,
    program: u64,
    site: u64,
    function: String,
    wrapper: Option<ProgramWrapperMetadata>,
    values_fallback: Option<Rc<ExecutedValuesFallback>>,
    condition: output::RuntimeCondition,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutedValuesFallback {
    result: BTreeMap<ObjectId, SnapshotId>,
    source: crate::ValuesDefaultSource,
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

#[cfg(test)]
#[path = "../tests/runtime_join_output.rs"]
mod join_output_tests;

#[cfg(test)]
#[path = "../tests/runtime_member_hosts.rs"]
mod member_host_tests;

#[cfg(test)]
#[path = "../tests/runtime_span_text.rs"]
mod span_text_tests;

const MAX_EXECUTION_ALTERNATIVES: usize = 4096;

#[cfg(test)]
#[path = "../tests/runtime_hosts.rs"]
mod host_tests;

#[cfg(test)]
#[path = "../tests/runtime_delta_joins.rs"]
mod delta_join_tests;
