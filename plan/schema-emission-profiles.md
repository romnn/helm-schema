# Schema emission policy: passes, profiles, and configuration — v2.6 (2026-08-02)

Goal unchanged from v1 (2026-07-17): during live validation, helm-schema
*generation* is not the dominant cost — `helm lint` is, because Helm 4
recompiles our large generated `values.schema.json` on every invocation
(generation itself is a separate, recorded performance follow-up).
v2 replaces v1's one opaque `full|lean` switch with **named,
individually reasoned emission policies**; the built-in profiles remain
exactly two — `full` and `lean` — as preset policy-sets, overridable per
knob through a `helm-schema.yaml` config file. Default behavior must not
change: full-fidelity schemas stay the default and every configuration
obeys the policy laws below.

Revision history: v2.1 — fact tags, monotone projection, harness-first,
canonical emission, reduced knob surface. v2.2 — Feature dimension,
semantic anchoring, step-1 split, step-0 failure branch, kind-partition
deletion semantics, canonicalization fallback, extraction placement,
temporal migration, annotation stage. v2.3 — total policy algebra (sum
type + decision table), step-0 re-split, `LoweredEmissionPlan`, phase
order, corrected annotation/fixture story, fidelity oracle,
`EmissionReport`, root-source config boundary. v2.4 — complete phase
enumeration (policy-sensitive spine vs completion passes), the
kind-partition anchoring audit (root-only is UNPROVEN in code),
override-digest merge-intent identity, config policy-vocabulary
pinning, fact-vs-carrier accounting with conservation invariants,
fixture-lane relocation, representable unconditional termination,
Boolean-root annotation, single `ReferencePolicy`, hermetic/live oracle
lanes, scoped lint floor, and plan-artifact cost metrics. v2.5 —
implementation-boundary contracts: the `Terminal::Always`
representation/producer split, the `ProjectedTree` /
`CompletedGeneratedSchema` stage types, fact-based policy floors,
`PreparedOverride` as the runtime source of merge intent, the public
`EmissionSelection` type, mismatch-free reference-policy ownership,
benchmark visibility, config-version support policy and source
eligibility, complete Boolean-root handling, tri-state transport-aware
oracle verdicts, definition-pruning churn isolation, and two wording
corrections. v2.6 — four phase-boundary contracts: out-of-band
replacement intent from the initial override read, preset-plus-delta
`EmissionSelection`, the completion-pass monotonicity obligation, and
the emission-vs-serialized finality split with a late reachability
prune. **v2.6 is the final paper revision; further review happens
against the implementation (step 0a onward), not this document.**

This plan is written with the architecture-review-v3 target in mind
(compiler-style phases, one typed schema tree): the fact model here is
the provenance vocabulary v3 step 11 needs, and the policy projection is
the backend pass v3's total `SchemaNode` will host.

## Status of v1 (reconciliation, 2026-08-02)

- v1 steps 1–2 and the CLI flag landed in the fiftieth round:
  `SchemaProfile { Full, Lean }` (`gen/src/emission_policy.rs:9-20`), two
  gates (`gen/src/lib.rs:283-285`, `lib.rs:384-392`), `--profile` on the
  CLI, and the luup2 temporal chart adopted lean downstream (the only
  adopter, via `HELM_SCHEMA_OPTIONS` in its taskfile).
- v1 steps 3–4 (lean fixtures, widening test, measurements) never ran.
  There are **no lean fixtures and no widening harness** — the
  procedural root cause of everything below; hence harness = step 0a.
- **Today's lean over-drops.** v1 predicted lean ≈ 2.1 MB / ~74 K nodes
  for temporal; the current binary emits **50.7 KB / 2,437 nodes**, and
  v1's own step-4.3 wrong-type control now FAILS under lean:
  `temporal.server.replicaCount: "three"` is accepted (full rejects).
  Cause: rounds 51–57 moved base ownership, presence pairs, and
  merge-arm typing *into* the conditional channel, and the round-50 gate
  clears that whole vector. Nothing pinned lean, so the drift was
  silent.
