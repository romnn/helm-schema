# R3b: one value algebra, one execution, one coupled landing

Status: design for review, 2026-10-24; no production implementation or landing claim.
Design checkout: `r3b/design`, main `0a11d18b`. Paths below are relative to
`crates/helm-schema-ir/src/` unless qualified. Construction: **12 non-landable commits**,
preceded by one independently landable differential-tool commit; one final semantic landing.

## 1. Contract and inputs

The interpreter recovers the configurations under which a chart can render and the input
origins of its emitted fields. It must account for a template changing a map, passing that
same map into a helper, testing it, and emitting text at different times. These are different
observations of one evolving value, not independently maintained interpretations of syntax.
The present design is a structural local maximum: changing the store without changing
execution and output exposes compensating errors. R3b replaces that arrangement, rather than
adding another truth/dispatch layer. `ObservedFacts → ContractIr → FinalizedContract` stays.

Authoritative inputs read: `value-algebra-interpreter.md`; all of `value-algebra-r0.md`;
ADRs 0069, 0066, 0016, 0019, 0028, 0064; the declaration inventory; admission README and tables.
Adjacent contracts: `/Volumes/T7/dev/round8/design-r2.md` **§7 overrides earlier sections**;
`/Volumes/T7/dev/round8/brief-r3a-codex.md`. Read-only historical inputs:
- R1b carry `d038e682` in `/Volumes/T7/dev/round8-r2`, including `leaf-fact-contracts.md`:
  **all thirteen LocalSlot fields remain**, with no successful leaf transfer claimed.
- Admission extension `0db9bbf6` there: **1,821 cells, 687 open** (672 `R1-R3`, 15 `ADR-0042`).
  The design checkout still has the older 1,609-cell/575-obligation suite.
- Current inventory: **165 R3b rows** = F58/S25/E42/K6/R33/D1. R2 adds six F2 E rows;
  R3a may carry additional rows. Reconcile against the actual composed base before building.

R2 supplies the sealed **type + optional content leaf**, signatures and write-free policy.
It does **not** supply F2 (`required` subject), nilness, determinism or full invocation order.
Its legacy type-test and operand-selection compatibility views are deleted here, not promoted
into exact Go tests. R3a supplies the State facade and collapsed HoleEval; its operand demand,
merge order and resolver contexts are deliberately legacy. Its `snapshot_id` is not yet a
semantic heap identity and cannot be used unexamined as the new cache key.

Decided D1–D4 stand: no fallback type hint after piped `required`; the fifteen closed-root
cells remain policy exceptions; sequence B; SDK-native Go-int values/custom functions are
outside the stock CLI contract. This is **semantic**, so ADR 0066 clauses 1–3 apply at landing;
clause 2b cannot excuse missing closures or changed bytes.

## 2. Module boundary and concrete representation

Only `value/` constructs or mutates values, slots, decisions, frames, heap objects or snapshots.
Declare their opaque handles/private fields in `value/mod.rs`; child implementation modules
can access the parent's fields. No `pub(crate)` raw fields, update records, mutable views,
`DerefMut`, or outside constructors. The compiler, inventory and negative privacy tests enforce
Rule 1. `function_semantics` remains the signature/facet owner; it supplies declarations to
`value`, never an evaluator. Existing predicate/transform types are reused, not reimplemented.

| Module | Responsibility after cut-over |
|---|---|
| `mod.rs`, `algebra.rs` | Opaque handles; guarded decision DAG; typed leaves; normalization |
| `heap.rs` | Objects, references, allocation classes, read/write, changed-object joins |
| `state.rs` | Lexical frames, call/loop context, heap forks, mode and execution cursor |
| `initialize.rs` | Symbolic Helm render-tree graph and initial namespace/schedule |
| `eval.rs`, `invoke.rs` | The sole expression walk and argument evaluation/validation |
| `control.rs` | Node walk, headers, branches, range, four-way flow, structural widening |
| `operations.rs` | Pure/heap transfer functions over already evaluated operands |
| `kinds.rs`, `query.rs` | Assignability, F2, truth, equality, content and provenance views |
| `output.rs` | Emission and serialization snapshots; bounded text terms/site records |
| `helpers.rs` | Same-executor helper/tpl entry, namespace overlays, certified cache |
| `shape_summary.rs` | The five explicitly retained bounded recognizers (§7) |

`fragment_eval/{domain,lower,project,summary}.rs` retains structural YAML/output projection,
including `collect_summary_rows`; it consumes executed site records and frozen text. It may
walk the static CST to place those records, but cannot evaluate a TemplateExpr, resolve a
helper, mutate State or reconstruct truth. `resource_identity` queries those same records.
`analysis_db` owns parse indexes, immutable catalog/capability inputs and caches, not root values.

### 2.1 Value and heap sketches

These are Rust ownership/type sketches, not a second implementation. Existing names such as
`Predicate`, `ValuesPath`, `TransformProgram`, `SiteId` denote the existing typed equivalents;
new IDs are private newtypes with deterministic allocation. Exhaustive matches are required.

```rust
pub(crate) struct Value { node: Rc<ValueNode> }
enum ValueNode {
    Leaf(Leaf),
    Select { decision: Rc<BindingDecision>, yes: Value, no: Value },
}
struct BindingDecision { test: FrozenTest }
struct Leaf { datum: Datum, origin: Origin }
enum Datum {
    Scalar(ScalarValue),
    Map(MapValue),
    Sequence(InterfaceSequence),
    Strings(StringSlice),
    Slot(StringSlot),
    Host(HostValue),
    Unknown(UnknownValue),
}
struct ScalarValue { go_type: ScalarType, content: Option<ScalarContent> }
enum GoType { Scalar(ScalarType), Map(MapType), Slice(SliceElement), Host(HostType), Unknown(KindSet) }
enum SliceElement { Interface, String, Uint8, Int }
enum ScalarType { Nil, Bool, String, Int, Int64, Uint8, Uint64, Float64, Complex128 }
enum MapType { Interface, String, Values }
struct MapValue { go_type: MapType, target: MapTarget }
enum MapTarget { Nil, Object(ObjectId) }
struct InterfaceSequence { storage: Option<Rc<SequenceStorage>>, window: Window }
struct SequenceStorage { items: Vec<Value>, remainder: SequenceRemainder }
enum SequenceRemainder { Closed, Unknown(UnknownValue) }
struct StringSlice { backing: Option<ObjectId>, window: Window }
struct Window { start: usize, length: Extent, capacity: Extent }
enum Extent { Exact(usize), Unknown }
struct StringSlot { backing: ObjectId, index: SlotIndex }
enum SlotIndex { Exact(usize), Summary }
enum TypeCoverage { Exact(GoType), Alternatives { known: BTreeSet<GoType>, remainder: KindSet } }
enum AliasSet { Known(BTreeSet<ObjectId>), Region(RegionId) }
struct UnknownValue { types: TypeCoverage, aliases: AliasSet, influences: InputSet }
struct HostValue { host_type: HostType, object: ObjectId }
struct Origin { input: Option<InputIdentity>, derivation: Derivation, placement: Placement }
enum Derivation { Exact(TransformProgram), InfluenceOnly(InputSet) }
struct InputIdentity { path: ValuesPath, role: InputRole }
enum InputRole { Whole, Member { iteration: IterationId }, Key { iteration: IterationId } }

pub(crate) struct Heap { version: Rc<HeapVersion>, arena: Rc<AllocationArena> }
struct HeapVersion { id: HeapSnapshotId, objects: ObjectTable }
struct ObjectTable { pages: BTreeMap<u64, Rc<ObjectPage>> }
struct ObjectPage { objects: BTreeMap<ObjectId, Rc<ObjectVersion>> }
struct ObjectVersion { revision: ObjectRevision, multiplicity: Multiplicity, data: ObjectData }
enum Multiplicity { Singleton, Summary }
enum ObjectData { Map(MapObject), Strings(StringBacking), Opaque(OpaqueMutable) }
struct MapObject { fields: BTreeMap<String, Field>, remainder: Remainder }
enum Field { Absent, Present(Value), Select { decision: Rc<BindingDecision>, yes: Box<Field>, no: Box<Field> }, Unknown(UnknownValue) }
enum Remainder { Closed, Unknown(UnknownValue) }
struct StringBacking { slots: Vec<Value>, remainder: SequenceRemainder }
struct OpaqueMutable { reachable: AliasSet, content: Option<HostContent> }
struct AllocationKey { site: AllocationSite, context: CallContext, iteration: LoopClass }
enum AllocationSite { Input(InputNodeId), Template(SiteId), Copy { site: SiteId, occurrence: CopyPath } }
struct CallContext { calls: Vec<CallOccurrence>, truncated: bool }
struct CallOccurrence { namespace: NamespaceId, site: SiteId, invocation: InvocationClass }
enum InvocationClass { Exact(u32), Summary }
enum LoopClass { Outside, Exact(Vec<IterationId>), Summary(LoopSiteId) }
```

