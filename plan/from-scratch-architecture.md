# helm-schema — from-scratch target architecture

Status: design document. No code change is implied by this file by itself.

This is the architecture I would build if helm-schema were rewritten today from
first principles, knowing everything the current implementation has learned the
hard way. It is deliberately written as a *clean-room* design: it describes the
ideal decomposition, the domain model, the trait (port) boundaries, and the
reasoning behind each decision — not a refactoring schedule.

Relationship to the other plan documents:

- `single-abstract-interpreter.md` is the active migration track for the
  analysis core. This document **absorbs that design unchanged** as the heart
  of the semantics layer and extends the same thinking to the whole system:
  chart acquisition, parsing, knowledge lookup, schema synthesis, emission,
  diagnostics, and testing.
- `next-priorities.md` correctly warns against broad trait-heavy redesign
  before stable boundaries are proven. This document does not contradict that:
  it is the *north star* the targeted cleanups should converge on, so that each
  incremental step lands on a boundary that survives. A migration
  correspondence table is included at the end, but sequencing remains owned by
  `next-priorities.md`.

---

## 1. The problem, stated from first principles

helm-schema answers one question:

> Given a Helm chart, what is the most precise, structurally justified
> JSON Schema for its `values.yaml` contract?

Decomposing that question reveals the natural domains of the system. Each
domain has a distinct *kind* of knowledge, a distinct failure mode, and a
distinct rate of change — which is exactly the criterion for drawing module
boundaries:

| Domain | Question it answers | Kind of knowledge | Changes when… |
|---|---|---|---|
| **Chart model** | What is this chart made of? | Filesystem / packaging conventions | Helm packaging evolves (OCI, tgz, deps) |
| **Syntax** | What does this template text *say*? | Grammar | Go-template / YAML syntax evolves (rarely) |
| **Semantics** | What does this template *mean*? | Helm evaluation semantics | Sprig/Helm function semantics evolve |
| **Knowledge** | What does Kubernetes expect at this field? | External schema corpora | K8s releases, CRD catalogs, mirrors |
| **Synthesis** | Given all evidence, what is the contract? | Decision policy / schema algebra | Our inference policy improves |
| **Emission** | How do we write it down? | JSON Schema dialects, output transforms | Schema drafts, consumer needs |

Two cross-cutting concerns thread through all of them:

- **Provenance** — every fact must be traceable to the source span and helper
  chain that produced it, because the project's bar is "diagnosable, never a
  silent guess".
- **Ambiguity** — "unknown" and "several alternatives" are first-class values
  at every layer, never collapsed early. Precision-first means abstention must
  be representable everywhere, from a tri-state cache lookup to a `Union`
  abstract value to an `anyOf` schema node.

The current implementation contains all six domains, but their boundaries are
smeared: semantics is split across ~10 parallel evaluators, synthesis re-derives
semantics from a lossy IR, syntax is parsed three times and loses spans, and
library logic lives in the CLI crate. The architecture below gives each domain
one home, one vocabulary, and one owner.

## 2. What the current implementation teaches us

A detailed review of the codebase (≈29K LOC core) surfaces recurring structural
problems. They are listed here not as criticism — the system is *correct* and
well-tested — but because each one is an input to the design. A from-scratch
architecture is only "best" relative to the failure modes it provably removes.

### 2.1 Semantics is implemented N times