- **Today's lean has a structural widen-only suspect.** Conditional
  emission carries base-support mutations: unconditional object-host
  folds and host-type relaxations (`overlay_lowering.rs:1860-1881`; the
  comment "Only arms that actually emit may relax — a dropped arm would
  turn the relaxation into a plain widening" states the coupling). The
  lean gate drops arms *before* this point, so a nil-safe member host
  keeps the strict `type: object` that full relaxes — lean would reject
  a null/absent host state that full accepts and helm renders.
  Unconfirmed; step 0a adjudicates it with an explicit failure branch.
- The precision work of rounds 43–57 did not change what lints; it added
  conditional machinery so the schema matches helm exactly in edge
  states. That precision must be purchasable per user.

## Measured evidence

### v1 baseline (temporal, 2026-07-17, conclusions that still stand)

Compile cost, not validation, dominates; helm recompiles per
invocation; `$ref` interning is a mitigation (inlining measured 3×
worse). Full v1 table in this file's git history.

### v2 refresh (temporal wrapper chart, helm 4.2.3, jv 0.7.0, 2026-08-02)

| design point | objects | bytes | jv compile | `helm lint --strict` | `"three"` control |
|---|---|---|---|---|---|
| full | 150,299 | 4.19 MB | 58 s | 110 s | rejects ✓ |
| full minus 444 root `allOf` arms (jq approximation) | 132,204 | 3.84 MB | 8.9 s | 17.3 s | rejects ✓ |
| full minus every `if/then/else` | 114,301 | — | 0.12 s | ≈0.5 s | accepts ✗ |
| today's lean | 2,437 | 50.7 KB | 0.05 s | 0.14 s | accepts ✗ |
| full minus every `pattern` | 150,299 | — | 56 s | — | n/a |

The 17.3 s row is a **jq approximation** (`del(.allOf)`), not the
semantic lean preset. Step 2 measures the exact preset and per-knob
deltas through the projection harness, and the lean veto is
**reconfirmed after step 3** — canonical emission and definition
pruning change the final compile/size numbers.

Conclusions:

1. **Conditional evaluation dominates compile cost in this case, and
   root-anchored arms are the most expensive class per unit.** Not yet
   a proof that cost is a function of keyword count alone — placement,
   condition-tree size, uniqueness, and nesting are confounded; the
   benchmark separates them.
2. **Patterns remain compile-irrelevant** (33 distinct regexes today);
   spelling knobs would be about size/readability, never speed — none
   are exposed in this plan.
3. **The teeth interleave with the cost ladder**: temporal's
   `replicaCount` typing is locally anchored, so anchoring must be a
   first-class policy dimension.
4. **The doc-page complaint decomposes cleanly** into canonical
   emission (validation-equivalent) plus the spelling union (genuine
   precision — `{type: integer}` alone would falsely reject
   `replicas: "3"`).
5. Generation itself now takes 15–26 s on temporal (v1: 7.1 s).
   Emission policy cannot fix analysis cost — follow-up.

## Design

### The fact model

A lowered constraint is a payload with a policy classification. The
classification is a sum type so that incoherent states (an unconditional
fact carrying an anchor) are unrepresentable — while everything Helm can
structurally express stays representable:

```text
LoweredConjunct {
    class:              EmissionClass,
    origin:             Origin,          // diagnostics only
    carrier:            target path / carrier shape,
    schema:             the conjunct's schema payload,
    provider_candidate: Option<ProviderSchemaCandidate>,
}

EmissionClass =
    Mandatory
  | Conditional {
      guards: GuardScopes,               // owned here, nowhere else
      anchor: Root | Local(path),        // minimum safe semantic anchor
      flavor: Ordinary | KindPartition,
    }
  | Terminal { when: Always | Guarded(NonEmptyGuardScopes) }

Origin = Overlay | FailImplication | MergeShadow | OmittedMember
       | Backprojection | ProviderPayload | BaseType
       | SpellingUnion | Presence | …
```

- **`Terminal::Always` is deliberately representable**: an unguarded
  Helm `fail` is meaningful — full emits an always-false constraint;
  `terminal-clauses: off` may soundly drop it. The vocabulary must not
  make an analyzable Helm behavior impossible — but the current signal
  builder discards empty fail conjunctions
  (`contract_signal_builder/builder.rs:1766` area), so *emitting* this
  is an analyzer behavior change: step 1a adds only the variant and
  constructors (no producer, fixture-identical), and step 1a.1 teaches
  the analyzer to produce it, with its own behavior-changing fixture
  and oracle adjudication via the unconditional-fail microchart.
- The `Conditional` variant **owns the guard scopes** — it replaces,
  not duplicates, the current `guards`/`nested_guard_scopes` fields.
- **`anchor` is semantic**: the *minimum safe anchor* computed during
  lowering (Root exactly when a union alternative at some ancestor
  could bypass the constraint, `overlay_lowering.rs:290-293`). Emitter
  relocation can never silently change profile membership.
- Mandatory facts keep their carrier/schema payload — canonical
  emission's fallback needs them.
- `Origin` feeds diagnostics, the `EmissionReport`, and the v3
  provenance enum; policy never reads it. Unguarded fail implications
  and object-host requirements ride today's conditional vector as
  unconditional fragments (`overlay_lowering.rs:1957-1966`), so any
  Mandatory fact must survive every profile regardless of producer.

### The selection function is a total decision table

```text
Mandatory                                      → always emit
Conditional { flavor: Ordinary,      anchor: Root  } → root-anchored-conditionals
Conditional { flavor: Ordinary,      anchor: Local } → local-conditionals
Conditional { flavor: KindPartition, anchor }        → kind-partitions AND the anchor's knob
Terminal                                       → terminal-clauses
```

Every fact matches exactly one row. **The kind-partition anchoring
question is open**: the code does not prove partitions are root-only —
`kind_selector_path` accepts selector paths anywhere and the anchor is
the selector/target common prefix, which can be non-root
(`overlay_lowering.rs:1071`, `:1525`). Step 1a includes an audit plus a
local kind-partition microchart, then either (a) root-only is proven
and `KindPartition` gets an intrinsically-Root constructor, making
`kind-partitions: on` + `root-anchored-conditionals: off` the one
diagnosed-invalid combination, or (b) arbitrary anchors are retained
and the only universally invalid combination is `kind-partitions: on`
with **both** anchor knobs off. Configuration validity must never be
chart-dependent. (Lean itself — root-anchored and kind-partitions off,
local-conditionals ON — is valid under either resolution.)

### Policy classes and the laws

- **VE — validation-equivalent.** Accepts exactly the same instances.
  Not "semantics-preserving": description removal changes annotations,
  and reference-mode equivalence holds only under a fixed resolver
  environment — reference transport stays outside the core
  acceptance-law harness.
- **W — widen.** Accepts the same or more (`accepts(full) ⊆
  accepts(config)`; equality when a chart has no affected facts). The
  only class profiles may toggle. Invariant retained verbatim from
  `emission_policy.rs:5-8`.
- **X — narrow.** Beyond proven facts. Never in a profile, never
  activatable from discovered config. `infer-required` is X
  (`session.rs:220` area) and stays an explicit CLI opt-in with its own
  policy type.

Laws:

1. **Monotonicity law** (VE and W, a policy law): `accepts(full) ⊆
   accepts(config)`; equality for VE-only deltas. Harness output is
   *regression evidence*; VE passes additionally carry a narrow
   algebraic rewrite specification with exhaustive small-domain
   property tests per recognized shape.
2. **Lint floor** (a release acceptance criterion, not a general law),
   **conditioned on the oracle**: composed chart defaults and CI values
   validate under the built-in profiles and every committed
   configuration the harness exercises *when Helm successfully renders
   them*. Charts whose shipped defaults genuinely abort — the corpus
   already tracks three (`KNOWN_VALUES_REJECTIONS` in
   `chart_corpus.rs:53`: aws-load-balancer-controller, karpenter,
   loki) and an unconditional-fail chart is another — must instead
   REJECT under full. Arbitrary caller overrides and explicit X
   policies are excluded — a root override of `false` trivially fails
   any floor.

### The public knob matrix

| knob | class | selects | notes |
|---|---|---|---|
| `root-anchored-conditionals` | W | Conditional{Ordinary, Root} | overlay, fail, merge-shadow, omitted-member, guarded-backprojection arms |
| `local-conditionals` | W | Conditional{Ordinary, Local} | dependency-gated and path-local refinements |
| `terminal-clauses` | W | Terminal (Always and Guarded) | independent of anchor |
| `kind-partitions` | W | Conditional{KindPartition, _} AND the anchor's knob | **off = DELETE the partition refinements** (pure widening). The "one union arm" substitution is NOT automatically weaker (unknown selectors: full accepts vacuously, the union rejects); the provable `sel ∉ {K1,K2} ∨ S1 ∨ S2` weakening is a follow-up with an algebraic obligation. Validity rule per the audit above. |

Deliberately **not** exposed: presence/not-null, values-default
backfill, declared-default preservation, falsy escapes, program
wrappers (Mandatory or X-class if dropped); normalization (canonical
emission, below); `scalar-spellings: plain` (designed, unexposed until
a measured benefit); `assume-typed-scalars` (not offered — fidelity
charter; reopening conditions in follow-ups).

Output-side options (`refs`, `descriptions`, `minify`) remain
`OutputPipelineOptions`; `infer-required` remains its own X policy. One
top-level CLI/config resolver owns all three structs.

### Profiles are presets — and the lean decision

```text
full: every W knob on.
lean: root-anchored-conditionals off, kind-partitions off,
      terminal-clauses off, local-conditionals ON.
```

**Lean adopts the measured middle point** (approximation 17.3 s vs
110 s, `"three"` control retained; exact preset measured in step 2,
reconfirmed after step 3). Rationale: the tooth loss was the motivating
regression; wrapper charts keep most typing value through local
conditionals; size is not binding today (3.84 vs 4.19 MB, both under
Helm's 5 MiB limit) but has limited headroom, so the structural floors
include a shipped-bytes budget. The sub-second point remains one config
line away (`local-conditionals: off`). This is a **redefinition with a
retention contract**: lean keeps *every Mandatory fact plus every
locally-anchored conditional refinement*. Roman can veto toward the
fast variant; contract text and expected control verdicts swap, nothing
else changes.

**Temporal migration (exact).** Remove `--profile lean` from temporal's
`HELM_SCHEMA_OPTIONS` and add:

```yaml
# temporal/helm-schema.yaml
version: 1
profile: lean
emission:
  local-conditionals: off
```

An explicit CLI `--profile` resets file-level knob deltas, so keeping
the flag instead intentionally means "standard lean". This exact
combination becomes a precedence integration test.

### The `LoweredEmissionPlan` and the monotone projection

```text
LoweredEmissionPlan
  = owned provider-resolved base inputs
  + immutable support plan (host preparation, base ownership,
    default refill — computed from ALL facts)
  + immutable tagged conjuncts (LoweredConjunct values)

impl LoweredEmissionPlan {
    fn project(&self, policy: &EmissionPolicy) -> ProjectedTree
}

// Two stages, because completion crosses into `Value` at into_value()
// (gen/src/lib.rs:429) and rewrites whole documents afterward — a
// pre-completion report must not be mistaken for the final one, and
// this boundary is exactly the v3 step-11 migration seam:
ProjectedTree            { document: SchemaDocument, fact_accounting: FactAccounting }
CompletedGeneratedSchema { schema: Value, emission_report: EmissionReport }
```

Invariants:

- **No provider, network, or cache access after the plan is built.**
- Support preparation runs once, idempotent, policy-free — which fixes
  the lean relax-host class by construction (lean still applies host
  relaxations while dropping arms).
- Each projection gets fresh mutable state; projection order cannot
  affect output; filtering and `$defs` rewriting operate on clones or
  owned projected facts, never the shared plan.
- **Projection invariant**: a W policy may only delete tagged conjuncts
  or replace one with a proven weaker conjunct; it must never affect
  resolution, base ownership, host preparation, default refill, or
  analysis.
- The public session stays single-policy; the multi-policy API is
  crate-private. The benchmark therefore runs as an **ignored in-crate
  benchmark test invoked by the script** (a workspace binary crate
  could not call a `pub(crate)` API); a feature-gated `bench-support`
  module is the fallback only if the in-crate test cannot cover
  temporal end to end. No permanently public unstable analysis API for
  benchmarking.
- **Cost accounting**: the plan retains provider payloads and clones
  per projection — the benchmark records plan construction time,
  per-policy projection time, peak RSS, retained plan/candidate bytes,
  and validator peak RSS. On temporal, validators run sequentially:
  keep the full validator, compile/use/drop one comparison policy at a
  time (per-schema validators are still compiled once and reused across
  all probes).

### The policy-sensitive spine, and the completion passes

The **spine** (where policy acts):

```text
lower policy-free LoweredEmissionPlan
→ prepare support from ALL facts (policy-free)
→ select emitted conjuncts (decision table)
→ canonical emission
→ source-aware provider `$defs` extraction from the SELECTED
  candidate-bearing facts (needs ProviderSchemaCandidate metadata,
  provider_definitions.rs:23 — never a raw JSON walk)
→ prune unreachable provider definitions
```

The **completion passes**, all policy-free, in their current relative
order, which this plan freezes (`gen/src/lib.rs:393-481` tail): values-
default backfill → `open_helm_global_namespace` → declared-default
preservation → repeated-provider-payload extraction → truthy/quoted
shared-definition insertion → program-wrapper alternatives →
description application. Program wrappers matter here: they inspect and
rewrite conditional arms (`program_wrapper.rs:185` area,
`scope_conditional_arms_to_non_wrappers`), so the **`EmissionReport`'s
carrier section is finalized only after the completion passes** —
condition metrics recorded earlier would not describe the schema Helm
compiles.

**Completion-pass obligation.** "Policy-free" does not by itself
preserve widening: several completion passes branch on the projected
schema's *shape* — declared-default preservation conditionally adds
alternatives (`resolve_policy.rs:1452` area) and program wrappers
rewrite according to accepted shapes (`program_wrapper.rs:73`) — so a
monotone projection does not automatically yield a monotone completed
output. Therefore every completion pass carries an explicit obligation:
**it must be validation-equivalent or monotone over schema
acceptance**, and stage-pair tests check `accepts(full) ⊆
accepts(widened)` after each shape-sensitive pass on the exhaustive
microcharts. A pass that cannot meet the obligation has its
policy-sensitive decision moved into lowering/support — it does not
stay in completion. X policies and arbitrary caller overrides remain
explicitly outside this law. This localizes monotonicity failures to a
named pass instead of a distant end-to-end harness flip. Then the session-level tail:

```text
→ required inference (X policy, if requested)
→ apply overrides
→ reference transport
→ strip descriptions
→ generic repeated-subtree minification (sees the final shape)
→ authoritative generated/policy annotations
→ serialization (write_schema_json)
```

Two sharing mechanisms stay distinct: source-aware provider extraction
(projected facts + candidate metadata) vs generic whole-document
minification (final `Value` shape). Annotation assembly happens at the
session/composition boundary — the only place emission policy,
narrowing policy, and output modifiers all exist.

### Canonical emission

Mandatory presence/not-null emit **directly** as root `required`
entries and property-slot conjuncts. Rules: conjoin via explicit
`allOf` in the existing slot, or drop when `schema_excludes_type(null)`
(`schema_model.rs:158-206`) proves implication; never through
`merge_into_schema_slot` (the evidence-union combiner). **Fallback is
total**: `canonicalize → Applied | NotApplicable(original)` — a
missing slot under the closed root retains the original root-anchored
conjunct unchanged (skipping widens; inserting `required` without an
allowed property can make the schema unsatisfiable). Invariant test:
every removed carrier produced an equivalent direct constraint or was
proven redundant. Type-union collapse happens at generator-owned
constructors, never a raw walk (provider-owned payloads must not be
rewritten). Canonical emission runs before overrides and ref bundling;
after its equivalence evidence lands it is permanent.

### Configuration surface and trust

```yaml
# helm-schema.yaml — root chart directory only, discovered through the
# root chart source, or --config.
version: 1
profile: lean
emission:
  local-conditionals: off
```

- **The `version` field pins the complete interpretation**: config
  syntax AND policy vocabulary AND preset membership (the mapping
  `version 1 → vocabulary 1 → lean/full definitions` is permanently
  frozen). The **support policy**: a binary honors every config version
  it ships support for, exactly; for versions it does not support
  (older or newer) it fails with a diagnostic naming the config's
  version, the supported range, and the remediation (update the config
  or the binary). It never silently reinterprets a packaged chart's
  policy. Evolving lean's definition or a knob's meaning bumps the
  config version; that is the deliberate price of drift-proof packaged
  configs.
- **Source eligibility**: discovered config controls **presets and
  W-class emission knobs only**. X policies, override paths, and
  reference-transport settings stay CLI/library-only unless chart-author
  demand is demonstrated — a chart on disk must not be able to enroll
  the caller in narrowing or I/O behavior.
- **Precedence**: explicit CLI > config file > profile preset >
  built-in default. CLI overrides are tri-state (`Option<_>`-backed;
  today's always-populated `--profile`, `cli/mod.rs:62`, cannot express
  "unset"). Explicit CLI `--profile` resets file-level knob deltas.
- **Root-source boundary**: one CLI-owned "open root chart source"
  phase resolves a directory or top-level archive into a root VFS
  handle; config discovery and `--print-effective-config` use ONLY that
  handle — no recursive discovery, no provider construction, no
  analysis, no network. Tests: directory/packaged resolution agree;
  dependency configs ignored; relative `--config` semantics and
  `--config`/`--no-config` conflicts deterministic. (Today the CLI
  wraps the path in a physical VFS, `cli/src/lib.rs:62`, while archive
  extraction exists for vendored dependencies,
  `chart/discovery.rs:110` — root-archive extraction is confirmed or
  added here.)
- **Trust**: unknown keys, invalid combinations, and vocabulary
  mismatches are hard errors; malformed discovered config is a hard
  failure; `--no-config` ignores discovered policy; X-class policies
  are never activatable from discovered config.
- **Visibility**: one aggregated diagnostic when discovered config
  weakens relative to full; `--print-effective-config` shows each
  field's source (built-in / profile / file path / CLI).
- **Library boundary — the public policy type is specified before step
  4** (avoiding parallel `profile + overrides` fields that recreate
  invalid combinations; today's public `GenerateOptions.profile`
  migrates onto it):

  ```text
  EmissionSelection =
      Preset { profile: SchemaProfile, delta: EmissionPolicyDelta }
    | Explicit(EmissionPolicy)
  // EmissionPolicyDelta: optional W knobs only, so "preset plus knob
  // deltas" (the temporal config: lean + local-conditionals off) keeps
  // its provenance instead of collapsing to Explicit with a null
  // requested-profile — resolved exactly once into:
  ResolvedEmissionPolicy {
      requested_profile: Option<SchemaProfile>,   // the annotation's
      policy: EmissionPolicy,                     // single source
  }
  ```

  `EmissionPolicy` fields stay private behind exhaustive checked
  constructors. Discovery belongs to the CLI composition root; the
  library takes `EmissionSelection` explicitly.
- **Reference policy has no mismatch state**: one request owns it —
  `EmitRequest { reference_policy: ReferencePolicy, output:
  OutputOptionsWithoutReferenceMode }` — and prepared overrides carry
  that same policy, so final transforms read it from the request rather
  than a second field (today `ReferenceMode` appears independently in
  `PolicyInputOptions` and `OutputPipelineOptions`,
  `output_pipeline/options.rs:9`; a prepare-inlined/emit-preserved
  mismatch would make the annotation lie). Structuring the duplicate
  away beats diagnosing it.
- **Cache law** (unchanged): policy changes emission only; emission
  stages are keyed by resolved policy.

### The resolved-policy annotation

Final emitted documents self-identify; `GeneratedSchema` stays
unannotated. Inserted at the end of the output pipeline, overwriting
any caller-provided key:

```json
"x-helm-schema-policy": {
  "annotation-format-version": 1,
  "policy-vocabulary-version": 1,
  "requested-profile": "lean",          // null for explicit library policies
  "resolved": { "root-anchored-conditionals": false, "...": "..." },
  "narrowing": ["infer-required"],
  "modifiers": {
    "overrides": { "count": 2, "digest": "<sha256, see below>" },
    "reference-mode": "bundled"
  },
  "policy-fingerprint": "<sha256 over canonical JSON (sorted keys) of
                          policy-vocabulary-version + resolved
                          + narrowing + modifiers>"
}
```

- `policy-vocabulary-version` is inside the fingerprint: identical
  boolean objects must change identity when knob meanings change.
- **Merge intent is out-of-band from the first read**: replacement
  pointers are collected while initially reading the override, and the
  JSON is **never mutated with control metadata** — today's
  `mark_refs_for_replacement` (`schema_override.rs:16`) inserts
  `$ref-replace` in-band and silently overwrites a caller-authored key
  of that name, then rides the marker through reference preparation;
  extracting after preparation would keep that collision window open.

  ```text
  UnpreparedOverride { schema: Value, replace_at: sorted JSON pointers }
      → reference preparation →
  PreparedOverride   { schema: Value, replace_at: sorted JSON pointers }
  impl PreparedOverride { fn identity(&self) -> PreparedOverrideIdentity }
  ```

  Descendant pointers shadowed by a replaced ancestor are normalized
  away. **Both merging and hashing consume `replace_at`** — one
  representation, no synchronization, no reserved keyword in caller
  JSON. The digest is over the canonical application-ordered identity
  array. Rationale: replacement changes deep-merge into subtree
  replacement (`output_pipeline/overrides.rs:55`,
  `schema_override.rs:43`), so two content-identical schemas can merge
  differently. Tests cover every reference mode plus caller-authored
  `$ref-replace` keys at ref and non-ref locations, and the
  inline-vs-ref-resolved collision case.
- `reference-mode` identifies configuration, not validation semantics
  under every external resolver.
- **Boolean roots**: a root replacement override can produce `true` or
  `false`, and today's pipeline inserts markers only into object roots
  (`transforms.rs:39-41`). Final documents self-identify without
  exception, so Boolean roots are VE-wrapped **including the dialect
  declaration**:

  ```json
  { "$schema": "http://json-schema.org/draft-07/schema#",
    "allOf": [false],
    "x-helm-schema-generated": true, "x-helm-schema-policy": { } }
  ```

  Override loading REJECTS root `null`/number/string/array values (a
  JSON Schema root is an object or a Boolean), so no root shape escapes
  the annotation promise. Helm-validator compatibility controls cover
  both Boolean wrappers.
- The generator release version is deliberately not embedded.

**Fixture placement (verified)**: the corpus roundtrip helper calls
`generated_schema()` + minify (`schema_roundtrip.rs:63` area) and its
fixtures carry no final annotations — semantic corpus lanes stay
unannotated and untouched. A small **final-output fixture lane** covers
full/lean annotations, caller-key overwrite, Boolean roots,
deterministic fingerprinting, modifier changes, and a library call with
no discovered config. Lean semantic fixtures live in their own tree —
`testdata/emission-profile-schemas/lean/<chart>.schema.json` — NOT as
`<chart>.lean.schema.json` beside the full corpus, which existing
scripts would misparse as chart "`<chart>.lean`"
(`scan-ci-values.py:43` strips only `.schema.json`; the full corpus
keeps its one-file-per-chart contract).

### The `EmissionReport`

Produced by the same emitter path as the document, outside the JSON and
the fingerprint, finalized after the completion passes. Facts and
emitted carriers are accounted separately (grouping merges several
facts into one carrier, so a single post-grouping count is ambiguous):

```text
facts:            lowered / selected / dropped, by class and origin
carriers:         emitted root/local carriers, condition nodes,
                  grouping fan-in
canonicalization: applied / redundant / fallback
```

Conservation invariants: `lowered = selected + dropped`;
`mandatory_dropped = 0`; **every Mandatory fact is selected and reaches
an emitted, equivalent, redundant, or fallback outcome** (this replaces
the vague "Mandatory facts present in output"). **Policy floors are
fact-based, never carrier-based** — Mandatory root carriers and
generator-owned nested `if` schemas inside payloads
(`path_resolver.rs:524` area) legitimately exist independent of W-class
policy carriers, so "zero emitter-owned conditionals" would be wrong:

```text
all-conditionals-off recipe: selected Conditional = 0, selected Terminal = 0
lean:                        selected Ordinary/Root = 0,
                             selected KindPartition = 0,
                             selected Terminal = 0
every policy:                mandatory_dropped = 0
```

Carrier counts remain for grouping/performance accounting. **Serialized
reality is a separate, later artifact**: the `EmissionReport` is final
at generator completion, but overrides can add or remove conditionals,
reference inlining can duplicate them, bundling can introduce them, and
minification can share them (`transforms.rs:29-76`) — so
`FinalOutputMetrics` is computed after reference transport,
minification, and serialization, with bytes taken from the exact
`write_schema_json` output. Those counts, not provenance, are what Helm
compiles; size floors and compile-cost accounting read
`FinalOutputMetrics`, fact floors read the `EmissionReport`. At the
same late boundary, an **ownership-aware second reachability prune**
removes generator-owned definitions orphaned by inlining or overrides —
the early projection prune cannot see that far, and the minifier
deliberately reinserts all pre-existing `$defs` unchanged
(`helm-schema-json-schema-minify/src/lib.rs:27-43`), so fully inlined
output would otherwise retain both the inlined payloads and their dead
definitions. Both reports reach the shell benchmark through the
in-crate benchmark test, not tracing scraping.

## Verification design

1. **The harness** (compiled Rust, `jsonschema` crate). Compile each
   schema once, reuse the validator across probes; sequential across
   policies on large anchors (memory). Probe construction **enforces
   coalesced-values semantics internally** (defaults + null-deletion
   merge). Probe classes: replacement with every JSON type, null
   deletion, key deletion, coercible and non-coercible strings, unknown
   object members, empty and non-empty collection elements, guard
   boundary values, pattern near-misses. Exhaustive combinations on
   microcharts (ordinary suite); pairwise on large anchors and the
   temporal matrix (`integration` profile). From step 1b on, all
   policies project from one `LoweredEmissionPlan`; before that, the
   step-0a harness generates schemas separately per profile.
2. **The fidelity oracle, in two lanes.** Each focused control records
   `(instance, adjudicated contract verdict, full verdict, policy
   verdict, rationale)` — distinguishing chart-render failures,
   provider-backed invalid manifests, and deliberate profile widening.
   Verdicts are **tri-state** per the fidelity charter (unknown beats a
   false answer): `ContractVerdict = Accept | Reject(reason) |
   Unresolved(reason)` — an unresolved provider kind/schema never
   becomes a positive or negative control. Each instance records its
   **transport** (`ValuesFileJson | Set | SetString`): a JSON values
   file preserves the validator instance's exact JSON types, while
   `--set`/`--set-string` have distinct coercion behavior and are
   separate controls. **Hermetic lane**: ordinary tests consume
   committed adjudicated verdicts; no Helm executable required. **Live
   lane**: a pinned integration/maintenance command replays the cases
   against actual Helm and rendered-sink validation, recording Helm
   version, provider/Kubernetes schema version, chart digest, and
   rendered-manifest verdict. A small **validator-parity suite** checks
   the constructs this plan touches — Boolean schemas, `if/then`, type
   arrays, internal refs, extension annotations — against the pinned
   Helm 4 embedded validator: the Rust `jsonschema` harness passing
   does not prove Helm interprets every construct identically. Controls
   encode only intended behavior; no contract test enshrines a known
   violation.
3. **Three-category semantic controls** per profile promise: retained
   tooth / intentionally removed tooth / positive control. Coverage:
   unconditional provider typing, scalar spellings, presence, not-null,
   object-host typing (relax-host is priority one), patterns,
   dependency-gated typing, unconditional `fail`, a local
   kind-partition.
4. **The temporal anchor is the wrapper shape**: a minimal pinned
   wrapper vendored into `testdata/charts/` — same dependency edge,
   alias/condition semantics, relevant wrapper defaults, exact upstream
   temporal 0.62.0 archive (`common` only if it affects the lowered
   artifact); chart version, source checksum, and dependency lock in
   corpus-integrity metadata. Vendoring upstream alone would change the
   values prefix and possibly the anchoring under test.
5. **Structural CI floors**, read from the `EmissionReport`'s
   fact-based invariants (see the report section — selected-fact
   counts, never carrier counts); plus per-anchor size budgets measured
   on the actual shipped bytes (`write_schema_json` including trailing
   newline, `output_pipeline/format.rs:8`) — e.g. standard-lean
   temporal under 4.5 MiB. Wall-clock non-gating.
6. **Benchmark** (dedicated workspace command + script under
   `plan/chart-corpus-scripts/`), per release: report-derived root vs
   local carrier counts, condition AST nodes, unique conditions/`then`
   payloads, output bytes/objects, baseline lint without schema,
   repeated median/range warm and cold, plan construction/projection
   times, peak RSS (plan and validators), tool/chart versions, machine
   metadata.

## Implementation steps

Ground rules as in `plan/architecture-review-v3.md` (per-step gates:
fmt, `task lint`, workspace nextest; full AGENTS.md gate list per
round; `sim_assert_eq!`; one commit per step). No public field ships
before its implementation works independently.

### Step 0a — harness against today's binary, with a failure branch

Build the validator/probe framework, both oracle lanes, and the
semantic controls using **separately generated** current full/lean
schemas. Vendor the temporal wrapper anchor. Run the relax-host suspect
probe as preflight: pass → commit harness, no production changes; fail
→ insert a priority bug-fix step (regression test + minimal fix — apply
host preparation under every policy — landing together; step 1b's plan
subsumes the patch; rides the round-58 closure if the rounds align).

### Step 1a — fact model, decision table, `EmissionReport`, phase split (fixture-identical)

Introduce `LoweredConjunct`/`EmissionClass` (including the
`Terminal::Always` variant and constructors — **no producer yet**), the
decision-table selection, and the report; split
`append_conditional_schemas` into "prepare hosts" / "append selected
constraints" while exactly emulating current behavior. **Audit
kind-partition anchoring** (local kind-partition microchart; resolve
the constructor-vs-both-knobs question). `EmissionPolicy` (internal)
replaces the bare enum. Floors begin reporting, not gating. The
completion-pass obligation's **stage-pair tests** land here too (the
pass list is now enumerable): `accepts(full) ⊆ accepts(widened)` after
each shape-sensitive completion pass on the microcharts. Gate: both
profiles byte-identical on every fixture.

### Step 1a.1 — unconditional termination producer (behavior-changing)

Teach the signal builder to emit `Terminal::Always` for unguarded
`fail` conjunctions (today discarded,
`contract_signal_builder/builder.rs:1766` area), with the
unconditional-fail microchart, its own fixture, and oracle adjudication
(full rejects everything; `terminal-clauses: off` soundly accepts; the
chart joins the oracle-conditioned floor's reject list).

### Step 1b — `LoweredEmissionPlan`, full-fact support, extraction move

Introduce the plan artifact with its ownership invariants and the
`ProjectedTree`/`CompletedGeneratedSchema` stage types; enable support
preparation from ALL facts under every policy; **move source-aware
provider extraction after projection** (so multi-projection and all
subsequent measurements observe the intended architecture). Full output
unchanged; lean changes exactly where support mutations previously
vanished — dedicated controls pin the delta; harness shows lean's
acceptance only grows. **Unreachable-definition pruning is preflighted
against full**: if full stays byte-identical it lands here; if not, it
becomes its own VE step with a zero-flip proof and fixture adjudication
rather than hiding output churn inside the support bug fix. Either way,
state that caller overrides may not reference generator-private `$defs`
names (or defer pruning until such references are considered). Switch
the harness to mandatory one-artifact multi-projection; floors become
hard gates.

### Step 2 — lean per the decided contract (private policy)

Measure the exact semantic preset and per-knob deltas through the
projection harness; flip lean's preset (middle point unless the
measurements change the veto — **preliminary decision**, reconfirmed
after step 3). Add the resolved-policy annotation and the final-output
fixture lane; semantic corpus fixtures untouched. Bootstrap the lean
fixture lane (in its own tree) and the three-category controls;
adjudicate via both oracle lanes. Execute the temporal migration or
record Roman's veto.

### Step 3 — canonical emission (one regeneration of both semantic lanes)

Canonical Mandatory emission with the `Applied | NotApplicable`
fallback and invariant test; constructor-level type-union collapse;
delete the arm-then-fold vestige. Introduce `FinalOutputMetrics` at the
serialization boundary and the **late ownership-aware reachability
prune** for generator-owned definitions orphaned by inlining or
overrides (its own VE evidence, like the early prune). VE evidence:
algebraic rewrite spec + exhaustive small-domain property tests per
shape + zero-flip battery. One clean dump; luup2 gate. **Reconfirm the
lean veto** with final compile/size numbers under the final phase
ordering.

### Step 4 — configuration surface

Root-source boundary, `helm-schema.yaml` loader (version pinning with
honor-or-fail, root chart only, `--config`, `--no-config`, hard-fail on
malformed/unknown), tri-state CLI overrides, precedence +
`--print-effective-config` with per-field sources, aggregated weakening
diagnostic, single `ReferencePolicy` with the mismatch test,
packaged-chart tests, precedence tests including the exact temporal
combination. The knob matrix goes public. No semantic fixture churn
expected; churn is a red flag.

### Step 5 — benchmark and documentation

The dedicated benchmark command + script; README/`--help` with the
retention contract; the config reference with the precedence table.
Measure `scalar-spellings: plain` benefit; decide exposure on numbers.

### Step 6 — (deferred) v3 step-11 convergence

Projection and canonical emission move onto provenance-tagged passes
over the total `SchemaNode`; no config or fixture change — the
acceptance test that the policy vocabulary was chosen at the right
level.

## Ordering

1. **After** the round-58 review-findings closure; if step 0a confirms
   the relax-host violation, its minimal fix belongs in that round.
2. Steps 0a–1b **before** further ledger-style precision rounds: every
   new conditional fact lands already classified.
3. Steps 0a–5 **before** the v3 gen campaign (independent files; step
   11a relocates producers with their tags; the lean lane gives 11c a
   second full-equality corpus). v3 steps 4/12a compose cleanly.
4. Step 6 rides v3.

## Recorded follow-ups (not scheduled)

1. **Generation performance regression**: 7.1 s → 15–26 s on temporal;
   profile the analysis phases (BDD normalization, backprojection,
   global projection are the round-51–57 suspects).
2. **Root-arm compile cost upstream**: helm recompiles per invocation;
   santhosh-tekuri compile superlinear in root-arm count (444 arms ≈ 49
   of 58 s). Both v1 upstream reports stand.
3. **Guard-grouping canonicalization**: identical fragments already
   disjoin guards (`overlay_lowering.rs:1933` area); rerun/canonicalize
   after canonical emission and measure the incremental root-arm
   reduction — not a new pass.
4. **`kind-partitions` weakened substitution**: `sel ∉ {K1,K2} ∨ S1 ∨
   S2` with its algebraic obligation and exhaustive selector tests —
   only if deletion proves too coarse.
5. **`scalar-spellings: plain`**: designed, unexposed; revisit with
   step-5 measurements.
6. **`assume-typed-scalars`**: not offered; reopening requires
   demonstrated demand, an unmistakably unsafe name, a per-path
   narrowing diagnostic, full annotation, CLI-only activation.
7. **`jv` meta-validation** in the downstream lint task pays a second
   compile; revisit after lean adoption.