`ObjectTable` is a concrete COW page table: private ObjectId ordinals select 64-object pages;
a fork clones the page index and shares pages, and a write copies only its shared page.
No new dependency or generic storage trait is required. `GoType` is a **query** over Datum: scalar type, map's Go view type, sequence kind, slot dereference or
explicit unknown coverage. There is no independent kind/truth/dispatch field. Map view type
belongs to the reference: `common.Values` and its unnamed-map conversion can share one object.
A scalar content field may be absent while type is exact; a container of unknown shape
uses an open object or UnknownValue with exact container type, not a fake scalar container. Unknown nilness is an alternative
between typed nil and nonnil references (or unknown coverage), never a guessed `nil: false`.

`ScalarContent` reuses R2 literal/symbolic terms, with frozen serialization/text terms added.
The normalized decision DAG replaces both `LocalBinding` and scalar dispatch: no cartesian
second `proven_operands` tree. Content/kind queries return borrowed guarded views and explicit
unresolved coverage; a borrowed view cannot be installed as a new semantic owner.
`Unknown` is a real leaf, not an omitted arm. Empty alternative sets mean unreachable only
when control proved that; incomplete content is never promoted to complete type coverage.

A nil map has no ObjectId; a nonnil empty map does. A nil sequence has `storage/backing=None`
and zero window. A map's omitted field is absent only if its remainder is Closed. A known
write creates Present even on an open map; `unset` creates an Absent tombstone but does not
close the remainder. Dynamic keys weakly affect every possibly matching known field and the
remainder. Fresh decoders/dicts with unknown keys are open (R0 c03/c04). Unknown aliases carry
either a finite ID set or an explicit unknown-reachable region; never silently an empty set.

`[]interface{}` has immutable copied slots containing possibly shared map references. Its
slice windows retain capacity; supported Sprig writers do not mutate those slots. `[]string`
uses mutable backing objects and live slots. Unsupported mutable values (notably Files byte
backings and method receivers) have an **opaque heap node**, not an exact byte-slice algebra.
Unknown calls invalidate that node and all possibly reachable objects; `.Files.Get` after an
opaque write cannot reuse an old exact payload. Immutable text emitted earlier stays frozen.

### 2.2 Identity and updates

Initial object IDs follow graph construction, never contents or path-string equality.
An allocation site's context includes the bounded helper call string and invocation occurrence,
so two calls at the same syntactic site or two exact iterations cannot accidentally alias.
Known finite executions use exact ordinals; collapse to a summary when the structural context
budget is exceeded. Retarget all extant references during collapse, joining their contents;
never reuse an exact ID for a different concrete object. Deep-copy occurrence paths distinguish
repeated visits through two fields to the same source (R0 a09).

Strong update requires a proved singleton target **and** singleton key under the current path.
Summary or may-alias targets receive weak update: join old/new fields under the execution
condition, including the old Absent case. Weak `unset` cannot prove absence. Each map/slot
mutation is performed at its actual invocation once; there is no mutation effect to replay.
Unknown writers invalidate transitive reachability through maps, sequences, string backings,
opaque hosts and unknown regions. A dynamic tpl can reach everything in its argument graph;
a lost footprint conservatively reaches the current render graph. Dependency reads survive.

A definite write to a singleton known key can recover precision after earlier widening;
a summary object cannot regain singleton status. Lazy input materialization interns by InputNodeId in the immutable input arena; it is not a
template write and must not mint a different alias on a second lookup.
Input provenance on a written field becomes
the written expression's origin; the old raw path is no longer its current value identity.
This is what scopes D4 and prevents constraints against pre-write inputs.

### 2.3 Frames, namespaces, flow and frozen values

```rust
pub(crate) struct State {
    heap: Heap, frames: Vec<Frame>, namespace: Rc<Namespace>, mode: EvalMode,
    active: FrozenPath, calls: CallContext, seen: SeenSet, budget: Budget,
}
struct Frame { bindings: BTreeMap<LocalName, Value>, dot: Option<Value>, scope: ScopeId, boundary: FrameBoundary }
enum FrameBoundary { Call, Lexical }
struct Namespace { id: NamespaceId, parent: Option<Rc<Namespace>>, definitions: BTreeMap<String, ProgramId> }
struct FrozenTest { id: DecisionId, when_true: Predicate, when_false: Predicate, coverage: Coverage }
enum Coverage { Exact, Partial }
struct FrozenPath { decisions: Vec<DecisionEdge>, input_predicate: Predicate }
struct FrozenValue { heap: Rc<HeapVersion>, value: Value }
struct TextBuffer { parts: Vec<TextPart>, budget: OutputBudget }
enum TextPart { Bytes(Rc<str>), Term(FrozenTextTerm), Unknown(InputSet) }
struct Execution { exits: Vec<Exit> }
struct Exit { path: FrozenPath, state: State, flow: Flow, observations: Observations }
enum Flow { Normal(Normal), Abort(AbortReason), Break(LoopId), Continue(LoopId) }
struct Normal { value: Value, output: TextBuffer }
struct CallOutcome { execution: Execution, completeness: Completeness }
enum Completeness { Complete, Truncated(BoundReason) }
```

`FrozenValue` pins an immutable heap version whose ID is the snapshot identity, so a serializer
never follows the current heap. It is created only at a language snapshot boundary. `Flow`
variants and execution exits own their states: abort is not encoded as Unknown or no value.
Break/continue carry the nearest loop identity; a helper cannot consume a caller's loop control
(d76).
Unknown evaluation is Normal(unknown), possibly with an unknown-abort alternative; it cannot
support a rejection. Observations record facts, dependencies and gaps, never writes to replay.

Frames: `:=` introduces in the active lexical frame; `=` resolves the nearest existing binding.
Lookup stops at the call boundary. `$` is an ordinary binding installed in that frame, not a
second `root` field that can drift when `$` is assigned; `read_root` resolves that binding
(d75). Rebinding `$` leaves dot unchanged.
Dot overrides exist only where scope changes dot; other frames inherit the nearest dot.
An if/with header frame spans both arms then expires; assignments to outer bindings survive.
`with` changes dot only in its taken body. Range evaluates its pipeline once, performs header
assignment/declaration even for zero iterations (d23/d54), then binds index/key and element per
iteration; one range variable is the element (d47). Helper entry creates a fresh frame with
both `$` and `.` equal to its evaluated argument, no caller locals. `template` without a pipeline
gets nil (d34); syntactically invalid free locals are parse failures (d35).