The IR crate contains at least six overlapping evaluators over the same
semantic domain ("what value does this expression denote, under these
bindings?"):

- `expr_eval::eval_expr` → `AbstractValue`
- `helper_binding_eval::binding_from_expr` → `HelperBinding`
- `fragment_expr_eval::fragment_binding_from_expr` → `FragmentBinding`
- `fragment_binding_eval::fragment_binding_from_outer_expr` → `FragmentBinding`
- `helper_eval::helper_evaluate` → a fourth, literal-only evaluator used solely
  for apiVersion helpers (1 480 lines)
- the chart-facts walker in `abstract_eval.rs`

with **three parallel value lattices** (`AbstractValue`, `HelperBinding`,
`FragmentBinding`) that each define `paths()`, `choice()`, `apply_to_path()`,
and lossy conversions between them (`HelperBinding::OutputSet` carries
metadata; `FragmentBinding::OutputSet` drops it). Helper bodies are walked
**twice** by near-identical traversals (`helper_value_analysis` vs
`helper_fragment_output_uses`). Every new Helm semantic (a `with … | default`
pattern, a `set` mutation, a `toYaml` fragment) must be taught to several of
these at once; forgetting one is the documented source of repeated bug classes.

### 2.2 Syntax is lossy, stringly, and parsed three times

`HelmAst` stores control-flow conditions and range headers as **raw strings**
(`If { cond: String }`), and template actions as opaque text
(`HelmExpr { text }`), forcing every downstream consumer to re-parse via
`parse_action_expressions` (mitigated by a thread-local cache). One file is
parsed by the go-template grammar, its YAML fragments re-parsed by the fused
grammar, and each action re-parsed again for expressions. **No source spans
survive** into `HelmAst`, so no diagnostic can point at a file/line, and the
walker must re-derive positions through a separate, indentation-heuristic YAML
shape tracker (`yaml_shape.rs`) — a line-shape heuristic at the heart of a
system whose charter says "parsers over string heuristics".

### 2.3 The IR is lossy, so the generator re-derives semantics

`ValueUse { source_expr: String, path, kind, guards, resource }` is the entire
hand-off between analysis and synthesis. Falsey-state admission, open-object
facts, fragment provenance, and mutation effects do not survive the hand-off,
so `helm-schema-gen` reconstructs Helm semantics from the outside:
`build_root_schema` is a 134-line decision loop juggling six signal sources
with 15+ interleaved special cases, and precedence rules live as inline `if`
chains in `resolve_schema_for_value_path` rather than as a policy. The active
`single-abstract-interpreter.md` plan (Phase 6) already identifies this.

### 2.4 Schema synthesis is stringly typed

Every schema in `helm-schema-gen` and `merge.rs` is a raw `serde_json::Value`:
construction is `Map::insert("type", …)`, inspection is
`obj.get("additionalProperties").and_then(Value::as_object)`. There are three
different definitions of "is this schema scalar?", and the empty object `{}`
means three different things depending on context (identity, "unknown object",
exact-empty marker). The merge semantics (union `required`, intersect `enum`,
structured-beats-map-like) are correct but exist only as code paths — there is
no algebra that can be property-tested.

### 2.5 The knowledge layer is the best part — and still monolithic

`helm-schema-k8s` already has real ports: `K8sSchemaProvider`, `HttpFetcher`,
`DiagnosticSink`, a tri-state capability oracle with a hard-won
cache-is-not-an-oracle contract. But each provider is a 400–900 line monolith
mixing configuration, fetching, two cache layers, `$ref` resolution,
capability probing, and inference; `write_atomic` and layout checks are
duplicated across providers; and the `Chain` both composes providers *and*
implements the capability oracle, so branch selection can't be tested without
a full chain.

### 2.6 Orchestration and library logic are fused

Chart discovery, values composition (the two-pass global hoist/mirror),
schema overrides, `$ref` flattening, and required-inference glue all live in
`helm-schema-cli`, so nothing can consume them without the CLI crate. The
9-stage pipeline exists only implicitly as the body of `run_inner()`.

### 2.7 What is already right (and must be preserved)

- The **tri-state capability oracle** and its offline-safety contract.
- **Typed branch preservation** for `Capabilities.APIVersions.Has` chains
  (`ResourceRef::api_version_branches`) — exactly the "preserve ambiguity"
  principle in action.
- The `json-schema-minify` crate: self-contained, Helm-free, single concern.
- Full-schema-equality integration tests over real charts.
- The in-flight `AbstractValue`/`Effects`/`eval_expr` unification.

---

## 3. Design principles → architectural commitments

The project charter (structural analysis first; preserve ambiguity; cache is
never an oracle; everything diagnosable) translates into five concrete
commitments that shape everything below:

1. **One semantic model.** Helm evaluation semantics exist in exactly one
   place: a single abstract interpreter (`eval_expr` / `eval_node`) over one
   value lattice. Resource identity, helper outputs, guard extraction, default
   tracking, fragment analysis — all are *projections of the same
   interpretation*, never separate walkers.

2. **Typed and spanned everywhere.** Paths, coordinates, guards, schemas are
   domain types, not strings or raw JSON. Every syntax node and every derived
   fact carries a `Provenance`. Stringly representations may exist only at
   serialization edges.

3. **Abstention is a value.** Lookups return `Found / Absent / Unknown`,
   abstract values include `Unknown` and `Union`, sink attribution includes
   `Abstained`. No layer is allowed to turn "I don't know" into a guess; only
   the synthesis policy layer may *widen*, and it must emit a diagnostic when
   it does.

4. **Ports at variation points, functions everywhere else.** Traits exist
   exactly where the system meets something genuinely replaceable: the
   filesystem/packaging, the parser backend, remote schema corpora, caches,
   the network, output dialects, and the diagnostics channel. The pure
   middle — interpretation, synthesis, merging — is plain data and functions.
   This is "ports and adapters", applied with restraint: a hexagon, not trait
   soup. (Pure logic behind a trait would add indirection without a second
   implementation ever existing.)

5. **Policy is data, mechanics are code.** Anything that encodes a *choice*
   (evidence precedence, widening rules, well-known-kind probe table, version
   fallback windows) is an explicit, inspectable value handed to a generic
   mechanism — so choices can be seen, tested, and explained in one place.

---

## 4. System overview

### 4.1 Crate graph

```
                       ┌────────────────────┐
                       │  helm-schema-core   │  paths, spans, provenance,
                       │  (pure vocabulary)  │  diagnostics, tri-state Lookup
                       └─────────┬──────────┘
          ┌──────────────┬───────┴────────┬─────────────────┐
          ▼              ▼                ▼                 ▼
 ┌────────────────┐ ┌──────────────┐ ┌──────────────┐ ┌───────────────┐
 │ helm-schema-    │ │ helm-schema- │ │ helm-schema- │ │ helm-schema-  │
 │ syntax          │ │ chart        │ │ knowledge    │ │ values        │
 │ grammar+parser  │ │ chart model  │ │ K8s/CRD      │ │ values.yaml   │
 │ → spanned tree  │ │ + sources    │ │ catalogs     │ │ model + docs  │
 └───────┬────────┘ └──────┬───────┘ └──────┬───────┘ └──────┬────────┘
         ▼                 │                │                │
 ┌────────────────┐        │                │                │
 │ helm-schema-    │        │                │                │
 │ semantics       │◄───────┘ (templates)    │                │
 │ abstract        │                         │                │
 │ interpreter     │                         │                │
 └───────┬────────┘                          │                │
         ▼  ChartAnalysis (facts)            │                │
 ┌─────────────────────────────────────────┐ │                │
 │ helm-schema-synthesis                    │◄┘ (impl of its   │
 │ evidence → typed Schema                  │◄── lookup port) ─┘
 └───────┬─────────────────────────────────┘
         ▼  Schema (typed)
 ┌────────────────┐     ┌─────────────────────┐
 │ helm-schema-    │     │ json-schema-minify   │ (unchanged, Helm-free)
 │ emit            │────►│                      │
 │ draft-07 +      │     └─────────────────────┘
 │ transforms      │
 └───────┬────────┘
         ▼
 ┌────────────────┐      ┌──────────────────┐
 │ helm-schema     │      │ helm-schema-cli   │ thin: clap → config →
 │ (facade lib)    │◄─────│                  │ facade → stdout/file
 └────────────────┘      └──────────────────┘
```

Dependency rules (enforced by `Cargo.toml`, checkable in CI):

- `core` depends on nothing in the workspace.
- `semantics` depends only on `core` + `syntax`. **It performs no I/O and
  holds no `dyn` dependency** — it is a pure function from parsed chart to
  facts. This is what makes the heart of the system trivially testable and
  parallelizable.
- `synthesis` depends on `core` (+ fact types from `semantics`) and *defines*
  the lookup port it needs; `knowledge` implements that port. Dependency
  points inward (hexagonal): the domain owns the interface, the adapter crate
  satisfies it.
- `emit` and `chart` are leaf adapters around the pure middle.
- The facade is the only crate that knows every concrete adapter; the CLI is
  the only crate that knows `clap`.

### 4.2 Data flow (one pipeline, explicit types between stages)

```
ChartSource ──► ChartSet ──┬─► per template: parse ──► TemplateTree (spanned, typed)
                           │
                           └─► ValuesModel (composed defaults + descriptions)

TemplateTree* + DefineIndex ──► interpret (eval_node/eval_expr)
                              ──► ChartAnalysis {
                                     facts: Vec<Fact>,          // provenance-carrying
                                     resources: Vec<ResourceIdentity>,
                                     helper_summaries,           // memoized
                                  }

ChartAnalysis + ValuesModel + dyn ResourceSchemaOracle
        ──► synthesize ──► Schema (typed) + Vec<Diagnostic>

Schema ──► emit draft-07 ──► SchemaTransform pipeline
            (override merge → ref flatten → required pass → strip → minify)
        ──► bytes
```

Every arrow is a named type. The pipeline is a value (a struct of stages), not
a function body — so tests, benchmarks, and future tools (LSP, `--explain`)
can run any prefix of it.

---

## 5. The crates in detail

### 5.1 `helm-schema-core` — the shared vocabulary

Zero-I/O domain types used by everyone. Small on purpose; nothing here should
ever need a second implementation, so there are no traits except the
diagnostics sink.

```rust
/// A path into the chart's values space (`.Values.…`).
/// Typed segments end the dotted-string/Vec<String> duality and make
/// wildcards impossible to confuse with literal keys.
pub struct ValuePath(SmallVec<[ValueSeg; 4]>);

pub enum ValueSeg {
    Key(Interned<str>),
    AnyItem,        // an element of a ranged sequence
    AnyKey,         // a key of an open map range
}

/// A path into a rendered manifest document (today's `YamlPath`).
pub struct DocPath(SmallVec<[DocSeg; 6]>);

pub enum DocSeg { Key(Interned<str>), Item(usize), AnyItem }

/// Where a fact came from: file, byte span, and the include chain that
/// was active. This is what makes every inference explainable.
pub struct Provenance {
    pub span: Span,                       // FileId + byte range
    pub via: SmallVec<[HelperFrame; 2]>,  // include/template call chain
}

/// Tri-state result for any lookup against an external corpus.
/// `Absent` is *authoritative* absence (e.g. confirmed upstream 404);
/// `Unknown` is abstention (offline miss, no negative record).
/// Encoding the cache-is-not-an-oracle contract in the type system
/// makes the round-7/8/10 bug class unrepresentable.
pub enum Lookup<T> { Found(T), Absent, Unknown }

/// Helm truthiness: which "empty" states a value may take while the
/// chart still behaves (because `default`/`with`/`if` handle them).
pub struct FalseySet {
    pub null: bool,
    pub empty_string: bool,
    pub empty_object: bool,
    pub empty_array: bool,
    pub boolean_false: bool,
    pub numeric_zero: bool,
}

/// Structured, deduplicating diagnostics channel (grown from the
/// existing helm-schema-k8s sink, with span support added).
pub trait DiagnosticSink: Send + Sync {
    fn emit(&self, d: Diagnostic);
}
```

Interning: `Interned<str>` is backed by a per-session interner owned by the
pipeline (no global state), because path segments dominate allocations in the
current profile and `BTreeSet<String>` churn is the main RSS driver after
minification.

**What this eliminates:** the stringly `path: String` / `Vec<String>` duality
(§2.1), guards matched by string equality, the `"*"` / `"__any__"` sentinel
segments in gen, and positionless diagnostics.

### 5.2 `helm-schema-syntax` — parse once, keep everything

Owns the tree-sitter grammars (the existing `helm-schema-template-grammar`
crate folds in here) and produces the **single** syntax artifact everything
else consumes:

```rust
/// Port: the only place a parser backend is pluggable.
pub trait TemplateParser {
    fn parse(&self, file: FileId, src: &str) -> Result<TemplateTree, ParseError>;
}

/// Fully-typed, spanned, lossless-enough tree fusing YAML structure
/// and template structure. Conditions and headers are parsed
/// expressions, not strings. Every node has a span.
pub enum TemplateNode {
    Document { items: Vec<TemplateNode>, span: Span },
    Mapping  { entries: Vec<MappingEntry>, span: Span },
    Sequence { items: Vec<TemplateNode>, span: Span },
    Scalar   { text: Interned<str>, style: ScalarStyle, span: Span },
    Action   { expr: Expr, trim: TrimMode, span: Span },          // {{ … }}
    If       { arms: Vec<(Expr, Vec<TemplateNode>)>,
               else_arm: Option<Vec<TemplateNode>>, span: Span },
    With     { header: Expr, body: Vec<TemplateNode>,
               else_arm: Option<Vec<TemplateNode>>, span: Span },
    Range    { binding: RangeBinding, header: Expr,
               body: Vec<TemplateNode>,
               else_arm: Option<Vec<TemplateNode>>, span: Span },
    Define   { name: Interned<str>, body: Vec<TemplateNode>, span: Span },
    Comment  { text: String, span: Span },
}

pub struct RangeBinding { pub key: Option<Name>, pub value: Option<Name> }
```

`Expr` is today's `TemplateExpr` (which is already good), with spans attached
and `Unknown(String)` retained as the explicit "syntax we don't model" escape
hatch.

Construction detail (an adapter concern, invisible behind the port): the
go-template grammar parses the action/control skeleton and the fused grammar
parses YAML-with-actions; the builder fuses them **once per file** into
`TemplateTree`. The contract upward is what matters:

- one parse per file, ever — no downstream re-parsing, no expression caches;
- control-flow headers arrive parsed (`Expr`), killing the §2.2 re-parse class;
- spans survive, enabling provenance and real error messages;
- where the source is genuinely unparseable, nodes degrade to explicit
  `Unknown`/error nodes with spans — never silently dropped.

`values_comments` (the values.yaml description extractor) moves to
`helm-schema-values` (§5.4) since it parses values files, not templates.

**What this eliminates:** triple parsing, `template_expr_cache`,
stringly `cond`/`header`, spanless diagnostics — and, most importantly, it
removes the *need* for `yaml_shape.rs`'s indent-heuristic shape tracking,
because YAML structure around each action is now in the tree (see §5.5 on
sink attribution for the honest caveats).

### 5.3 `helm-schema-chart` — the chart object model

What a chart *is*, divorced from where it came from:

```rust
/// Port: chart acquisition. Adapters: directory, .tgz archive,
/// in-memory (tests); future: OCI registry, URL.
pub trait ChartSource {
    fn load(&self) -> Result<RawChart, ChartError>;
}

pub struct ChartSet {
    /// Root + recursively discovered dependencies, each carrying its
    /// values prefix (alias-aware), library flag, templates, helpers,
    /// extra file sources (`.Files.Get`), and Chart.yaml metadata.
    pub charts: Vec<ChartContext>,
}
```

The existing VFS approach (physical FS + in-memory FS for extracted archives)
is kept — it is already the right abstraction — but moves out of the CLI so
any consumer (tests, a future LSP, a CI bot) can load charts without clap.

Values *composition* (the two-pass global hoist/mirror that encodes Helm's
runtime behavior) becomes a pure, named, unit-tested function here:

```rust
pub fn compose_values(set: &ChartSet, opts: &ValuesCompositionOptions)
    -> ComposedValues;   // typed YAML tree + per-chart prefixes
```

**What this eliminates:** library-tier chart logic trapped in the CLI (§2.6),
and the implicit coupling between discovery, composition, and the global
mirroring step buried in `run_inner`.

### 5.4 `helm-schema-values` — the values model

Small crate for the *documentation and defaults* side of the contract:

- parse composed `values.yaml` (+ extra values files) into a typed
  `ValuesModel`: per-`ValuePath` default values (with YAML provenance) and
  descriptions (the existing comment extractor, including `@param` and
  helm-docs conventions);
- enforce the existing invariant *in the type system*: a `ValuesModel` entry
  can contribute **defaults and metadata only** — it is not a `Fact` and
  cannot create paths or types on its own. Synthesis consumes it through a
  separate parameter, so the invariant "comments never influence inference"
  is structural, not disciplinary.

### 5.5 `helm-schema-semantics` — the one interpreter

This is `single-abstract-interpreter.md`, promoted to its own crate and to the
*only* implementation of Helm meaning. The core shape is exactly the one that
plan specifies:

```rust
pub fn eval_expr(expr: &Expr, env: &EvalEnv) -> EvalOutcome;
pub fn eval_node(node: &TemplateNode, env: &EvalEnv, cx: &mut InterpretCx);

pub struct EvalOutcome { pub value: AbstractValue, pub effects: Effects }
```

with the single lattice (variants per the active plan: `Unknown`, `Root`,
`Values(ValuePath)`, `Scalar(AbstractScalar)`, `Object`, `Array`,
`Fragment`, `Overlay`, `Union`) and `Effects` as the only fact carrier.
Architectural refinements on top of that plan:

**(a) The environment is a value, not mutable walker state.**

```rust
/// Immutable; control flow produces child environments. `set` and
/// `$x = …` mutations produce a *new* env threaded forward in source
/// order. This deletes the manual snapshot/restore protocol in
/// walker.rs (the "add a field, forget to restore it" hazard) by
/// construction.
pub struct EvalEnv {
    dot: AbstractValue,
    root: AbstractValue,
    locals: ScopeChain,            // persistent map, O(1) child scopes
    guards: GuardStack,            // active control-flow predicates
}
```

**(b) Sink attribution is a structural query, with explicit abstention.**
Where a render-use lands in the manifest is computed from the spanned
`TemplateTree`'s YAML ancestry plus a small set of *typed* fragment rules
(`nindent`-under-a-key ⇒ child fragment; block scalar ⇒ opaque string sink;
key position ⇒ dynamic-key fact, not a path). This replaces incremental
line-ingestion shape tracking. Honesty clause: templated YAML can defeat
structural attribution (an action emitting half a key, adjacent scalar
splicing). Those cases produce

```rust
pub enum SinkSite {
    Exact { doc: DocId, path: DocPath, role: SinkRole },
    Abstained { doc: DocId, reason: AttributionGap, span: Span },
}
```

— an abstained site still records the read (so the path exists in the schema)
but contributes no resource-schema evidence, and emits a diagnostic. This is
the project's "prefer ambiguous over wrong" principle applied to the one place
the current code is still heuristic.

**(c) Helpers are summaries from the same interpreter.**
`include`/`template` evaluates the helper body with `eval_node` under an env
derived from the (abstract) argument, memoized by
`(HelperId, fingerprint(arg_value))`, with a recursion guard. The summary is
`EvalOutcome` — value *and* effects — so reads, mutations, defaults, fragment
provenance, and guards all propagate through one mechanism. The 1 480-line
literal-only `helper_eval.rs` disappears: an apiVersion helper is just a
summary whose value is `Scalar { known_strings }` or a `Union` with
capability-guard effects.

**(d) Resource identity is a projection, not a detector.**
Per document: evaluate the top-level `apiVersion:` / `kind:` entries with the
same interpreter, then *project* the result:

```rust
pub struct ResourceIdentity {
    pub kind: Alternatives<Interned<str>>,
    pub api_version: Alternatives<Interned<str>>,
    pub span: Span,
}

/// A guarded alternative tree — the generalization of today's
/// `api_version_branches`. Preserves `Capabilities.APIVersions.Has`
/// chains (and any future structurally-decoded guard) without
/// flattening mutually-exclusive branches into peer candidates.
pub enum Alternatives<T> {
    One(T),
    Branched(Vec<(GuardExpr, Alternatives<T>)>),
    AnyOf(BTreeSet<T>),
    Unknown,
}
```

`kind: List` envelopes are handled here too: identity projection descends
`items[*]`, yielding per-item identities with rebased `DocPath`s — the
already-landed behavior, expressed as part of one analysis instead of a
byte-cursor side channel.

**(e) The output is facts, not `ValueUse`.**

```rust
pub struct ChartAnalysis {
    pub facts: Vec<Fact>,
    pub resources: Vec<(DocId, ResourceIdentity)>,
    pub helper_call_graph: HelperCallGraph,
}

pub enum Fact {
    RenderUse   { path: ValuePath, sink: SinkSite, shape: RenderShape,
                  guards: Vec<Guard>, prov: Provenance },
    FragmentUse { path: ValuePath, sink: SinkSite, prov: Provenance },
    Default     { path: ValuePath, admits: FalseySet, prov: Provenance },
    TypeEvidence{ path: ValuePath, hint: SchemaTypeHint, origin: HintOrigin,
                  prov: Provenance },
    Guarded     { path: ValuePath, predicate: Predicate, prov: Provenance },
    Mutation    { target: ValuePath, key: Interned<str>, prov: Provenance },
    OpenObject  { path: ValuePath, why: OpenObjectReason, prov: Provenance },
    Iterated    { path: ValuePath, item_shape: IterShape, prov: Provenance },
}
```

`Guard` keeps today's well-designed predicate vocabulary (`Truthy`, `Not`,
`Eq`, `Or`, `Range`, `With`, `Default`, `TypeIs`) over `ValuePath` instead of
`String`. Facts carry everything synthesis needs **so synthesis never
re-derives Helm semantics** — this is Phase 6 of the active plan, made the
permanent contract. A `ValueUse` compatibility projection
(`fn value_uses(&ChartAnalysis) -> Vec<ValueUse>`) exists during migration
and for the serialized debug output, then becomes a test fixture format only.

