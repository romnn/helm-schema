# helm-schema — from-scratch target architecture (v2)

Status: design document. No code change is implied by this file by itself.

This is the architecture I would build if helm-schema were rewritten today from
first principles. It is a *clean-room* design: the current implementation and
the other plan documents are treated as **evidence** (what worked, what bit
us, what real charts do), never as constraints.

**Revision note.** v2 is the result of an adversarial review of v1 by five
independent critics (minimalist architecture, program-analysis soundness,
Helm/K8s domain realism, Rust API/performance, security/evolution pre-mortem)
plus a first-principles re-derivation. The load-bearing changes from v1:

1. A **formal correctness contract** now anchors the whole design (§2); every
   widening/abstention rule is derived from it via a polarity table instead of
   being re-litigated per call site.
2. The central analysis artifact is an **abstract document model** (the
   interpreter builds an abstract rendered manifest per template), not a flat
   fact stream. Sink attribution, resource identity, `kind: List` descent and
   K8s anchoring become structural projections instead of side channels.
3. Guards are a **compositional predicate algebra** over typed places (with
   `And`/`Or`/`Not`, environment atoms like `semverCompare`, three-valued
   evaluation) — the flat v1 `Guard` vocabulary could not even represent an
   `else` branch.
4. The trait census shrank from ~12 ports to **3** (`Fetch`,
   `ResourceSchemaOracle`, `CapabilityOracle`); the knowledge layer became a
   **pure lookup planner + one executor** instead of trait combinators, which
   also makes "what was tried" diagnostics fall out of the executed plan.
5. The schema model is **two-tier**: upstream K8s/CRD schemas stay verbatim
   JSON behind lazy `$ref` documents (no round-trip risk, no materialized
   expansion — the real RSS lever); the typed, law-bearing algebra covers only
   what we synthesize from chart evidence.
6. The input universe widened to what real charts actually are: `files/`
   manifests reached via `tpl`/`fromYaml`/`mergeOverwrite`, chart-shipped
   `crds/`, `Chart.yaml` dependency `condition:`/`tags:`, shipped subchart
   `values.schema.json`, NOTES.txt/tests as text-mode contract sources.