The exposed State operations are `bind`, `assign`, `read_local`, `read_root`, `read`,
`write_field`, `fork`, `join`, `snapshot_id`; getters return opaque Value/borrowed views.
External projection gets only executed-body and query views. ASTs never enter operations,
queries, observations, cached results or projected resource identity.

## 3. Execution protocol and transfers

### 3.1 The invocation owner

`eval.rs` handles literals, selectors, bindings and pipelines; `invoke.rs` alone consumes call
syntax. `operations.rs` accepts `Invocation { callee, operands: Vec<Operand>, site }` and
`&mut State`, never `TemplateExpr`. An Operand is an owned Value plus its source site and
`ArgumentForm::{DirectLiteral, Evaluated}`. Syntax/literal tokens stay only in `invoke.rs` until
contextual literal resolution; transfers receive the resolved value, not syntax to execute.

For **each reachable execution exit**:
1. Evaluate the previous pipeline stage first, preserving its writes/abort. Never evaluate it twice.
2. Resolve the callable/receiver and check total argument count, including a present pipe value.
   A count error stops before explicit arguments; a failing pipe wins over count (d01/d02).
3. Visit explicit arguments left to right, evaluating then validating each against the catalog
   parameter at that position. Failure stops that exit before the next argument. Resolve direct
   numeric/rune literals against the expected parameter; grouped/bound/piped literals are already
   runtime values. Keep parentheses on this path. Unsupported literal forms widen, never mismatch.
4. Validate the already evaluated pipe at index `explicit.len()`: for a variadic signature this
   can still be a fixed parameter (`int 1 | round 1`; `1 | printf`), otherwise the variadic element.
5. Execute the transfer once on the resulting state; stamp fixed success type only on Normal.
   Signature success says nothing about value-dependent aborts. Selected returns retain the
   selected Value/reference. Both ternary arms execute before its Bool condition is selected.

Validation must not freeze an addressable operand. d25/d26/d50 show that even a Go `string`
parameter may observe a slot changed by a later argument. Preserve StringSlot through argument
assembly; validate type at its prescribed point, read contents at actual call consumption.
Map operands similarly retain reference identity. Tests distinguish argument evaluation,
validation, transfer and emission times; one test with final output alone is insufficient.

`and`/`or` bypass eager transfer: check arity, evaluate/validate explicit operands in order,
freeze each truth test immediately and return the determining operand. Evaluate the next only
on its continuing alternatives. The pipe already executed; it is the last candidate if reached.
No rollback of its mutations is permitted if an earlier explicit operand determines the result.
Unknown truth forks both paths; it is not false. Ordinary `coalesce`/`default` evaluate all
arguments, then test emptiness against the resulting heap (R0 a05, d58). All-empty coalesce
returns nil interface; default returns its fallback verbatim. Preserve D1's no-hint policy.

### 3.2 Required, lookup and actual Go types

There is exactly one F2 consumer, `required_subject` in `value/kinds.rs`, invoked after the
message has passed Go-string validation. It uses **content and nilness**, not general truth:

| Subject leaf at consumption | Render outcome |
|---|---|
| Nil interface | Abort |
| String with exact empty bytes | Abort |
| String with proved nonempty bytes | Normal, same subject |
| String of unknown content | Split on proved empty preimage, otherwise unknown coverage |
| Bool, numbers, nonnil or typed-nil map/slice | Normal, same subject |
| Unknown including nil/string | Preserve all alternatives; no blanket rejection |

For raw coalesced inputs, the empty preimage may be expressed as original-input absence/null
or empty string. For transformed values use the existing sound transform preimage language;
no raw-path requirement through an arbitrary transform. Typed nil maps/arrays pass required
but are falsy and equal nil (R0 a04, d49). `set` on nil map aborts; `unset` succeeds. Merge with
nil destination returns an allocated map without changing a variable holding the original nil
(d53). Lint mode suppresses required/fail aborts, **not** signature/type errors (d32/d33);
failed required and fail return empty String in that mode (d65), not the original nil subject.

Field presence is separate from present nil. Retain R0 §4.2 lookup modes: chained missing
intermediate can abort, grouping changes the boundary, get-missing is empty string, index-missing
is the map element's zero, strict field lookup aborts (strict-engine s01–s03). Values methods precede keys for all
Values-typed references, including Table results; AsMap returns an unnamed-map view. Its
empty case allocates a new map, its populated case aliases (d29–d31).

R2's actual Go parameter catalog stays authoritative. Exact `typeIs`/`kindIs`/descriptors derive
from concrete Go type (d55); delete compatibility classification from old content/fragment
forms. Unknown types keep coverage. Document numbers retain Float64/Int64 loader alternatives
under the same numeric-input predicate; do not collapse them to a single runtime type merely
because JSON Schema has one numeric class. Emit a rejection only if **every feasible runtime
alternative** for the input fails. D4 applies only while raw document identity remains unchanged;
`set .Values "x" (int .Values.x)` creates an Int leaf and makes that actual parameter valid.

### 3.3 Aliasing and snapshots by operation

| Operation | Transfer, identity and observation instant |
|---|---|
| dict/list | Fresh outer structure, share element references; dict coerces keys, odd last key gets `""` (d37) |
| fromYaml/fromJson/fromToml | Fresh decoded graph; unknown content has open remainder; Helm map/error outcomes retained |
| mustFromJson / array decoders | Actual selected/fixed return contracts, typed nil retained (d49); never assume map |
| pick/omit/pluck | Fresh outer map/sequence, selected nested references shared (R0 a09b, d13) |
| append/prepend and copying list family | Copy slots, share referenced maps; []string input becomes interface slots (d14/d56) |
| slice | View with start/length/capacity; may reslice within capacity (d28) |
| index/range on []string | Live backing slot; no scalar copy until a snapshot operation |
| sortAlpha / toStrings | String backing sorted through views; toStrings([]string) aliases (R0 a02, d27) |
| sortAlpha on interface sequence | Copies/stringifies slots, leaving original slots unchanged (R0 a02b) |
| set/unset | Target mutation once; nested references installed verbatim; typed-nil exception above |
| merge/mustMerge | Destination wins populated conflicts; fills/recurses and can install source references (d15/d16/d66/d71/d72) |
| mergeOverwrite/mustMergeOverwrite | Later sources override; nested destination identity retained when recursively merged (d17/d67) |
| deepCopy/mustDeepCopy | Fresh per occurrence, including string backings; repeated source aliases split (R0 a09, d57) |
| unknown writer | Invalidate all possibly reachable mutable objects; preserve influences/unknown flow |

Merge transfers use the pinned structural rules, with tests for missing/empty/nil/differently
shaped fields and source order; an unsupported mergo case widens affected fields, not a guessed
"overlay" value. Recursive graph traversal uses a visited worklist; depth/cycles trigger
widening, not infinite deepCopy/serialization analysis. Copy occurrence paths are bounded.

Freeze guard predicates at the test, serialization payloads at the call, and text at emission.
A local captures a non-addressable scalar by value, a map by ID, a string index by SlotRef
(d11/d59). Map range copies the sorted key/value entry list at entry; nested references remain
live while later scalar rebindings do not change the entry (d12/d46). Slice range retains the
entry window and reads live string slots each iteration (R0 b02b). Emit and include return
snapshots cannot change when an aliased map is later written (d18/d19).

## 4. Whole render graph, output and bounds

### 4.1 Initialization and scheduling

Current `helm-schema/src/analysis/collection.rs` constructs a symbolic context per non-library
chart, and `symbolic/mod.rs` executes each document separately. That cannot model shared Release
or cross-file writes. Add a **render-tree session** entry point in `symbolic/mod.rs`; collection
supplies all enabled chart instances, files, dependency activation predicates, defaults and
static metadata. The session schedules all executable files and owns one State. Preserve the
single-source API as a fresh one-source session for tests/callers; never leak state across API calls.