**Purity contract:** this crate does no I/O, takes no `dyn` dependencies, and
is deterministic. Capability guards are *preserved*, never *evaluated* here —
evaluation needs the knowledge layer and happens downstream. That keeps the
interpreter runnable offline and per-template parallel (it is a pure fold).

**What this eliminates:** all of §2.1 — the six evaluators, three lattices,
twin helper walks, callback indirection, `is_fragment_expr` text sniffing
(fragmentness comes from the typed `RenderShape` of the evaluated value),
manual scope snapshots, and the source-order-fragile `chart_value_defaults`
side channel (mutation facts are ordered effects).

### 5.6 `helm-schema-knowledge` — corpora behind tri-state ports

The current crate's trait-based bones are kept and the monoliths are decomposed
into a small set of orthogonal pieces. Key insight from the review: the K8s
provider and the CRD provider differ only in **catalog layout** (how a
coordinate maps to a file/URL) and **version policy**; everything else
(fetch-on-miss, atomic write, mem/disk caching, negative cache, layout check)
is duplicated. So:

```rust
/// Typed coordinate; parsed once at the boundary, never re-split.
pub struct ResourceCoordinate {
    pub group: Option<Interned<str>>,
    pub version: Interned<str>,
    pub kind: Interned<str>,
}

/// Port the synthesis layer consumes (defined in synthesis, see §5.7;
/// implemented here). Tri-state by construction.
impl ResourceSchemaOracle for CatalogChain { … }

/// Internal ports of this crate:
pub trait ArtifactFetcher: Send + Sync {           // network edge
    fn fetch(&self, url: &Url) -> Result<FetchOutcome, FetchError>;
}
pub enum FetchOutcome { Ok(Vec<u8>), NotFound /* authoritative 404 */ }

pub trait ArtifactStore: Send + Sync {             // disk/mem cache edge
    fn get(&self, key: &ArtifactKey) -> Option<Arc<[u8]>>;
    fn put(&self, key: &ArtifactKey, bytes: &[u8]);
    fn negative(&self, key: &ArtifactKey) -> bool;     // recorded 404s only
    fn record_negative(&self, key: &ArtifactKey);
}

/// One generic catalog engine instead of two provider monoliths.
/// K8s vs CRD vs local-override differ only in their `CatalogLayout`.
pub struct RemoteCatalog<L: CatalogLayout> {
    layout: L,                       // coordinate → relative path/URL
    sources: Vec<SourceId>,          // default + mirrors, in priority order
    fetcher: Arc<dyn ArtifactFetcher>,
    store: Arc<dyn ArtifactStore>,
    diags: Arc<dyn DiagnosticSink>,
}

pub trait CatalogLayout {
    fn locate(&self, c: &ResourceCoordinate) -> Vec<RelativePath>; // candidates
    fn layout_marker(&self) -> LayoutVersion;                      // cache contract
}
```

