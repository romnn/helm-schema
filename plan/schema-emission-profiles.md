# Schema emission policy: passes, profiles, and configuration — v2.3 (2026-08-02)

Goal unchanged from v1 (2026-07-17): during live validation, helm-schema
*generation* is not the slow part — `helm lint` is, because Helm 4
recompiles our large generated `values.schema.json` on every invocation.
v2 replaces v1's one opaque `full|lean` switch with **named,
individually reasoned emission policies**; the built-in profiles remain
exactly two — `full` and `lean` — as preset policy-sets, overridable per
knob through a `helm-schema.yaml` config file. Default behavior must not
change: full-fidelity schemas stay the default and every configuration
obeys the policy laws below.

Revision history: v2.1 added the fact-tag model, the monotone policy
projection, harness-first ordering, canonical emission, and the reduced
knob surface. v2.2 added the `Feature` dimension, semantic anchoring,
the step-1 split, the step-0 failure branch, deletion semantics for
`kind-partitions: off`, canonicalization fallback, `$defs` extraction
placement, the temporal migration, and the annotation stage. v2.3 makes
the policy algebra total (a sum type and a decision table instead of
independent filters), splits step 0 so nothing depends on facilities
later steps build, specifies the `LoweredEmissionPlan` ownership
boundary, corrects the phase order (minify placement) and the annotation
fixture story (the corpus lane never sees final annotations — verified),
and adds the fidelity oracle, the `EmissionReport` observation seam, the
exact-preset measurement obligation, and the root-source config
boundary.

This plan is written with the architecture-review-v3 target in mind
(compiler-style phases, one typed schema tree): the fact model here is
the provenance vocabulary v3 step 11 needs, and the policy projection is
the backend pass v3's total `SchemaNode` will host. Nothing here adds a
representation v3 would have to delete.

## Status of v1 (reconciliation, 2026-08-02)

- v1 steps 1–2 and the CLI flag landed in the fiftieth round:
  `SchemaProfile { Full, Lean }` (`gen/src/emission_policy.rs:9-20`), two
  gates (`gen/src/lib.rs:283-285` clears the conditional-arm vector,
  `lib.rs:384-392` gates terminal clauses), `--profile` on the CLI, and
  the luup2 temporal chart adopted lean downstream (the only adopter,
  via `HELM_SCHEMA_OPTIONS: "--strip-descriptions --compact --profile
  lean"` in its taskfile).
- v1 steps 3–4 (lean fixtures, widening test, measurements) never ran.
  There are **no lean fixtures and no widening harness** — the procedural
  root cause of everything below, which is why the harness is step 0a.
- **Today's lean over-drops.** v1 predicted lean ≈ 2.1 MB / ~74 K nodes
  for temporal; the current binary emits **50.7 KB / 2,437 nodes**, and
  v1's own step-4.3 wrong-type control now FAILS under lean:
  `temporal.server.replicaCount: "three"` is accepted (full rejects it).
  Cause: rounds 51–57 moved base ownership, presence pairs, and merge-arm
  typing *into* the conditional channel, and the round-50 gate clears
  that whole vector. No fixture or control pinned lean, so the drift was
  silent.