Build a **symbolic coalesced** input graph, not constant default values. Runtime overrides remain
alternatives. Default-backed null deletion, undeclared null retention and dependency coalescence
follow the existing value loader contract; original-input predicates target the coalesced document.
Allocate conditional objects by stable input node IDs, intern once for repeated reads, and retain
open unknown tails. Equal maps under different keys do not alias just because they look equal.

- Each chart context root is a mutable map. `.Values.<child>` and `.Subcharts.<child>.Values`
  initially reference the same backing, with the appropriate Go view type. Replacing the parent's
  child field later does **not** retarget the captured subchart reference (d61).
- Global propagation creates a new global outer map at each level and shallow copies map-valued
  entries; deeper descendants may share. d40 gives `old/old/parent`, disproving universal sharing
  and universal deep copy. Preserve parent-precedence merges where child globals already exist.
- Release is shared; Chart metadata maps are per chart and mutable (d41/d64; R0 b03/b03b).
- Capabilities is an opaque typed host value. Files is not set-compatible but its byte backings
  can escape through methods (R2 correction); use opaque mutable reachability, not "all read-only".
- `.Template` is freshly replaced per file, and prior references remain prior objects (d44/d70); its Go view type is Values (d69).

Parse all definitions before any file executes; order files by descending slash-depth then reverse
lexical path, using Helm chart-instance paths, not filesystem discovery order (d42). Apply namespace
redefinitions in that parse order (d43). Library definitions participate, library ordinary files do
not render; underscore-prefixed partials parse but do not render directly (d77/d78). Dependency activation gates execution and exported observations, never manufacture an
alias to an inactive chart. Chart-local contract projection rebases original input identities once. A definite Abort stops the render session before later files; only normally completed
render alternatives publish output-derived placement facts. An analyzer parse limitation is a
gap/unknown, distinct from a proven Helm parse failure; neither is a successful empty helper.

