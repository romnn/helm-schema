# helm-schema — from-scratch target architecture (v3)

Status: design document. No code change is implied by this file by itself.

This is the architecture I would build if helm-schema were rewritten today
from first principles. It is a *clean-room* design: the current
implementation and the other plan documents are treated as **evidence** (what
worked, what bit us, what real charts do), never as constraints.

**Revision history.**

*v2* was the result of an adversarial review of v1 by five independent
critics (minimalist architecture, program-analysis soundness, Helm/K8s domain
realism, Rust API/performance, security/evolution pre-mortem). Its
load-bearing changes: a formal correctness contract with a polarity table; an
abstract document model in the interpreter; a compositional predicate algebra
replacing the flat guard vocabulary; the trait census cut to three; a pure
lookup planner/executor for the knowledge layer; a two-tier schema model
(foreign JSON verbatim, typed algebra for synthesized content); the input
universe widened to real charts (`files/` manifests, `crds/`, Chart.yaml
conditions, shipped schemas); first-class security budgets; and corrected
Rust shapes (no interner, state-passing environments, fingerprinted
Arc-shared lattice nodes).

Two further external review rounds tightened v2 in place: an explicit
strict-core / assistance / overrides authority model with a cache-independent
advisor; values-overridable capability shims lowering to mixed `Values`/`Env`
predicates instead of oracle atoms; shipped `values.schema.json` reclassified
from evidence to **enforced constraints**; template-rendered CRD extraction
promoted to a pipeline edge; and the AbsDoc escape hatches absorbed into the
model (`Merged` lattice node, wildcard-anchored attribution).

*v3* (this revision) responds to a third review round that found the
remaining local maxima:

1. **The public seam moves from `ChartAnalysis` to a guarded `ContractIR`.**
   v2 exposed the abstract-document forest across the analysis→synthesis
   crate boundary, freezing a manifest-centric intermediate the next stage
   had to semantically reinterpret. v3 makes the abstract document **private
   to the engine**: everything resolution needs — anchored spines with path
   conditions, identity decision lists, overlay modes — is projected into a
   per-path constraint graph, which becomes the stable public artifact.
2. **One pure engine instead of two pure crates.** Parse, interpretation,
   contract projection, chart-local knowledge extraction, resolution and
   schema lowering co-evolve (every corpus feature touches several of them),
   so they are one crate; the only hard product boundaries left are the
   external knowledge resolver and the IO shell. Today's plumbing-heavy
   `generate_values_schema_full_with_*` arity ladder is what a premature
   public boundary on this seam looks like.
3. **Guarded typing is a precision ladder, not a blanket weakening.** v2's
   P1 overstated JSON Schema's limits: predicates decidable over the values
   document alone (`Values` atoms) can be *lowered* to draft-07
   `if`/`then`/`dependencies` at the nearest common ancestor under policy
   bounds; only environment-dependent and opaque predicates must widen.
4. **Chart-authored contracts purged of "evidence" framing everywhere**
   (shipped schemas are constraints, full stop), and the chart-local
   knowledge pass is specified as a single forward edge — with the argument
   for why no fixed-point iteration is needed.
5. **The output contract defaults to a bundled, self-contained document with
   internal `$defs`**; full flattening becomes an explicit export mode. v2
   had inherited today's flatten-by-default, which inflates output on the
   hot path and then re-deduplicates it in the minifier.

Relationship to other plan documents: `single-abstract-interpreter.md`
remains the right *seed* for the interpreter and is generalized here;
`next-priorities.md` keeps ownership of sequencing — §15 records the route.

---

## 1. The problem, from first principles

helm-schema answers one question:

> Given a Helm chart, what is the most precise, structurally justified
> JSON Schema for its values contract?

Decomposing the question yields the system's natural domains. Note that
domains are *conceptual* — they map to modules; only domains with genuinely
independent consumers, dependencies, or rates of change become crates (§5):

| Domain | Question | Knowledge kind |
|---|---|---|
| **Chart model** | What is this chart made of? | Helm packaging conventions |
| **Syntax** | What does this text say? | Grammars |
| **Semantics** | What does this template mean? | Helm/Sprig evaluation semantics |
| **Knowledge** | What does Kubernetes expect here? | External + chart-local schema corpora |
| **Synthesis** | Given all constraints, what is the contract? | Decision policy + schema algebra |
| **Emission** | How is it written down? | JSON Schema dialects, consumers |

Two concerns cut across everything: **provenance** (every fact traceable to a
span and helper chain) and **ambiguity** (unknown and "several alternatives"
are first-class values at every layer). The rule that tells every layer
*which way to lean* when it doesn't know is the correctness contract.

## 2. The correctness contract (the spine of the design)

Fix an environment `E` (Capabilities, KubeVersion, Release, …) and a
knowledge corpus `K`. For a chart `C` with defaults `D` and user input `U`,
Helm validates the **coalesced** values `V = coalesce(D, U)` — and Helm's
coalescing deletes keys the user sets to `null`. The schema's domain is
coalesced documents. Define:

- `AccRender(C, E)` — values for which `helm template` succeeds. Non-trivial:
  deep access through a `null` parent fails rendering; `required`/`fail` fail
  it explicitly; and Helm validates each chart's coalesced values against
  that chart's **shipped `values.schema.json`** before rendering — shipped
  schemas are part of the accepted-set *definition*, not inference evidence
  (the §6.5 intersection rule follows from the contract, not from a
  precedence choice).
- `AccK8s(C, E, K)` — the subset whose emitted manifests (where identity is
  resolvable) validate against their resource schemas.

**Soundness obligation (hard).** For every `E` not refuted by the oracle:
`AccK8s(C, E, K) ⊆ L(S)` — the generated schema never rejects a working
configuration.

**Precision objective (soft).** Minimize `L(S) \ AccK8s`; every narrowing
must be justified by a provenance-carrying piece of evidence.

Two deliberate, *named* policies (not silent behavior):

- **P1 — guarded typing as a precision ladder.** A constraint that holds
  only under a path condition `g` is emitted at one of three precision
  levels, chosen per predicate class:
  1. **Lowered** — if `g` is decidable over the concrete values document
     alone (all atoms over `PlaceBase::Values`: `Eq`, `Contains`, `TypeIs`,
     scalar truthiness), it can be expressed in draft-07 as
     `if`/`then`(/`else`) or `dependencies` at the nearest common ancestor
     of the involved paths. This is *strictly more precise* and still sound:
     the guard partitions concrete values documents. Lowering is policy-
     bounded (discriminator-style `Eq`/`TypeIs` first; caps on count and
     nesting; opt-out), because conditional subschemas cost output size and
     degrade editor completion even where validation is correct.
  2. **Projected** (the default floor) — guard-insensitive per-path
     constraints: the constraint at `p` must accept every value that flows
     into any read of `p` in some accepted configuration. The differential
     harness's guard-flipping samples (§9) are the acceptance gate for this
     level, and they directly exercise lowered conditionals too.
  3. **Widened** — predicates containing `Env` atoms (oracle-dependent) or
     `Opaque` atoms are never lowered: their branches *join* (§6.5).
- **P2 — typo strictness.** Closing structurally enumerated objects
  (`additionalProperties: false`) rejects unread keys that `AccRender`
  technically accepts. Deliberate, diagnosable, off-switchable.

Two mechanical self-obligations: the chart's own composed defaults `D` must
validate against `S` (a pipeline postcondition — `helm lint` enforces exactly
this), and shipped subchart schemas must not contradict `S` on their prefix.

**The polarity table.** Every evidence kind may err in exactly one
direction; uncertainty always moves `L(S)` *up* (wider):

