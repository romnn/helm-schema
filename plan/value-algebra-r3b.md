# R3b: one value algebra, one execution, one coupled landing

Status: design for review, 2026-10-24; no production implementation or landing claim.
Design checkout: `r3b/design`, main `0a11d18b`. Paths below are relative to
`crates/helm-schema-ir/src/` unless qualified. Construction: **10 non-landable commits**,
preceded by one independently landable differential-tool commit; one final semantic landing.

## 0. Review disposition (12c80f2d → this revision)

Both reviews are accepted in full; no finding is refuted. A = architecture review
`20261024T165622-b3c569a6`; S = semantics review `20261024T165630-5cac3e07`.
The probe receipt is in §9; analysis-only tests are explicitly future builder obligations.

| Finding | Disposition and resolution |
|---|---|
| A P1-1: active path missing in cache | §4.2 adds canonical FrozenPathKey; §9 sem-cache controls; cold/hit/disabled opposite-path test mandatory |
| A P1-2: construction dependency cycles | §5 C02 atomically integrates heap, frames, Flow, helper entry and snapshots before transport/replay deletion; C01 has no active migration |
| A P1-3: battery reads committed fixtures | §6 standalone spec §1 extracts test-support comparison core with verified dump pairs and separate construction/landing policies |
| A P1-4: cap displacement hides regressions | Standalone spec §4 persists every discovered witness and chains versioned receipts; union replay/backfill is outside discovery caps |
| A P2-5: unlisted semantic producers | §5.1 and deletion map add fail-syntax producer/four consumer edges and five resource selector/guard producers/two consumer edges |
| A P2-6: recognizer result ownership | §7 four typed SourceSummaryPlan variants, sole completion seam; pair-merge recognizer deleted, not retained as proposed |
| A P2-7: allocation collapse mutates history | §2.2 preserves old exact IDs/versions; only future allocations use summaries; cap widening is current-version-only |
| A performance note | §2.1/§8 charge page-index copying, fields copied and snapshot-pinned bytes explicitly |
| S P1-1: scalar-slot consumption | §3.1/§3.3 materialize ordinary-call scalars at consumption; retain builtin index/and/or slots; sem-call-copy/live-slot |
| S P1-2: Sprig slice | §3.3 empty-input nil, only first two coerced indices, ignored extra values still eagerly evaluated; sem-slice |
| S P1-3: unordered aliased merges | §3.3 preserves feasible orders or widens; §4.2 determinism facet; sem-merge-order |
| S P2-1: nil merge destination | §3.2 conditional allocation; sem-merge-nil covers all four variants × four source cases |
| S P2-2: empty definitions | §4.1/§4.2 syntax-empty replacement rule, runtime-empty distinction; sem-namespace |
| S P2-3: library parse filtering | §4.1 excludes ordinary library files before parsing; all three sem-library probes run |
| S admission-family table | §9 maps every requested family to concrete Helm evidence and additional interpreter/admission tests; suffix-write widening fixed in §4.3 |

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
| `shape_summary.rs` | The four explicitly retained bounded recognizers (§7) |

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
struct HeapVersion { id: HeapSnapshotId, objects: ObjectTable, regions: BTreeMap<RegionId, UnknownRegion> }
struct UnknownRegion { reachable: AliasSet, influences: InputSet }
struct ObjectTable { pages: BTreeMap<u64, Rc<ObjectPage>> }
struct ObjectPage { objects: BTreeMap<ObjectId, Rc<ObjectVersion>> }
struct ObjectVersion { revision: ObjectRevision, multiplicity: Multiplicity, data: ObjectData }
enum Multiplicity { Singleton, Summary }
enum ObjectData { Map(MapObject), Strings(StringBacking), Opaque(OpaqueMutable), Widened(RegionId) }
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
a fork clones the page index in O(page count) and shares pages. A write copies its shared
page and the changed object/field map; this is not O(1) fork or O(1) large-map mutation.
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
budget is exceeded. **Existing exact IDs and their versions never change identity.** Only
subsequent allocations at the collapsed site/context use a new summary ID, with weak updates.
Current references may therefore target exact objects plus a summary; never globally retarget
references or rewrite an older HeapVersion. Never reuse an exact ID for a different object.
On aggregate-cap exhaustion, create a widened *current* version/region, preserving old exact-ID
entries reachable from frozen observations; future reads through its region view lose precision.
Current-version entries become ObjectData::Widened(region); the region includes all lost aliases
and influences. Future over-cap allocations use a summary target in that region. Frozen reads
use their saved version without current-region forwarding; old entries are never edited in place.
Garbage collection may
reclaim an exact object only when no current state, snapshot or cache entry retains its version.
Test emission/guard/serialization before collapse, then writes after collapse, including aliases.
Deep-copy occurrence paths distinguish
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
struct FrozenPathKey { edges: Vec<FrozenEdgeKey>, input: CanonicalPredicate }
struct FrozenEdgeKey { test: FrozenTestKey, taken: bool }
struct FrozenTestKey { test: DecisionId, coverage: Coverage, true_key: CanonicalPredicate, false_key: CanonicalPredicate }
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
   Signature success says nothing about value-dependent aborts. At ordinary-call consumption,
   materialize every scalar StringSlot from the resulting heap into a scalar Value, then run
   the transfer. Preserve map IDs and slice backing/window identities. Both ternary arms execute
   before its Bool condition selects a **consumed** operand. Fixed returns use the actual Go view.