Composition replaces inheritance-by-bloat — each policy is a ~100-line
combinator over the same `SchemaCatalog` interface:

```
LocalOverrideCatalog                       // never wiped, top priority
  ▸ then CrdCatalog = RemoteCatalog<CrdLayout>
  ▸ then K8sCatalog = VersionFallback(RemoteCatalog<K8sLayout>, version_chain)
  = PriorityChain([...])                   // first Found or authoritative Absent wins,
                                           // Unknown falls through, diagnostics at the end
```

The **capability oracle** becomes its own small adapter over a catalog probe,
no longer welded to the chain:

```rust
pub struct CatalogCapabilityOracle<C> { catalog: C, probe_table: ProbeTable }

impl<C: SchemaCatalog> CapabilityOracle for CatalogCapabilityOracle<C> {
    /// Some(true)/Some(false) only on authoritative signals;
    /// None on any uncertainty — preserving the documented contract,
    /// now testable against a fake catalog without a full chain.
    fn has(&self, gv: &GroupVersion) -> Lookup<bool>;
}
```

`ProbeTable` is the existing `well_known_kind_at()` map, kept (it remains the
documented, bounded structural debt) but promoted to *data* — a declarative
table shipped with the crate, diffable and unit-checked against the shortlist.

The **apiVersion advisor** (today's `inference/` module — cache scan,
shortlist, online probe, aggregation) survives as one clearly-quarantined
adapter implementing an explicit *heuristic* port, off by default, exactly as
now. Its tier ordering (`Shortlist > LocalCacheScan > OnlineProbe`) becomes a
declared `AdvisorPolicy` value.