| Evidence | May err toward | Because |
|---|---|---|
| path existence (a read) | over-approx, but typed `Any` | a spurious path with a narrow type would reject |
| type evidence (sink schema, `typeIs`, string ops) | under-approx — omit when unsure | omitted type = `Any` = wide |
| falsey admission (`default`, `with`, guarded reads) | over-approx — admit when unsure | failing to admit a handled `null` rejects |
| object openness | over-approx openness; close only on proof + P2 | closing is the narrowing move |
| sink attribution | exact, or wildcard-anchored where the target schema is uniform; else abstain ⇒ no resource evidence | a wrong anchor imports wrong constraints |
| requiredness | drastic under-approx | §6.5; today's heuristic is provably unsound |
| branch liveness | unknown ⇒ live | pruning a live branch drops its admissions |
| conditional lowering (P1.1) | only on fully `Values`-decidable predicates | a lowered `Env` predicate would bake one cluster's truth into the schema |

Derived consequences used throughout: `Lookup::Unknown` never prunes and
never narrows; evidence-source precedence may never *override* (an override
can violate `D ∈ L(S)`) — disagreement reconciles by widening plus a
conflict diagnostic (this rule governs *inference evidence*; constraints
Helm itself enforces, i.e. shipped schemas, are part of `Acc` and compose by
intersection — §6.5); determinism is claimed **given identical oracle
answers**, and cache state may move output only in the widening direction
(reproducibility over time is the lockfile's job).

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
- **A premature public seam, demonstrated.** The IR→gen boundary is public,
  and the data it must carry kept growing — producing the
  `generate_values_schema_full_with_facts_and_descriptions(uses, provider,
  values_yaml, type_hints, chart_facts, values_descriptions)` arity ladder.
  This is direct evidence that the analysis→synthesis seam wants to be
  *inside* one engine, not between products.
- **Today's guard model cannot represent an `else` branch.** The walker
  restores scope and walks alternatives with *no* guard pushed; `not (eq …)`
  and `or (eq …) .b` are unrepresentable; `with (or A B)` is encoded by a
  convention downstream consumers must just know.
- **The knowledge crate is the best part** (real ports, a hard-won tri-state
  capability oracle) but providers are 400–900-line monoliths with duplicated
  cache/fetch/layout code, and the per-resource **materialized `$ref`
  expansion is a dominant RSS term** (temporal: 2s / 177MB) — compounded by
  an output pipeline that *flattens* refs by default and then re-deduplicates
  the blow-up in the minifier.
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
  templates instantiate in `crds/`; **cert-manager** renders CRDs *from
  templates* with literal `openAPIV3Schema` payloads — authoritative,
  version-exact schema sources the external catalogs often lack.
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

1. **One semantic engine.** Helm meaning lives in exactly one abstract
   interpreter, and everything that co-evolves with it — parsing, contract
   projection, chart-local knowledge extraction, resolution, lowering —
   lives in the same pure crate. Helper outputs, resource identity, guard
   extraction, defaults, fragments: all projections of one interpretation.
2. **The contract graph is the stable artifact.** The engine's public output
   is a guarded, provenance-carrying per-path constraint structure
   (`ContractIR`), not the manifest-shaped internals that produce it. The
   abstract document model is an internal mechanism.
3. **Typed and spanned everywhere.** Paths, predicates, coordinates, schemas
   are domain types; every node and fact carries provenance. Stringly forms
   exist only at serialization edges, as `Display`/DTO projections.
4. **Abstention is a value with a direction.** `Found/Absent/Unknown`
   lookups, `Top`/`Union` values, `Opaque` document regions — and the §2
   polarity table dictates what each consumer does with them.
5. **Ports at proven variation points; functions and data everywhere else.**
   Three traits total (§5). Catalog policy, synthesis policy, widening
   bounds, probe tables, builtin coverage, lowering bounds are *data*
   interpreted by small engines.
6. **Untrusted input is budgeted.** Charts, archives, upstream schemas and
   override files are attacker-controlled inputs; every edge that touches
   them takes an explicit `FetchPolicy`/`LoadBudget`.
7. **Parity is part of correctness.** The existing CLI surface and output
   behaviors are an explicit checklist (§13) with deliberate divergences
   named — not folklore.

## 5. System overview

### 5.1 Crates

Six crates (plus `json-schema-minify` and `test-util`, unchanged):

```
helm-schema-template-grammar   tree-sitter C grammars + build.rs only
                               (build isolation; exempt from workspace lints)

helm-schema-core               MINIMAL shared boundary types — only what both
                               sides of the engine↔knowledge seam must name:
                               ResourceCoordinate/GroupVersion/KubeVersion,
                               Lookup<T>, SchemaDoc (foreign JSON, lazy refs),
                               trait ResourceSchemaOracle, trait
                               CapabilityOracle, Diagnostic envelope DTOs,
                               RelPath, FetchPolicy/LoadBudget.
                               NOT a giant vocabulary crate: paths, predicate
                               algebra, lattice, contract types are
                               engine-owned.

helm-schema-engine             THE pure semantic engine. Modules, not crates:
                                 parse/     spanned typed TemplateTree
                                 interp/    lattice, env, summaries, abstract
                                            documents (PRIVATE), attribution
                                 contract/  ContractIR — the public artifact
                                 extract/   chart-local knowledge projection
                                            (CRD docs from analysis + crds/)
                                 resolve/   anchor co-walk against oracles,
                                            branch liveness, decision policy
                                 lower/     typed SchemaNode algebra, foreign
                                            composition, bundled draft-07
                               [deps: core, grammar, tree-sitter, serde,
                                serde_json. NO IO crates, no dyn fields —
                                THE purity boundary, CI-enforced]

helm-schema-knowledge          IO: exact external resolver only. Catalog
                               config as data, pure plan() + execute() +
                               LookupTrace, CacheDir, trait Fetch {Ureq,Mock},
                               capability oracle, quarantined advisor.
                               [deps: core, ureq. Never sees engine types]

helm-schema (facade)           the stable product surface + IO shell: chart
                               loading (vfs dir/tgz/mem + budgets, FileRole,
                               Chart.yaml model), values composition,
                               ChartProgram assembly, pipeline wiring +
                               parallel fan-out, override merge, optional
                               flatten export, minify, diagnostics
                               projection, postcondition checks, lockfile.

helm-schema-cli                thin binary: clap → Config (env vars
                               interpreted HERE) → facade → output/exit codes.
                               Kept as its own tiny crate so the facade
                               library stays clap-free without feature
                               gymnastics.
```

Dependency rules with bite (CI-checked): the engine has no IO crates and no
`dyn` dependencies; `core` is the only shared parent, so `knowledge` builds
in parallel with the engine and never sees tree-sitter *or engine types* —
the v2 sketch where the oracle returned analysis-flavored types is gone.
Every crate sets `[lints] workspace = true`.

**Trait census (complete).** `Fetch` (network edge; Ureq/Mock exist),
`ResourceSchemaOracle` and `CapabilityOracle` (the pure↔IO seam in `core`;
real second implementations: in-memory fakes and the `--no-k8s-schemas` null
oracle). Everything else — parser, chart source, emitter, transforms,
diagnostics sink, retriever, catalogs, layouts, stores — is a function, a
concrete type, or data.

**Why this graph and not v2's eight.** Two review rounds converged on the
same audit: the only *product* boundaries are (a) the pure engine vs the IO
world, (b) the external knowledge resolver, (c) the grammar's build
isolation. Parse/interpret/contract/resolve/lower co-evolve — the corpus
shows every feature touching several at once (tpl-admits-string: interpreter
marker + lowering rule; overlays: lattice node + co-walk + `partialize`;
shipped schemas: chart model + resolution) — and the current code's
`generate_values_schema_full_with_*` arity ladder is the fossil record of
publishing that seam too early. Internal phase boundaries stay (modules with
named types between them); they just stop being semver surfaces.

### 5.2 The pipeline

```
1. load_chart      (facade, IO)      dir/tgz/mem ──► ChartProgram
                                     {files+roles, Chart.yaml model, values
                                      docs, crds/, shipped schemas — all
                                      bytes loaded, zero IO beyond here}

2. analyze         (engine, pure)    ChartProgram ──► ContractIR
                                     parse* ∥ → interpret* (shared summary
                                     memo) → project anchors/identities/
                                     constraints; AbsDoc never escapes

3. knowledge       (engine + knowledge)
   extract_chart_local_knowledge(&ContractIR) ──► ChartCrdIndex
   resolve(&ContractIR, &dyn ResourceSchemaOracle, &dyn CapabilityOracle,
           &ResolvePolicy) ──► ResolvedContract
                                     branch liveness, anchor co-walk,
                                     shipped-schema intersection, decisions

4. emit            (engine lower/ + facade passes)
   lower(&ResolvedContract, EmitMode::Bundled | EmitMode::FlattenedExport)
        ──► draft-07 Value (self-contained, internal $defs by default)
   facade: override merge → optional flatten export → minify →
           postcondition-validate(defaults) → bytes + diagnostics
```

Every arrow is a named public type; the facade exposes the stage functions
individually — that, not a "pipeline object", is what lets tests, tools and
a future LSP run any prefix. The chart-local knowledge step is a **single
forward edge, not a fixed point**: knowledge never influences
interpretation (only resolution consumes oracles, by design), so
analyze → extract → resolve terminates in one pass. The only would-be loop —
CRDs *emitted under capability guards* whose liveness depends on the oracle —
is handled by registering unconditional CRD documents and abstaining on
guarded ones (§14).

## 6. The layers in detail

### 6.1 `core` — the minimal boundary vocabulary

Only what crosses the engine↔knowledge↔facade seams:

```rust
/// Typed coordinate; parsed once at the boundary, never re-split.
pub struct ResourceCoordinate { pub group: Option<Name>, pub version: Name, pub kind: Name }

#[must_use]
pub enum Lookup<T> { Found(T), Absent /* authoritative */, Unknown /* abstain */ }
// Combinators (map, and_then, or_unknown). Adapter rule: IO errors map to
// Unknown + diagnostic — never Absent, never panic.

/// Foreign JSON schema document with lazy refs — never materialized, never
/// lifted into a closed enum. Root + local definitions over
/// Arc<serde_json::Value>; cross-document refs are RefTarget::External
/// (coordinate) resolved through further oracle calls. Query functions over
/// it live in the engine; JSON here is an opaque payload.
pub struct SchemaDoc { /* root, defs, version metadata */ }

pub trait ResourceSchemaOracle: Send + Sync {
    fn resolve(&self, c: &ResourceCoordinate) -> Lookup<Arc<SchemaDoc>>;
}
pub trait CapabilityOracle: Send + Sync {
    fn has_api(&self, gv: &GroupVersion) -> Lookup<bool>;
    fn kube_version(&self) -> Lookup<KubeVersion>;  // from --k8s-version / Chart.yaml
}

/// Validated at construction: no '..', no absolute, segments ⊆ [a-z0-9._-].
/// The ONLY type the cache/url layer accepts — coordinates are
/// attacker-controlled chart text.
pub struct RelPath(String);

pub struct FetchPolicy { /* scheme/host allowlist, deny link-local/loopback,
                            size+time budgets, ref depth & doc caps */ }
pub struct LoadBudget { /* archive entries/bytes, parse file size,
                           interpreter step budget */ }

/// Concrete deduplicating handle (today's k8s sink generalized + spans).
/// Ordered by key ⇒ deterministic under parallelism; JSON output is a
/// versioned envelope (a stable machine interface).
pub struct Diagnostics(/* Arc<Mutex<BTreeMap<DiagKey, Diagnostic>>> */);
```

`Name` (a `SmolStr`-class content-stable small string — no interner exists,
so `Ord`/`Hash`/`Serialize` derive naturally and everything stays
`Send + 'static`), `ValuePath`/`DocPath`, the predicate algebra, the value
lattice, and `ContractIR` are **engine-owned**: knowledge never needs them,
so they do not belong in the shared boundary. (`Name` sits in core only
because `ResourceCoordinate` uses it.)

### 6.2 `engine::parse` — parse once, keep everything

`pub fn parse(file: FileId, src: &str, mode: ParseMode) ->
Result<TemplateTree, ParseError>` — a function, not a trait (tree-sitter's
`Parser` is `!Sync`; a `&self` port could not be implemented honestly). One
parse per file. The tree is fully typed and spanned; control-flow headers
arrive as parsed expressions; `Unknown` nodes (with spans) are the explicit
degradation for unparseable regions. A failed parse of one file is
**per-file recoverable**: the file contributes an opaque analysis plus a gap
record; the chart fails only if nothing parses (`--strict` upgrades gaps to
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

`ParseMode` comes from the facade's `FileRole` (§6.6): **manifest mode**
(fused YAML+template) vs **text mode** (template-only: NOTES.txt, config
file fragments) — text mode yields reads/predicates/aborts but no document
and no sinks.

### 6.3 `engine::interp` — the one interpreter (abstract documents internal)

**The predicate algebra** — one representation for path conditions, branch
guards, and capability decisions:

```rust
pub struct Place { pub base: PlaceBase, pub path: ValuePath }
pub enum PlaceBase {
    Values,           // schema-relevant
    Env(EnvRoot),     // Capabilities/KubeVersion/Release/Chart/Files — late
    Local(BindId),    // SSA-numbered local or rebound-dot snapshot
    Synth(ValueId),   // result of an expression (default-wrapped, merged, …)
}

pub enum Pred { True, False, Atom(Atom), Not(PredRef), And(Box<[PredRef]>), Or(Box<[PredRef]>) }
pub enum Atom {
    Truthy(Place),                 // Helm emptiness test
    Eq(Place, ScalarConst),
    TypeIs(Place, HelmType),
    Contains(Place, ScalarConst),  // membership (sprig `has` / `hasKey`)
    NonEmpty(Place),               // range body executes ≥ 1 time
    ApiVersionsHas(GroupVersion),  // leaf fact: .Capabilities.APIVersions.Has
    KubeVersionCmp(CmpOp, SemverReq), // leaf fact: semverCompare over KubeVersion
    Opaque(SpanId),                // unmodeled condition; valuation Unknown
}
```

`PredRef` is hash-consed (flattened, sorted, deduped at construction), so
predicates are cheap to share per control-flow frame and stable to
fingerprint. Evaluation is three-valued (Kleene) under partial assumptions:
abstract values for value atoms, the environment oracle for `Env` atoms
(evaluated *late*, in resolution), and probe assignments like `p := absent`.
Predicates carry a derived classification used by P1's ladder:
`ValuesDecidable` (all atoms over `PlaceBase::Values`, lowerable),
`EnvDependent`, or `Mixed/Opaque` (widen).

- `if`/`else if`/`else` arms carry `P₁`, `¬P₁∧P₂`, `¬P₁∧¬P₂` — else branches
  finally have conditions (today they have none).
- `with X` pushes `Truthy(place(X))` *and* binds dot to the same `Place`;
  null-tolerance of `a.b` read under `with .Values.a` is the entailment
  "`pc` is false when `a := null`" — by construction, not string matching.
- `null_tolerant(hole)`, `required(p)`, `live(branch)` are defined queries
  with a small sound entailment table (default answer `Unknown`, resolved by
  polarity).

`Env` atoms are deliberately *leaf facts* — exactly what the oracle can
answer, nothing more. Values-overridable capability shims (the bitnami
`common.capabilities.*` pattern, §3.2) are **not** special-cased into `Env`
atoms: they lower through ordinary interpretation into mixed predicates —
`(Truthy(V) ∧ Contains(V, gv)) ∨ (¬Truthy(V) ∧ ApiVersionsHas(gv))` for
`V = Values.apiVersions / Values.global.apiVersions` — so the `.Values.*`
dependence stays schema-relevant and an oracle refutation alone can never
prune a branch a user can re-enable through values: a predicate containing
`Values` atoms evaluates to `Unknown`, which §2 keeps live.

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
    Merged { base: AbsVal, over: AbsVal, mode: MergeMode }, // merge/mergeOverwrite/JSON-patch, kept structural
    Union(Box<[AbsVal]>),              // canonical: fp-sorted, deduped, flat, no Top, len ≥ 2
}
```

- `Object{rest}` subsumes a separate `Overlay` (open rest = fallback).
- `Merged` keeps `merge`/`mergeOverwrite`/JSON-patch relationships **in the
  lattice** instead of eagerly flattening them (normalization is lazy). When
  the base carries a resource identity (a file-fragment document) and the
  overlay is values-rooted, the anchor projection derives deep-partial
  anchoring *from this structure*.
- Join is defined on canonical forms (associative/commutative/idempotent —
  property-tested), and **`Top` absorbs**: today's evaluators *drop* Unknown
  from choices, silently converting "x or something unknown" into "x" — the
  wrong direction whenever a consumer derives exclusivity. Positive evidence
  (reads) is harvested into the evidence channel when the operand was
  evaluated; nothing is lost by absorbing.
- **Widening is specified**: recursive `include` ⇒ `Top` + gap + memo
  poisoning for the cycle; `range` bodies with mutation ⇒ body transfer to
  env-fixpoint, widening changed entries after k iterations; const-set and
  union-width caps. All bounds are policy data.

**The environment** — a value with state-passing and explicit join
(Go-template `=` assigns in the *defining* scope and persists past `end`;
branches require joining out-states):

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

Conditional mutation joins as a guarded entry (predicate-tagged union),
which polarity licenses collapsing toward over-approximation when a flat
answer is required; must-style facts (key definitely present) take the meet.
Effects accumulate into `cx`, with `cx.capture(|cx| …)` scoping at summary
boundaries.

**Abstract documents — internal.** For each manifest-mode template the
interpreter builds the abstract rendered manifest (`Map`/`Seq`/`Lit`/`Hole`/
`Cond`/`Iter`/`Splice`/`StrRegion`/`Opaque`/`Docs`, with `KeyForm::Lit|Dyn`
keys). This is the mechanism that makes the formerly hardest problems
constructors or projections — sink attribution is the spine from root to a
`Hole` with its path condition as the conjunction of `Cond` arms; resource
identity is the projection of top-level `apiVersion`/`kind` entries through
`Cond`/`Splice` into guarded decision lists; `kind: List` descent and
per-iteration documents (`DocId = (FileId, doc-index, IterCtx)`) are
structural. **But the document forest never crosses the engine boundary**:
everything downstream stages need is projected into `ContractIR` (§6.4).
That keeps the document representation free to evolve with parsing-fidelity
work without breaking any consumer.

Attribution has three tiers, with a checkable contract:

- **Exact** — action is a complete YAML event; ancestors literal or
  resolved-dynamic; splice indent contracts *verified against syntactic
  position, not trusted*; block-scalar interiors are exact string sinks
  (positive `string` evidence); document membership decidable.
- **Anchored** — a spine whose dynamic segments are wildcards
  (`KeyForm::Dyn` keys with unresolved value, unknown indices). Anchored
  sites contribute resource evidence only where the foreign schema is
  *uniform* at the wildcard (`additionalProperties` for keys, `items` for
  items) — sound by construction; recovers the ubiquitous
  `{{ $k }}: {{ $v }}` pattern (known finite key sets still refine to exact
  properties).
- **Opaque** — reads recorded, path exists as `Any`, no resource evidence,
  one gap record with an enumerated `AttributionGap` reason.

**Builtins are a table, not code sprawl.** Every Helm/Sprig function gets a
row: transfer function, evidence emitted, or principled abstention.
Load-bearing rows: `default` (admits a `FalseySet` of the *tested,
pre-transform* place — over-approximate when the transform's falsey inverse
image is unknown), `set/unset/merge/mergeOverwrite` (env mutation + openness
facts + `Merged` values), `toYaml`/`tpl`-on-literal/`include` (fragments
with provenance), `tpl` on non-literal ⇒ `Top` + gap, `lookup` ⇒ `Top` by
definition (cluster state) with an origin record, `.Files.Get` with
statically resolvable path ⇒ parse that file as a fragment document (the
chart program is a pure input — the nats pattern), `required`/`fail` ⇒
abort evidence `{place, pc, message}`, `semverCompare` over
`.Capabilities.KubeVersion` ⇒ `Env` leaf atom — but values-overridable
capability shims lower to mixed `Values`/`Env` predicates, never to oracle
state; string ops ⇒ string-typed with provenance.

**Helper summaries** — same interpreter, memoized, with a soundness
contract: computed under **empty path condition** and re-guarded at call
sites; keyed by `(HelperId, Fp128)` over the **env-closed, canonicalized**
argument (a `Ref` into mutable values state is closed against the current
overlay, or the key includes a values-epoch); the summary is
`{ value, doc_fragments, evidence, env_delta }`, with `env_delta` composed
into the caller conditionally under the call-site predicate. Recursion ⇒
`Top` + poisoned memo + gap. The define namespace is **global across the
chart set** with Helm's parse-order-wins collision rule reproduced
deterministically and a diagnostic on differing-body collisions; a helper's
values-prefix view is a property of its *argument environment*, never of its
defining chart. File templates are indexed under their Helm path names so
`include (print $.Template.BasePath "/configmap.yaml")` resolves;
`$.Template.*` are evaluable constants.

### 6.4 `engine::contract` — `ContractIR`, the public artifact

The stable seam between interpretation and everything downstream — and the
engine's primary public type. It is a guarded constraint graph over the
values space, plus the chart's resource claims; *not* a manifest model:

```rust
pub struct ContractIR {
    /// Per-path guarded constraints, each with provenance.
    pub paths: BTreeMap<ValuePath, PathContract>,
    /// Resource claims: identity decision lists + anchor patterns projected
    /// from the (internal) abstract documents.
    pub resources: Vec<ResourceClaim>,
    /// Chart-local knowledge handles: extracted CRD documents (fully
    /// literal), shipped-schema references per chart prefix.
    pub chart_knowledge: ChartLocalKnowledge,
    pub gaps: Vec<Gap>,
    pub preds: PredTable, pub sources: SourceMap, pub helpers: HelperTable,
}

pub struct PathContract { pub items: Vec<Guarded<Constraint>> }
pub struct Guarded<T> { pub pc: PredRef, pub item: T, pub prov: Provenance }

pub enum Constraint {
    Read     { shape: RenderShape },                  // existence + scalar/fragment shape
    Anchor   { claim: ResourceClaimId, at: DocPathPattern, role: SinkRole,
               mode: AnchorMode /* Exact | Overlay(MergeMode) | Uniform */ },
    Admits   (FalseySet),                             // default/with admissions
    TypeEv   { hint: SchemaTypeHint, origin: HintOrigin },
    Abort    { kind: Required | Fail, message: Option<Name> },
    Open     { why: OpenObjectReason },
    Iterated { item: IterShape },
    ViaTpl,                                           // admits string (named rule)
}

pub struct ResourceClaim {
    pub doc: DocId,
    pub identity: Vec<(PredRef, Ident)>,              // guarded decision list
}
```

Design points:

- **Everything resolution needs is here.** Anchors carry the doc-path
  pattern (wildcards included), the overlay mode (from `Merged` values), the
  sink role, and a reference into the identity decision list — so the oracle
  co-walk works entirely from `ContractIR`. The abstract documents stay
  private.
- **Guards are stored once** (hash-consed `PredRef` per constraint), not
  snapshotted; cross-constraint correlation ("these twelve constraints share
  one conditional") survives.
- **Provenance is intrinsic** — spans + helper chains on every constraint;
  `--explain` and positioned diagnostics are projections.
- **Serialization rule:** internal graphs never derive `Serialize`; the only
  serialized form is a flat DTO projection (which doubles as the
  migration-era `ValueUse` fixture format, never a production consumer).
- Determinism: a 100-run byte-identical property test on the DTO projection
  is part of the engine's acceptance criteria.

### 6.5 `engine::resolve` + `engine::lower` — from contract to schema

**Resolution** (`resolve(&ContractIR, oracles, &ResolvePolicy) ->
ResolvedContract`) — pure given oracle answers:

- Evaluate `Env` atoms against the oracles (three-valued); compute branch
  liveness. **Type evidence from guarded alternatives is the join over all
  possibly-live branches** — first-live selection is only legal when the
  oracle authoritatively refutes the others; falsey admissions from any
  possibly-live branch are admitted (polarity). A predicate containing
  `Values` atoms evaluates to `Unknown` by construction, so user-overridable
  capability branches stay live regardless of oracle state.
- Resolve each `Anchor` against `SchemaDoc`s fetched through the oracle:
  exact anchors yield resource-anchored constraints; `Uniform` anchors
  contribute under the §6.3 uniformity rule; `Overlay` anchors yield
  **deep-partial** constraints (the `partialize` operator — recursively
  strip `required`) for the values-rooted overlay. Chart-local CRDs
  (extracted by `engine::extract` from `crds/` files and from fully-literal
  template-rendered CRD documents) are registered in the oracle composition
  ahead of remote catalogs before this step runs.
- **Shipped `values.schema.json` files are enforced constraints, not
  evidence.** Helm validates each chart's coalesced values against its own
  shipped schema at lint/install time, so shipped schemas are part of `Acc`
  (§2): they compose by **intersection** on the chart's prefix — through the
  foreign tier, with the same coalescing and global-injection care Helm
  applies — never by ranking against template evidence. Template evidence
  contradicting the shipped schema means the chart is broken at install
  time: a conflict diagnostic, not something to widen over. A policy switch
  disables the intersection for users who knowingly diverge.
- Fold in `ValuesModel` defaults/descriptions, Chart.yaml condition
  evidence (each `condition:` path contributes boolean type evidence and a
  chart-level predicate on all of that subchart's constraints), abort
  evidence, and `ViaTpl` (admit `string`).
- **Per-path decision under `ResolvePolicy`** — the entire rulebook as one
  inspectable value: widening rules (FalseySet → nullable/empty variants),
  scalar restriction, openness rules (incl. reserved keys `global`/`tags`),
  P2 closure policy, **P1 lowering bounds** (which predicate classes lower,
  count/nesting caps), and required rules. **Requiredness** is built on
  abort evidence: `required(p)` only with a render-failure witness whose
  path condition is true when `p` is absent — conservatively, an unguarded
  `required`/`fail`/nil-deref; guarded aborts lower to `if/then` under P1 or
  emit a diagnostic. (Today's truthy-header heuristic is unsound under §2
  and survives only as an opt-in legacy policy.) Every `Decision` records
  the evidence it used.

**Lowering** (`lower(&ResolvedContract, EmitMode) -> Value`) — the two-tier
schema model:

- **Foreign tier:** upstream and shipped subtrees remain verbatim
  `Arc<Value>` behind `SchemaDoc` pointers — never lifted into a closed enum
  (no round-trip risk for `patternProperties`,
  `x-kubernetes-int-or-string`, `oneOf` discriminators, vendor extensions).
  Operations on foreign subtrees are **total named JSON functions** with
  corpus tests: `restrict_to_scalar`, `partialize`, `ensure_metadata`,
  `is_open`, `admits_type`, `is_uniform_at`.
- **Typed tier:** a small closed `SchemaNode`
  (`Any/Scalar/Object/Array/Union/Conditional` + inline `Meta`) for
  everything synthesized from chart evidence, with a total, law-stated merge
  (commutative/associative/idempotent on canonical forms; conflicts returned
  **as data** — `MergeConflict{at, left, right, rule}` — positioned
  diagnostics attached by the caller). `Conditional` carries the P1-lowered
  `if/then/else` structures for `ValuesDecidable` predicates.
- **Output contract (deliberate change from today):** the default
  `EmitMode::Bundled` produces a **single self-contained draft-07 document**
  — foreign subtrees land under deterministic, collision-free `$defs` names
  (GVK-derived) and are referenced internally; shared K8s types appear once.
  No external `$ref` ever remains; no network or file dependency at
  validation time. `EmitMode::FlattenedExport` (full inlining — today's
  default shape) remains as an explicit export for consumers that reject
  refs. This deletes the current inline-everything-then-re-deduplicate cycle
  between flatten and the minifier; `json-schema-minify` remains as optional
  extra compaction. Internal-ref support is solid in both validators that
  matter (helm 3's gojsonschema, helm 4) and in yaml-language-server.
- The **consumer matrix** is pinned by tests: helm 3 (gojsonschema draft-07
  semantics — including `if/then` and `dependencies`, which P1 lowering
  relies on; per-chart validation of *coalesced* values with `global`
  injected), helm 4's validator, yaml-language-server (descriptions/defaults
  survive; validation of conditionals is supported, completion inside them
  degrades gracefully — one reason lowering is policy-bounded), and Helm's
  null-deletes-key coalescing (why `anyOf [null, T]` is the nullable
  encoding).

### 6.6 `knowledge` — the exact external resolver

Planner/executor over data-described sources; policy is data, the mechanism
is two functions. Depends on `core` only — it never sees engine types.

```rust
pub struct SourceSpec { pub id: SourceId, pub base: Base /* Url | Dir */,
                        pub kind: SourceKind, pub priority: u8 }
pub enum SourceKind {
    K8sBundle  { versions: VersionChain },     // explicit + auto-fallback window
    CrdCatalog { loose: bool },                // loose ⇒ cross-version scan + hint
    LocalDir,                                  // override layer; never wiped
    ChartCrds(Arc<ChartCrdIndex>),             // chart-local, in-memory, version-exact
}

pub struct Probe { pub source: SourceId, pub rel: RelPath,
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
  notes falls out of the executed plan.
- **Four states internally, three at the surface.** Per-source outcomes keep
  today's distinctions (`Found / PathUnresolved / DocMissing / NotOwned`);
  aggregation resolves precedence first; only the chain-final answer is
  exposed as `Lookup` through the oracle.
- **`CacheDir` is concrete** (no store trait): tri-state
  `get → Hit | KnownAbsent | Miss` as one atomic question, fallible `put`,
  an explicit read-bypass-write-refresh mode (`--no-cache` parity), layout
  versioning; XDG/env resolution happens in the CLI, never in libraries.
- **No materialization.** `resolve()` returns lazy-ref `SchemaDoc`s; descent
  is O(path) with a cycle set. This deletes the per-resource
  `$ref`-expansion cache — the dominant knowledge-side RSS term — and
  composes with bundled emission (§6.5), which shares each foreign subtree
  once instead of inlining it per use.
- **Version fallback is archive search, not target inference.** The chart
  pins its coordinate structurally; upstream bundles merely index schemas by
  K8s release. Walking older bundles for a removed-but-pinned GVK locates
  the authoritative definition of an *exactly named* coordinate. Guardrails:
  `ResolvedFromFallbackVersion` diagnostics, `--strict-k8s-version` opt-out,
  and fallback bundles are never inputs to apiVersion inference.
- **Capability oracle** = a thin adapter over the same planner/executor
  probe plus the declarative `ProbeTable` (the documented well-known-kind
  debt, now diffable data) and `kube_version()` from configuration —
  tri-state offline contract preserved verbatim.

**The authority model — strict core / assistance / overrides** (three layers
with different epistemic standing, never blended silently):

1. **The strict resolver** (everything above): exact coordinates against
   explicitly configured sources in explicit priority order; outcome typed
   `Resolved | Ambiguous | Unresolved` with the executed trace —
   `Ambiguous`/`Unresolved` flow to resolution as widening, never a forced
   pick. Boolean ownership probes (today's `has_resource`) are
   unrepresentable.
2. **The assistance layer** — the apiVersion advisor: opt-in
   (`--api-version-guess`), bounded, quarantined behind `AdvisorPolicy`
   data, and **cache-independent**: its evidence tiers are the static
   shortlist table and the authoritative online probe; the local-cache
   cross-scan is demoted to a hint diagnostic and never participates in the
   pick. Advisor-affected resolutions always carry an
   `InferredApiVersion`-style diagnostic.
3. **Explicit overrides** (`LocalDir`): user-authored schemas as first-class
   *inputs*, not silent recovery.

### 6.7 `helm-schema` (facade) and `helm-schema-cli`

The facade is the stable product surface and the IO shell. It owns:

- **Chart loading → `ChartProgram`**: discovery over `vfs` (directory,
  `.tgz` into MemoryFS — structurally preventing zip-slip; under a
  `LoadBudget`), dependency aliasing and recursion, **`FileRole`**
  assignment (`Manifest | Notes | Test | Partial | FileFragment | Crd |
  ShippedSchema`) driving parse modes and policy (`--exclude-tests` is a
  role filter), the Chart.yaml model (deps with `alias`, `condition:` paths
  and `tags:`, `kubeVersion` constraints; `import-values`/`export-values`
  modeled if trivial, otherwise structured abstention), `compose_values`
  (the two-pass global hoist/mirror as a pure named function over typed YAML
  parsed with a maintained, span-preserving YAML crate; anchors resolved;
  JSON values files accepted), and `ValuesModel` (per-path defaults with
  spans + descriptions; invariant stated precisely: values files contribute
  defaults, descriptions, and **top-level key existence** as an explicit
  named policy — never types, shapes, nullability or requiredness for
  nested paths). `ChartProgram` is the engine's complete, IO-free input.
- **Pipeline wiring** (§5.2) with honest parallelism: parse+interpret fan
  out per template (order-preserving collect; shared helper-summary memo in
  a concurrent map — racy recomputation is benign because summaries are
  pure; never evaluate while holding a shard lock); knowledge prefetch
  parallelizes IO discovered from `ContractIR`'s claims; resolution,
  lowering and minify remain serial. Expected win ~1.5–2× on large charts —
  the bigger levers are lazy schema docs and allocation discipline.
- **Output passes as sequential typed functions**: override merge
  (replace-on-`$ref` markers, override-file-relative base URI, `--keep-refs`
  honored; external `$ref`s in overrides resolved under `FetchPolicy` — the
  only retrieval on the output path), optional `FlattenedExport`,
  description strip, minify, **global-schema mirroring into subcharts** (a
  named pass), and the **postcondition**: composed defaults validate against
  the emitted schema (hard diagnostic on failure).
- **The lockfile** (`--locked`): coordinate → content digest + source URL +
  version; re-fetches that would change a pinned digest fail.
- Diagnostics projection (engine gaps + knowledge traces + facade conflicts
  → the versioned envelope), and the facade exposes every stage function
  publicly; `ContractIR` is available to tooling but explicitly
  semver-unstable.

The CLI maps ~30 flags onto the policy/config objects (§13 is the checked
table), interprets env vars (`HELM_SCHEMA_*`, XDG) — libraries never read
the environment — renders diagnostics as text or the versioned JSON
envelope, and implements the exit-code policy: 0 clean; distinct codes for
parse-failure / generation-failure; `--fail-on=gaps|conflicts` upgrades
recorded abstentions.

## 7. Why this is the right architecture

**Because every rule has a reason.** The correctness contract plus polarity
table turn scattered instincts (why `eq` guards widen, why unknown
capability branches stay live, why `required` is dangerous, *when* a guard
may become an `if/then`) into derivable, checkable consequences.

**Because the contract graph is what the system is actually about.** A
values schema is a set of guarded constraints over the values space.
`ContractIR` makes that the stable artifact; the abstract document — the
mechanism that *finds* those constraints structurally — stays private and
free to evolve with parsing fidelity. v2 had this backwards: it published
the mechanism and derived the meaning downstream.

**Because one engine is the only enforceable home for "no heuristics".**
Six evaluators each understanding 80% of Helm is the documented root of
every recurring bug class — and a public seam through the middle of the
semantics (today's `ValueUse`, v2's `ChartAnalysis`) is how re-derivation
and arity ladders happen. One lattice, one builtin table, one crate: a new
Helm semantic is one row, every consumer inherits it, and there is exactly
one place a text-sniffing shortcut could creep in.

**Because policy-as-data beat traits on their own turf.** The honest audit
left three traits. The knowledge planner/executor is the showcase: probe
order is a unit-tested sort instead of a decorator-nesting accident, the
tri-state contract is proven once, "what was tried" diagnostics are the
executed plan, and new sources are config values.

**Because the two-tier schema model refuses a fight it cannot win.**
Upstream corpora are arbitrary JSON Schema; a closed algebra either drops
keywords silently or grows the escape hatch that swallows the design.
Foreign content stays verbatim with total, corpus-tested operations; the
lawful typed algebra covers what we synthesize — including, now, the
P1-lowered conditionals.

**Because bundled output is the honest product contract.** Inline-by-default
re-expands exactly the sharing the lazy `SchemaDoc` tier worked to preserve,
then pays the minifier to rediscover it. Internal `$defs` with deterministic
names keeps output small, self-contained, offline-validatable, and
diff-friendly; full flattening remains one export call away.

**Alternatives rejected** (with reasons on record): render-then-infer
(samples can't cover guard space; rendering is a *test oracle only*);
annotation-driven schemas (comments stay metadata, enforced by type);
per-path profiles joined in the interpreter (destroys provenance and bakes
policy into analysis); a closed typed model for upstream schemas (above);
decorator catalogs (above); maximal trait-per-stage pipelines and a
"pipeline object" (stage functions deliver the substance); per-session
interning (breaks Ord/serde/`'static` for marginal wins); publishing the
abstract-document forest as the analysis artifact (v2 — frozen mechanism,
reinterpreting consumer); a fixed-point analysis↔knowledge loop (knowledge
never influences interpretation, so one forward extract-then-resolve pass
provably suffices; the only residue is guard-conditional CRD emission, §14).

## 8. Cross-cutting policies

1. **Errors:** typed `thiserror` enums per crate; `PipelineError` is
   stage-tagged; **abstaining subsystems contribute no error variants** —
   knowledge failures are `Unknown` + diagnostics by design; eyre only in
   `main`.
2. **Diagnostics:** one model in core with spans; deduplicating concrete
   handle; versioned JSON envelope as a stable machine interface; the pure
   engine records gaps as data, the facade projects.
3. **Determinism:** ordered collections at boundaries; no env reads, clocks,
   or randomness in libraries; byte-identical output given identical oracle
   answers; cache state moves output only toward widening; lockfile for
   cross-time reproducibility.
4. **Security:** `FetchPolicy` on every network/file edge (override-`$ref`
   retrieval included); `LoadBudget` on archives/parsing/interpretation;
   `RelPath` validated newtype at the coordinate→path boundary; archive
   extraction stays memory-backed.
5. **Serde boundary:** internal graphs never derive `Serialize`; canonical
   `Display`/`FromStr` for paths; DTO projections for contract dumps and
   fixtures.
6. **Performance:** lazy `$ref` docs + bundled emission (no materialization,
   no inline blow-up), Arc-shared fingerprinted values, effects-into-context,
   predicate hash-consing, honest Amdahl accounting; Perfetto/`tracing`
   stays the profiling truth, plus a `[profile.profiling]` with symbols.
   Budgets double as DoS bounds.
7. **Lints/deps:** `[lints] workspace = true` in every crate;
   tree-sitter/`cc` only under the grammar crate; `ureq` (pinned TLS
   feature) only in knowledge; `jsonschema` only in the facade (override
   resolution, postcondition validation, flatten export); the YAML parser is
   a maintained, span-preserving one (`serde_yaml` is unmaintained and
   span-less).

## 9. Testing architecture

1. **Law tests (property-based):** value-lattice join laws on canonical
   forms; typed-merge laws + conflict totality; predicate algebra (NNF
   round-trips, three-valued evaluation monotone in assumptions; the
   `ValuesDecidable` classifier sound — a predicate classified decidable
   evaluates to `T`/`F` on every concrete values document); FalseySet
   widening monotonicity.
2. **Transfer-function tables:** snippet → expected contract constraints,
   one row per builtin and construct (`default`, `with…default`, `set` in
   helpers, map ranges, `tpl`, `.Files.Get`, `required` under else,
   `semverCompare` chains, capability shims, dynamic keys, List envelopes…).
3. **Contract suites:** the executor's tri-state honesty (cold cache ⇒
   `Unknown`; offline never fetches; `Absent` only on negative witness;
   persist-failure ⇒ `Unknown`; error ⇒ `Unknown` + diagnostic) against
   real-dir and fake stores; oracle fakes for resolution.
4. **Golden full-schema equality** over the real-chart corpus (the project
   standard), hermetic via recorded catalog fixtures. No acceptance gate
   depends on live network — the differential harness runs a local `helm`
   binary against vendored charts; network-touching paths get smoke tests
   only. Bundled-output goldens additionally pin the deterministic `$defs`
   naming.
5. **Differential validation:** render fixtures with `helm template` under
   default + guard-flipping samples; every accepted sample must validate
   (the §2 soundness probe, the P1 acceptance gate — it directly exercises
   lowered `if/then` conditionals); run `helm lint` too (consumer matrix).
6. **Gate tests** (pre-merge for the respective migration steps): the
   abstained-enrichment budget (no corpus chart loses a type enrichment vs
   the current tool); 100-run byte-determinism of the contract DTO;
   security regressions (traversal coordinates, oversized archives,
   metadata-endpoint `$ref`s).

## 10. Acceptance criteria per crate (definition of done)

- **core:** law tests for `Lookup` combinators; zero IO deps;
  `Display`/`FromStr` round-trips pinned; envelope versioning tested.
- **engine:** transfer table green; lattice/predicate/merge law tests;
  determinism property; purity CI check (no IO deps, no `dyn` fields);
  parse-failure degrades per-file; budget enforcement observable as gaps;
  decision records present for every narrowing; `ValuesDecidable` lowering
  bounded by policy and covered by guard-flipping differentials.
- **knowledge:** contract suite green; probe-order unit tests; trace→
  diagnostics projection parity with today's `MissingSchema` richness;
  advisor off by default and cache-independent.
- **facade/cli:** golden corpus equality (bundled default + flattened
  export); postcondition active; parity checklist (§13) checked off;
  exit-code table tested; corpus discovery parity (incl.
  `Chart.template.yaml`, tgz, aliases); compose_values fixtures;
  conditions/tags/crds/shipped-schema extraction tested; budgets enforced.

## 11. Sizing sanity check

This lands *smaller* than today's ≈29K LOC: one interpreter replaces six
evaluators + `helper_eval` (≈5–6K of near-duplicates); the planner/executor
replaces two provider monoliths + chain (≈2K → ≈0.6K); contract projections
replace the byte-cursor/indent machinery; the policy object replaces the
generator's scattered conditional lattice; bundled emission deletes the
flatten hot path — while adding spans, the predicate algebra, conditional
lowering, security budgets, and chart-local knowledge.

## 12. Migration correspondence (informative)

The in-flight single-abstract-interpreter phases land directly on this
design (its `AbstractValue`/`Effects`/`eval_expr` become §6.3's
lattice/evidence/interpreter with laws added). This table records *where*
current code lands; §15 turns it into an ordered route (*when*). **One rule
from the pre-mortem: every migration step's completion criterion is that the
module it replaces is deleted** — no new crate while its predecessor lives,
and the `ValueUse` projection never gains a production consumer.

| Today | Target |
|---|---|
| `helm-schema-template-grammar` | unchanged role |
| `helm-schema-ast` (parser, `TemplateExpr`, fuse) | `engine::parse` (spans, typed headers, one parse) |
| `helm-schema-ast::values_comments` | facade `ValuesModel` |
| IR evaluators (`expr_eval`, `binding`, `fragment_*`, `helper_*`, `bound_*`, walkers) | `engine::interp` (lattice + builtin table + summaries) |
| `helper_eval.rs`, `resource_detector/locator` | identity projection over internal documents |
| `yaml_shape.rs`, `rendered_yaml_context.rs` | structural attribution + exact/anchored/opaque contract (tracker survives only as upgrader until the budget gate passes) |
| `ValueUse` + postprocess | DTO projection of `ContractIR` (fixtures only) |
| `helm-schema-k8s` providers/chain | `knowledge` planner/executor + sources-as-data |
| `capability_eval.rs` + chain oracle impl | `CapabilityOracle` adapter + engine-side liveness |
| `inference/*` | quarantined advisor module |
| `helm-schema-gen` (lib/merge/required_inference) | `engine::resolve` + `engine::lower` + policy |
| CLI `chart.rs` | facade chart loading (`ChartProgram`) |
| CLI `schema_override.rs`, `flatten.rs`, mirroring pass | facade output passes (+ `FetchPolicy`); flatten becomes `EmitMode::FlattenedExport` |
| CLI `lib.rs` pipeline | facade stage functions |
| `json-schema-minify` | unchanged (optional pass; bundling removes its hottest input) |

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
(`--crd-catalog-dir`); `--api-version-guess` (deliberate change: the
advisor's local-cache-scan tier becomes hint-only, per §6.6's authority
model); `--override-schema` semantics (replace-on-`$ref`,
override-dir-relative resolution, keep-refs, required union); top-level
values-key seeding (named policy); global-schema mirroring into subcharts;
subchart values composition + library-chart scoping + `Chart.template.yaml`;
`--exclude-tests` (as role filter); extra values files; `--infer-required`
(legacy policy + new witness-based rules); diag text/JSON formats (now
versioned); `--trace-output`; defaults-validate postcondition (new, but
required for §2).

**Deliberate output-contract change:** the default emission becomes a
bundled, self-contained document with internal `$defs` (§6.5); today's
fully-inlined shape remains available as the flatten export mode. Golden
fixtures regenerate once for the new default, with the export mode pinned
against the old fixtures.

## 14. Open questions (the honest residue)

1. **Fused-parse fidelity / `Opaque` rate** on the corpus — measured before
   the attribution tracker is deleted (the budget gate exists for this).
2. **`env_delta` composition cost** for mutation-heavy helpers — whether the
   conditional-overlay representation needs further bounding in practice.
3. **`import-values`/`export-values`** — model vs structured abstention;
   needs a corpus survey of real usage.
4. **Lockfile format and scope** — per-chart vs per-workspace; interaction
   with mirrors and the negative cache.
5. **Conditionally-emitted CRD schemas** — extraction registers only
   fully-literal, unconditional `openAPIV3Schema` subtrees; whether per-arm
   registration under guard predicates is worth the complexity needs corpus
   data.
6. **P1 lowering ergonomics** — how far down the discriminator ladder
   (`Eq`/`TypeIs` → `Contains` → scalar truthiness) lowering stays
   readable for schema consumers before the size/UX cost outweighs
   precision; needs corpus measurement with the differential harness.

## 15. Implementation roadmap (from the current tree to this architecture)

Written from the state of the tree at the time of writing:
single-abstract-interpreter phases 0–2 complete, phase 3 in progress, golden
corpus green. Consistent with `next-priorities.md`'s ordering philosophy
(targeted cleanup on stable boundaries first, broad reorganization last).

### 15.1 Ordering principles

1. **Shape first, move last.** Do *not* create the target crates and migrate
   code into them up front. Fix semantics in place behind the existing seams
   (`IrGenerator`, `K8sSchemaProvider`, `ValuesSchemaGenerator`,
   `ValueUseSink`, `HttpFetcher`); let target module boundaries emerge; the
   crate consolidation is then a cheap mechanical final step — and v3's
   target (one engine crate) makes that step *smaller* than v2's.
2. **Every step deletes its predecessor in the same PR series.** No parallel
   engines, ever.
3. **Gates before risk.** The measurement/test infrastructure lands *before*
   the steps it gates.
4. **Parallelize only across seams.** Knowledge and the CLI/chart edges are
   independent of the interpreter; the semantic core is strictly sequential.

### 15.2 Step 0 — lock in the ratchet (first, independent of everything)

- **Differential harness** (§9.5): `helm template` + `helm lint` over the
  fixture corpus with default and guard-flipping values samples, plus the
  composed-defaults-must-validate postcondition as a test.
- **Security closures**: validated `RelPath` at the coordinate→cache-path
  boundary; gate `file://` and add size/time budgets in `$ref` resolution
  (`FetchPolicy` seed); `LoadBudget` on tgz extraction.
- **`[lints] workspace = true`** in every production crate.

### 15.3 Workstream A — semantic core (the critical path, sequential)

- **A1 — finish interpreter phases 3–4 with the corrected shapes**:
  state-passing `eval_node` with explicit join (Go-template `=` vs `:=`,
  branch out-states); **Top-absorbing** value join; control flow on a
  minimal internal predicate core (atoms + `And`/`Not`, else-branches carry
  `¬P`) projected to flat `Guard`s at the `ValueUse` boundary. Deletes:
  walker control-flow handling, manual scope snapshot/restore.
- **A2 — helper summaries under the §6.3 contract**: empty-pc summaries
  re-guarded at call sites; env-closed fingerprints; recursion ⇒ Top +
  poisoned memo. Deletes: the twin helper-body walks, the fragment/helper
  binding evaluators, and — once resource identity consumes interpreter
  summaries — the 1,480-line `helper_eval.rs`.
- **A3 — internal documents + contract projection** (the riskiest step;
  gated): `eval_node` builds abstract documents; anchors/identities/
  constraints are projected **feeding the existing `ValueUseSink`**, so
  downstream is untouched while the artifact changes underneath. Gate: the
  abstained-enrichment budget — no corpus chart loses a type enrichment vs
  the current tool; `yaml_shape` survives as an upgrader until the gate
  passes, then is deleted.
- **A4 — `ContractIR` + resolution/lowering (phase 6 fulfilled)**: the
  guarded constraint graph becomes the seam; polarity-table policy extracted
  from gen's god-loop into `ResolvePolicy`; two-tier operations
  (`partialize`, `restrict_to_scalar`) as named corpus-tested functions; the
  predicate algebra replaces flat `Guard`; P1 conditional lowering lands
  behind policy with the guard-flipping differentials as its gate;
  `ValueUse` demoted to a DTO/fixture format with no production consumer.
  The policy-extraction half does **not** depend on A3 and can start earlier
  against today's `ValueUse`.
- **A5 — bundled emission**: switch the default output to the
  self-contained `$defs` document; keep flatten as export mode; regenerate
  goldens once (deliberate, documented change).

### 15.4 Workstream B — knowledge (parallel, behind `K8sSchemaProvider`)

- **B1 — planner/executor**: pure `plan()` + one `execute()` +
  `LookupTrace`; collapse both provider monoliths and the chain; diagnostics
  parity proven by projecting today's `MissingSchema` richness from the
  trace.
- **B2 — lazy `SchemaDoc`**: delete the materialized per-resource `$ref`
  expansion (the dominant RSS lever) — before profiling the new
  interpreter, so memory blame lands on the right layer. Also the
  prerequisite for A5's bundling.
- **B3 — capability oracle adapter** + `kube_version()`; `ProbeTable` as
  declarative data.
- **B4 — chart-local CRDs as a source** (static `crds/`; the
  template-rendered projection additionally needs A3's documents). Shipped
  `values.schema.json` is *not* a knowledge source — it lands in A4's
  resolution as the enforced-constraint intersection.

### 15.5 Workstream C — chart/facade edges (parallel filler, low risk)

- **C1 — extract library logic from the CLI** (discovery, `compose_values`,
  overrides, flatten): *move, don't redesign*. Immediate payoff: hermetic
  in-process integration tests.
- **C2 — `FileRole` model + Chart.yaml `condition:`/`tags:` evidence**
  (feeds A4 policy; unlocks B4).
- **C3 — the crate consolidation to §5.1's layout: last**, once module
  shapes match their target homes. Under v3 this is mostly *merging*
  (ast+ir+gen → engine) rather than splitting — strictly easier.

### 15.6 Dependencies and sync points

A2 → A3 → A4(anchor half) → A5; A4(policy half) anytime after Step 0;
B2 → A5; C2 → B4; C3 strictly last. B and C run concurrently with A.

### 15.7 Risk register

A1–A2 is the grind and the critical path. A3 is where surprises live
(fused-parse fidelity), hence the hard quantitative gate and the upgrader
fallback. A5 changes the product's output shape — flag it, regenerate
goldens once, keep the export mode pinned against old fixtures. B is the
best use of parallel capacity. Explicitly deferred to post-parity:
`--explain`, the lockfile, a 2020-12 emitter, `import-values` modeling,
per-arm registration of guard-conditional CRD schemas, and P1 lowering
beyond the discriminator tier (§14).

### 15.8 Plan-document bookkeeping

`single-abstract-interpreter.md`'s phase 4–6 descriptions encode v1-era
shapes (sink-only `eval_node`, flat guards, unspecified summary context).
Amend them to reference the §6.3 corrections — the state-passing signature
with join, Top-absorbing join direction, the minimal predicate core, and the
summary contract — so the in-flight plan and this document cannot drift
apart. When a workstream step completes, strike the corresponding row from
§12's table; when all rows are struck, this document stops being a plan and
becomes the architecture description.