Source cross-checks: [Helm 4.2.3 engine](https://github.com/helm/helm/blob/v4.2.3/pkg/engine/engine.go),
[coalescing](https://github.com/helm/helm/blob/v4.2.3/pkg/chart/common/util/coalesce.go).
The executable evidence is R0's scripts and `value-algebra-r3b-probes/review-results.txt`.

### 4.2 Helpers, output and cache

Helper/include/template execution enters the same evaluator and heap; only lexical frame,
namespace cursor, call stack and output buffer change. Include returns String with `<no value>`
kept; template writes into the caller buffer; tpl executes in a cloned/overlay namespace and
strips `<no value>` at return. Top-level stripping happens after concatenation (d60), not per
chunk. Include moves its child buffer into the returned String and clears the outward emission
buffer; evaluating it as an argument or discarded assignment never emits that text in the
caller. Template actions splice once; helper/tpl exits restore caller frames while retaining
the heap changes. Trim markers apply at parsed text boundaries. Text remains bounded symbolic output with
unknown spans; no second renderer and no executing holes during fragment projection.

A literal/file-derived tpl body is parsed via TreeSitterParser/HelmAst and
`parse_action_expressions`, then executed with the evaluated argument and current heap. A finite
set of known programs branches in namespaces keyed by program/parent identity. Unknown programs
produce unknown text and conservative reachable writes, not an empty output or ignored invocation.
`include`/tpl text terms preserve body sites and transform programs for downstream typed sinks;
payload truth is a query on frozen payload, never a live object query beside the string.

```rust
struct HelperCacheKey {
    program: ProgramId, namespace: NamespaceId, mode: EvalMode,
    argument: ValueIdentity, heap: ReachableSnapshotId, seen: SeenKey,
    policy: PolicyId, context: ContextKey, budget: BudgetKey,
}
enum ContextKey { Independent, BoundSensitive(CallContextId) }
struct CachedHelper { output: FrozenTextTerm, observations: Observations, flow: CachedFlow, charge: BudgetCharge }
enum CachedFlow { Normal, Abort(AbortReason) }
struct MemoCertificate { deterministic: Deterministic, readonly: ReadOnly, complete: Complete, no_escape: NoEscape }
```

The key retains the **full seen set**, namespace-qualified. Use ContextKey::Independent only
when relative observations and nonescaping allocation prove caller-context independence; otherwise
include the context. BudgetKey covers remaining structural capacity read by execution. Check for recursive re-entry before cache lookup; key by the caller's seen set before insertion,
then push the callee and restore on every exit. No legacy reachable-seen reduction until
equivalence is separately proved. Mode includes strict/lint/render and all behavior-affecting settings; PolicyId covers
capabilities, file/program sources, loader coverage and structural bound configuration. An arena
is session-owned, so IDs cannot collide across sessions; no pointer addresses in serialized IR.

Memoize only with all four certificates: audited transitive determinism, read-only execution,
untruncated completion, no escaping allocations (including references in output/observations).
Unknown catalog entries default ineligible. Random/time/DNS/lookup/crypto and unsorted map
keys/values disqualify; deterministic+unknown is unknown. Local allocations are permitted only
if neither mutation touches caller-reachable state nor allocations escape; conservative first
implementation treats every write facet as ineligible. No cached writer replay.

The determinism facet lives beside R2's mutation/signature facets, exhaustively propagated across
helper calls and literal tpl namespaces. It is not a blacklist of helper names. Dynamic callees,
unresolved methods and parse/budget failure disqualify. Recursion re-entry yields widened output,
may-write invalidation and a gap, never an empty successful summary; its truncation poisons all
ancestors' memo eligibility. Conservative cut results are never cached, even at the cycle head.
Completed cache hits reuse relative observations re-guarded once at the call site (ADR 0016).
Cold/hit/cache-disabled results must be identical (ADR 0028); adding the optional fast path is C10.
Bounds count logical abstract structure, not CPU steps or implementation allocations. Check
entry budgets before lookup and apply the same recorded logical BudgetCharge on hit as on miss.
Cache-only retained versions do not consume a semantic live-object budget; their memory is
controlled by eviction. If context/budget independence cannot be proved, retain the full key
or decline caching, rather than allowing a warm cache to bypass a cold-run widening.

### 4.3 Joins, loops and budgets

Fork COW State at a branch; existing ObjectIds survive. Join the union of changed IDs and
bindings under **the same frozen decision**, including one-sided allocations/absences. Do not
join Abort state into Normal continuation. Merge observation paths with their frozen guards.
A rejection covers an input predicate only when all feasible executions for that input abort;
partial-polarity tests/unknown alternatives prevent universal rejection. `when_true=false` with
partial coverage means "unproved", not "unreachable". Keep decision identity even when its
predicate is not expressible in schema language; it still correlates stores within execution.

Known finite range: sorted map-entry snapshot or sequence window; precise bounded iterations,
including zero case, with Break exiting that loop and Continue advancing it (d21/d22/d48).
Unknown range: preserve the zero-iteration exit; execute a summary member through the same
body, collect transitive writes including helpers, and compute a bounded join/fixed point.
On exhaustion, widen **every touched object and changed outer binding**, plus its remainder,
not just its last observed fields. Unknown call footprints widen reachable graph regions.
There is no first-iteration truth accumulator carried as a final-value proof.

Retain existing limits initially: helper structural width 32 (leaf width, not depth), scalar
fan-out 128 and existing stamped-guard budget. Reuse the existing constants after one-owner
relocation. New bounds: call-string depth 8, exact invocation/loop occurrences 32 per site,
object fields/string slots 128, output terms 128 per buffer, loop iterations 32 for exact
unrolling and 4 summary stabilization passes. These are proposed ceilings, **semantic and
reviewed**, not performance tuning. Compare cap−1/cap/cap+1 under renamed programs. Aggregate
work is also bounded by 4,096 live object classes and 4,096 retained execution exits per
session. On overflow, merge a region/exit union to Unknown with coverage retained; never stop
after a prefix of files. These limits need cap-neighbor/adjudication evidence. Exceeding any
cap records BoundReason via tracing and only widens. Continue conservative dependency/write
collection over the remaining source set after a state union loses execution precision.

Do not discard excess alternatives: replace their union with Unknown carrying all influencing
paths and reachable aliases; if those exceed bounds, promote to an unknown region. Guard-budget
loss removes exactness before join. Frozen guards/bytes already established remain frozen;
future queries of widened state abstain. Unknown loops lose touched-object facts even when a
single body pass seems monotone; a proved finite loop may retain them. This replaces all three
truth-reduction/history fields rather than encoding their sticky flags elsewhere.

## 5. Construction branch: 12 commits, no intermediate landing

Prerequisite T0 (§6) lands as test infrastructure on main before construction. Compose reviewed
R2 and R3a first, freeze their actual tree/suite and capture a fresh main baseline. Every commit
compiles and runs focused unit/integration tests, inventory checks and the two-binary gate.
The ordinary admission test may report explicitly enumerated construction mismatches; do not
edit its expected-main table to hide them. No production dual-run flag, alternative engine,
chart-name whitelist or cargo feature. Only the final union is offered for landing.

`value-algebra-r3b-deletions.tsv` assigns **every current R3b row and the six F2 carries** to
one checkpoint or a named retention exception: **171 rows, 164 deletions, five retained E and
two retained K**. This is a design map, not a mutation of the frozen inventory. Relocations from R2/R3a must be resolved by the inventory's qualified-key
relocation records. At branch start, append any carried R3a row with an explicit C02/C04/C06/C09
owner; fail if any R3b row is unassigned. Old baseline rows are never removed from classification.

Gate notation: I = identical bytes/verdicts; H = Helm-adjudicated closure/change; R = exact
construction regression set, shrinking as §6 specifies. No commit is allowed an unclassified diff.

| # | Adds/migrates | Rows/declarations removed at checkpoint | Gate and lanes expected to move |
|---|---|---|---|
| C01 | Private value DAG, typed leaf/query API; live EvalResult migration; minimum executable heap behind State; disable legacy memo | EvalResult's parallel truth/dispatch/payload/selection and per-field stores; old helper cache/key fields | R **seed**; no promised closures; capture real losses across all lanes |
| C02 | Single invocation owner; convert operand transfers/collectors to owned operands; contextual literals, pipe position, and/or | proven_operands tree; strict-operand re-evaluators; AST scalar-pattern/string/split fact functions | R↓/H; arity54, argument6, piped3, message16, literal96, r2/twins |
| C03 | Precise map/backing allocation, aliases, write/invalidate, operation-specific copy/merge | local/root mutation effects and replay functions; root_mutation AST predicates | R↓/H; store24, guarded-writes, review-cx3, review-p1, r0 a01–a07 |
| C04 | Real frames; State-only local/root access; origins and selector/member views | all 13 LocalSlot fields, RootSlot, LocalSlots, SymbolicLocalState, EvalEnv slot stores, BoundValueContext copies, GetBinding carriers, DotFrame twins and caller-root/dot resolution copies | R↓/H; header10, controls32, range/member/default-join controls, r2 reassignment twins |
| C05 | Whole-tree initialization/session/schedule, chart/global/subchart graph, Values methods | IrAnalysisDb.static_root_fields; per-interpreter root/dot stores | R↓/H; dependency-null, Airflow populated context, d29–d31/d40–d44/d61 |
| C06 | Frozen branch state joins, range snapshots/summaries, explicit Flow | ControlOutcome/HeaderTransition/RootSetState/LoopControl; RangeSubject/RangeValue*/RangeIteration* and iteration aliases | R↓/H; header fallthrough, guarded-writes, reviewer42, supplement3, d20–d24/d46–d48/d54/d68/d75/d76 |
| C07 | Shared-state helper/tpl entry, fresh lexical frames, namespace overlays, initial in-walk text; cache disabled | DeferredScalarProjection, bound_helper_interpreter, eval_bound_helper_fragment, remaining resolution wrapper; nested file interpreter and static helper-request walk | R↓/H; helper-maybe-dict, tpl-maybe-text, fable B/argument, alias/helper-boundary controls |
| C08 | Complete in-walk emission/serialization and YAML placement from site records | FragmentSummary value/dispatch/payload copies, scalar_dispatch_from_fragment, HelperOutputEvaluator, helper_literal_dispatch; remaining payload-output stores | R↓/H; output-snapshot, comparison, astra-coalesce-emitters, frozen614 |
| C09 | F2, exact type tests and final kind/arity contracts under current execution; coalesce nil fold | six F2 carries; R2 certificate/legacy eligibility/type-test compatibility; remaining condition/path re-derivation | R↓/H; required64, empty31, nil31, intrinsic60, coalesce-state/matrix, r2/twins |
| C10 | Determinism facet, eligible memo entries and reachable snapshot IDs | none (old cache/key fields already removed in C01) | I versus C09, R unchanged; all lanes; cache on/off byte identity |
| C11 | Bounded recognizer contracts and executed-output integration; retained E exception audit | relocate only five named recognizers/caches; no deletion credit for retained behavior | I or H, R non-increasing; Airflow, Bitnami, NATS controls |
| C12 | Remove all temporary construction conversions; complete provenance/export wiring; cut-over receipt | all 19 AbstractValue variants, SelectionChainIdentities, residual ScalarValueDispatch transport; every remaining scheduled row | H, **R=∅**; full suite, corpus/battery/sweep, all required R3b classes |

C01 is an executable migration, not dead definitions: expression results use the new value and
views. Temporary legacy-to-value conversions are **inside value**, inventoried, and marked C12;
unmigrated transfers keep only sound preserved content/origin, otherwise explicit Unknown.
Legacy effects/slots may persist solely until their listed checkpoint. They cannot become a
second maintained heap or trigger an old/new dual evaluation. If an earlier type removal affects a later-deleted
field, convert that field to opaque Value/State transport in the earlier commit; retain its
classified row until actual deletion, and never use the transport to overwrite authoritative
state. This avoids dangling references to deleted slot types between checkpoints.
If C03/C07 exposes a new regression
outside C01's seed, fix it before that checkpoint can pass; do not enlarge the ledger. The sizes
above are work packages, not promises that an unreviewed partial patch is green.

### 5.1 The 122 operand sites and final deletion proof

R0's 122 is a frozen estimate, not a stopping count. A current textual audit finds **123 call
lines** excluding declarations/tests: expr_call_eval mod31/serialization24/collections21/
strict5/root5/comparisons4/traversal4; condition_predicate12/path_resolution2; expr_eval9;
fragment_expr_eval4; assignments1; resource_identity1. R3a removes some before R3b starts.
T0 records actual parsed call expressions with qualified enclosing function, source span and
callee; text counts are only a cross-check (multiline calls and definitions are distinguished).

C02 classifies every site: (a) legitimate evaluator recursion, moves to eval/invoke/control;
(b) transfer/contract consumer, replaced by an Operand view; (c) demand replay, deleted in
C02/C03/C07/C08/C09; (d) source-only path discovery, parsed structurally or reads an executed
origin, never executes with `EvalEnv::default()`. Each row names its checkpoint and post-move
callable. An AST inventory test rejects TemplateExpr in transfer/collector entry signatures,
and evaluator calls outside the sole executor. A new method/name cannot escape by renaming
`eval_expr`: the rule enumerates calls to the evaluator API and semantic type closure.

AbstractValue replacement is exhaustive: Top/Unknown/Widened → explicit coverage+influences;
ValuesPath/JsonDecodedPath/OutputPath/RootContext/RangeKey → Origin or a real root/member reference;
StringSet/DerivedBoolean/SplitSegment → typed scalar terms; Dict/Overlay/MergedLayers → heap
fields; List/KeysList/SplitList → sequences/backings; Choice/FirstTruthy → decision DAG after
actual evaluation/selection. No renamed 19-way parallel enum or compatibility accessor survives.
`proven_operands` disappears once each call owns operands and alternatives retain correlation.
`collect_summary_rows` survives as projection only; keeping it does not justify deferred execution.

R1b's default_paths/output_meta/typeof/int_cast metadata becomes origin/transform/placement on
leaves and executed emissions, with absence of evidence distinct from evidence of no defaults.
Preserve the six full-schema `local_default_join.rs` controls. Member/key/iterable roles are
separate typed origins; derive definite membership only from an exact iteration plan. The three
truth/history flags become flow/coverage behavior, never leaf booleans. This is a semantic
replacement with Helm adjudication, not a claimed byte-equivalent union of old leaf metadata.

## 6. T0: implementable two-binary differential gate specification

This section is the tool builder's contract. Proposed files:
`crates/helm-schema-test-support/src/value_algebra_diff.rs`, its `src/bin/value_algebra_diff.rs`,
`tests/value_algebra_diff.rs`, the companion `src/bin/value_algebra_export.rs`, and test-only
IR-export integration in the existing producer. The public `AnalysisSession::contract_document()` already supplies the IR.
Land T0 alone: it must change no production semantics, captures main==main evidence and records
newly exposed obligations without pretending to close them (ADR 0066 infrastructure exception).
Use the existing admission, cell_matrix, manifest, registry, HelmRunner and adjudication APIs.
T0 also adds an explicit inventory construction mode: exact keys due by Cxx come from the
reviewed deletion map, absent rows need surviving producer mappings, and frozen classes/seals
still apply. New temporary F/S/E rows need `replaces` plus a C12 deletion. This mode must not
modify `landed.txt` or weaken ordinary `task lint`; final verification uses the ordinary gate.
Without it the existing checker correctly rejects an early partial R3b deletion as premature.

### 6.1 Inputs and capture

CLI has `capture`, `compare`, `verify` subcommands. `capture` takes `--bin PATH --commit SHA
--producer PATH --exporter PATH --checkout DIR --build-receipt FILE --suite DIR --bundle DIR
--helm PATH --helmsweep PATH --out NEW_DIR`.
The producer/exporter are built from the **same tree** as their CLI. The build receipt records
Cargo JSON artifact paths/hashes, command and source digest before/after one build; reject a
changed input tree or unverifiable provenance. A manually supplied commit label alone is not proof.
`compare --main DIR --candidate DIR --previous RECEIPT --ledger FILE --out NEW_DIR`
requires the previous receipt except with `--seed` at C01; `--landing` requires zero regressions. `verify DIR` rehashes all
inputs/artifacts and validates receipt completeness. Arguments are typed; outputs atomic,
exclusive/create-new. Only private scratch/Helm replay caches may be written; source charts, expectations, shared
caches and Git index are read-only.

Capture manifest version 1 records full git/tree/source digests, CLI/producer/tool/Helm SHA256,
Helm version (must 4.2.3), suite roster and contents digests, provider bundle digests, pinned
Kubernetes release, recipe/declared-types/profile, inventory version, every exit and artifact hash.
A producer source digest matters for dirty test-only additions; commit text alone is insufficient.
Use a private prepared chart tree with shipped schemas/tests removed exactly as cell_matrix does.
Generation runs offline, `--no-config`, bundled Kubernetes schemas, full profile and each suite
mode. Preserve per-job stdout/stderr/status along with raw schema bytes; normalize **only**
recorded scratch-root prefixes for comparison, never JSON ordering or semantic content.

Generate one job per `(chart digest, declared-types mode, generation recipe)`, then validate
**every cell** over its recorded Helm-coalesced document. Verify inputs_sha256/document_sha256,
roster completeness, no empty tables/files, obligations/main agreement and the 15 exact policy
IDs. Never validate bare overrides or substitute `{}` for defaults. Both snapshots must use
the same frozen suite, including the R2 extension and deterministic R3b render-verdict probes
converted to cells before
C01. Lint-mode and exact-output probes use a separate mode-stamped control registry, not a
forged render-class row. Their recorded Helm bytes/aborts ground interpreter unit tests as
C03–C09 adds the corresponding operations; the schema CLI is not a renderer. Verdict-only
admission cannot detect an incorrect but still rendering abstract output.

Use existing cell_matrix with `main` and `candidate` labels for live pinned-Helm remeasurement
at baseline freeze and for **all changed verdicts**. Ordinary construction comparisons may
reuse content-addressed Helm classes only when input, mode, Helm hash and coalesced hash match.
Helm-class drift is infrastructure failure, not a closure. Separate template abort, rendered
resource violation and policy rejection; Kubernetes-invalid output is not a template abort.
Non-deterministic probes never become a frozen pass/reject oracle; cache tests assert invariance
of analysis, not equality of random sampled Helm outputs.

Produce a clean complete corpus dump using each tree's `corpus_generation --out DIR --helm-ready`;
compare registry artifact **keys** and bytes (schemas, final-output policies, lean variants,
IR/symbolic artifacts, Helm-ready companions/name maps). Verify each manifest with its own
producer in its recorded checkout. Missing/extra keys, stale producer, panic, timeout, invalid
schema or parse failure in the tool are infrastructure failures, never tolerated regressions.
CLI corpus generation additionally uses registry whole-chart recipes for all CLI-representable
schema jobs and compares CLI bytes/verdicts with that tree's producer output. Library-only
fixture recipes stay producer comparisons, explicitly marked; do not fabricate a CLI equivalent.

Admission IR is not available from the production CLI by setting `IR_DUMP`. Build a companion
`value_algebra_export` in test-support at each revision: construct `AnalysisSession` with the
same GenerateOptions/provider/output policy as the CLI recipe, call its existing public
`contract_document()` and emit via the same session output pipeline. No production export API
is needed. Serialize the ContractDocument with the producer's stable JSON convention; retain
registered template IR recipes via `generate::ir_document`. T0 compares each exporter schema
with its CLI bytes to catch recipe drift. Record compiler/source provenance for both executables;
a binary-only baseline without the companion export is incomplete and refused.

### 6.2 Comparison classes and proof coverage

Write `cells.tsv`, `artifacts.tsv`, `changes.jsonl`, `regressions.tsv`, `closures.tsv`,
`coverage.json`, and a SHA256-sealed `receipt.json`. Every input/job/cell/artifact must appear
exactly once. JSONL is machine-readable; TSV identities never depend on row order.

| Class | Definition / required disposition |
|---|---|
| IDENTICAL | Same cell verdict; byte identity separately stated for each artifact, stdout/stderr/status |
| INHERITED | Main and candidate disagree with Helm identically; exact obligation remains |
| POLICY | Exact frozen ADR-0042 IDs, separately counted; cannot count as semantic closure |
| CLOSURE | Main wrong, candidate agrees with Helm; live replay required, owner/lane recorded |
| REGRESSION | Main right, candidate wrong, or an already earned closure is lost |
| EQUIVALENT-CHANGE | Bytes differ with a reviewed proof of unchanged denotation/observation; no closure credit |
| UNADJUDICATED | Changed schema/IR/output without complete evidence; comparison fails |
| INFRASTRUCTURE | Provenance/roster/tool/exit/parse/timeout/incomplete capture failure; always fatal |

IDENTICAL verdicts do **not** imply identical schemas/IR. Every changed schema runs the existing
acceptance battery against the two clean dumps, with `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` set,
and all generated acceptance flips adjudicated by real Helm. Record coverage/probe caps and
non-vacuous counts; coalesced null-deletion semantics and forced raw audits remain explicit.
Changed IR gets a structural path diff, mapped to its source site, projected schema artifacts
and new targeted witnesses. Do not delete source/provenance/truncation fields to make it compare.
A changed IR fact without a schema flip needs either a concrete Helm output/abort witness or
an independently reviewed representation-equivalence certificate. That certificate includes
base/candidate artifact hashes, JSON pointers, reason and reviewer, and expires on either hash
change. Same verdict on finite cells alone is not such a proof.

A closure for one cell cannot license all byte changes in its chart. `changes.jsonl` links each
schema/IR change group to specific proof IDs, including negative controls and projection traces.
The tool can verify linkage and hash integrity; reviewers adjudicate semantic adequacy. No
wildcard file/directory waivers. UNADJUDICATED is not admitted as "tracked regression".

### 6.3 Monotone construction receipts

Pin main M once after R2/R3a; do not compare merely with the previous branch commit. T0's M==M
check is a tool self-test, not the first construction regression set. C01 is the first live
migration and establishes a **measured**, reviewed finite R₁. No invented allowance count,
blanket lane entry or preemptive "all cases" set. Each seed row stores full cell/artifact proof
identity, M answer, candidate answer, Helm class, input digest, owner, cause and repair checkpoint.
Seeding is one-time and recorded outside ordinary admission obligations. Capture may emit a
seed proposal; `compare --seed` succeeds only when its complete measured set equals the
reviewed ledger supplied with reviewer/evidence IDs. It authorizes no landing.

For every Cᵢ after C01: `Rᵢ ⊆ Rᵢ₋₁`; once removed, an ID cannot reappear. Use set inclusion,
not counts (one fixed and one newly broken must fail). Also preserve earned closures: the set
of previously correct *or closed* cells cannot shrink. Unchanged nonempty R is allowed for
representation/cache checkpoints; every semantic checkpoint must shrink R or add verified
closures, and the receipt reports which. A changed failure cause/verdict/affected JSON pointer
requires fresh adjudication; retaining its cell name does not cover a stronger failure.

No candidate-generated expectations feed the comparison. Regression ledger reductions are
emitted as a proposed exact edit; stale rows fail until removed, larger sets fail. `--seed`
refuses an existing branch receipt/ledger identity. New tests discovered mid-branch are first
measured on M and every retained checkpoint (or cause a stop to reconstruct that history);
they cannot silently expand R at the newest checkpoint. On main movement, replay the entire
checkpoint chain against new M before adopting a new baseline; preserve old evidence and
previously earned closures. A new branch name cannot reset the regression history.

Exit codes: 0 = complete comparison satisfying its mode; 1 = semantic/unadjudicated/monotonicity
failure; 2 = invalid request or mismatched identities; 3 = infrastructure execution failure.
A construction receipt prominently says **NON-LANDABLE**, even at exit 0. `--landing` requires
R empty, strictly fewer inherited semantic failures, complete adjudication, no unapproved new
owners, and all applicable ordinary gates. T0 tests: self comparison, missing cell/artifact,
stale digest, swapped modes, invalid schema/timeout, closure, new regression, equal-count set
swap, lost closure, stale ledger row, empty/vacuous battery, mismatched producer, IR-only change,
wrong coalesced document, and attempted second seed. None requires changing production code.

## 7. Helper-shape exceptions and admission ownership

Retain **all five recognizers provisionally**, not an untested claim that bounded execution
reproduces arbitrary recursive helper output. This is R0's explicit exception requiring reviewer
sign-off; do not count relocation as deletion. Move their implementation together to
`value/shape_summary.rs`, retaining class E and named obligations. The two classification caches
retain K. Proposed exact obligation IDs:

| E recognizer | Obligation ID | Why execution may not replace it in R3b |
|---|---|---|
| `program_wrapper_sentinels` | `r3b-shape-program-wrapper` | Bounded family recognition of unknown values-authored tpl programs |
| `classify_nil_scrub_helper` | `r3b-shape-nil-scrub` | Recursive arbitrary-depth nil-removal provenance |
| `classify_pair_merge_helper` | `r3b-shape-pair-merge` | Recursive merge of unknown input maps and literal overwrite-key policy |
| `classify_parsed_map_list_merge_helper` | `r3b-shape-parsed-map-list` | Unknown iteration count with render/decode/merge composition |
| `classify_typed_yaml_renderer` | `r3b-shape-typed-yaml-renderer` | Structural render/program provenance used by the preceding recognizer |

Family cap stays 16; add a 128-action structural cap per recognized body, bounds return no
recognition (including partial family discovery), never partial proof. These are source-shape
summaries of the **same** transfer, with namespace/program digests, not runtime values cached by
helper name. Parse structurally, reject extra writers/branches/output outside the matched shape,
and preserve current verdicts on positive, renamed and near-miss controls. Do not strengthen a
recognizer while moving it without separate Helm evidence. A successful summary can replace the
body only with an audited exact transfer/readonly proof; otherwise it contributes bounded
provenance hints after execution and cannot override concrete state, abort, type or frozen text.

For each, T0/C11 register full-schema corpus controls (Airflow, Bitnami, NATS), a minimal renamed
positive and one-action near misses, plus over-cap behavior. New discovery belongs in its exact
obligation. If the unified evaluator reproduces all of that recognizer's outputs/observations,
delete it and its cache in C11 and remove the exception. Without sign-off, the cut-over is blocked;
retagging five E rows `keep` without review would violate the deletion receipt.

The 687 snapshot obligations are not a promised closure count: R2 is concurrent. At freeze,
reclassify residuals by mechanism, preserving IDs/owners; never move misses to policy for convenience.
Required landing closures are all R0 §5.5 stateful classes: store24, guarded-writes, header10,
reviewer42, piped3, supplement3, coalesce-state/nullary controls, Fable B/argument controls,
Codex merge/helper/alias/incidental writes, output snapshots, dependency-null and Airflow controls,
plus F2 and deferred helper/tpl cases. Zero-open control lanes must remain zero-open. Large lanes
(frozen614 292; r2-twins 79; r2 33; literal96 30; arity54 28 at the extension snapshot) are reported
cell-by-cell, not claimed closed from one representative. The fifteen ADR-0042 cells stay separate.

## 8. Performance and LOC budget

### 8.1 ADR 0064 measurement plan

No performance claim is made by this design. Capture baseline release CLI and dev/producer
binaries **before** implementation and copy them out of target; same for candidate before any
A/B timing. Freeze a private provider-cache snapshot, warm online once, prove offline output
identity, then time only offline at pinned Kubernetes version. Hash schema, stdout, JSON
stderr and exit. A changed byte leaves the representation-performance track: adjudicate first,
then report semantic cost separately. Never trade a lower bound for a speed claim.

Measurement checkpoints: baseline; C01 (value/State footprint); C03 (allocation/reachability);
C05 (whole-tree scheduling); C07/C08 (removing second walks); C10 (new key/cache); C12 final.
Use the ADR reference charts plus Bitnami fan-out (Airflow/common.tplvalues and a second common
library consumer), NATS, Otel, ingress-nginx and the admission suite. Record sizes/inputs,
allocation sites, maximum live heap versions, weak updates, widened regions, output terms,
helper invocations, cache eligibility/miss reasons, reachable-key sizes and seen-set sizes.
Add `tracing::instrument` spans to fork/join, allocation, invocation, helper entry, reachable
snapshot construction, serialization and widening; use the existing dev Perfetto layer.
Traced runs explain phase proportions only; no custom timers or traced timing comparisons.

Snapshot identity is version-sensitive: a write creates a new ObjectRevision; branch joins
include every reachable changed descendant. Initial safe key uses whole heap snapshot ID.
C10 may intern a canonical sorted vector of `(ObjectId, ObjectRevision)` for reachable args/
ambient host inputs as ReachableSnapshotId, using structural equality to resolve hash collisions.
Compute graph reachability cycle-safely; unresolved aliases use the whole reachable-region epoch.
Memoize reachability per immutable version. An unrelated allocation can then leave a helper's
reachable ID unchanged. Do not hash/clone the entire heap at every query, ignore a nested write,
or share keys for structurally equal but differently aliased argument graphs.

Per-site summary objects bound recursive/loop allocation; COW versions/delta joins avoid O(heap)
cloning per branch. Release unreferenced snapshots/cache entries at session boundaries; use an
entry/byte-limited LRU if measured memory requires one, eviction changes cost only. No escaping
allocation cache means fresh-dict Bitnami calls may miss often. Measure this loss against saved
replays before proposing further eligibility. Do not omit `seen` to restore an old hit rate.

Final decision uses at least seven interleaved A/B pairs on a recorded quiet/loaded host,
median/min–max wall and CPU, peak RSS and output/schema sizes. A paired range crossing zero is
not a gain. Report per-chart and aggregate, including worst outlier; no average hiding fan-out.
Any material slowdown needs an attributed reviewer disposition before landing; tolerance is Q2.

### 8.2 Region budgets (production Rust code lines)

Measured here: `task tokei:core` exit 0, **77,053 Rust code lines** on 0a11d18b; no production
edits. The R0 sizes were physical lines, not deletion credits. Current region code counts below
come from tokei JSON; final budgets must be rebased after actual R2/R3a, with no double credit.

| Region | Current code | Added ceiling/range | Deleted target |
|---|---:|---:|---:|
| Algebra/queries + abstract_value.rs | 1,581 | 250–400 | 900–1,300 |
| Heap/allocation/copy/opaque reachability | new | 1,000–1,450 | 0 |
| State/frames: value/* + symbolic_local_state + eval_env | 1,512 | 180–300 | 800–1,100 |
| Invocation/operands + provenance migration | distributed | 400–650 | 250–450 |
| Snapshots/output: summary.rs | 1,089 | 250–400 | 500–750 |
| Flow/loops: control.rs | 2,800 | 350–550 | 300–550 |
| Init/session integration, including helm-schema collection | new | 250–400 | 0–100 |
| Cache/namespaces + retained shape tightening | analysis_db 1,567 | 180–300 | 150–250 |
| Mutation/replay: root_mutation + assignments | 1,086 | 0–80 | 400–600 |
| F2 hole_effects + condition_predicate | 3,663 | 120–220 | 500–950 |
| HelperOutputEvaluator/helper_literal/files/static requests | 1,549 combined files | 0 | 500–800 |
| **Total affected implementation** | overlapping files counted once above | **2,980–4,750** | **4,300–6,850** |

Expected net **−3,870…+450**; +450 is a review ceiling, not permission for growth. R0 additions
were 2,130–3,700: the increase explicitly budgets real frames, opaque reachability, whole-render
session integration and retained recognizer audit. R1b residuals give no advance deletion credit.
Tests/tooling and documentation are reported separately. Validate net with the actual task,
then justify any positive total by closed defect classes under ADR 0066 clause 3.

## 9. Probe receipt, risks, questions and completion

`value-algebra-r3b-probes/review.sh` creates unique charts under `<clone>/target/probes/`, checks
Helm version and records binary SHA256, command, full template/values/additional files, exit,
expected observation and actual output. Run: `bash plan/value-algebra-r3b-probes/review.sh >
plan/value-algebra-r3b-probes/review-results.txt`. Current receipt: **78/78 CLI checks pass**.
`strict.sh > strict-results.txt` additionally runs **3/3 engine checks**, using Helm module
v4.2.3 with its pinned dependency set and Go 1.26.5, offline. This is necessary because the
Helm CLI does not expose Engine.Strict; `helm lint --strict` is a different option. The engine
script removes an inherited mismatching GOROOT, verifies build versions and records command
exit 0. Total: **81/81 checks**; scripts and the small engine probe source are included.
Errors in initial probe YAML quoting were corrected and the complete suite rerun; none were
accepted as template abort evidence. Expected aborts and lint cases are checked individually.

| Evidence | Newly pinned facts beyond R0 |
|---|---|
| d01–d10, d32–d39, d49/d55/d58/d65/d66/d73/d74 | Count/pipe/eager order, selected operands, F2 boundaries, lint types, frame parse rules, dict odd keys, exact type tests |
| d11–d19, d25–d28, d50–d53, d56/d57/d59/d67/d71/d72 | Scalar vs live-slot capture, map range, copy/merge aliasing, serialization, capacity, nil merge, allocation separation |
| d20–d24, d46–d48/d54/d68/d75/d76 | Scope, break/continue, empty range assignment and entry snapshot |
| d29–d31, d40–d45/d61–d64/d69/d70/d77/d78 | AsMap/Table, global depth sharing, Release/Chart, file/definition order, Template replacement, opaque byte types, subchart rebinding |
| d36/d60 | Top-level stripping after concatenation |
| strict s01–s03 | Strict field access aborts; index/get missing-key defaults bypass that mode |

R0 evidence remains binding for unknown-map remainders, grouped lookup, required typed-nil map,
short-circuit pipes, include/tpl namespace and output differences, random helpers and selectors.
Proposed analysis caps/cache eligibility are conservative design choices, not claimed Helm facts.
Do not infer unprobed exact transfers for unsupported functions: widen and preserve reads/writes.

Top risks, in order:
1. **Whole-tree state meets old per-chart analysis.** Cross-file/root writes require the collection
   integration in C05; an IR-only patch that leaves old isolated sessions is incomplete.
2. **Live references and snapshot boundaries.** Eager operand validation does not copy slots;
   cache/output keys that use later heap state can recreate the original stale-fact bug.
3. **Provenance and incomplete guard coverage.** R1b's failed transfer shows why unioning metadata
   can tighten; full schemas, partial-polarity joins and universal failure proofs are mandatory.
4. **Construction masking/large first seed.** The gate must preserve earned closures and compare
   full artifacts; a shrinking count or a refreshed-main table can hide a new failure.
5. **Cache fan-out and summary allocations.** Correct seen/snapshot keys and no escaping allocation
   may lose Bitnami hit rate; bounded weak updates can also widen too much. Measure both precision
   and cost; no cap reduction or stale helper caching to make numbers look better.

Questions for the user/review (no implementation blocker for writing this design):
- **Q1:** accept the five named, bounded E exceptions at cut-over if execution-equivalence is not
  established? Recommendation: yes with the exact obligations/control tests and reviewer sign-off;
  do not silently expand their recognition scope. R0 permits this, but the retained set is now explicit.
- **Q2:** what measured semantic slowdown/RSS increase is acceptable for required closures?
  Recommendation: decide from C07/C10 paired results; approve no fixed percentage without those data.
  Cache correctness, zero new failures and the agreed admission closures are not tradeable.

Final cut-over: inventory/self-tests/privacy/operand ownership pass; all scheduled deletions and
approved exception mappings survive; differential R empty and verified closures >0; one clean
final dump; every acceptance flip Helm-adjudicated; admission refresh only now, shrink-only;
`cargo fmt --check`, `task lint`, `task lint:fc`, `cargo nextest run --workspace`,
`task test:integration`, `task test:all`, CLI install + downstream luup2 gate, `task tokei:core`,
and sealed landing-chain receipt each reported independently. No fixture or code edits after
those results without rerunning the stale gates. Construction commits do not individually land.
This design task ran probes and LOC measurement only; it does not claim implementation gates.