Validation must not freeze an addressable operand. d25/d26/d50 show that even a Go `string`
parameter may observe a slot changed by a later argument. Preserve StringSlot through argument
assembly; validate type at its prescribed point, read contents at actual call consumption.
Sprig first/last and related element projections also copy scalar elements at consumption;
builtin index and and/or may return live slots. dict/list/set copy scalar slots when installing
arguments. Map operands retain reference identity. Tests distinguish argument evaluation,
validation, transfer and emission times; one test with final output alone is insufficient.

`and`/`or` bypass eager transfer: check arity, evaluate/validate explicit operands in order,
freeze each truth test immediately and return the determining operand. Evaluate the next only
on its continuing alternatives. The pipe already executed; it is the last candidate if reached.
No rollback of its mutations is permitted if an earlier explicit operand determines the result.
Unknown truth forks both paths; it is not false. Ordinary `coalesce`/`default` evaluate all
arguments, then test emptiness against the resulting heap (R0 a05, d58). All-empty coalesce
returns nil interface; default returns its consumed fallback, with container identity preserved.
Preserve D1's no-hint policy.

### 3.2 Required, lookup and actual Go types

There is exactly one F2 consumer, `required_subject` in `value/kinds.rs`, invoked after the
message has passed Go-string validation. It uses **content and nilness**, not general truth:

| Subject leaf at consumption | Render outcome |
|---|---|
| Nil interface | Abort |
| String with exact empty bytes | Abort |
| String with proved nonempty bytes | Normal, consumed subject |
| String of unknown content | Split on proved empty preimage, otherwise unknown coverage |
| Bool, numbers, nonnil or typed-nil map/slice | Normal, consumed subject |
| Unknown including nil/string | Preserve all alternatives; no blanket rejection |

For raw coalesced inputs, the empty preimage may be expressed as original-input absence/null
or empty string. For transformed values use the existing sound transform preimage language;
no raw-path requirement through an arbitrary transform. Typed nil maps/arrays pass required
but are falsy and equal nil (R0 a04, d49). `set` on nil map aborts; `unset` succeeds.
Merge with a nil destination leaves the result typed-nil when there are no sources or all
sources are nil. Any nonnil source, even an empty map, allocates the returned destination;
the original nil variable stays nil (d53; sem-merge-nil). All four merge variants obey this.
Lint mode suppresses required/fail aborts, **not** signature/type errors (d32/d33);
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
| dict/list | Fresh outer structure, copy scalar slots at consumption, share map/backing references; dict coerces keys, odd last key gets `""` (d37) |
| fromYaml/fromJson/fromToml | Fresh decoded graph; unknown content has open remainder; Helm map/error outcomes retained |
| mustFromJson / array decoders | Actual selected/fixed return contracts, typed nil retained (d49); never assume map |
| pick/omit/pluck | Fresh outer map/sequence, selected nested references shared (R0 a09b, d13) |
| append/prepend and copying list family | Copy slots, share referenced maps; []string input becomes interface slots (d14/d56) |
| slice | Sprig: typed empty/nil slice returns nil interface before index coercion/bounds; nonempty slice creates a view using only first two indices, coerced by Sprig; extra indices do not set capacity (sem-slice) |
| index/range on []string | Live backing slot; copy at ordinary-call scalar consumption or snapshot boundary |
| sortAlpha / toStrings | String backing sorted through views; toStrings([]string) aliases (R0 a02, d27) |
| sortAlpha on interface sequence | Copies/stringifies slots, leaving original slots unchanged (R0 a02b) |
| set/unset | Target mutation once; scalar slots copied, nested map/backing identities shared; return unnamed-map view aliasing target; typed-nil exception above |
| merge/mustMerge | Destination wins populated conflicts; fills/recurses and can install source references (d15/d16/d66/d71/d72) |
| mergeOverwrite/mustMergeOverwrite | Later sources override; nested destination identity retained when recursively merged (d17/d67) |
| deepCopy/mustDeepCopy | Fresh per occurrence, including string backings; repeated source aliases split (R0 a09, d57) |
| unknown writer | Invalidate all possibly reachable mutable objects; preserve influences/unknown flow |

Nonempty Sprig slice preserves available capacity; a third index is ignored, not a full-slice
capacity bound. Nevertheless **all** extra arguments execute/validate first, even for empty input.
Nil interface is not a typed empty slice and errors. Index coercion/bounds and element-kind
behavior follow pinned Sprig; unsupported cases widen, never apply Go builtin slice assumptions.