`$ref` resolution (`ResolveCtx`) stays as a pure function over a
`DocumentLoader` closure — it is already well-shaped; it just moves out of the
916-line provider file.

**What this eliminates:** §2.5 — provider monoliths, duplicated atomic-write /
layout / scan code, chain↔oracle coupling, and the builder that must know 7
concrete types (the facade now assembles combinators).

### 5.7 `helm-schema-synthesis` — evidence in, typed schema out

The decision layer. It owns the port it needs (hexagonal: consumer defines
the interface):

```rust
/// Defined here; implemented by helm-schema-knowledge.
pub trait ResourceSchemaOracle: Send + Sync {
    fn schema_at(&self, id: &ResourceIdentity, path: &DocPath)
        -> Lookup<Arc<SchemaNode>>;
    fn capabilities(&self) -> &dyn CapabilityOracle;
}
```

(A `NoKnowledge` null adapter gives `--no-k8s-schemas` for free, and tests get
in-memory oracles without touching disk or network.)

Synthesis is three explicit, separately-testable stages:

**Stage A — evidence collection.** Group `Fact`s by `ValuePath`; resolve each
exact `SinkSite` against the oracle (evaluating preserved capability-guard
`Alternatives` here, where the oracle lives — selecting the first live branch,
exactly today's semantics); fold in `ValuesModel` defaults/descriptions. The
result is a flat, inspectable `BTreeMap<ValuePath, EvidenceSet>` — the
debuggable midpoint the current god-loop lacks.

**Stage B — per-path decision under an explicit policy.**

```rust
/// The entire precedence/widening rulebook as one inspectable value.
/// Today these rules exist as ~15 interleaved conditionals across
/// build_root_schema and resolve_schema_for_value_path.
pub struct SynthesisPolicy {
    pub source_rank: SourceRank,      // ChartStructure > Knowledge > Defaults > Hints
    pub widening: WideningRules,      // FalseySet admission → nullable/empty variants
    pub scalar_restriction: ScalarRestrictionRules,
    pub open_object: OpenObjectRules,
    pub required: Option<RequiredRules>,   // the optional required-inference pass
}

pub fn decide(path: &ValuePath, ev: &EvidenceSet, policy: &SynthesisPolicy)
    -> (SchemaNode, Vec<Decision>);   // Decision = what was chosen and why
```

Every `Decision` carries the evidence IDs it used — that record is what makes
a future `helm-schema --explain .Values.foo.bar` a projection instead of a
project.

**Stage C — assembly and merging over a typed schema algebra.**

```rust
/// Typed model. JSON appears only in helm-schema-emit.
pub enum SchemaNode {
    Any,
    Scalar  { types: TypeSet, enum_: Option<BTreeSet<ScalarValue>>,
              constraints: ScalarConstraints },
    Object  { properties: BTreeMap<Interned<str>, SchemaNode>,
              additional: Additional, required: BTreeSet<Interned<str>> },
    Array   { items: Box<SchemaNode> },
    Union   (Vec<SchemaNode>),
    Annotated{ inner: Box<SchemaNode>, meta: Meta },   // description, provenance
}

/// THE merge, with stated algebraic laws (commutative, associative,
/// idempotent; `Any` absorbing) — property-tested, replacing today's
/// three string-matching definitions of "scalar" and the triple
/// meaning of `{}`.
pub fn merge(a: &SchemaNode, b: &SchemaNode) -> MergeOutcome;

pub enum MergeOutcome {
    Merged(SchemaNode),
    Union(SchemaNode),                       // principled anyOf
    Conflict { merged: SchemaNode, diag: Diagnostic },  // never silent
}
```

Kubernetes-specific keyword behaviors (`x-kubernetes-preserve-unknown-fields`,
structured-beats-map-like, `required` union, `enum` intersection) become named
rules in the merge module with direct unit tests, instead of branches in a
787-line file.

**What this eliminates:** §2.3 and §2.4 in full — the god loop, scattered
precedence, stringly schemas, silent unions, and the generator re-deriving
Helm semantics (nullability now comes from `Fact::Default { admits }`, open
objects from `Fact::OpenObject`, fragmentness from `RenderShape`).

### 5.8 `helm-schema-emit` — dialects and output transforms

- `SchemaEmitter` port: typed `SchemaNode` → `serde_json::Value` for a
  dialect. Day one: `Draft07Emitter` (byte-compatible with current output);
  a 2020-12 emitter becomes an adapter, not a rewrite.
- The post-processing chain becomes a uniform pass pipeline over emitted JSON
  (JSON is correct here — these passes interoperate with *external* schemas):

```rust
pub trait SchemaTransform {
    fn name(&self) -> &'static str;
    fn apply(&self, schema: Value, cx: &TransformCx) -> Result<Value, TransformError>;
}
// OverrideMerge { replace-on-$ref semantics }, FlattenRefs { dyn DocumentRetriever },
// StripDescriptions, Minify (delegates to json-schema-minify), …
```

- `DocumentRetriever` port (file/http retrieval for `$ref` flattening) keeps
  the current `jsonschema::Retrieve`-backed implementation as its adapter.
- `json-schema-minify` remains exactly the standalone crate it is — it already
  matches this architecture.

### 5.9 `helm-schema` (facade) and `helm-schema-cli`

The facade is the only place wiring happens, and the *entire* public story for
embedding:

```rust
pub struct Pipeline { /* stages + adapters + policy + diagnostics */ }

impl Pipeline {
    pub fn builder() -> PipelineBuilder;          // defaults = today's behavior
    pub fn generate(&self, source: &dyn ChartSource)
        -> Result<GenerateOutput, PipelineError>;
}

pub struct GenerateOutput {
    pub schema: Value,
    pub diagnostics: Vec<Diagnostic>,
    pub analysis: Option<ChartAnalysis>,   // opt-in, for tooling/--explain
}
```

`helm-schema-cli` shrinks to: clap arg structs → `PipelineBuilder` calls →
write output / emit diagnostics in text or JSON. Typed errors per crate,
converted to `color_eyre::Report` in `main` only (current workspace standard,
kept). Every behavior reachable from the CLI is reachable from the facade —
which is what makes the full integration suite runnable in-process with
in-memory adapters.

---

## 6. Why this is the right architecture (decision rationale)

**One interpreter, because semantics duplication is the proven failure mode.**
Every recurring bug class in the project's own history (`with … default`
rebinding, helper-bound `set`, `toYaml` fragments, map ranges, empty
placeholders) traces to the same root: N evaluators that each understand 80%
of Helm. A single lattice + transfer functions means a new Helm semantic is
*one* function arm, and every consumer — value uses, helper outputs, resource
identity, chart facts — inherits it simultaneously. This is also the only
design under which "no heuristic where structure suffices" is enforceable:
there is exactly one place a text-sniffing shortcut could creep in, and it is
the one place reviewers watch.