7. Security became first-class: `FetchPolicy` and `LoadBudget` objects at
   every network/file/archive edge, and a validated path newtype at the
   coordinate→path boundary (the review found a live path-traversal shape and
   an SSRF surface in today's code that v1 would have reproduced).
8. Assorted Rust corrections: no interner (content-stable `Name`),
   state-passing environments with explicit join (Go-template `=` vs `:=`),
   fingerprinted Arc-shared lattice nodes, honest parallelism accounting.

Relationship to other plan documents: `single-abstract-interpreter.md` remains
the right *seed* for the interpreter and is generalized here (its lattice and
`Effects` carry over with laws added); `next-priorities.md` keeps ownership of
sequencing — §12 only records where things land and adds one migration rule.

---

## 1. The problem, from first principles

helm-schema answers one question:

> Given a Helm chart, what is the most precise, structurally justified
> JSON Schema for its values contract?

Decomposing the question yields the system's natural domains. Boundaries are
drawn where the *kind of knowledge* and the *rate of change* differ:

| Domain | Question | Knowledge kind | Changes when… |
|---|---|---|---|
| **Chart model** | What is this chart made of? | Helm packaging conventions | Helm packaging evolves (OCI, deps, crds/) |
| **Syntax** | What does this text say? | Grammars | Go-template/YAML syntax evolves (rarely) |
| **Semantics** | What does this template mean? | Helm/Sprig evaluation semantics | Helm function semantics evolve |
| **Knowledge** | What does Kubernetes expect here? | External + chart-local schema corpora | K8s releases, CRD catalogs, mirrors |
| **Synthesis** | Given all evidence, what is the contract? | Decision policy + schema algebra | Our inference policy improves |
| **Emission** | How is it written down? | JSON Schema dialects, consumers | Helm 3/4 validators, editors |

Two concerns cut across everything: **provenance** (every fact traceable to a
span and helper chain) and **ambiguity** (unknown and "several alternatives"
are first-class values at every layer). What was missing until now is the rule
that tells every layer *which way to lean* when it doesn't know. That rule is
the correctness contract.

## 2. The correctness contract (the spine of the design)

Fix an environment `E` (Capabilities, KubeVersion, Release, …) and a knowledge
corpus `K`. For a chart `C` with defaults `D` and user input `U`, Helm
validates the **coalesced** values `V = coalesce(D, U)` — and Helm's
coalescing deletes keys the user sets to `null`. The schema's domain is
coalesced documents. Define:

- `AccRender(C, E)` — values for which `helm template` succeeds. Non-trivial:
  deep access through a `null` parent fails rendering; `required`/`fail` fail
  it explicitly.
- `AccK8s(C, E, K)` — the subset whose emitted manifests (where identity is
  resolvable) validate against their resource schemas.

**Soundness obligation (hard).** For every `E` not refuted by the oracle:
`AccK8s(C, E, K) ⊆ L(S)` — the generated schema never rejects a working
configuration.

**Precision objective (soft).** Minimize `L(S) \ AccK8s`; every narrowing must
be justified by a provenance-carrying piece of evidence.

Two deliberate, *named* weakenings (policies, not silent behavior):

- **P1 — as-if-used typing.** Plain JSON Schema cannot say "p must be an
  integer only when guard g holds." Per-path constraints are therefore
  guard-insensitive projections over the values that flow into reads. The
  differential harness's guard-flipping samples (§10) are the acceptance gate
  for this gap.
- **P2 — typo strictness.** Closing structurally enumerated objects
  (`additionalProperties: false`) rejects unread keys that `AccRender`
  technically accepts. Deliberate, diagnosable, and off-switchable.

Two mechanical self-obligations: the chart's own composed defaults `D` must
validate against `S` (checked as a pipeline postcondition, §8.4 — `helm lint`
enforces exactly this at install time), and shipped subchart schemas must not
contradict `S` on their prefix (§6.3).

**The polarity table.** Every evidence kind is allowed to err in exactly one
direction; uncertainty always moves `L(S)` *up* (wider):

| Evidence | May err toward | Because |
|---|---|---|
| path existence (a read) | over-approx, but typed `Any` | a spurious path with a narrow type would reject |
| type evidence (sink schema, `typeIs`, string ops) | under-approx — omit when unsure | omitted type = `Any` = wide |
| falsey admission (`default`, `with`, guarded reads) | over-approx — admit when unsure | failing to admit a handled `null` rejects |
| object openness | over-approx openness; close only on proof + P2 | closing is the narrowing move |
| sink attribution | exact or abstain; abstain ⇒ no resource evidence | a wrong anchor imports wrong constraints |
| requiredness | drastic under-approx | see §6.2; today's heuristic is provably unsound |
| branch liveness | unknown ⇒ live | pruning a live branch drops its admissions |

Derived consequences used throughout: `Lookup::Unknown` never prunes and never
narrows; evidence-source precedence may never *override* (an override can
violate `D ∈ L(S)`) — disagreement reconciles by widening plus a conflict
diagnostic; determinism is claimed **given identical oracle answers**, and
cache state may move output only in the widening direction (reproducibility
over time is the lockfile's job, §8.5).

## 3. What the evidence teaches

### 3.1 From the current implementation (≈29K LOC, reviewed in depth)

- **Semantics implemented ~6×.** `expr_eval`, `helper_binding_eval`,
  `fragment_expr_eval`, `fragment_binding_eval`, the 1,480-line literal-only
  `helper_eval`, and the chart-facts walker — over three near-identical value
  lattices (`AbstractValue` / `HelperBinding` / `FragmentBinding`) with lossy
  conversions between them, twin near-duplicate helper-body walks, and manual
  scope snapshot/restore. Every documented recurring bug class (`with …
  default` rebinding, helper-bound `set`, `toYaml` fragments, map ranges,
  empty placeholders) traces to this split: fixing one shape means
  remembering to update several collectors.
- **Syntax is lossy and parsed three times**; `HelmAst` stores control-flow
  headers as raw strings (`If { cond: String }`) and drops all spans, so
  position-aware diagnostics are impossible, every consumer re-parses action
  text (mitigated by thread-local caches), and a 900-line
  indentation-heuristic shape tracker (`yaml_shape.rs`) reconstructs what the
  parse threw away — a line-shape heuristic at the core of a project whose
  charter says "parsers over string heuristics".
- **The IR is lossy, so the generator re-derives semantics**: `ValueUse`
  drops falsey admissions, openness, fragment provenance and mutations;
  `helm-schema-gen` rebuilds them in a 134-line god-loop
  (`build_root_schema`) over stringly `serde_json::Value`, with ~15
  interleaved special cases, precedence rules scattered as inline
  conditionals, three competing definitions of "scalar", and a
  triple-meaning `{}`.
- **Today's guard model cannot represent an `else` branch.** The walker
  restores scope and walks alternatives with *no* guard pushed; `not (eq …)`
  and `or (eq …) .b` are unrepresentable; `with (or A B)` is encoded by a
  convention downstream consumers must just know.
- **The knowledge crate is the best part** (real ports, a hard-won tri-state
  capability oracle) but providers are 400–900-line monoliths with duplicated
  cache/fetch/layout code, and the per-resource **materialized `$ref`
  expansion is a dominant RSS term** (temporal: 2s / 177MB).
- **Library logic is trapped in the CLI.** Chart discovery, vendored-archive
  extraction, values composition (the two-pass global hoist/mirror), schema
  overrides, `$ref` flattening and the required-inference glue all live in
  `helm-schema-cli`, and the nine-stage pipeline exists only implicitly as
  the body of `run_inner()` — so nothing (tests, tooling, a future LSP) can
  reuse any of it without depending on the CLI crate.
- **Two latent security issues** the new design must close by construction:
  chart-controlled `apiVersion` text flows into cache paths and fetch URLs
  unvalidated (path-traversal shape), and `$ref` flattening fetches arbitrary
  `http(s)://` and reads arbitrary `file://` targets from override files with
  no policy (SSRF / local-file exfiltration into the emitted schema).
- **Worth preserving:** the tri-state oracle and its offline-safety contract,
  typed capability-branch preservation, `json-schema-minify` as a standalone
  Helm-free crate, full-schema-equality golden tests over real charts, and
  the in-flight `AbstractValue`/`Effects` unification.

### 3.2 From the chart corpus (what "a chart" actually is)

The fixture corpus falsifies the naive model "a chart = templates/ +
values.yaml":

- **nats** ships its StatefulSet in `files/stateful-set/*.yaml`, reached via
  `tpl (.Files.Get (printf "files/%s" .file)) | fromYaml`, then
  `mergeOverwrite`d with `.Values.statefulSet` and JSON-patched. The template
  file itself contains *no* YAML node.
- **zalando-postgres-operator / clickhouse / nack** ship the CRDs their own
  templates instantiate in `crds/` — an authoritative, version-exact schema
  source the external catalogs often lack.
- **signoz / bitnami common** decide apiVersions with
  `and (.Capabilities.APIVersions.Has …) (semverCompare ">=1.19-0"
  .Capabilities.KubeVersion.Version)` and with **values-overridable**
  capability shims (`default .context.Values.apiVersions (...)`), returning
  truthiness as `"true"`/`""` strings through `include`.
- **Umbrella charts** declare `condition: clickhouse.enabled` / `tags:` in
  Chart.yaml — declarative boolean evidence and chart-level guards, for free.
- **bitnami-redis** ships its own `values.schema.json` (Helm validates each
  chart's coalesced values against its own shipped schema independently),
  routes ~40 paths through `common.tplvalues.render` (every such path must
  also admit `string`), and uses `lookup` (cluster state — statically
  `Unknown` by definition).
- **values.yaml uses YAML anchors/aliases** (signoz) — the values parser must
  resolve them or every aliased default is wrong.
- `required`/`fail` appear as *conditional contract statements*
  ("externalClickhouse.host is required if not clickhouse.enabled").

Each of these is structural — parseable, span-carrying, no text heuristics
needed — so by the project's charter they belong in the architecture, not a
backlog.

## 4. Design commitments

1. **One semantic model.** Helm meaning lives in exactly one abstract
   interpreter. Helper outputs, resource identity, guard extraction, defaults,
   fragments — all projections of one interpretation.
2. **Typed and spanned everywhere.** Paths, predicates, coordinates, schemas
   are domain types; every node and fact carries provenance. Stringly forms
   exist only at serialization edges, as `Display`/DTO projections.
3. **Abstention is a value with a direction.** `Found/Absent/Unknown`
   lookups, `Top`/`Union` values, `Opaque` document regions — and the §2
   polarity table dictates what each consumer does with them.
4. **Ports at proven variation points; functions and data everywhere else.**
   Three traits total (§5). Catalog policy, synthesis policy, widening
   bounds, probe tables, builtin coverage are *data* interpreted by small
   engines.
5. **Untrusted input is budgeted.** Charts, archives, upstream schemas and
   override files are attacker-controlled inputs; every edge that touches
   them takes an explicit `FetchPolicy`/`LoadBudget`.
6. **Parity is part of correctness.** The existing CLI surface and output
   behaviors are an explicit checklist (§13), not folklore.

## 5. System overview

### 5.1 Crates

Eight product crates (plus `json-schema-minify` and `test-util`, unchanged):

```
helm-schema-grammar     tree-sitter C grammars + build.rs only (lint-exempt build isolation)

helm-schema-core        vocabulary: Name, ValuePath/DocPath, Span/Provenance/SourceMap,
                        Lookup<T>, FalseySet, Pred/Place algebra, ResourceCoordinate,
                        SchemaDoc (foreign JSON, lazy refs), Diagnostics (concrete),
                        FetchPolicy/LoadBudget, the two oracle traits
                        [deps: serde, serde_json (opaque payloads only), smol_str]

helm-schema-analysis    PURE. parse/ (spanned typed TemplateTree; thread-local parser fn)
                        + interp/ (AbsVal lattice, eval_expr/eval_node, helper summaries,
                        abstract documents, identity projection)
                        → ChartAnalysis
                        [deps: core, grammar, tree-sitter. NO IO, no oracle types]

helm-schema-chart       IO: discovery (dir/tgz/memory via vfs + LoadBudget), FileRole
                        assignment, Chart.yaml model (deps/conditions/tags/kubeVersion),
                        compose_values, ValuesModel (spans via a maintained YAML parser),
                        crds/ extraction, shipped values.schema.json collection
                        [deps: core, vfs, tar/flate2, saphyr-or-equivalent]

helm-schema-knowledge   IO: catalog config as data, pure lookup planner + one executor,
                        CacheDir (concrete), trait Fetch {Ureq, Mock}, capability oracle,
                        quarantined apiVersion advisor; implements core's oracle traits
                        [deps: core, ureq (pinned TLS feature)]

helm-schema-synthesis   PURE. evidence derivation (AbsDoc × oracle co-walk), branch
                        liveness, decision policy, typed SchemaNode algebra + lowering,
                        foreign-tier JSON composition rules, emit_draft07
                        [deps: core, serde_json]

helm-schema (facade)    wiring + Config, parallel fan-out, output passes (override merge,
                        ref flatten, strip, minify), postcondition validation, lockfile
                        [thin; golden integration tests live here]

helm-schema-cli         clap → Config (env vars interpreted HERE, not in libraries) →
                        facade → output/diagnostics/exit codes
```

Dependency rules with bite (CI-checked via `cargo deny`/metadata):
`analysis` and `synthesis` have no IO crates and no `dyn` dependencies —
*that* is the purity boundary; `core` is the only shared parent, so
`knowledge` builds in parallel with `analysis` and never sees tree-sitter.
Every crate sets `[lints] workspace = true` (the current workspace forgets
this, which is how denied lints ship today).

**Trait census (complete).** `Fetch` (network edge; Ureq/Mock exist),
`ResourceSchemaOracle` and `CapabilityOracle` (the pure↔IO seam; real second
implementations: in-memory fakes and the `--no-k8s-schemas` null oracle).
Both oracle traits live in `core` so the dependency arrow points inward from
both sides — v1 placed them in `synthesis`, which would have made the IO crate
transitively depend on the entire pure stack. Everything that was a port in
v1 — parser, chart source, emitter, transforms, diagnostics sink, retriever,
catalogs, layouts, stores — is now a function, a concrete type, or data,
because no second implementation exists or the variation is data-shaped.

### 5.2 Data flow

```
chart dir/tgz ──chart──► ChartSet {charts, roles, crds, shipped schemas, conditions}
                              │
            ┌─────────────────┼──────────────────────────┐
            ▼                 ▼                          ▼
      ValuesModel      per template: parse ──► TemplateTree (spanned, typed)
   (defaults+docs,            │
    anchors resolved)         ▼  interpret (shared helper-summary memo)
                        TemplateAnalysis { docs: AbsDoc forest, evidence, gaps }
                              │ fold (deterministic order)
                              ▼
                        ChartAnalysis ──synthesis──► per-path EvidenceSet
                                          ▲                │ decide(policy)
        knowledge: plan() → execute() ────┘                ▼
        (oracles: schemas, capabilities)            SchemaExpr (typed ∪ foreign)
                                                           │ lower + compose
                                                           ▼
                              draft-07 Value ──facade──► override → flatten →
                              strip → minify → postcondition-validate → bytes
```

Every arrow is a named public type; the facade exposes the stage functions
individually (that — not a "pipeline object" — is what lets tests, tools and
a future LSP run any prefix).

## 6. The layers in detail

### 6.1 `core` — vocabulary with the contract baked in

```rust
/// Content-stable small string (≤23 bytes inline, O(1) clone).
/// No interner exists: Ord/Hash/Serialize derive naturally, ChartAnalysis is
/// Send + 'static, and golden fixtures serialize by content. (v1's
/// per-session interner was incoherent: symbol Ord breaks determinism, serde
/// has no context parameter, and embedders lose 'static.)
pub type Name = SmolStr;

pub struct ValuePath(SmallVec<[ValueSeg; 4]>);
pub enum ValueSeg { Key(Name), AnyItem, AnyKey }
// Display/FromStr define the canonical text form; serde goes through it.
// Numeric ids (FileId, DocId, HelperId, BindId) exist only for closed
// per-chart namespaces whose tables travel inside the owning artifact.

#[must_use]
pub enum Lookup<T> { Found(T), Absent /* authoritative */, Unknown /* abstain */ }
// Combinators (map, and_then, or_unknown) so adapters never hand-roll
// collapsing matches. Adapter rule: IO errors map to Unknown + diagnostic —
// never Absent, never panic.

/// Helm truthiness states a path may take while the chart still behaves.
/// u8 bitset newtype: joined constantly during widening.
pub struct FalseySet(u8);
```

**The predicate algebra** — one representation for path conditions, branch
guards, and capability decisions (replacing v1's three: `Guard`,
`Alternatives`, `GuardExpr`):

```rust
pub struct Place { pub base: PlaceBase, pub path: ValuePath }
pub enum PlaceBase {
    Values,           // schema-relevant
    Env(EnvRoot),     // Capabilities/KubeVersion/Release/Chart/Files — evaluated late
    Local(BindId),    // SSA-numbered local or rebound-dot snapshot
    Synth(ValueId),   // result of an expression (default-wrapped, merged, …)
}

pub enum Pred { True, False, Atom(Atom), Not(PredRef), And(Box<[PredRef]>), Or(Box<[PredRef]>) }
pub enum Atom {
    Truthy(Place),                 // Helm emptiness test
    Eq(Place, ScalarConst),
    TypeIs(Place, HelmType),
    NonEmpty(Place),               // range body executes ≥ 1 time
    ApiVersionsHas(GroupVersion),  // .Capabilities.APIVersions.Has
    KubeVersionCmp(CmpOp, SemverReq), // semverCompare over .Capabilities.KubeVersion
    Opaque(SpanId),                // unmodeled condition; valuation Unknown
}
```

`PredRef` is hash-consed (flattened, sorted, deduped at construction), so
predicates are cheap to share per control-flow frame and stable to
fingerprint. Evaluation is three-valued (Kleene) under partial assumptions:
abstract values for value atoms, the environment oracle for `Env` atoms
(evaluated *late*, in synthesis), and probe assignments like `p := absent`.
This makes the previously impossible representable and the previously
heuristic definable:

- `if`/`else if`/`else` arms carry `P₁`, `¬P₁∧P₂`, `¬P₁∧¬P₂` — else branches
  finally have conditions (today they have none).
- `with X` pushes `Truthy(place(X))` *and* binds dot to the same `Place`;
  null-tolerance of `a.b` read under `with .Values.a` is the entailment
  "`pc` is false when `a := null`" — by construction, not string matching.
- `null_tolerant(hole)`, `required(p)`, `live(branch)` are defined queries
  with a small sound entailment table (default answer `Unknown`, resolved by
  polarity).

**Provenance and diagnostics.**

```rust
pub struct Provenance { pub span: Span, pub via: SmallVec<[HelperFrame; 2]> }
pub struct SourceMap { files: Vec<SourceFile> }   // owned by ChartAnalysis

/// Concrete deduplicating handle (today's k8s sink generalized + spans).
/// Not a trait: one implementation, ordered by key => deterministic under
/// parallelism. JSON output is a versioned envelope (machine interface).
pub struct Diagnostics(Arc<Mutex<BTreeMap<DiagKey, Diagnostic>>>);
```

The pure crates never push diagnostics; they record gaps/conflicts **as
data** in their outputs, and the facade projects diagnostics from them. (This
resolves v1's contradiction between "semantics emits a diagnostic" and
"semantics holds no `dyn`".)

**Security objects** (threaded to every untrusted edge):

```rust
pub struct FetchPolicy {  // schemes/hosts allowlist, deny link-local & loopback,
                          // per-fetch size & time budget, max $ref depth & doc count
}
pub struct LoadBudget {   // archive: max entries, max decompressed bytes;
                          // parse: max file size; interp: node/step budget
}
/// Validated at construction: no '..', no absolute, segments ⊆ [a-z0-9._-].
/// The ONLY type the cache/url layer accepts — coordinates are
/// attacker-controlled chart text.
pub struct RelPath(String);
```

**Oracle ports** (the pure↔IO seam; both implemented by `knowledge`, both
trivially fakeable):

```rust
pub trait ResourceSchemaOracle: Send + Sync {
    /// Foreign JSON document with lazy refs — NOT a typed schema. The lift
    /// into anything typed is synthesis's job (it owns the conservative
    /// fallback). Carries the resolved version for diagnostics.
    fn resolve(&self, c: &ResourceCoordinate) -> Lookup<Arc<SchemaDoc>>;
}
pub trait CapabilityOracle: Send + Sync {
    fn has_api(&self, gv: &GroupVersion) -> Lookup<bool>;
    fn kube_version(&self) -> Lookup<KubeVersion>;   // from --k8s-version / Chart.yaml
}
```

`SchemaDoc` lives in core (both sides name it): root + local definitions over
`Arc<serde_json::Value>` subtrees, refs resolved lazily during descent with a
cycle set — never materialized. Discipline rule: JSON in core is an opaque
payload; all query functions over it live in synthesis.

### 6.2 `analysis` — parse once, interpret once

**Parsing.** `pub fn parse(file: FileId, src: &str, mode: ParseMode) ->
Result<TemplateTree, ParseError>` — a function, not a trait (tree-sitter's
`Parser` is `!Sync`; a `&self` port could not even be implemented honestly).
One parse per file. The tree is fully typed and spanned; control-flow headers
arrive as parsed expressions; `Unknown` nodes (with spans) are the explicit
degradation for unparseable regions. A failed parse of one file is **per-file
recoverable**: the file contributes an `Opaque` analysis plus a gap record;
the chart fails only if nothing parses (CLI `--strict` upgrades gaps to
failures).

```rust
pub enum TemplateNode {
    Document { items: Vec<NodeId> },
    Mapping  { entries: Vec<(KeyNode, NodeId)> },
    Sequence { items: Vec<NodeId> },
    Scalar   { text: Name, style: ScalarStyle },
    Action   { expr: Expr, trim: TrimMode },
    If       { arms: Vec<(Expr, Block)>, else_arm: Option<Block> },
    With     { header: Expr, body: Block, else_arm: Option<Block> },
    Range    { binding: RangeBinding, header: Expr, body: Block, else_arm: Option<Block> },
    Define   { name: Name, body: Block },
    Unknown  { reason: ParseGap },
}
// spans + file ids in a side table keyed by NodeId; YAML anchors/aliases are
// resolved by the builder or degrade to Unknown — never silently dropped.
```

`ParseMode` comes from the chart layer's `FileRole` (§6.3): **manifest mode**
(fused YAML+template) vs **text mode** (template-only: NOTES.txt, config file
fragments) — text mode yields reads/predicates/aborts but no document and no
sinks.

**The value lattice** — one type, with laws:

```rust
#[derive(Clone)]                       // O(1): Arc + 128-bit structural fingerprint
pub struct AbsVal(Arc<VNode>);
struct VNode { fp: Fp128, kind: VKind }
enum VKind {
    Top,                               // any Helm value; join-absorbing
    Root,                              // the chart root object $
    Ref(Place),                        // symbolic contents of a place
    Scalar(ScalarAbs),                 // type flags + known consts (capped, then ConstTop)
    Object { entries: BTreeMap<Name, AbsVal>, rest: Rest }, // Rest::Closed | Open(AbsVal)
    Array  { item: AbsVal, prefix: Box<[AbsVal]> },
    Fragment(FragId),                  // a rendered abstract-document fragment
    Union(Box<[AbsVal]>),              // canonical: fp-sorted, deduped, flat, no Top, len ≥ 2
}
```

- `Object{rest}` **subsumes** v1's separate `Overlay` (open rest = fallback),
  deleting a normalization class.
- Join is defined on canonical forms (associative/commutative/idempotent —
  property-tested), and **`Top` absorbs**. Today's evaluators *drop* Unknown
  from choices, which silently converts "x or something unknown" into "x" —
  the wrong direction whenever a consumer derives exclusivity (e.g. treating
  a key set as exhaustive). Positive evidence (reads) is harvested into the
  evidence channel when the operand was evaluated; nothing is lost by
  absorbing.
- **Widening is specified, not vibes**: recursive `include` ⇒ `Top` + gap
  record + memo poisoning for the cycle; `range` bodies with mutation ⇒
  re-execute the body transfer to env-fixpoint, widening changed entries
  after k iterations; const-set and union-width caps. All bounds are policy
  data.

**The environment** — a value with state-passing and explicit join (v1's
`&EvalEnv` signature could not express Go-template semantics: `=` assigns in
the *defining* scope and persists past `end`; branches require joining
out-states):

```rust
pub fn eval_expr(expr: &Expr, env: &EvalEnv, cx: &mut Cx) -> AbsVal;
pub fn eval_node(node: NodeId, env: EvalEnv, cx: &mut Cx) -> EvalEnv;

impl EvalEnv {  // Arc-linked tiny frames; scopes are shallow — no HAMT crates
    pub fn child(&self) -> EvalEnv;
    pub fn declare(&self, n: Name, v: AbsVal) -> EvalEnv;     // :=
    pub fn assign(&self, n: &Name, v: AbsVal) -> EvalEnv;     // =  (defining scope)
    pub fn join(&self, other: &EvalEnv) -> EvalEnv;           // ptr_eq fast paths
}
```

Conditional mutation joins as a *guarded* entry (predicate-tagged union),
which the polarity table licenses collapsing toward over-approximation when a
flat answer is required; must-style facts (key definitely present) take the
meet. Effects accumulate into `cx` (not returned per node — allocation
discipline), with `cx.capture(|cx| …)` scoping at summary boundaries.

**The central artifact: abstract documents.** The interpreter does not emit
"facts about a tree someone else tracks" — for each manifest-mode template it
*builds the abstract rendered manifest*:

```rust
pub enum AbsDoc {
    Map  { entries: Vec<(KeyForm, AbsDoc)> },
    Seq  (Vec<AbsDoc>),
    Lit  { text: Name, style: ScalarStyle },
    Hole { value: AbsVal, shape: RenderShape },          // {{ expr }} at a value position
    Cond { arms: Vec<(PredRef, AbsDoc)> },               // if/with incl. else arms
    Iter { source: AbsVal, binder: IterBinder, body: Box<AbsDoc> },
    Splice { frag: FragId, indent: IndentContract },     // toYaml / include output
    StrRegion(Vec<StrPart>),                             // block scalars, partial tokens
    Opaque { gap: AttributionGap, reads: Box<[ReadId]> },
    Docs (Vec<AbsDoc>),                                  // --- multi-document
}
pub enum KeyForm { Lit(Name), Dyn(AbsVal) }              // {{ $k }}: v
```

This single structure makes the formerly hardest problems *constructors or
projections*:

- **Sink attribution** = the spine from root to a `Hole`; its path condition
  = conjunction of `Cond` arms on the spine (stored once per node, not
  snapshotted per fact). Exactness has a checkable contract: action is a
  complete YAML event; ancestors literal or resolved-dynamic; **splice indent
  contracts verified against syntactic position, not trusted**; block-scalar
  interiors are *exact string sinks* (positive `string` evidence, per
  polarity); document membership decidable. Anything else is `Opaque{gap}` —
  reads recorded, path exists as `Any`, no resource evidence, one gap record.
- **Resource identity** = projecting top-level `apiVersion`/`kind` entries
  through `Cond`/`Splice` into a guarded decision list `Vec<(PredRef,
  Ident)>` — the same predicates as everywhere (v1's separate
  `Alternatives<T>` is deleted). Because identity is a projection over the
  *abstract value of the document*, a template whose entire body is
  `{{ include "nats.loadMergePatch" … }}` resolving to a file-fragment
  StatefulSet gets a real identity — syntax-level detection cannot do this.
- **`kind: List`** = matching `items` under the projection and rebasing
  spines. **Multi-doc and per-iteration documents** (`range` around `---`)
  get `DocId = (FileId, doc-index, IterCtx)` with `AnyItem` abstraction.
- **K8s anchoring** = synthesis co-walks `AbsDoc` with the oracle's schema
  document — no byte cursors, no line ingestion.

The honest caveat survives from v1, now measurable: the fused grammar's
fidelity on templated YAML decides the `Opaque` rate. The migration gate is
an **abstained-type-enrichment budget** — no corpus chart may lose a single
type enrichment relative to the current tool — and the current
`yaml_shape` tracker may live on as an *upgrader* (`Opaque → exact` only when
consistent with structural evidence), never as a silent primary.

**Builtins are a table, not code sprawl.** Every Helm/Sprig function gets a
row: transfer function, evidence emitted, or principled abstention. Load-
bearing rows: `default` (admits `FalseySet` of the *tested, pre-transform*
place — over-approximate when the transform's falsey inverse image is
unknown), `set/unset/merge/mergeOverwrite` (env mutation + openness facts),
`toYaml/tpl-on-literal/include` (fragments with provenance), `tpl` on
non-literal ⇒ `Top` + gap, `lookup` ⇒ `Top` by definition (cluster state)
with an origin record, `.Files.Get` with statically resolvable path ⇒ parse
that file as a fragment document (the chart set is a pure input — the nats
pattern), `required`/`fail` ⇒ **abort evidence** `{place, pc, message}`,
`semverCompare`/capability shims ⇒ `Env` atoms, string ops ⇒ string-typed
with provenance.

**Helper summaries** — same interpreter, memoized, with a soundness contract
v1 lacked: computed under **empty path condition** and re-guarded at the call
site (no guard contamination across memo hits); keyed by
`(HelperId, Fp128)` where the fingerprint is taken over the **env-closed,
canonicalized** argument (a `Ref` into mutable values state is closed against
the current overlay, or the key includes a values-epoch); the summary is
`{ value, doc_fragments, evidence, env_delta }` and `env_delta` composes into
the caller conditionally under the call-site predicate. Recursion ⇒ `Top` +
poisoned memo + gap. The define namespace is **global across the chart set**
with Helm's parse-order-wins collision rule reproduced deterministically and
a diagnostic on differing-body collisions; a helper's values-prefix view is a
property of its *argument environment*, never of its defining chart (this is
what keeps per-chart analysis compositional under signoz-style cross-chart
helpers). File templates are indexed under their Helm path names so
`include (print $.Template.BasePath "/configmap.yaml")` resolves;
`$.Template.*` are evaluable constants.

**Output:**

```rust
pub struct ChartAnalysis {
    pub docs: Vec<(DocId, AbsDoc)>,
    pub identities: Vec<(DocId, Vec<(PredRef, Ident)>)>,
    pub evidence: Vec<EvidenceRecord>,   // non-document-anchored: aborts, type
                                         // evidence from string ops, admissions,
                                         // openness facts — each with pc + provenance
    pub gaps: Vec<Gap>,                  // parse, attribution, recursion, budget
    pub preds: PredTable, pub sources: SourceMap, pub helpers: HelperTable,
}
```

Purity contract (CI-enforced): no IO deps, no `dyn`, deterministic — a
100-run byte-identical property test is part of the crate's acceptance
criteria. Serialization rule: derived serde on internal graphs is forbidden;
the only serialized analysis artifact is a flat DTO projection (which doubles
as the migration-era `ValueUse` fixture format).

### 6.3 `chart` — the chart object model (IO)

What a chart *is*: discovery over `vfs` (directory, `.tgz` into MemoryFS —
which structurally prevents zip-slip onto the real filesystem; keep that
property — under a `LoadBudget`), dependency aliasing and recursion, and:

- **`FileRole` assignment** — `Manifest | Notes | Test | Partial |
  FileFragment | Crd | ShippedSchema`, by packaging rules. Roles drive parse
  mode and policy (`--exclude-tests` becomes a role filter); `files/**` are
  parseable fragment sources, not opaque bytes; `crds/` are plain YAML by
  Helm's rules (never templated).
- **Chart.yaml model** — dependencies with `alias`, **`condition:` paths and
  `tags:`** (each condition contributes boolean type evidence *and* a
  chart-level predicate conjoined onto every fact from that subchart; `tags`
  and `global` are reserved root keys the schema must always admit),
  `kubeVersion` constraints (an input to the capability oracle), and
  `import-values`/`export-values` — modeled if trivial, otherwise the
  affected subtree abstains with a structured gap (silence here would mean
  silently wrong prefixes).
- **`compose_values`** — the two-pass global hoist/mirror as a pure named
  function over typed YAML (parsed with a maintained, span-preserving YAML
  crate; `serde_yaml` is unmaintained and span-less). Anchors/aliases
  resolved. JSON values files accepted.
- **`ValuesModel`** — per-path defaults (with spans) and descriptions
  (`@param`, helm-docs conventions). Invariant, stated precisely this time:
  values files contribute defaults, descriptions, and **top-level key
  existence** (an explicit, named policy — today's behavior, which v1's
  "metadata only" wording accidentally contradicted) — never types, shapes,
  nullability or requiredness for nested paths.
- **Chart-local knowledge extraction** — `crds/` parsed into an in-memory CRD
  index (every served version), shipped `values.schema.json` collected per
  chart. Both feed §6.4/§6.5.

### 6.4 `knowledge` — planner/executor over data-described sources

v1's catalog traits + decorator combinators are gone. Policy is data; the
mechanism is two functions. (Decorator stacks also made the most subtle
policy — version-major vs mirror-major probing — an invisible nesting
property, and scattered "what was tried" across layers.)

```rust
pub struct SourceSpec { pub id: SourceId, pub base: Base /* Url | Dir */,
                        pub kind: SourceKind, pub priority: u8 }
pub enum SourceKind {
    K8sBundle  { versions: VersionChain },     // explicit + auto-fallback window
    CrdCatalog { loose: bool },                // loose ⇒ cross-version scan + hint
    LocalDir,                                  // override layer; never wiped
    ChartCrds(Arc<ChartCrdIndex>),             // §6.3 — in-memory, version-exact
}

pub struct Probe { pub source: SourceId, pub rel: RelPath,   // RelPath: validated newtype
                   pub url: Option<Url>, pub version: Option<K8sVersion> }

/// Pure: the exact probe order is an explicit, unit-testable sort.
pub fn plan(c: &ResourceCoordinate, cfg: &CatalogConfig, inv: &StoreInventory) -> Vec<Probe>;

/// The ONLY place the cache-is-not-an-oracle rule lives:
/// mem → disk → negative-marker → fetch (NetMode permitting); a negative
/// record is only constructible from a NotFound404 witness; persist failure
/// degrades to Unknown (never Found); Absent only when every relevant probe
/// answered authoritatively. Returns the executed trace.
pub fn execute(plan: &[Probe], store: &CacheDir, fetch: &dyn Fetch,
               mode: NetMode, policy: &FetchPolicy) -> (Outcome, LookupTrace);
```

- **Diagnostics are a projection of `LookupTrace`** — `MissingSchema` with
  versions tried, mirrors consulted, stale-cache hints, fallback-resolution
  notes falls out of the executed plan instead of being threaded through
  wrappers.
- **Four states internally, three at the surface.** Per-source outcomes keep
  today's distinctions (`Found / PathUnresolved / DocMissing / NotOwned`) —
  the chain's precedence depends on "owned but path-less" vs "not mine".
  Aggregation resolves precedence first; only the chain-final answer is
  exposed as `Lookup` through the oracle. (Collapsing early would change
  CRD+K8s overlap behavior and kill the deliberate silent-coverage-gap
  contract.)
- **`CacheDir` is concrete** (no store trait): tri-state
  `get → Hit | KnownAbsent | Miss` as one atomic question, fallible `put`,
  an explicit *read-bypass-write-refresh* mode (`--no-cache` parity), layout
  versioning, XDG resolution provided by the CLI (libraries never read env).
- **No materialization.** `resolve()` returns lazy-ref `SchemaDoc`s;
  cross-document refs are `RefTarget::External(coordinate)` resolved through
  further oracle calls. Descent is O(path) with a cycle set. This deletes the
  per-resource `$ref`-expansion cache — the dominant knowledge-side RSS term.
- **Capability oracle** = a thin adapter over the same planner/executor probe
  plus the declarative `ProbeTable` (the documented well-known-kind debt,
  now diffable data) and `kube_version()` from configuration — preserving the
  tri-state offline contract verbatim, testable against a fake store without
  any chain.
- **apiVersion advisor** (cache scan / shortlist / online probe aggregation)
  stays a quarantined module behind its `AdvisorPolicy` data, off by default.

### 6.5 `synthesis` — evidence → schema, under one policy

**Stage A — evidence derivation.** One projection
`derive(per ChartAnalysis, ValuesModel, shipped schemas, oracles, policy) →
BTreeMap<ValuePath, EvidenceSet>`:

- Evaluate `Env` atoms against the oracles (three-valued); compute branch
  liveness. **Type evidence from guarded alternatives is the join over all
  possibly-live branches** — first-live selection is only legal when the
  oracle authoritatively refutes the others (v1 got this wrong); falsey
  admissions from any possibly-live branch are admitted (polarity).
- Co-walk each `AbsDoc` (live arms) against resolved `SchemaDoc`s; every
  exact `Hole` spine yields resource-anchored evidence; `Splice` of a values
  object onto a known base document yields **overlay evidence**
  (`OverlayOnto{base, at, mode}`) rather than verbatim anchoring — the
  deep-partial reality of `mergeOverwrite` charts.
- Fold in `ValuesModel` defaults/descriptions, shipped subchart schemas
  (rank-bearing evidence on their prefix), Chart.yaml condition evidence,
  abort evidence, and the tpl-route marker (paths through
  `tpl`/`tplvalues.render` additionally admit `string` — a named widening
  rule, structurally derived from the helper's `typeIs "string"` branch).

**Stage B — per-path decision.** `decide(path, EvidenceSet, &SynthesisPolicy)
→ (SchemaExpr, Vec<Decision>)`. The policy object is the entire rulebook:
source ranks (reconcile-by-widening, never override), widening rules
(FalseySet → nullable/empty variants), scalar restriction, openness rules
(incl. reserved keys `global`/`tags`), P2 closure policy, required rules.
**Requiredness** is rebuilt on abort evidence: `required(p)` only with a
render-failure witness whose path condition is true when `p` is absent —
conservatively, an unguarded `required`/`fail`/nil-deref; guarded aborts may
emit draft-07 `if/then` or a diagnostic per policy. (Today's
truthy-header heuristic is unsound under §2 and survives only as an opt-in
legacy policy.) Every `Decision` records the evidence it used — spans make
diagnostics precise today and leave `--explain` as a cheap projection later
(explicitly out of v1 scope).

**Stage C — the two-tier schema model.** The honest resolution of "typed
algebra vs upstream JSON" (v1's open question; the pre-mortem's top risk):

- **Foreign tier:** upstream K8s/CRD subtrees remain verbatim
  `Arc<Value>` behind `SchemaDoc` pointers — *never* lifted into a closed
  enum, so nothing (`patternProperties`, `x-kubernetes-int-or-string`,
  `oneOf` discriminators, vendor extensions) can be dropped by a lossy
  round-trip. Operations needed on foreign subtrees are **total named JSON
  functions** with corpus tests: `restrict_to_scalar`, `partialize`
  (recursively strip `required` — the overlay operator), `ensure_metadata`,
  `is_open`, `admits_type`.
- **Typed tier:** a small closed `SchemaNode`
  (`Any/Scalar/Object/Array/Union` + inline `Meta`) for everything we
  *synthesize* from chart evidence, with a total, law-stated merge
  (commutative/associative/idempotent on canonical forms; conflicts returned
  **as data** — `MergeConflict{at, left, right, rule}` — and turned into
  positioned diagnostics by the caller; `ScalarConst` has a lawful total
  `Ord`, NaN rejected at construction).
- **Composition:** `SchemaExpr = Node(SchemaNode) | Foreign(ref) |
  Compose(rules)`; lowering emits draft-07 and applies the K8s-aware
  foreign×typed merge rules (today's `merge.rs` semantics — required-union,
  enum-intersection, preserve-unknown-fields — formalized as named rules with
  property tests over generated schema-shaped JSON and corpus differential
  tests against the current tool's output). This keeps today's clean merged
  output shape (no `allOf` nesting regressions for editors).

**Emission** is `fn emit_draft07(...) -> Value` (a function; a 2020-12
emitter is a second function when Helm 4 demand arrives) plus an explicit
**consumer matrix** the tests pin: helm 3 (gojsonschema draft-07 semantics,
per-chart validation of *coalesced* values with `global` injected — hence the
reserved-keys rule), helm 4's validator, yaml-language-server (descriptions/
defaults survive minification; flattened refs keep editors offline-capable),
and Helm's null-deletes-key coalescing (why `anyOf [null, T]` is the nullable
encoding).

### 6.6 Facade and CLI

The facade exposes the stage functions plus one `generate(&Config) ->
Result<GenerateOutput, PipelineError>`; `Config` is a plain struct with
defaults (no builder ceremony), `GenerateOutput { schema, diagnostics }`
(tools wanting `ChartAnalysis` call the stage functions; the analysis types
are explicitly semver-unstable). It owns:

- **Parallelism, honestly:** parse+interpret fan out per template (order-
  preserving collect; shared helper-summary memo in a concurrent map — racy
  recomputation is benign because summaries are pure; never evaluate while
  holding a shard lock); knowledge prefetch parallelizes IO discovered by a
  first co-walk pass; synthesis/emit/minify remain serial. Expected win is
  ~1.5–2× on large charts — the bigger levers are lazy schema docs and
  allocation discipline, and the doc says so instead of promising "zero
  locks".
- **Output passes as sequential typed functions** (not a transform trait):
  override merge (replace-on-`$ref` markers, override-file-relative base
  URI, `--keep-refs` honored), ref flatten (via the `jsonschema` crate's
  retriever interface directly, wrapped in `FetchPolicy`), description strip,
  minify (delegates to `json-schema-minify`, unchanged), **global-schema
  mirroring into subcharts** (a named pass — v1 lost it), and the
  **postcondition**: composed defaults validate against the emitted schema
  (hard diagnostic on failure).
- **The lockfile** (`--locked`): coordinate → content digest + source URL +
  version; re-fetches that would change a pinned digest fail. This is what
  "reproducible over time" means; in-run determinism is §2's
  given-identical-oracle-answers guarantee.
- The CLI maps ~30 flags onto the policy/config objects (the mapping is a
  checked table, §13), interprets env vars (`HELM_SCHEMA_*`, XDG) — libraries
  never read the environment — renders diagnostics as text or the versioned
  JSON envelope, and implements the exit-code policy: 0 clean; distinct codes
  for parse-failure / generation-failure; `--fail-on=gaps|conflicts` upgrades
  recorded abstentions.

## 7. Why this is the right architecture

**Because every rule now has a reason.** The correctness contract plus
polarity table turn a dozen scattered instincts (why `eq` guards widen to
`anyOf[enum, string]`, why unknown capability branches stay live, why
`required` is dangerous) into derivable, checkable consequences. This is the
difference between "preserve ambiguity" as a slogan and as a type-checked
direction.

**Because the abstract document is what a template *is*.** A Helm template
denotes a YAML document with holes, conditions, iterations and splices. Every
hard sub-problem the current code solves with side machinery — byte-cursor
resource tracking, indent heuristics, List descent, branch preservation,
helper-output projection — is a constructor or projection of that one
structure. The system that models its true subject needs less code, and the
review's strongest signal was two independent derivations (mine and the
soundness critic's) converging on it.

**Because one interpreter is the only enforceable home for "no heuristics".**
Six evaluators each understanding 80% of Helm is the documented root of every
recurring bug class. One lattice + one builtin table means a new Helm
semantic is one row, every consumer inherits it, and there is exactly one
place a text-sniffing shortcut could creep in — the place reviewers watch.

**Because policy-as-data beat traits on their own turf.** v1 already
preached restraint and still shipped 12 ports; the honest audit left 3. The
knowledge planner/executor is the showcase: probe order becomes a unit-tested
sort instead of a decorator-nesting accident, the tri-state contract is
proven once in one executor instead of per-wrapper, "what was tried"
diagnostics are the executed plan, and new sources (mirror, vendored dir,
chart-local CRDs, airgap) are config values, not types.

**Because the two-tier schema model refuses a fight it cannot win.** Upstream
corpora are arbitrary JSON Schema; a closed algebra either drops keywords
silently (a regression invisible until a user hits it) or grows the JSON
escape hatch that swallows the design. Keeping foreign content verbatim with
total, corpus-tested operations — and reserving the lawful typed algebra for
what we synthesize — gets property-testing where it's possible and
losslessness where it's mandatory.

**Because the boundaries are load-bearing, not aesthetic.** Two pure crates
(different deps, different rates of change), two IO crates, one vocabulary
crate whose placement of the oracle traits keeps both dependency arrows
pointing inward, grammar isolated for build/lint reasons, facade thin for
rebuild speed. Each crate has acceptance criteria (§11); none exists to
decorate a diagram.

**Alternatives rejected** (with the reasons on record): render-then-infer
(samples can't cover guard space; rendering is a *test oracle only*);
annotation-driven schemas (already rejected by the README; enforced by type
here); per-path profiles joined in the interpreter (destroys provenance and
bakes policy into analysis); a closed typed model for upstream schemas (see
above); decorator catalogs (see above); maximal trait-per-stage pipelines and
a "pipeline object" (ceremony; stage functions deliver the substance);
per-session interning (breaks Ord/serde/`'static` for marginal wins — typed
segments and Arc-shared nodes remove the churn that motivated it).

## 8. Cross-cutting policies

1. **Errors:** typed `thiserror` enums per crate; `PipelineError` is
   stage-tagged; **abstaining subsystems contribute no error variants** —
   knowledge failures are `Unknown` + diagnostics by design; eyre only in
   `main`.
2. **Diagnostics:** one model in core with spans; deduplicating concrete
   handle; versioned JSON envelope as a stable machine interface; pure crates
   record gaps as data, the facade projects.
3. **Determinism:** ordered collections at boundaries; no env reads, clocks,
   or randomness in libraries; byte-identical output given identical oracle
   answers; cache state moves output only toward widening; lockfile for
   cross-time reproducibility.
4. **Security:** `FetchPolicy` on every network/file edge (flatten retriever
   included: scheme/host allowlists, link-local denial, size/time budgets,
   ref depth/doc caps); `LoadBudget` on archives/parsing/interpretation;
   `RelPath` validated newtype at the coordinate→path boundary; archive
   extraction stays memory-backed.
5. **Serde boundary:** internal graphs never derive `Serialize`; canonical
   `Display`/`FromStr` for paths; DTO projections for analysis dumps and
   fixtures.
6. **Performance:** lazy `$ref` docs (no materialized expansion), Arc-shared
   fingerprinted values, effects-into-context, predicate hash-consing, and
   honest Amdahl accounting; Perfetto/`tracing` stays the profiling truth,
   plus a `[profile.profiling]` with symbols (the release profile strips
   them). Budgets double as DoS bounds.
7. **Lints/deps:** `[lints] workspace = true` in every crate;
   tree-sitter/`cc` only under grammar; `ureq` (pinned TLS feature) only in
   knowledge; `jsonschema` only in the facade; `serde_json` absent from
   `analysis`; the YAML parser is a maintained, span-preserving one.

## 9. Testing architecture

1. **Law tests (property-based):** value-lattice join laws on canonical
   forms; typed-merge laws + conflict totality; predicate algebra (NNF
   round-trips, three-valued evaluation monotone in assumptions); FalseySet
   widening monotonicity.
2. **Transfer-function tables:** snippet → expected `AbsDoc`/evidence, one
   row per builtin and construct (`default`, `with…default`, `set` in
   helpers, map ranges, `tpl`, `.Files.Get`, `required` under else,
   `semverCompare` chains, dynamic keys, List envelopes…). These pin each
   semantic exactly once.
3. **Contract suites:** the executor's tri-state honesty (cold cache ⇒
   `Unknown`; offline never fetches; `Absent` only on negative witness;
   persist-failure ⇒ `Unknown`; error ⇒ `Unknown` + diagnostic) run against
   real-dir and fake stores; oracle fakes for synthesis.
4. **Golden full-schema equality** over the real-chart corpus (the project
   standard, unchanged), hermetic via recorded catalog fixtures.
5. **Differential validation:** render fixtures with `helm template` under
   default + guard-flipping samples; every accepted sample must validate
   (the §2 soundness probe, and the P1 acceptance gate); run `helm lint` too
   (consumer matrix).
6. **Gate tests** (pre-merge for the respective migration steps): the
   abstained-enrichment budget (no corpus chart loses a type enrichment vs
   the current tool); 100-run byte-determinism of `ChartAnalysis`;
   security regressions (traversal coordinates, oversized archives,
   metadata-endpoint `$ref`s).

## 10. Acceptance criteria per crate (definition of done)

- **core:** law tests green; zero IO deps; `Display`/`FromStr` round-trips
  pinned.
- **analysis:** transfer table green; determinism property; purity CI check;
  parse-failure degrades per-file; budget enforcement observable as gaps.
- **chart:** corpus discovery parity (incl. `Chart.template.yaml`, tgz,
  aliases); compose_values fixtures; conditions/tags/crds/shipped-schema
  extraction tested; budgets enforced.
- **knowledge:** contract suite green; probe-order unit tests; trace→
  diagnostics projection parity with today's `MissingSchema` richness;
  advisor off by default.
- **synthesis:** law + corpus tests on foreign operations; decision records
  present for every narrowing; required-rules sound under §2 (witness-based).
- **facade/cli:** golden corpus equality; postcondition active; parity
  checklist (§13) checked off; exit-code table tested.

## 11. Sizing sanity check

This lands *smaller* than today's ≈29K LOC: one interpreter replaces six
evaluators + `helper_eval` (≈5–6K of near-duplicates); the planner/executor
replaces two provider monoliths + chain (≈2K → ≈0.6K); `AbsDoc` projections
replace the byte-cursor/indent machinery; the policy object replaces the
generator's scattered conditional lattice — while adding spans, the predicate
algebra, security budgets, and chart-local knowledge.

## 12. Migration correspondence (informative)

Sequencing remains owned by `next-priorities.md`; the in-flight
single-abstract-interpreter phases land directly on this design (its
`AbstractValue`/`Effects`/`eval_expr` become §6.2's lattice/evidence/
interpreter with laws added). Strangler seams: `IrGenerator`,
`K8sSchemaProvider`, `ValueUseSink`, `HttpFetcher`. **One rule from the
pre-mortem: every migration step's completion criterion is that the module it
replaces is deleted** — no new crate while its predecessor lives, and the
`ValueUse` projection never gains a production consumer.

| Today | Target |
|---|---|
| `helm-schema-template-grammar` | `helm-schema-grammar` (unchanged role) |
| `helm-schema-ast` (parser, `TemplateExpr`, fuse) | `analysis::parse` (spans, typed headers, one parse) |
| `helm-schema-ast::values_comments` | `chart::ValuesModel` |
| IR evaluators (`expr_eval`, `binding`, `fragment_*`, `helper_*`, `bound_*`, walkers) | `analysis::interp` (lattice + builtin table + summaries) |
| `helper_eval.rs`, `resource_detector/locator` | identity projection over `AbsDoc` |
| `yaml_shape`, `rendered_yaml_context` | `AbsDoc` construction + exact-or-abstain contract (tracker survives only as upgrader until the budget gate passes) |
| `ValueUse` + postprocess | DTO projection of `ChartAnalysis` (fixtures only) |
| `helm-schema-k8s` providers/chain | `knowledge` planner/executor + sources-as-data |
| `capability_eval` + chain oracle | `CapabilityOracle` adapter + synthesis-side liveness |
| `inference/*` | quarantined advisor module |
| `helm-schema-gen` (lib/merge/required) | `synthesis` stages A–C + policy + two-tier model |
| CLI `chart.rs` | `helm-schema-chart` |
| CLI `schema_override.rs`, `flatten.rs`, mirroring pass | facade output passes (+ `FetchPolicy`) |
| CLI `lib.rs` pipeline | facade stage functions |
| `json-schema-minify` | unchanged |

## 13. Parity checklist (current behavior the design must reproduce)

Audited from the CLI arg modules and pipeline; each item has a named home
above: output file/stdout, `--compact`, `--strip-descriptions`,
`--keep-refs`, `--minimize`; `--k8s-version` list + `--k8s-version-fallback=
auto|N` (window 15, single-version validation) + `--strict-k8s-version`;
mirrors with per-source cache namespacing; cache dirs + XDG/env resolution
(CLI-side) + `--no-cache` read-bypass/write-refresh; `--offline`;
`--no-k8s-schemas`; CRD `strict|loose` lookup (+ cross-scan hint diagnostic),
CRD mirrors/cache/`--crd-override-dir` (never wiped) /
`--crd-cache-record-source` sidecars; removed-flag courtesy errors
(`--crd-catalog-dir`); `--api-version-guess`; `--override-schema` semantics
(replace-on-`$ref`, override-dir-relative resolution, keep-refs, required
union); top-level values-key seeding (named policy); global-schema mirroring
into subcharts; subchart values composition + library-chart scoping +
`Chart.template.yaml`; `--exclude-tests` (as role filter); extra values
files; `--infer-required` (legacy policy + new witness-based rules);
diag text/JSON formats (now versioned); `--trace-output`; defaults-validate
postcondition (new, but required for §2).

## 14. Open questions (the honest residue)

1. **Fused-parse fidelity / `Opaque` rate** on the corpus — measured before
   the attribution tracker is deleted (the budget gate exists for this).
2. **`env_delta` composition cost** for mutation-heavy helpers — whether the
   conditional-overlay representation needs further bounding in practice.
3. **`import-values`/`export-values`** — model vs structured abstention;
   needs a corpus survey of real usage.
4. **Lockfile format and scope** — per-chart vs per-workspace; interaction
   with mirrors and the negative cache.
5. **Template-rendered CRDs** (cert-manager pattern) — the
   knowledge-from-analysis feedback stage is sketched but deliberately
   deferred behind chart-local `crds/` support.