- **Today's lean has a structural widen-only suspect.** Conditional
  emission carries base-support mutations: unconditional object-host
  folds and host-type relaxations (`overlay_lowering.rs:1860-1881`). The
  relaxation comment states the coupling ("Only arms that actually emit
  may relax — a dropped arm would turn the relaxation into a plain
  widening"), but the lean gate drops arms *before* this point, so a
  nil-safe member host keeps the strict descendant-materialized
  `type: object` that full relaxes — lean would then reject a null/absent
  host state that full accepts and helm renders. Unconfirmed empirically;
  step 0a adjudicates it and has an explicit failure branch.
- The precision work of rounds 43–57 did not change what lints — the
  charts linted long ago; it added conditional machinery so the schema
  matches helm exactly in edge states. That precision must be
  purchasable per user; that is what the policy surface is for.

## Measured evidence

### v1 baseline (temporal, 2026-07-17, conclusions that still stand)

Compile cost, not validation, dominates (`jv` against an empty instance
costs the same as real values); helm recompiles per invocation; `$ref`
interning is a mitigation (inlining measured 3× worse). See git history
of this file for the full v1 table.

### v2 refresh (temporal wrapper chart, helm 4.2.3, jv 0.7.0, 2026-08-02)

| design point | objects | bytes | jv compile | `helm lint --strict` | `"three"` control |
|---|---|---|---|---|---|
| full | 150,299 | 4.19 MB | 58 s | 110 s | rejects ✓ |
| full minus 444 root `allOf` arms (jq approximation) | 132,204 | 3.84 MB | 8.9 s | 17.3 s | rejects ✓ |
| full minus every `if/then/else` | 114,301 | — | 0.12 s | ≈0.5 s | accepts ✗ |
| today's lean | 2,437 | 50.7 KB | 0.05 s | 0.14 s | accepts ✗ |
| full minus every `pattern` | 150,299 | — | 56 s | — | n/a |

The 17.3 s row is a **jq approximation** (`del(.allOf)`), not the
semantic lean preset: it also deletes the unconditional presence arms
the preset keeps, and it says nothing about per-knob contributions. Step
2 measures the exact preset and individual-knob deltas through the
projection harness before the lean veto decision is made.

Conclusions:

1. **Conditional evaluation dominates compile cost in this case, and
   root-anchored arms are the most expensive class per unit.** Removing
   the 444 root arms (12% of nodes) removes ~49 of 58 s; removing the
   remaining ~1,124 nested `if`s removes the rest. This does NOT yet
   prove cost is a function of keyword count alone — placement,
   condition-tree size, uniqueness, and nesting are confounded here; the
   step-5 benchmark separates them.
2. **Patterns remain compile-irrelevant** (33 distinct regexes today).
   Any spelling knob is motivated by size/readability, never speed —
   which is why no spelling knob is exposed in this plan.
3. **The teeth interleave with the cost ladder.** Temporal's
   `replicaCount` typing lives in *local* (path-anchored) conditional
   context, so "drop root arms only" keeps it while "drop all
   conditionals" loses it. Anchoring must be a first-class policy
   dimension.
4. **The doc-page complaint decomposes cleanly.** For the two-key
   Deployment example, full emits four unconditional `allOf` arms
   (per-fact presence + not-null carriers) beside an interned
   `anyOf[{type:integer,format:int32},{type:string,pattern:RADIX}]`
   union. The delta to the "human" schema is: (a) canonical emission of
   unconditional facts (validation-equivalent; the not-null conjunct is
   provably redundant against the union), and (b) the spelling union,
   which is genuine precision (`{type: integer}` alone would falsely
   reject `replicas: "3"`, which helm renders).
5. Generation itself now takes 15–26 s on temporal (v1 recorded 7.1 s).
   Emission policy cannot fix analysis cost — recorded as a follow-up.

## Design

### The fact model: a sum type, not a product

Meaningless combinations (an unconditional terminal clause, an
unconditional fact carrying an anchor) must be unrepresentable:

```text
EmissionFact =
    Mandatory                              // no guards, no anchor
  | Conditional {
      guards:  GuardScopes,                // owned here, nowhere else
      anchor:  Root | Local(path),         // minimum safe semantic anchor
      flavor:  Ordinary | KindPartition,
    }
  | Terminal { guards: GuardScopes }       // the `if G then false` class

Origin = Overlay | FailImplication | MergeShadow | OmittedMember
       | Backprojection | ProviderPayload | BaseType
       | SpellingUnion | Presence | …      // diagnostics only
```

- The variant **owns the guard scopes** — it replaces, not duplicates,
  the current `guards`/`nested_guard_scopes` fields.
- **`anchor` is semantic, not incidental JSON placement**: the *minimum
  safe anchor* computed during lowering (Root exactly when a union
  alternative at some ancestor could bypass the constraint — the
  bypass-proofness reason in `overlay_lowering.rs:290-293`). An emitter
  relocation can never silently change profile membership.
- If kind partitions are provably always root-anchored, that invariant
  is encoded in their constructor.
- `Origin` exists for diagnostics, the `EmissionReport`, and the v3
  provenance enum — policy never reads it. The conditional vector today
  carries unguarded fail implications and object-host requirements
  emitted as unconditional fragments (`overlay_lowering.rs:1957-1966`),
  so *any* Mandatory fact must survive every profile no matter which
  producer created it.

### The selection function is a total decision table

```text
Mandatory                                   → always emit
Conditional { flavor: Ordinary,     anchor: Root  } → root-anchored-conditionals
Conditional { flavor: Ordinary,     anchor: Local } → local-conditionals
Conditional { flavor: KindPartition, anchor }       → kind-partitions AND the anchor's knob
Terminal                                    → terminal-clauses
```

Every fact matches exactly one row; there is no fact a policy cannot
classify. The one invalid configuration is precisely
`kind-partitions: on` with `root-anchored-conditionals: off` (a
partition whose carrier class is disabled) — diagnosed as an error.
Both off — as lean uses — is valid.

### Policy classes and the two laws

- **VE — validation-equivalent.** Accepts exactly the same instances
  (canonical emission, `$defs` interning/minify, annotation handling).
  Not "semantics-preserving": description removal changes annotations,
  and reference-mode equivalence holds only under a fixed resolver
  environment — reference transport stays outside the core
  acceptance-law harness.
- **W — widen.** Drops a constraint class; accepts the same or more
  (`accepts(full) ⊆ accepts(config)`; equality when a chart has no
  affected facts). The only class profiles may toggle. Invariant
  retained verbatim from `emission_policy.rs:5-8`.
- **X — narrow.** Adds or keeps constraints beyond proven facts, or
  rejects something helm renders. **Never part of any profile and never
  activatable by discovered chart config.** `infer-required` is X (a
  heuristic that adds constraints after the resolved contract,
  `session.rs:220` area); it stays an explicit CLI opt-in with its own
  policy type.

Laws every configuration must satisfy, checked by the harness:

1. **Monotonicity law** (VE and W): `accepts(full) ⊆ accepts(config)`;
   for a VE-only delta, equality. The harness provides *regression
   evidence* — a finite battery cannot prove equivalence, so VE passes
   additionally carry a narrow algebraic rewrite specification with
   exhaustive small-domain property tests per recognized shape.
2. **Lint floor** (all classes including X): composed chart defaults and
   CI values files must validate.

### The public knob matrix

| knob | class | selects | notes |
|---|---|---|---|
| `root-anchored-conditionals` | W | Conditional{Ordinary, Root} | overlay, fail, merge-shadow, omitted-member, guarded-backprojection arms |
| `local-conditionals` | W | Conditional{Ordinary, Local} | dependency-gated and path-local `if/then` refinements |
| `terminal-clauses` | W | Terminal | independent of anchor |
| `kind-partitions` | W | Conditional{KindPartition, _} (AND the anchor's knob) | **off = DELETE the partition refinements** (pure widening). The tempting "one union arm" substitution is NOT automatically weaker: full's `(sel=K1 ⇒ S1) ∧ (sel=K2 ⇒ S2)` vacuously accepts unknown selectors, while an unconditional `S1 ∨ S2` rejects them. The provable weakening `sel ∉ {K1,K2} ∨ S1 ∨ S2` carries an algebraic obligation plus exhaustive selector tests — a follow-up, not an assumption. |

Deliberately **not** exposed:

- **Presence, not-null, values-default backfill, declared-default
  preservation, falsy escapes, program wrappers** — Mandatory facts in
  every profile. Presence has no measured compile cost and unconditional
  facts are the soundness floor; the rest are X-class if dropped.
- **Normalization** — not a knob; canonical emission (below). A
  temporary internal flag exists only during the step-3 rollout and is
  deleted with it.
- **`scalar-spellings: plain`** — designed (W-class at the preimage
  producers `resolve_policy.rs:832-1158`) but **not exposed** until a
  real size/readability benefit is measured.
- **`assume-typed-scalars`** — **not offered.** Violates the fidelity
  charter, hides the quoted-scalar checks that catch the most common
  helm values mistake, and would be unsafe in discovered chart config.
  Reopening conditions in follow-ups.

Output-side options (`refs`, `descriptions`, `minify`) remain their own
`OutputPipelineOptions` (`output_pipeline/options.rs`), and
`infer-required` its own explicitly-X policy. One top-level CLI/config
resolver owns all three structs; there is deliberately no single
`EmissionConfig` conflating validation policy with output transport.

### Profiles are presets — and the lean decision

```text
full: every W knob on.
lean: root-anchored-conditionals off, kind-partitions off,
      terminal-clauses off, local-conditionals ON.
```

**This adopts the measured middle point as lean** (approximation:
17.3 s lint vs 110 s full, `"three"` control retained; the exact preset
is measured in step 2 before the veto), rather than the current
all-conditionals-off behavior. Rationale: the tooth loss was the
motivating regression, wrapper charts keep most of their typing value
through local conditionals, and size is not binding **today** — 3.84 MB
vs 4.19 MB, both under Helm's 5 MiB limit, though with limited headroom,
so the structural floors include a size budget on the actual shipped
bytes. The sub-second point remains one config line away
(`local-conditionals: off`). This is a **redefinition with a retention
contract**, not a "repair": lean keeps *every Mandatory fact plus every
locally-anchored conditional refinement; only root-anchored guard arms,
kind partitions, and terminal clauses are dropped*. Roman can veto
toward the fast variant; then the contract text and the harness
controls' expected verdicts swap, and nothing else changes.

**Temporal migration (exact).** Temporal's taskfile currently passes
`--profile lean`. Under the precedence rules an explicit CLI profile
resets file-level knob deltas — so a discovered
`local-conditionals: off` would be discarded while that flag remains.
Migration: remove `--profile lean` from `HELM_SCHEMA_OPTIONS` and add:

```yaml
# temporal/helm-schema.yaml
version: 1
profile: lean
emission:
  local-conditionals: off
```

This exact combination becomes a precedence integration test. Keeping
the CLI flag instead is a supported choice that intentionally means
"standard lean, ignore file-level deltas".

### The `LoweredEmissionPlan` and the monotone projection

"Analyze once, project many" needs a concrete immutable artifact:

```text
LoweredEmissionPlan
  = owned provider-resolved base inputs
  + immutable support plan (host preparation, base ownership,
    default refill — computed from ALL facts)
  + immutable tagged conjuncts (EmissionFact values)

impl LoweredEmissionPlan {
    fn project(&self, policy: &EmissionPolicy) -> SchemaDocument  // fresh
}
```

Invariants:

- **No provider access, network access, or cache consultation after the
  plan is built.** Everything the projection needs is owned by the plan.
- Support preparation runs once and is idempotent; it is policy-free by
  construction, which is what fixes the lean relax-host class: lean
  still applies host relaxations while dropping the arms — widen-only by
  construction.
- Each `project` call receives fresh mutable schema state; projection
  order cannot affect output. Filtering and `$defs` rewriting operate on
  clones or owned projected facts, never on the shared plan.
- The projection invariant: **a W policy may only delete tagged
  conjuncts or replace one with a proven weaker conjunct; it must never
  affect resolution, base ownership, host preparation, default refill,
  or analysis.**
- The ordinary public session stays single-policy; the multi-policy API
  is crate-private for the harness. `AnalysisSession` is not redesigned
  for tests; `prepared`/`finalized_contract` stay policy-free per the
  cache law.

Before v3's total `SchemaNode` exists, the plan is implemented by
splitting `append_conditional_schemas` into "prepare hosts from full
facts" and "append selected constraints"; under v3 step 11c the
selection becomes a tag-directed pass over one typed tree, with no
config or fixture change (that migration being invisible is the
acceptance test for the policy vocabulary).

### The exact phase order

```text
lower policy-free LoweredEmissionPlan
→ prepare support from ALL facts (policy-free)
→ select emitted conjuncts by policy (the decision table)
→ canonical emission
→ source-aware provider `$defs` extraction from the SELECTED
  candidate-bearing facts (needs ProviderSchemaCandidate metadata —
  provider_definitions.rs:23; never a raw JSON walk)
→ prune unreachable provider definitions
→ required inference (X policy, if requested)
→ apply overrides
→ reference transport
→ strip descriptions
→ generic repeated-subtree minification (sees the final
  override/reference/description shape — today's transforms.rs order)
→ authoritative generated/policy annotations
→ serialization
```

Two sharing mechanisms, deliberately distinct: source-aware provider
extraction operates on projected facts with their candidate metadata;
generic minification is a whole-document `Value` pass and must see the
final shape. Policy annotation assembly happens at the
session/composition boundary — the only place emission policy, narrowing
policy, and output modifiers are all in scope — never inside the generic
output-transform options.

### Canonical emission (was: normalize knobs)

Once facts are tagged, Mandatory presence/not-null emit **directly** as
root `required` entries and property-slot conjuncts — constructing a
root arm and folding it back is an avoidable representation. Rules:

- Slot conjuncts conjoin via explicit `allOf` in the existing slot, or
  are dropped when `schema_excludes_type(null)`
  (`schema_model.rs:158-206`) proves implication; never routed through
  `merge_into_schema_slot` (the evidence-union combiner,
  `path_resolver.rs:1157-1160`).
- **Fallback is total**: `canonicalize → Applied |
  NotApplicable(original constraint)`. When the expected slot is absent
  under the closed root, the original root-anchored conjunct is retained
  unchanged — skipping would widen; inserting `required` without an
  allowed property can make the schema unsatisfiable. Invariant test:
  every removed carrier either produced an equivalent direct constraint
  or was proven redundant.
- Constraint-free single-type `anyOf` arms collapse to `type` arrays —
  at **generator-owned constructors**, never a raw walk over emitted
  JSON (the document embeds provider-owned foreign payloads that must
  not be rewritten).
- Canonical emission runs on generated schemas only, **before** user
  overrides and ref bundling.

After the equivalence evidence lands, canonical emission is permanent:
the doc-example output becomes the human schema modulo the spelling
union, in both profiles, with no toggle kept alive.

### Configuration surface and trust

```yaml
# helm-schema.yaml — root chart directory only (never read from
# dependencies), discovered through the root chart source, or --config.
version: 1            # config-format version
profile: lean
emission:
  local-conditionals: off
```

- **Precedence**: explicit CLI > config file > profile preset > built-in
  default. CLI overrides are tri-state (unset / explicit) — the current
  always-populated `--profile` (`cli/mod.rs:62` area) moves to
  `Option<_>`-backed args. An explicit CLI `--profile` resets file-level
  knob overrides. The resolution table with concrete conflict examples
  (including the temporal case) ships in the config reference.
- **Root-source boundary**: one small CLI-owned "open root chart
  source" phase resolves a directory or top-level archive into a root
  VFS handle. Config discovery and `--print-effective-config` use ONLY
  that handle — no recursive chart discovery, no provider construction,
  no template analysis, no network. Tests: directory and packaged-chart
  resolution agree; dependency configs are ignored; relative `--config`
  path semantics and `--config`/`--no-config` conflicts are
  deterministic. (Today the CLI wraps the path in a physical VFS,
  `cli/src/lib.rs:62`, while archive extraction happens for vendored
  dependencies, `chart/discovery.rs:110` — root-archive extraction is
  confirmed or added here.)
- **Trust**: unknown keys and invalid combinations (per the decision
  table) are hard errors; malformed discovered config is a hard
  failure; `--no-config` ignores discovered chart policy; X-class
  policies are never activatable from discovered config.
- **Visibility of automatic weakening**: when discovered config
  disables constraints relative to full, the CLI emits **one aggregated
  diagnostic** ("loaded helm-schema.yaml; disabled root-anchored
  conditionals and terminal clauses") — never one message per fact.
  `--print-effective-config` shows each field's source (built-in /
  profile / file path / CLI).
- **Library boundary**: the library API takes explicit typed policy
  only; config discovery belongs to the CLI composition root.
- **Cache law** (unchanged): policy changes emission only, never
  analysis; emission stages are keyed by resolved policy.

### The resolved-policy annotation

Final emitted documents self-identify; the library's `GeneratedSchema`
artifact stays unannotated. The annotation is inserted at the end of the
output pipeline (see phase order) beside the generated marker, and
overwrites any caller-provided key of the same name:

```json
"x-helm-schema-policy": {
  "annotation-format-version": 1,
  "policy-vocabulary-version": 1,
  "requested-profile": "lean",          // null for explicit library policies
  "resolved": { "root-anchored-conditionals": false, "...": "..." },
  "narrowing": ["infer-required"],
  "modifiers": {
    "overrides": { "count": 2, "digest": "<sha256, see below>" },
    "reference-mode": "bundled"         // identifies configuration, not
  },                                    // semantics under every resolver
  "policy-fingerprint": "<sha256 over canonical JSON (sorted keys) of
                          policy-vocabulary-version + resolved
                          + narrowing + modifiers>"
}
```

- `annotation-format-version`, not `config-format-version`: a
  library-emitted artifact may have parsed no config file.
- `policy-vocabulary-version` is **inside** the fingerprint: when knob
  meanings change, an identical boolean object must stop having the
  same identity.
- The overrides digest hashes one canonical, **application-ordered**
  array (order is semantically significant) of the **prepared effective
  schemas after reference resolution**, private merge markers excluded —
  hashing raw files would let changed referenced content leave the
  fingerprint unchanged.
- The generator release version is deliberately not embedded
  (`x-helm-schema-generated` marks provenance; version-stamping would
  churn every fixture per release with no semantic change).

**Fixture consequence (verified against the code)**: the corpus
roundtrip helper calls `generated_schema()` + minify directly
(`schema_roundtrip.rs:63` area) and its fixtures contain no final
annotations at all — so the semantic corpus lanes stay **unannotated
and unchanged** by this work. A small new **final-output fixture lane**
covers what the corpus cannot: full and lean annotations, caller-key
overwrite, deterministic fingerprinting, modifier changes, and a
library call with no discovered config.

### The `EmissionReport` observation seam

Tags disappear at serialization, so structural floors need a sidecar
produced by the same emitter path as the document:

```text
EmissionReport {
  counts by: applicability/variant, anchor, flavor, origin,
  selected conjuncts, emitted conditionals (post-grouping),
  canonicalization outcomes (Applied / NotApplicable),
  grouping results,
}
```

- Derived during projection, counted **after** selection and grouping
  (what actually emitted, not what should have).
- Outside the JSON document and outside the fingerprint.
- Drives the structural floors and the benchmark accounting — no
  foreign-schema traversal, no raw `if` counting (provider and override
  schemas legitimately contain their own conditionals).

## Verification design

1. **The harness** (compiled Rust, `jsonschema` crate). For any policy
   under test: compile each schema once and reuse the validator across
   every probe; assert law 1 over the probe set and law 2 over defaults
   + CI values. Probe construction **enforces coalesced-values
   semantics internally** (defaults plus null-deletion merge) so no
   test can accidentally validate a bare fragment. Probe classes:
   replacement with every JSON type, null deletion, key deletion,
   coercible and non-coercible strings, unknown object members, empty
   and non-empty collection elements, guard boundary values, pattern
   near-misses. Exhaustive combination coverage on focused microcharts
   (ordinary suite); pairwise coverage on large anchors and the
   temporal matrix (nextest `integration` profile only). From step 1b
   on, all policies project from ONE `LoweredEmissionPlan` (before
   that, the step-0a harness generates schemas separately per profile —
   the only mode today's binary supports).
2. **The fidelity oracle.** `accepts(full) ⊆ accepts(policy)` can pass
   while full itself is wrong, so each focused control records the
   five-tuple
   `(instance, adjudicated contract verdict, full verdict, policy
   verdict, rationale)` — the adjudicated verdict from actual Helm
   behavior plus, where rejection comes from provider typing, the
   intended rendered-sink validity claim. This distinguishes
   chart-render failures, provider-backed invalid manifests, and
   deliberate profile widening. Controls encode only **intended**
   behavior; characterization fixtures may record today's output, but
   no contract test enshrines a known violation.
3. **Three-category semantic controls** per profile promise: retained
   tooth (full rejects, lean rejects), intentionally removed tooth
   (full rejects, lean accepts — documents the trade), positive control
   (both accept). Coverage: unconditional provider typing, scalar
   spellings, presence, not-null, object-host typing (the relax-host
   suspect is priority one), patterns, dependency-gated typing.
4. **The temporal anchor is the wrapper shape, not upstream.** The path
   under test is `temporal.server.replicaCount` *because* the
   downstream chart wraps temporal 0.62.0 as a dependency — vendoring
   upstream alone changes the values prefix and can change
   root-vs-local anchoring. Vendor a minimal pinned wrapper into
   `testdata/charts/`: the same dependency edge, alias/condition
   semantics, the relevant wrapper defaults, and the exact upstream
   archive (include `common` only if it affects the lowered artifact).
   Record chart version, source checksum, and dependency lock in the
   corpus-integrity metadata.
5. **Lean fixture lane**: full-schema-equality fixtures
   (`testdata/chart-corpus-schemas/<chart>.lean.schema.json`) for the
   anchor charts: the doc-example microchart, one arm-heavy chart
   (velero or kyverno), the temporal wrapper and/or signoz-signoz. Same
   dump/adjudication discipline as the full lane; semantic corpus
   fixtures stay unannotated (see above).
6. **Structural CI floors** (deterministic, gateable), read from the
   `EmissionReport`: zero emitter-owned conditionals under the
   all-conditionals-off recipe; zero root-anchored arms under lean;
   Mandatory facts present in every profile's output; per-anchor size
   budgets measured on the **actual shipped bytes** — the production
   compact serializer including its trailing newline
   (`output_pipeline/format.rs:8` `write_schema_json`), not
   `Value::to_string().len()` — e.g. standard-lean temporal under
   4.5 MiB against Helm's 5 MiB limit. Wall-clock stays non-gating.
7. **Benchmark script** (`plan/chart-corpus-scripts/emission-bench.sh`
   or similar), per release, recording per design point: root vs local
   emitter-owned conditional counts (from the report), total condition
   AST nodes, unique conditions and unique `then` payloads, output
   bytes and object count, baseline `helm lint` without any schema,
   repeated median/range warm and cold, exact chart/tool versions and
   machine metadata.

## Implementation steps

Ground rules as in `plan/architecture-review-v3.md` (per-step gates:
fmt, `task lint`, workspace nextest; full AGENTS.md gate list per round;
`sim_assert_eq!`; one commit per step). No public field ships before its
implementation works independently.

### Step 0a — harness against today's binary, with a failure branch

Build the validator/probe framework and the semantic controls using
**separately generated** current full/lean schemas (the only mode the
current binary supports — no multi-projection, no tag floors yet).
Vendor the temporal wrapper anchor. Run the relax-host suspect probe as
preflight:

- Pass: commit the harness normally; no production changes.
- Fail (widen-only violation confirmed): insert a priority bug-fix
  step — regression test and the **minimal** fix land together (apply
  host preparation under every policy; step 1b's structural plan
  subsumes the patch). This fix naturally rides the round-58
  review-findings closure if the rounds align.

### Step 1a — fact model, tags, `EmissionReport`, phase split (fixture-identical)

Introduce the `EmissionFact` sum type on lowered constraints, the
decision-table selection, and the `EmissionReport`; split
`append_conditional_schemas` into "prepare hosts" / "append selected
constraints" while **exactly emulating current policy-specific
preparation** (including today's lean behavior unless 0a's branch fixed
it). `EmissionPolicy` (internal) replaces the bare enum behind the
existing `SchemaProfile` API. Structural floors begin reporting (not yet
gating). Gate: both profiles byte-identical on every fixture; harness
reports no acceptance change.

### Step 1b — `LoweredEmissionPlan` and full-fact support preparation

Introduce the plan artifact with its ownership invariants; enable
support preparation from ALL facts under every policy (the monotone
projection proper). Full output unchanged; lean changes exactly where
support mutations previously vanished with dropped arms — dedicated
controls pin the delta, and the harness must show lean's acceptance set
only grows. Switch the harness to mandatory one-artifact
multi-projection; tag floors become hard gates.

### Step 2 — lean per the decided contract (private policy)

Measure the **exact semantic preset** and individual-knob deltas
through the projection harness (replacing the jq approximation), then
flip lean's preset to the adopted definition (middle point unless the
measurements change the veto). Add the resolved-policy annotation at
the output-pipeline stage and bootstrap the **final-output fixture
lane**; semantic corpus fixtures are untouched. Bootstrap the lean
fixture lane and three-category controls; adjudicate with the harness
and the fidelity oracle. Re-measure temporal downstream; execute the
temporal migration (taskfile flag removed, chart-local
`helm-schema.yaml`) or record Roman's veto.

### Step 3 — canonical emission (one full+lean regeneration)

Emit Mandatory presence/not-null canonically with the
`Applied | NotApplicable(original)` fallback and its invariant test;
collapse constraint-free type unions at generator-owned constructors;
drop the arm-then-fold vestige; move provider-definition extraction
after projection (source-aware, on selected facts) and prune
unreachable definitions. VE evidence: algebraic rewrite spec +
exhaustive small-domain property tests per shape, plus a
zero-flip-both-directions battery run. One clean dump regenerates both
semantic lanes; luup2 gate.

### Step 4 — configuration surface

The root-source boundary ("open root chart source"), the
`helm-schema.yaml` loader (root chart only, `--config`, `--no-config`,
hard-fail on malformed/unknown), tri-state CLI overrides, precedence
resolution + `--print-effective-config` with per-field sources, the
aggregated weakening diagnostic, packaged-chart tests, config
parsing/precedence tests including the exact temporal combination. The
knob matrix goes public — each knob already works independently. No
semantic fixture churn expected; any churn is a red flag.

### Step 5 — benchmark and documentation

Verification item 7; README/`--help` describing profiles as presets
with the retention contract; the config reference with the precedence
table. Measure whether `scalar-spellings: plain` has a real
size/readability benefit and decide its exposure on numbers.

### Step 6 — (deferred) v3 step-11 convergence

Move the projection and canonical emission onto provenance-tagged
passes over the total `SchemaNode` when v3 step 11c lands. No config or
fixture change — the acceptance test that the policy vocabulary was
chosen at the right level.

## Ordering

1. **After** the round-58 review-findings closure (the confirmed printf
   false rejection and siblings) — do not regenerate fixtures twice
   around a known false-rejection fix. If step 0a confirms the
   relax-host violation, its minimal fix belongs in that same round.
2. Steps 0a–1b **before** any further ledger-style precision rounds:
   every new conditional fact lands already tagged.
3. Steps 0a–5 **before** the v3 gen campaign (independent files; v3
   step 11a's moves relocate producers with their tags; the lean lane
   gives 11c a second full-equality corpus to regression against). v3
   steps 4/12a still compose cleanly.
4. Step 6 rides v3, not this plan.

## Recorded follow-ups (not scheduled)

1. **Generation performance regression**: temporal analysis+generation
   was 7.1 s at v1, measured 15–26 s today. Emission policy cannot
   help; profile the analysis phases (BDD normalization,
   backprojection, global projection are the round-51–57 suspects).
2. **Root-arm compile cost upstream**: helm recompiles per invocation
   and santhosh-tekuri compile scales superlinearly with root-arm count
   (444 arms ≈ 49 of 58 s). Both v1 upstream reports stand.
3. **Guard-grouping canonicalization**: identical fragments already
   disjoin their guards at emission (`overlay_lowering.rs:1933` area).
   The follow-up is rerunning/canonicalizing that existing grouping
   after canonical emission changes repetition structure, and measuring
   the incremental root-arm reduction — not a new grouping pass.
4. **`kind-partitions` weakened substitution**: the provable weakening
   `sel ∉ {K1,K2} ∨ S1 ∨ S2` with its algebraic obligation and
   exhaustive selector tests — only if deletion proves too coarse.
5. **`scalar-spellings: plain`**: designed, unexposed; revisit with
   step-5 measurements.
6. **`assume-typed-scalars`**: not offered. Reopening requires all of:
   demonstrated demand, an unmistakably unsafe name, a generation-time
   diagnostic listing every narrowed path, full resolved-policy
   annotation, and CLI-only activation (never discovered config).
7. **`jv` meta-validation** in the downstream lint task pays a second
   compile; if downstream lint time still matters after lean adoption,
   propose caching or dropping it there.