**Facts with provenance as the IR, because the alternative is re-derivation.**
The current `ValueUse` hand-off forces the generator to reverse-engineer
meaning the analyzer already had and threw away (§2.3). Passing typed facts
forward is strictly more information at zero ongoing cost, collapses the
generator's special-case lattice into policy + algebra, and buys
explainability (every schema node traceable to spans) — the project's
"diagnosable" bar, structurally guaranteed.

**Typed schema algebra, because merging is the system's real arithmetic.**
Schema merge/union decisions are where correctness lives in the synthesis
half, and today they are unfalsifiable — scattered over string-keyed JSON
edits. A closed `SchemaNode` algebra with stated laws is property-testable
(`merge(a,b) == merge(b,a)`, idempotence, absorption), makes illegal schemas
unrepresentable, and quarantines JSON to the emit edge where dialect choices
belong. The cost (conversion at the edge) is paid once; the current cost
(every rule change risks an untestable regression) is paid forever.

**Tri-state ports for knowledge, because the type system should carry the
contract.** The cache-as-oracle bug needed three rounds to fix and a 60-line
CLAUDE.md section to defend. `Lookup<T>` with authoritative-`Absent` vs
abstaining-`Unknown` makes the contract a compile-time property of every
adapter, and decorator composition (`VersionFallback(Mirrored(Cached(...)))`)
turns each policy into an independently testable ~100-line unit instead of a
916-line monolith. The knowledge layer is also where the project will grow
(new mirrors, OCI catalogs, vendored bundles, airgapped stores) — exactly the
axis ports are for.