Merge returns an unnamed-map view sharing the nonnil destination, including common.Values.
Its **source argument order is fixed**, but traversal within a source map is unordered. When
keys/recursive paths can interfere through aliased destination/source objects, preserve all
feasible outcomes with internal nondeterministic decisions, or widen the transitive affected
region. Never choose sorted-key execution as mergo semantics. Bounded enumeration is allowed
only when complete; exceeding the order bound widens the entire interference component.
Only a disjoint-footprint/commutativity proof permits collapsing to one outcome. A schema may
reject only if every feasible order fails. Alias uncertainty forbids that proof.
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
redefinitions in that parse order (d43). Filter **ordinary library files before parsing or
indexing definitions**; only underscore-prefixed library templates participate. All chart
partials parse but do not render directly (sem-library). An existing definition is not replaced
by a syntactically empty body (whitespace/comments only); an action that emits empty text is
nonempty syntax and does replace it. Two nonempty same-name definitions in one parse fail.
Use the same rule in tpl namespace overlays (sem-namespace); execution-time emptiness is irrelevant.
Dependency activation gates execution and exported observations, never manufacture an
alias to an inactive chart. Chart-local contract projection rebases original input identities once. A definite Abort stops the render session before later files; only normally completed
render alternatives publish output-derived placement facts. An analyzer parse limitation is a
gap/unknown, distinct from a proven Helm parse failure; neither is a successful empty helper.