**Hexagonal direction — consumers own their ports — because it keeps the core
pure.** `semantics` depends on nothing impure; `synthesis` names what it needs
(`ResourceSchemaOracle`) and `knowledge` supplies it. The pure middle is
therefore: deterministic (BTree everywhere, no clock, no global state),
trivially parallel (per-template interpretation is a pure fold — rayon across
templates with zero locks, replacing today's shared `Mutex` cache contention),
and testable without fixtures touching disk or network.

**Restraint on traits, because flexibility has a price.** Ports exist at
the named variation points only (parser backend, chart source, fetcher,
artifact store, schema catalogs, capability oracle, emitter, transforms,
diagnostics, retriever). Everything else — interpretation, projection, composition,
merging, policy application — is plain functions over plain data. This is the
explicit answer to `next-priorities.md`'s correct worry: abstraction only
where a second implementation demonstrably exists (mock vs real, k8s vs CRD
vs local, draft-07 vs 2020-12, dir vs tgz vs memory) or where the I/O edge
demands a seam.

**Alternatives considered and rejected:**

- *Render-then-infer* (execute `helm template` over sampled values and infer
  from outputs): abandons static precision, samples can't cover guard space,
  and contradicts the charter. Rendering is useful only as a *test oracle*
  (§8).
- *Annotation-driven* (`values.yaml` comments as the source of truth): already
  rejected by the README; comments stay metadata-only, now enforced by type.
- *One big crate with modules*: compile-time boundary enforcement is the only
  thing that has historically kept layers honest here (the k8s crate stayed
  clean; the IR crate, without internal boundaries, grew six evaluators).
- *Maximal trait-per-stage pipelines* (`trait Stage<In, Out>` chains):
  generic plumbing without a second implementation per stage; rejected for
  the same reason trait soup is.

**Sizing sanity check.** This design should land *smaller* than today's 29K:
one interpreter replaces ~6 evaluators plus `helper_eval` (≈5–6K LOC of
near-duplicates), one catalog engine replaces two provider monoliths (≈1K of
duplication), and the synthesis policy/algebra replaces the special-case
lattice in `gen` — while adding spans, explainability, and dialect freedom.

---

## 7. Cross-cutting policies

- **Errors:** typed `thiserror` enums per crate (`ParseError`, `ChartError`,
  `FetchError`, `SynthesisError`, …); `color_eyre::Report` only in `main`.
  No `Result` aliases (workspace standard).
- **Diagnostics:** one `Diagnostic` model in `core` (superset of today's k8s
  diagnostics, plus spanned analysis diagnostics like `AttributionGap` and
  `MergeConflict`); deduplicating sink; text and JSON renderers in the CLI.
  Rule: any abstention or widening that changes output emits one.
- **Determinism:** ordered collections at every boundary; interner IDs never
  serialized; no wall-clock or randomness anywhere in the pure middle.
  Identical inputs ⇒ byte-identical output, regardless of cache state
  (the cache contract, extended system-wide).
- **Concurrency:** parallelism only in the facade (per-template interpret,
  per-coordinate prefetch); the pure crates stay single-threaded-but-Send.
  Caches are the only shared state and live behind `ArtifactStore`.
- **Observability:** keep `tracing::instrument` + Perfetto as the profiling
  source of truth; stage boundaries in the facade are natural span roots.
- **Serialization stability:** `ChartAnalysis`→`ValueUse` projection and the
  emitted draft-07 output are the two compatibility surfaces; both are pinned
  by golden tests.

## 8. Testing architecture

The ports make the test pyramid cheap:

1. **Algebra property tests** (synthesis): merge laws, `FalseySet` widening
   monotonicity, `Alternatives` flattening — `proptest` over generated
   `SchemaNode`s. This class of test is *impossible* today (§2.4).
2. **Transfer-function tables** (semantics): tiny template snippet → expected
   `Fact`s/value, one table per Helm construct (`default`, `with`, `range`
   destructuring, `set`, `toYaml`, `tpl`, include-with-dict-arg…). These
   replace today's scattered walker tests and pin each semantic exactly once.
3. **Adapter contract tests** (knowledge): every `SchemaCatalog`/`ArtifactStore`
   adapter runs one shared behavioral suite (tri-state honesty: cold cache
   must yield `Unknown`, never `Absent`; offline never fetches; negative
   cache only after authoritative 404) — the regression suite that currently
   pins the oracle, generalized to every adapter.
4. **Golden full-schema equality** (facade): the existing real-chart corpus
   (`testdata/charts/*`) with `similar_asserts::assert_eq!` over complete
   schemas — unchanged, per the project standard; in-memory chart sources and
   recorded catalog fixtures make the suite hermetic and fast.
5. **Differential validation** (new, enabled by the architecture): for each
   fixture chart, render with `helm template` under N values samples (default
   values, plus guard-flipping samples derived from the chart's own
   `Fact::Guarded` set) and assert every sample that Helm accepts validates
   against the generated schema. This turns Helm itself into a soundness
   oracle without ever making rendering part of inference.

## 9. Migration correspondence (informative)

Sequencing is owned by `next-priorities.md` and the phase plan in
`single-abstract-interpreter.md`; this table only records where current code
lands so incremental cleanups can aim at the right home. The existing seams
(`IrGenerator`, `K8sSchemaProvider`, `ValuesSchemaGenerator`, `ValueUseSink`,
`HttpFetcher`) are the natural strangler points — each can be re-implemented
against the new crates one at a time while golden tests hold the line.

| Today | Target home |
|---|---|
| `helm-schema-template-grammar`, `helm-schema-ast` (parser, `TemplateExpr`) | `helm-schema-syntax` (spans added; headers typed; single parse) |
| `helm-schema-ast::values_comments` | `helm-schema-values` |
| `helm-schema-ir` `abstract_value/eval_env/eval_effect/expr_eval` | `helm-schema-semantics` core (already converging via phases 0–3) |
| `walker.rs`, `symbolic.rs`, `binding.rs`, `fragment_*`, `helper_*`, `bound_*`, `local_projection`, `output_*` | absorbed by `eval_node` + helper summaries + sink attribution (phases 4–5) |
| `helper_eval.rs`, `resource_detector.rs`, `resource_locator.rs` | resource-identity projection in `helm-schema-semantics` |
| `rendered_yaml_context.rs`, `yaml_shape.rs` | structural sink attribution (with explicit `Abstained`) |
| `value_use_postprocess.rs`, `ValueUse` | compatibility projection of `ChartAnalysis` |
| `helm-schema-k8s` providers | `RemoteCatalog<L>` + combinators in `helm-schema-knowledge` |
| `capability_eval.rs` + chain oracle impl | `CatalogCapabilityOracle` + pure guard evaluation |
| `inference/*` | the quarantined `ApiVersionAdvisor` adapter |
| `helm-schema-gen` lib/merge/required_inference | `helm-schema-synthesis` stages A–C + policy + algebra |
| `json-schema-minify` | unchanged (already conformant) |
| CLI `chart.rs` | `helm-schema-chart` |
| CLI `schema_override.rs`, `flatten.rs` | `SchemaTransform` adapters in `helm-schema-emit` |
| CLI `lib.rs` pipeline | `helm-schema` facade |

## 10. Open questions

- **Fused-parse fidelity:** how far the fused grammar can carry structural
  YAML attribution before `Abstained` rates become user-visible on real
  charts; needs measurement on the corpus before `yaml_shape`'s incremental
  tracker is declared fully replaceable (it may survive as one *adapter*
  behind the attribution module for the hard cases).
- **Helper summary fingerprints:** the memo key must hash abstract argument
  values; needs a canonical-form definition so `Union` ordering or `Overlay`
  nesting differences don't defeat memoization.
- **`Interned<str>` scope:** per-pipeline interner vs per-chart; affects
  whether `ChartAnalysis` is `Send + 'static` for tooling embedders.
- **Schema algebra completeness:** which JSON Schema keywords the typed model
  must represent losslessly for *pass-through* of upstream K8s/CRD schemas
  (`patternProperties`, `oneOf` discriminators, vendor `x-kubernetes-*`) vs
  which can remain opaque `Annotated` payloads.