Source cross-checks: [Helm 4.2.3 engine](https://github.com/helm/helm/blob/v4.2.3/pkg/engine/engine.go),
[coalescing](https://github.com/helm/helm/blob/v4.2.3/pkg/chart/common/util/coalesce.go).
The executable evidence is R0's scripts and `value-algebra-r3b-probes/review-results.txt`.

### 4.2 Helpers, output and cache

Each top-level executable file starts a fresh frame with `$` and dot equal to its captured
chart root; assignments to `$` in a previous file do not carry. Heap writes do carry. Strip
`<no value>` per file after concatenating its chunks, never across file boundaries. NOTES
files execute in the same schedule and retain writes, but have no manifest placement output.
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
    argument: ValueIdentity, heap: ReachableSnapshotId, seen: SeenKey, active: FrozenPathKey,
    policy: PolicyId, context: ContextKey, budget: BudgetKey,
}
enum ContextKey { Independent, BoundSensitive(CallContextId) }
struct CachedHelper { output: FrozenTextTerm, observations: Observations, flow: CachedFlow, charge: BudgetCharge }
enum CachedFlow { Normal, Abort(AbortReason) }
struct MemoCertificate { deterministic: Deterministic, readonly: ReadOnly, complete: Complete, no_escape: NoEscape }
```

The key retains the **full active path**, not just caller re-guarding of cached observations.
Canonicalize predicate structure and decision-edge order, preserving edge polarity, partial
coverage and frozen-test identity. Equivalent formatting may intern together; distinct unknown
predicates cannot. Hashes are accelerators, equality checks full keys. Opposite caller predicates
can simplify the same helper differently despite identical argument, heap and seen keys.
No path-neutral fast path in R3b; such a cache would require separately proved unsimplified,
relative results. Test identical argument/heap/seen under opposite paths with cache disabled,
cold and hit, asserting full output/IR/abort identity, including a partial-coverage decision.
The key also retains the **full seen set**, namespace-qualified. Use ContextKey::Independent only
when relative observations and nonescaping allocation prove caller-context independence; otherwise
include the context. BudgetKey covers remaining structural capacity read by execution. Check for recursive re-entry before cache lookup; key by the caller's seen set before insertion,
then push the callee and restore on every exit. No legacy reachable-seen reduction until
equivalence is separately proved. Mode includes strict/lint/render and all behavior-affecting settings; PolicyId covers
capabilities, file/program sources, loader coverage and structural bound configuration. An arena
is session-owned, so IDs cannot collide across sessions; no pointer addresses in serialized IR.

Memoize only with all four certificates: audited transitive determinism, read-only execution,
untruncated completion, no escaping allocations (including references in output/observations).
Unknown catalog entries default ineligible. Random/time/DNS/lookup/crypto and unsorted map
keys/values disqualify; deterministic+unknown is unknown. Mergo alias interference and
Files.AsConfig/AsSecrets basename collisions are internally unordered too; their facets default
ineligible unless an operation-specific order-independence proof is available. Sorting the
serialized result does not prove determinism of which overwritten value survived. Local allocations are permitted only
if neither mutation touches caller-reachable state nor allocations escape; conservative first
implementation treats every write facet as ineligible. No cached writer replay.

The determinism facet lives beside R2's mutation/signature facets, exhaustively propagated across
helper calls and literal tpl namespaces. It is not a blacklist of helper names. Dynamic callees,
unresolved methods and parse/budget failure disqualify. Recursion re-entry yields widened output,
may-write invalidation and a gap, never an empty successful summary; its truncation poisons all
ancestors' memo eligibility. Conservative cut results are never cached, even at the cycle head.
Completed cache hits reuse relative observations re-guarded once at the call site (ADR 0016).
Cold/hit/cache-disabled results must be identical (ADR 0028); adding the optional fast path is C08.
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
Integer range supports zero/one variable and nonpositive empty ranges; two variables fail even
at zero. Float64 input is not integer-range-compatible; Int64 is (sem-range-integer). Preserve
loader alternatives rather than treating all schema numbers as Go integers.
Unknown range: preserve the zero-iteration exit; execute a summary member through the same
body, collect transitive writes including helpers, and compute a bounded join/fixed point.
On exhaustion, widen **all may-written objects and outer bindings in the entire remaining
body/iterations**, including writes in unexecuted suffixes, plus remainders. The footprint pass
is structural dependency discovery, never a second value evaluator: follow known helper/tpl
programs and both branch arms, use aliases from State, and promote unresolved targets/callees
to the reachable render region. Do not use only writes observed in the unrolled prefix.
The 33rd-iteration-only write must invalidate a 32-step prefix fact. Unknown call footprints
widen reachable graph regions; break/continue paths cannot suppress another feasible suffix.
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

## 5. Construction branch: 10 commits, no intermediate landing

T0 (§6) lands first as test infrastructure. Compose reviewed R2/R3a, freeze the actual tree
and suite, then capture main M. Each checkpoint compiles, runs focused tests and the two-binary
gate. Ordinary admission mismatches remain named construction regressions; expectations are
not refreshed. There is no production dual-run flag, alternative engine or chart whitelist.
C02 is deliberately one coupled checkpoint: local preparatory commits may help its author,
but **none is a gate checkpoint or landable until the dependency closure below is complete**.

`value-algebra-r3b-deletions.tsv` maps 171 baseline/carry rows (165 R3b + six F2), now **165
deletions, four retained E and two retained K**, plus thirteen review-discovered deletion rows:
**184 total, 178 deletions**. `input_stage=review` distinguishes new obligations from the frozen
inventory; `edge:` keys name semantic consumer edges, not deletion of their entire projection
function. Resolve relocation records against actual R2/R3a and append explicit carries; no row
may be unassigned and the frozen classification is never rewritten to claim deletion credit.
I = byte/verdict identical; H = Helm-adjudicated closure/change; R = exact tracked regression set.

| # / planned subject | Adds and dependency-complete migration | Deleted rows (exact set is TSV) | Gate / admission lanes |
|---|---|---|---|
| C01 `refactor(ir): prepare private value runtime` | Test-only private DAG, heap/version IDs, Flow, frames, snapshots and invocation/result types with law tests; old executor still owns execution | None; no semantic store swap yet | I vs M; all lanes unchanged |
| C02 `refactor(ir): couple value execution and state ownership` | Activate sole evaluator + operand seam, basic heap transfers, frames, Flow exits, frozen joins, same-heap include/template/tpl, in-walk text and emission/serialization snapshots together; disable old memo | EvalResult parallel stores; proven_operands; strict/replayed operand evaluators; all 13 LocalSlot/RootSlot/slot stores; mutation effects/replay; DeferredScalarProjection/fresh helper interpreter/nested file interpreter/static helper walk; FragmentSummary mirrors; HelperOutputEvaluator/helper_literal_dispatch; syntax-fail producer/four edges; pair-merge recognizer/consumer; old cache/key and per-interpreter root/dot stores | Measured R seed, no promised closure count; helper mutations, aborts and snapshots all remain represented, possibly widened; all admission families measured |
| C03 `fix(ir): implement consumed operand and heap transfers` | Complete R2 signatures in real invocation order; direct literals/pipe/and/or; scalar consumption; F2/nilness; typed views, slice, copy, merge alternatives; executed-origin condition/path consumers | Six F2 E carries, residual condition/path re-derivers; R2 temporary certificates/type-test compatibility | R↓/H: arity54, literal96, required64, empty31, nil31, intrinsic60, store24, r2/twins, sem-call-copy/live-slot/map-view/slice/merge |
| C04 `fix(ir): execute the shared Helm render graph` | Whole-tree init, dependency/global alias graph, file frames/schedule, library pre-parse filter, namespace empty-definition rules and NOTES effects | static_root_fields initialization representation | R↓/H: dependency-null, Airflow, sem-file-boundary/library/namespace; existing d29–d44/d61 controls |
| C05 `fix(ir): preserve loop outcomes through widening` | Precise range entry snapshots, integer range, bounded summaries; remaining-body may-write footprints; snapshot-safe allocation collapse | None; old control/header/range carriers removed atomically in C02 | R↓/H: header10, guarded-writes, reviewer42, supplement3, sem-range-integer/loop-bounds/summary-loop |
| C06 `fix(ir): project executed output and provenance` | Complete text/YAML placement, current-origin/member/default transforms and resource selectors from frozen executed records | Source-derived resource selector/guard/capability producers and two consumer edges | R↓/H: frozen614, comparisons, output snapshots, coalesce emitters; six local_default_join controls; sem-output-boundary |
| C07 `refactor(ir): seal bounded source summary plans` | Complete certificates/transfers for four typed plans already wired through completion; preserve recursive helper behavior via execution/approved plans | None; pair-merge recognizer/consumer removed in C02; no deletion credit for four E/two K relocations | R↓/H: Airflow/Bitnami/NATS; renamed positives, near misses and over-cap negatives |
| C08 `perf(ir): memoize certified helper executions` | Determinism facet, full path/seen/snapshot/namespace keys, bounded no-escape read-only complete memo | None; old cache removed in C02 | I vs C07, R unchanged; cold/hit/disabled full artifacts, sem-cache controls |
| C09 `refactor(ir): remove legacy value adapters` | Finish lowering consumers to opaque executed views and seal Rule 1 everywhere | All 19 AbstractValue variants, SelectionChainIdentities, ScalarValueDispatch arms; every temporary adapter | I vs C08; all lanes; AST ownership/inventory/privacy audit |
| C10 `fix(ir): close value algebra admission obligations` | Resolve remaining tracked losses, adjudicate every semantic change, freeze bounds and final evidence | Any reconciled R3a carries/temporary rows must already have a named earlier owner; none may remain | R=∅, strict H>0; all agreed R3b lanes; complete ordinary landing gates |

C01 places unused foundations under private test support until C02 wires production callers;
no dead-code lint suppression or unused production evaluator is introduced.
C02 supplies working *conservative* versions of every owner before deleting predecessors: helper
bodies run on the caller heap; guards and serialization freeze at their actual boundary; all four
Flow exits propagate; bindings live in frames. Unsupported transfers widen with read/write/abort
coverage. Basic control may widen a loop using its whole-body footprint until C05 adds precision.
It must not call the old helper interpreter, replay a transported write or defer emission while
waiting for C03–C07. Later commits improve precision; they do not supply missing owners. The completion seam and
four typed-plan interfaces are wired in C02; a not-yet-certified plan declines to normal execution.
No legacy recognizer may produce runtime values while awaiting C07. At this point static_root_fields
is initialization-only; all live roots/dot are State-owned even before C04 joins render sessions.

C02 also converts every invocation to owned operands. It may initially retain R2-compatible
transfer cases in `value/operations.rs`, but no transfer can evaluate syntax. The old AbstractValue
survives through C08 only as a one-way projection adapter from canonical Value/site records,
with no writes, constructors outside that adapter, query feedback or semantic cache. New temporary
rows are inventoried with a C09 expiry. The supplemental AST-claim producers surviving to C06
are explicit construction debt, not permitted final boundaries. The gate makes their effects
visible; final Rule 1 has no exception for them.

### 5.1 The 122 operand sites and final deletion proof

R0's 122 is a frozen estimate, not a stopping count. A current textual audit finds **123 call
lines** excluding declarations/tests: expr_call_eval mod31/serialization24/collections21/
strict5/root5/comparisons4/traversal4; condition_predicate12/path_resolution2; expr_eval9;
fragment_expr_eval4; assignments1; resource_identity1. R3a removes some before R3b starts.
T0 records actual parsed call expressions with qualified enclosing function, source span and
callee; text counts are only a cross-check (multiline calls and definitions are distinguished).

C02 classifies every site: (a) legitimate evaluator recursion, moves to eval/invoke/control;
(b) transfer/contract consumer, replaced by an Operand view; (c) demand replay, deleted in
C02/C03/C06; (d) source-only path discovery, parsed structurally or reads an executed
origin, never executes with `EvalEnv::default()`. Each row names its checkpoint and post-move
callable. An AST inventory test rejects TemplateExpr in transfer/collector entry signatures,
and evaluator calls outside the sole executor. A new method/name cannot escape by renaming
`eval_expr`: the rule enumerates calls to the evaluator API and semantic type closure.

The audit also enumerates **semantic claim producers that never call eval_expr**. Delete
`expr_contains_fail_call` and its four uses in eval_output_action, eval_hole_parts,
eval_suppressed_fragment_hole and inline_child_arms: only executed Flow::Abort is termination.
Unreached fail in and/or or a failed earlier argument is not a terminal tooth. In resource_identity,
delete kind_equality_selector, values_path_literal_comparison, collect_kind_partition_literals,
branch_condition_header and capability_ternary_body as semantic producers; replace collect_if_region's
selector/guard and collect_window_spans' partition consumer edges with executed frozen records.
CST placement/span discovery survives; it cannot infer a selector from a path since overwritten,
a branch never reached, or a different dot frame. New supplemental `edge:` rows carry caller/callee
identities and original source spans in the T0 AST manifest; matching by line number alone is invalid.

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

## 6. T0 differential tool (independently landable prerequisite)

The normative, standalone builder contract is
[value-algebra-r3b-differential-tool.md](value-algebra-r3b-differential-tool.md).
T0 extracts a policy-free comparison core into test-support: **two manifest-verified dumps**,
not the ordinary battery's committed-fixture baseline. The final battery retains its own
roster/baseline policy. Both modes share discovery, composition, screening and adjudication.
The context roster, exact input bytes, modes, Helm/build/provider identities and coverage are
explicit immutable inputs. CLI schema, producer artifacts and companion ContractDocument
exports are captured from each revision; schema/IR/output changes all need linked evidence.

Every discovered witness enters a persistent union, independently replayed outside fresh-probe
caps. Versioned receipts bind their predecessor, baseline, suite, tool, universe and checkpoint;
new discoveries replay against retained history. Set inclusion governs regressions and earned
closures, including corpus and IR/output witnesses. C02 alone may establish the reviewed seed;
C01 is a main-identical prerequisite. An ordinary comparison cannot reset history or seed again.
Final landing requires an empty regression set, strict semantic progress and the ordinary gates.
See the standalone spec for APIs, command grammar, formats, exit codes and builder acceptance tests.

## 7. Helper-shape exceptions and admission ownership

Accept the review's four bounded E exceptions; **delete classify_pair_merge_helper**. Its current
shape discards overwrite-key policy and feeds a legacy consumer; retaining it as proposed is
rejected. C02 removes its `fragment_expr_eval/bound_helper_resolver.rs::custom_pair_merge_call` consumer
and CustomMergeHelper::Pair variant too; any loss enters the measured seed, not a late waiver. Execute that body through the
sole evaluator, conservatively widening recursion. If the Airflow/pair-merge controls regress,
landing blocks: do not silently reinstate the recognizer or drop the controls. A future exact
pair plan would need a separate reviewed transfer carrying its full overwrite-key policy.

| Retained E recognizer | Obligation | Typed plan |
|---|---|---|
| program_wrapper_sentinels | r3b-shape-program-wrapper | ProgramWrapperPlan with recognized wrapper/program relation |
| classify_nil_scrub_helper | r3b-shape-nil-scrub | NilScrubPlan with recursive field-preservation/removal rule |
| classify_parsed_map_list_merge_helper | r3b-shape-parsed-map-list | ParsedMapListMergePlan with ordered render/decode/merge composition |
| classify_typed_yaml_renderer | r3b-shape-typed-yaml-renderer | TypedYamlPlan with structural source/render relation |

```rust
enum SourceSummaryPlan {
    ProgramWrapper(ProgramWrapperPlan), NilScrub(NilScrubPlan),
    ParsedMapListMerge(ParsedMapListMergePlan), TypedYaml(TypedYamlPlan),
}
struct CertifiedSourcePlan { program: ProgramId, namespace: NamespaceId, proof: ShapeProof, plan: SourceSummaryPlan }
enum CompletionInput {
    Executed(Execution),
    Summarized { plan: CertifiedSourcePlan, operands: Vec<Operand>, state: State },
}
fn complete(input: CompletionInput) -> CallOutcome;
```

Each private plan contains typed operand roles, transfer policy and completeness/applicability
proof, never an already-constructed Value, Predicate or raw source string to execute later.
`shape_summary` can construct plans only. `value/invoke::complete` is the **sole result-construction
seam** for ordinary transfers, executed helper bodies and certified summaries; it applies a plan
to evaluated operands and current State, using the same private operations/queries and freezing
rules. Completion publishes one CallOutcome; it never returns a value alongside a parallel abort
flag or an effect record to replay. Already-executed exits are consumed without reevaluation. A plan may substitute for a body
only with an audited exact, effect-preserving transfer (or audited read-only transfer). Otherwise
execute normally; the recognizer may report discovery metadata but cannot enrich values,
constraints, aborts, output provenance or truth afterward. Unknown is not permission to assert a
recognizer hint. Unknown tpl writes, namespace changes and Flow remain owned by the executor.

Use namespace/program-digest-keyed classification caches (two retained K rows); a classification
hit is a plan, never a cached runtime result. Family cap 16 and action cap 128 per body remain;
partial family discovery/over-cap returns no plan. Parse structurally; reject extra branches,
writers/output beyond the certified shape. No renamed-helper assumption and no opportunistic
recognizer strengthening. Move all four together under `value/shape_summary.rs`.

T0/C07 tests include full-schema Airflow/Bitnami/NATS controls, renamed minimal positives, one-action
near misses, over-cap inputs, actual side effects and plan-vs-executed completion equivalence on
bounded cases. Inventory records retained E/K, never deletion credit for relocation. If execution
fully replaces a recognizer, delete it and its unused cache; expanding the approved set requires
review. An unproved transfer must decline, and any resulting required-control loss blocks landing.

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

Measurement checkpoints: baseline/C01 (inert footprint); C02 (coupled runtime/removing replays);
C03 (precise transfers/reachability); C04 (whole-tree schedule); C05 (summary allocation/loops);
C06/C07 (output/typed plans); C08 (full key/cache); C10 final.
Use the ADR reference charts plus Bitnami fan-out (Airflow/common.tplvalues and a second common
library consumer), NATS, Otel, ingress-nginx and the admission suite. Record sizes/inputs,
allocation sites, maximum live heap versions, weak updates, widened regions, output terms,
helper invocations, cache eligibility/miss reasons, reachable/path-key sizes and seen-set sizes.
Count page-index entries copied per fork, object fields/bytes copied per write, total bytes pinned
by frozen versions/cache entries, and peak simultaneously retained snapshots. Report these per
chart and per helper allocation site; COW sharing does not make fork or field-map cloning free.
Add `tracing::instrument` spans to fork/join, allocation, invocation, helper entry, reachable
snapshot construction, serialization and widening; use the existing dev Perfetto layer.
Traced runs explain phase proportions only; no custom timers or traced timing comparisons.

Snapshot identity is version-sensitive: a write creates a new ObjectRevision; branch joins
include every reachable changed descendant. Initial safe key uses whole heap snapshot ID.
C08 may intern a canonical sorted vector of `(ObjectId, ObjectRevision)` for reachable args/
ambient host inputs as ReachableSnapshotId, using structural equality to resolve hash collisions.
Compute graph reachability cycle-safely; unresolved aliases use the whole reachable-region epoch.
Memoize reachability per immutable version. An unrelated allocation can then leave a helper's
reachable ID unchanged. Do not hash/clone the entire heap at every query, ignore a nested write,
or share keys for structurally equal but differently aliased argument graphs.

Per-site summaries bound **future** recursive/loop allocations while prior exact IDs stay pinned
by snapshots. COW avoids copying every object on fork, but the page index still costs O(pages);
delta joins avoid rescanning unchanged object contents. Measure both, including large maps.
Release unreferenced snapshots/cache entries at session boundaries; use an
entry/byte-limited LRU if measured memory requires one, eviction changes cost only. No escaping
allocation cache means fresh-dict Bitnami calls may miss often. Measure this loss against saved
replays before proposing further eligibility. Do not omit `seen` to restore an old hit rate.

Final decision uses at least seven interleaved A/B pairs on a recorded quiet/loaded host,
median/min–max wall and CPU, peak RSS and output/schema sizes. A paired range crossing zero is
not a gain. Report per-chart and aggregate, including worst outlier; no average hiding fan-out.
Any material slowdown needs an attributed reviewer disposition before landing; tolerance is Q1.

### 8.2 Region budgets (production Rust code lines)

Measured here: `task tokei:core` exit 0, **77,053 Rust code lines** on 0a11d18b; no production
edits. The R0 sizes were physical lines, not deletion credits. Current region code counts below
come from tokei JSON; final budgets must be rebased after actual R2/R3a, with no double credit.

| Region | Current code | Added ceiling/range | Deleted target |
|---|---:|---:|---:|
| Algebra/queries + abstract_value.rs | 1,581 | 250–400 | 900–1,300 |
| Heap/allocation/copy/opaque reachability | new | 1,050–1,550 | 0 |
| State/frames: value/* + symbolic_local_state + eval_env | 1,512 | 180–300 | 800–1,100 |
| Invocation/operands + provenance migration | distributed | 400–650 | 250–450 |
| Snapshots/output: summary.rs | 1,089 | 250–400 | 500–750 |
| Flow/loops: control.rs | 2,800 | 350–550 | 300–550 |
| Init/session integration, including helm-schema collection | new | 250–400 | 0–100 |
| Cache/path keys/namespaces + typed shape plans | analysis_db 1,567 | 230–400 | 180–300 |
| Mutation/replay: root_mutation + assignments | 1,086 | 0–80 | 400–600 |
| F2 hole_effects + condition_predicate | 3,663 | 120–220 | 500–950 |
| HelperOutputEvaluator/helper_literal/files/static requests | 1,549 combined files | 0 | 500–800 |
| **Total affected implementation** | overlapping files counted once above | **3,080–4,950** | **4,330–6,900** |

Expected net **−3,820…+620**; +620 is a review ceiling, not permission for growth. R0 additions
were 2,130–3,700: the increase explicitly budgets real frames, opaque reachability, whole-render
session integration, path-complete cache keys, version-local widening and typed recognizer plans.
The revision adds 100–200 lines to the planned range and deletes the rejected pair recognizer;
these are budgets, not measured implementation savings. R1b residuals give no advance deletion credit.
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
exit 0. Original receipt: **81/81 checks**; scripts and the small engine probe source are included.
Review additions: `semantics.sh > semantics-results.txt`, **120/120 checks pass**, using the same
full-input/command/exit format and pinned binary. Combined rerun: **201/201**. Unordered merge
and Files collision samples accept only the specified outcome set; the receipt records both
outcomes observed, but never requires random coverage or claims sampling proves exhaustiveness.
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

Review admission/control families (concrete Helm grounds run now; cache/widening/IR assertions
run by the builder when that runtime exists, not claimed tested by the Helm CLI):

| Family in semantics.sh | Recorded outcome / required builder cells |
|---|---|
| sem-call-copy / sem-live-slot | Ordinary selectors/stores keep consumed scalars (`b`/last `a`); builtin index/and/or stay live `a`; later argument mutation consumed as `a`; five false-rejection guards render |
| sem-map-view | set/unset/merge return unnamed maps aliasing Values; default keeps common.Values; also preserve shallow nested map aliases |
| sem-slice | Empty/typed-nil slice → nil before bounds/coercion; nil interface errors; ignored extra index still executes/aborts; third index does not cap reslicing |
| sem-merge-nil | All four variants: none/nil sources keep nil; empty/populated nonnil source allocates; original nil stays nil |
| sem-merge-order | Aliased destinations observe 1 and 2; guarded chart both renders and aborts across fresh runs; overlapping source/destination control; add cache invariance, never freeze one sampled class |
| sem-namespace | File/tpl whitespace/comment definitions keep old body; runtime-empty replaces; duplicate nonempty errors; nested clones give inner/outer/old |
| sem-library | Invalid ordinary file ignored; invalid underscore partial errors; ordinary-file definition unavailable |
| sem-file-boundary | Fresh file `$`/dot; shared heap writes persist; captured child root survives parent-field replacement |
| sem-range-integer | Nonpositive empty, zero/one variable valid, two invalid even at zero; Float64 invalid vs Int64 valid; add document-loader alternative cells |
| sem-loop-bounds | 31/32/33 final-only writes all yield new; analyzer cap−1/cap/cap+1 must not reject valid 33rd-write witness or keep stale prefix facts |
| sem-summary-loop | 0/1/3 members, helper writes, break/continue, final reassign and later singleton overwrite; analyze as unknown-length inputs; test weak summaries and preserved zero exit |
| sem-output-boundary | Marker concatenation within file, isolation between files, discarded include effects and NOTES writes before a later file; no NOTES manifest placement |
| sem-cache | Opposite caller predicates render/abort as expected; AsConfig/AsSecrets basename collisions both observed; add full cold/hit/disabled results with same arg/heap/seen and distinct path/namespace/budget |
| sem-source-fail / sem-source-selector | Short-circuited fail does not terminate; selector after write uses current value; add guarded resource-kind/IR observations and renamed controls |

No production interpreter is implemented here: summary collapse/snapshot retention, structural cap
neighbors, cache states and full-schema expectations remain explicit C02–C08 tests. T0 registers
these obligations before the construction seed; finite concrete probes do not prove abstract joins.

R0 evidence remains binding for unknown-map remainders, grouped lookup, required typed-nil map,
short-circuit pipes, include/tpl namespace and output differences, random helpers and selectors.
Proposed analysis caps/cache eligibility are conservative design choices, not claimed Helm facts.
Do not infer unprobed exact transfers for unsupported functions: widen and preserve reads/writes.

Top risks, in order:
1. **Whole-tree state meets old per-chart analysis.** Cross-file/root writes require the collection
   integration in C04; an IR-only patch that leaves old isolated sessions is incomplete.
2. **Live references and snapshot boundaries.** Eager operand validation does not copy slots;
   cache/output keys that use later heap state can recreate the original stale-fact bug.
3. **Provenance, unordered merges and recursive helpers.** Dropped merge orders or unproved
   summary plans can tighten; deleting the pair recognizer may expose Airflow precision losses; full schemas, partial-polarity joins and universal failure proofs are mandatory.
4. **Construction masking/large first seed.** C02 couples the runtime; cap-independent witness
   replay and chained receipts must preserve earned closures and full artifacts; a shrinking count or a refreshed-main table can hide a new failure.
5. **Cache fan-out and summary allocations.** Correct seen/snapshot keys and no escaping allocation
   may lose Bitnami hit rate; bounded weak updates can also widen too much. Measure both precision
   and cost; no cap reduction or stale helper caching to make numbers look better.

User question (not a blocker for this design):
- **Q1:** what measured semantic slowdown/RSS increase is acceptable for required closures?
  Decide from C02/C05/C08 paired results; no fixed percentage is pre-approved. Cache correctness,
  zero new failures and agreed closures are not tradeable. The four recognizer exceptions are
  resolved by the review's typed-seam conditions; the rejected pair recognizer is not an open vote.

Final cut-over: inventory/self-tests/privacy/operand ownership pass; all scheduled deletions and
approved exception mappings survive; differential R empty and verified closures >0; one clean
final dump; every acceptance flip Helm-adjudicated; admission refresh only now, shrink-only;
`cargo fmt --check`, `task lint`, `task lint:fc`, `cargo nextest run --workspace`,
`task test:integration`, `task test:all`, CLI install + downstream luup2 gate, `task tokei:core`,
and sealed landing-chain receipt each reported independently. No fixture or code edits after
those results without rerunning the stale gates. Construction commits do not individually land.
This design task ran probes and LOC measurement only; it does not claim implementation gates.
