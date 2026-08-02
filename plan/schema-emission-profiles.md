# Schema emission policy: passes, profiles, and configuration — v2.2 (2026-08-02)

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
knob surface. v2.2 corrects the contract-level inconsistencies found in
review: a fourth `Feature` dimension (the previous three could not
express the knobs), semantic anchoring instead of incidental JSON
placement, the step-1 split (fixture identity and full-fact host
preparation are incompatible in one step), an explicit step-0 failure
branch, deletion instead of an unproven union substitution for
`kind-partitions: off`, canonicalization fallback semantics, provider
`$defs` extraction moved out of support mutations, the temporal
precedence migration, a fully specified annotation stage, and tightened
verification mechanics.

This plan is written with the architecture-review-v3 target in mind
(compiler-style phases, one typed schema tree): the fact tags here are
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
  root cause of everything below, which is why the harness is step 0.
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
  step 0 adjudicates it and has an explicit failure branch for it.
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
| full minus 444 root `allOf` arms | 132,204 | 3.84 MB | 8.9 s | 17.3 s | rejects ✓ |
| full minus every `if/then/else` | 114,301 | — | 0.12 s | ≈0.5 s | accepts ✗ |
| today's lean | 2,437 | 50.7 KB | 0.05 s | 0.14 s | accepts ✗ |
| full minus every `pattern` | 150,299 | — | 56 s | — | n/a |

Conclusions:

1. **Conditional evaluation dominates compile cost in this case, and
   root-anchored arms are the most expensive class per unit.** Removing
   the 444 root arms (12% of nodes) removes ~49 of 58 s; removing the
   remaining ~1,124 nested `if`s removes the rest. This does NOT yet
   prove cost is a function of keyword count alone — placement,
   condition-tree size, uniqueness, and nesting are confounded in these
   four points; the step-5 benchmark separates them.
2. **Patterns remain compile-irrelevant** (33 distinct regexes today).
   Any spelling knob is motivated by size/readability, never speed —
   which is why no spelling knob is exposed in this plan.
3. **The teeth interleave with the cost ladder.** Temporal's
   `replicaCount` typing lives in *local* (path-anchored) conditional
   context, so "drop root arms only" keeps it while "drop all
   conditionals" loses it. Anchoring must therefore be a first-class
   policy dimension.
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

### The fact model: four dimensions

Every lowered constraint carries:

```text
Applicability  = Unconditional | Guarded(guard scopes)   ← owns the guards
RequiredAnchor = Local(path) | Root
Feature        = ConditionalRefinement | TerminalClause | KindPartition
Origin         = Overlay | FailImplication | MergeShadow | OmittedMember
               | Backprojection | ProviderPayload | BaseType
               | SpellingUnion | Presence | …
```

- **Policy filters on `Applicability × RequiredAnchor × Feature` only.**
  `Origin` exists for diagnostics, the resolved-policy annotation, and
  the v3 provenance enum — never for policy decisions. This is the
  type-level protection against another round-50 drift: the conditional
  vector today also carries unguarded fail implications and object-host
  requirements emitted as unconditional fragments
  (`overlay_lowering.rs:1957-1966` drops the `if` wrapper for empty
  guard sets), so *any* unconditional fact must survive every sound
  profile no matter which producer created it.
- **`Applicability` owns the guard scopes** — it replaces, not
  duplicates, the current `guards`/`nested_guard_scopes` fields, so the
  illegal state "Unconditional with non-empty guards" is
  unrepresentable.
- **`RequiredAnchor` is semantic, not incidental JSON placement**: the
  *minimum safe anchor* computed during lowering (Root exactly when a
  union alternative at some ancestor could bypass the constraint —
  the bypass-proofness reason in `overlay_lowering.rs:290-293`). An
  emitter relocation can then never silently change profile membership,
  because membership was never defined by where the emitter happened to
  put the `if`.

### Policy classes and the two laws

- **VE — validation-equivalent.** Accepts exactly the same instances
  (canonical emission, `$defs` interning/minify, annotation handling).
  Not "semantics-preserving": description removal changes annotations,
  and reference-mode equivalence holds only under a fixed resolver
  environment — reference transport stays outside the core
  acceptance-law harness.
- **W — widen.** Drops a constraint class; accepts the same or more
  (`accepts(full) ⊆ accepts(config)`; equality when a chart has no
  affected facts). The only class profiles may toggle. The invariant is
  retained verbatim from `emission_policy.rs:5-8`: a reduced
  configuration may remove constraints and therefore widen acceptance,
  but must never introduce a rejection that the full profile does not.
- **X — narrow.** Adds or keeps constraints beyond proven facts, or
  rejects something helm renders. **Never part of any profile and never
  activatable by discovered chart config.** `infer-required` is X (the
  engine calls it a heuristic that adds constraints after the resolved
  contract, `session.rs:220` area); it stays an explicit CLI opt-in with
  its own policy type.

Laws every configuration must satisfy, checked by the step-0 harness:

1. **Monotonicity law** (VE and W): `accepts(full) ⊆ accepts(config)`;
   for a VE-only delta, equality. The harness provides *regression
   evidence* over the probe batteries — a finite battery cannot prove
   equivalence, so VE passes additionally carry a narrow algebraic
   rewrite specification with exhaustive small-domain property tests per
   recognized shape.
2. **Lint floor** (all classes including X): composed chart defaults and
   CI values files must validate.

### The public knob matrix

| knob | class | filter | notes |
|---|---|---|---|
| `root-anchored-conditionals` | W | Guarded ∧ Root ∧ ConditionalRefinement | overlay, fail, merge-shadow, omitted-member, guarded-backprojection arms |
| `local-conditionals` | W | Guarded ∧ Local ∧ ConditionalRefinement | dependency-gated and path-local `if/then` refinements |
| `terminal-clauses` | W | Feature = TerminalClause | independent of anchor |
| `kind-partitions` | W | Feature = KindPartition | **off = DELETE the partition refinements** (pure widening). The tempting "one union arm" substitution is NOT automatically weaker: full's `(sel=K1 ⇒ S1) ∧ (sel=K2 ⇒ S2)` vacuously accepts unknown selectors, while an unconditional `S1 ∨ S2` rejects them. A substitution shaped `sel ∉ {K1,K2} ∨ S1 ∨ S2` is a provable weakening but carries an algebraic obligation plus exhaustive selector tests — recorded as a follow-up, not assumed. **Dependent**: ineffective when `root-anchored-conditionals` is off; that combination is diagnosed, not silently accepted. |

Deliberately **not** exposed:

- **Presence, not-null, values-default backfill, declared-default
  preservation, falsy escapes, program wrappers** — mandatory in every
  profile. Presence has no measured compile cost and unconditional facts
  are the soundness floor; the rest are X-class if dropped.
- **Normalization** — not a knob; it becomes canonical emission (below).
  A temporary internal flag exists only during the step-3 rollout and is
  deleted with it.
- **`scalar-spellings: plain`** — designed (replace pattern'd string
  arms with a plain string type member; W-class at the preimage
  producers `resolve_policy.rs:832-1158`) but **not exposed** until a
  real size/readability benefit is measured.
- **`assume-typed-scalars`** (drop spelling-union string arms, leaving
  `{type: integer}`) — **not offered.** It violates the fidelity
  charter, hides the quoted-scalar checks that catch the most common
  helm values mistake, and would be unsafe in an automatically
  discovered chart config. Reopening conditions in follow-ups.

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

**This adopts the measured middle point as lean** (temporal: 17.3 s lint
vs 110 s full, `"three"` control retained), rather than the current
all-conditionals-off behavior. Rationale: the tooth loss was the
motivating regression, wrapper charts (the common downstream shape) keep
most of their typing value through local conditionals, and size is not
binding **today** — 3.84 MB vs 4.19 MB, both under Helm's 5 MiB limit,
though with limited headroom, so the verification floors include a
deterministic size budget per anchor chart to keep future precision
rounds from quietly pushing standard lean over the limit. The
sub-second point remains one config line away
(`local-conditionals: off`). This is stated plainly as a **redefinition
with a retention contract**, not a "repair": lean's contract is *every
unconditional fact plus every locally-anchored conditional refinement;
only root-anchored guard arms, kind partitions, and terminal clauses are
dropped*. Roman can veto toward the fast variant; then the contract text
swaps, the harness controls swap their expected verdicts, and nothing
else changes.

**Temporal migration (exact).** Temporal's taskfile currently passes
`--profile lean` on the CLI. Under the precedence rules below, an
explicit CLI profile resets file-level knob deltas — so a discovered
`local-conditionals: off` would be discarded while that flag remains.
The migration is: remove `--profile lean` from temporal's
`HELM_SCHEMA_OPTIONS` and add to the chart directory:

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

### The monotone policy projection

Conditional emission is not a pure appender today: it folds object
constraints into the base, relaxes host object typing, and reconciles
descendants (`overlay_lowering.rs:1860-1881`). Filtering arms before
these operations changes the *base*, which is how the lean relax-host
suspect arises. The pipeline becomes:

```text
lower ALL facts (policy-free)
→ compute base ownership, host preparation, and default refill
  from ALL facts                       ← support semantics, policy-free
→ select emitted constraints by policy ← the only policy-aware stage
→ canonical emission
→ VE output sharing: provider `$defs` extraction, interning, minify
→ output pipeline (refs → descriptions) and final annotation
```

The projection invariant: **a W policy may only delete tagged conjuncts
or replace one with a proven weaker conjunct; it must never affect
resolution, base ownership, host preparation, default refill, or
analysis.** Under this design lean still applies the host relaxations
(support mutations computed from all facts) while dropping the arms —
widen-only by construction.

Provider-definition extraction is deliberately **not** a support
mutation: it is validation-equivalent output sharing and operates on the
**projected** document, so dropped arms can neither strand unused
definitions nor shift interning thresholds through content that never
emits. Definition extraction cannot affect support semantics; unreachable
definitions are pruned.

A second consequence: after the projection exists, **several policies
project from one policy-free artifact** — the session lowers and
analyzes once, and the emission stages (keyed by resolved policy) re-run
per policy. The harness matrix depends on this; re-running temporal's
15–26 s analysis per policy would be prohibitive and would introduce
cache-state differences between compared outputs.

Before v3's total `SchemaNode` exists, the projection is implemented by
splitting `append_conditional_schemas` into "prepare hosts from full
facts" and "append selected constraints"; under v3 step 11c the
selection becomes a tag-directed pass over one typed tree, with no
config or fixture change (that migration being invisible is the
acceptance test for the policy vocabulary).

### Canonical emission (was: normalize knobs)

Once facts carry Applicability, unconditional presence/not-null emit
**directly** as root `required` entries and property-slot conjuncts —
constructing a root arm and folding it back is an avoidable
representation. Rules:

- Slot conjuncts conjoin via explicit `allOf` in the existing slot, or
  are dropped when `schema_excludes_type(null)`
  (`schema_model.rs:158-206`) proves implication; never routed through
  `merge_into_schema_slot` (the evidence-union combiner,
  `path_resolver.rs:1157-1160`).
- **Fallback is total**: `canonicalize → Applied |
  NotApplicable(original constraint)`. When the expected slot is absent
  under the closed root, the original root-anchored conjunct is retained
  unchanged — skipping the fact would widen (violating the VE claim),
  and inserting `required` without an allowed property can make the
  schema unsatisfiable. Invariant test: every removed carrier either
  produced an equivalent direct constraint or was proven redundant.
- Constraint-free single-type `anyOf` arms collapse to `type` arrays —
  implemented at **generator-owned constructors**, not as a raw walk
  over emitted JSON: the generated document embeds provider-owned
  foreign payloads that must never be rewritten accidentally.
- Canonical emission runs on generated schemas only, **before** user
  overrides and ref bundling; caller-owned override schemas and bundled
  foreign definitions are never rewritten.

After the equivalence evidence lands (law 1, VE), canonical emission is
permanent: the doc-example output becomes the human schema modulo the
spelling union, in both profiles, with no toggle kept alive.

### Configuration surface and trust

```yaml
# helm-schema.yaml — root chart directory only (never read from
# dependencies), discovered through the VFS or passed via --config.
version: 1            # config-format version
profile: lean
emission:
  local-conditionals: off
```

- **Precedence**: explicit CLI > config file > profile preset > built-in
  default. CLI overrides are tri-state (unset / explicit) — the current
  always-populated `--profile` (`cli/mod.rs:62` area) cannot distinguish
  "not supplied", so flags move to `Option<_>`-backed args. An explicit
  CLI `--profile` resets file-level knob overrides. The resolution table
  with concrete conflict examples (including the temporal case above)
  ships in the config reference.
- **Trust**: unknown keys and invalid/ineffective combinations are hard
  errors with diagnostics; malformed discovered config is a hard
  failure; `--no-config` ignores discovered chart policy; X-class
  policies are never activatable from discovered config.
- **Visibility of automatic weakening**: when discovered config
  disables constraints relative to full, the CLI emits **one aggregated
  diagnostic** ("loaded helm-schema.yaml; disabled root-anchored
  conditionals and terminal clauses") — never one message per fact.
  `--print-effective-config` prints the resolved policy **with each
  field's source** (built-in / profile / file path / CLI) and resolves
  without chart analysis, provider access, or network activity.
- **Library boundary**: the library API takes explicit typed policy
  only; config discovery belongs to the CLI composition root. Library
  callers are never surprised by a file on disk.
- **Packaged charts**: config discovery must work for `.tgz` inputs.
  Verification item: the CLI wraps the supplied path in a physical VFS
  (`cli/src/lib.rs:62`) while archive extraction demonstrably happens
  for vendored dependencies (`chart/discovery.rs:110`) — confirm or add
  root-archive extraction, with top-level packaged-chart tests.
- **Cache law** (unchanged): policy changes emission only, never
  analysis. `prepared`/`finalized_contract` stay policy-free; emission
  stages are keyed by resolved policy (required by the multi-policy
  projection above).

### The resolved-policy annotation

Final emitted documents self-identify; the library's `GeneratedSchema`
artifact stays unannotated. The annotation is inserted at the **end of
the output pipeline** — after overrides, required inference, output
transforms, and minification — beside `x-helm-schema-generated`, and
overwrites any caller-provided key of the same name so it remains
authoritative:

```json
"x-helm-schema-policy": {
  "config-format-version": 1,
  "policy-vocabulary-version": 1,
  "requested-profile": "lean",
  "resolved": { "root-anchored-conditionals": false, "...": "..." },
  "narrowing": ["infer-required"],
  "modifiers": {
    "overrides": { "count": 2, "digest": "<sha256 of canonical override content>" },
    "reference-mode": "bundled"
  },
  "fingerprint": "<sha256 over the canonical JSON (sorted keys) of resolved+narrowing+modifiers>"
}
```

Recording the non-emission validation modifiers (whether
`infer-required` ran, override count and content digests without leaking
paths, reference mode) prevents a schema with a replacement override
from claiming "full" while validating something else entirely. The
fingerprint hashes the resolved policy object, **not** the generator
version — embedding the version would churn every fixture on every
release with no semantic change; `x-helm-schema-generated` already marks
provenance.

**Fixture consequence, accepted explicitly**: the annotation changes
full output too. The full corpus lane regenerates ONCE when the
annotation lands, with a structural check that every diff touches only
`x-helm-schema-policy` (annotation keywords are acceptance-neutral in
Draft-07, so the battery trivially reports zero flips — run it anyway).

## Verification design (step 0, before any behavior change)

1. **The harness** (compiled Rust, `jsonschema` crate). For any policy
   under test: generate full and policy schemas **from one lowered
   artifact** (multi-policy projection), compile each schema once and
   reuse the validator across every probe, then assert law 1 over the
   probe set and law 2 over defaults + CI values. Probe classes:
   replacement with every JSON type, null deletion, key deletion,
   coercible and non-coercible strings, unknown object members, empty
   and non-empty collection elements, guard boundary values, pattern
   near-misses. Exhaustive combination coverage on focused microcharts
   (ordinary test suite); pairwise coverage on large anchors and the
   temporal matrix (nextest `integration` profile only).
2. **Three-category semantic controls** per profile promise:
   - retained tooth: full rejects, lean rejects;
   - intentionally removed tooth: full rejects, lean accepts (documents
     the trade);
   - positive control: both accept.
   Coverage across: unconditional provider typing, scalar spellings,
   presence, not-null, object-host typing (the relax-host suspect is
   the priority-one control), patterns, dependency-gated typing.
   Include the **exact temporal control** — vendor the temporal chart
   into `testdata/charts/` with pinned chart version, source checksum,
   and dependency lock recorded in the corpus-integrity metadata.
   Controls encode only **intended** behavior; characterization
   fixtures may record today's output, but no contract test enshrines a
   known violation.
3. **Lean fixture lane**: full-schema-equality fixtures
   (`testdata/chart-corpus-schemas/<chart>.lean.schema.json`) for anchor
   charts spanning the shapes that broke silently: the doc-example
   microchart, one arm-heavy chart (velero or kyverno), one
   wrapper-with-dependencies chart (temporal and/or signoz-signoz).
   Same dump/adjudication discipline as the full lane.
4. **Structural CI floors** (deterministic, gateable): counted over
   **emitter-owned tagged conditionals**, never raw `if` keys (foreign
   provider and override schemas legitimately contain their own
   conditionals). Floors: zero emitter-owned conditionals under the
   all-conditionals-off recipe; zero root-anchored arms under lean;
   presence facts present in every profile's output; per-anchor emitted
   size budgets with headroom (e.g. standard-lean temporal under
   4.5 MiB against Helm's 5 MiB limit). Wall-clock stays non-gating.
5. **Benchmark script** (`plan/chart-corpus-scripts/emission-bench.sh`
   or similar), run per release, recording per design point: root vs
   local `if` counts, total condition AST nodes, unique conditions and
   unique `then` payloads, output bytes and object count, baseline
   `helm lint` without any schema, repeated median/range warm and cold,
   exact chart/tool versions and machine metadata.

## Implementation steps

Ground rules as in `plan/architecture-review-v3.md` (per-step gates:
fmt, `task lint`, workspace nextest; full AGENTS.md gate list per round;
`sim_assert_eq!`; one commit per step). No public field ships before its
implementation works independently — an intermediate API whose knobs
share one combined behavior is a false contract.

### Step 0 — harness and semantic controls, with a failure branch

Verification items 1, 2, 4 against the CURRENT binary (both current
profiles), plus vendoring the temporal anchor. Run the relax-host
suspect probe as **preflight**:

- If it passes: commit the harness normally; no production changes.
- If it fails (widen-only violation confirmed): insert a priority
  bug-fix step — land the regression test and the **minimal** projection
  fix together (apply host preparation under every policy; the
  structural split in step 1b later subsumes the minimal patch), then
  resume the sequence. This fix naturally rides the round-58
  review-findings closure if the rounds align.

### Step 1a — fact tags and phase split (fixture-identical)

Introduce the four-dimension tags on lowered constraints and split
`append_conditional_schemas` into "prepare hosts" / "append selected
constraints" while **exactly emulating current policy-specific
preparation** (including today's lean behavior, unless step 0's failure
branch already fixed it). `EmissionPolicy` (internal) replaces the bare
enum behind the existing `SchemaProfile` API. Gate: both profiles
byte-identical on every fixture; harness reports no acceptance change.

### Step 1b — full-fact support preparation (behavior-changing, lean only)

Enable host preparation and default refill from ALL facts under every
policy — the monotone projection proper. Full output is unchanged
(nothing is dropped in full); lean output changes exactly where the
support mutations previously vanished with dropped arms. Dedicated
controls pin the delta; the harness must show lean's acceptance set only
grows (this is a widen-only bug-fix class).

### Step 2 — lean per the decided contract (private policy)

Flip lean's preset to the adopted definition (middle point unless
vetoed). Add the resolved-policy annotation at the output-pipeline
stage — the full corpus lane regenerates ONCE with annotation-only
diffs (structurally checked). Bootstrap the lean fixture lane and the
three-category controls with the new behavior; adjudicate with the
harness. Re-measure temporal downstream; execute the temporal migration
(taskfile flag removed, chart-local `helm-schema.yaml`) or record
Roman's veto.

### Step 3 — canonical emission (one full+lean regeneration)

Emit unconditional presence/not-null canonically with the
`Applied | NotApplicable(original)` fallback and its invariant test,
collapse constraint-free type unions at generator-owned constructors,
drop the arm-then-fold vestige. Move provider-definition extraction
after projection; prune unreachable definitions. VE evidence: algebraic
rewrite spec + exhaustive small-domain property tests per shape, plus a
zero-flip-both-directions battery run. One clean dump regenerates both
lanes; luup2 gate.

### Step 4 — configuration surface

The `helm-schema.yaml` loader (root chart only, VFS, `--config`,
`--no-config`, hard-fail on malformed/unknown), tri-state CLI overrides,
precedence resolution + `--print-effective-config` with per-field
sources, the aggregated weakening diagnostic, the packaged-chart
verification item, config parsing/precedence tests including the exact
temporal combination. The knob matrix goes public here — each knob
already works independently (steps 1–2). No fixture churn expected
(annotation landed in step 2); any churn is a red flag.

### Step 5 — benchmark and documentation

Verification item 5; README/`--help` describing profiles as presets with
the retention contract; the config reference with the precedence table.
Measure whether `scalar-spellings: plain` has a real size/readability
benefit and decide its exposure on numbers.

### Step 6 — (deferred) v3 step-11 convergence

Move the projection and canonical emission onto provenance-tagged passes
over the total `SchemaNode` when v3 step 11c lands. No config or fixture
change — the acceptance test that the policy vocabulary was chosen at
the right level.

## Ordering

1. **After** the round-58 review-findings closure (the confirmed printf
   false rejection and siblings) — do not regenerate fixtures twice
   around a known false-rejection fix. If step 0's failure branch
   confirms the relax-host violation, its minimal fix belongs in that
   same round.
2. Steps 0–1 **before** any further ledger-style precision rounds:
   every new conditional fact lands already tagged.
3. Steps 0–5 **before** the v3 gen campaign (independent files; v3 step
   11a's moves relocate producers with their tags; the lean lane gives
   11c a second full-equality corpus to regression against). v3 steps
   4/12a still compose cleanly.
4. Step 6 rides v3, not this plan.

## Recorded follow-ups (not scheduled)

1. **Generation performance regression**: temporal analysis+generation
   was 7.1 s at v1, measured 15–26 s today. Emission policy cannot help;
   profile the analysis phases (BDD normalization, backprojection,
   global projection are the round-51–57 suspects). Deserves its own
   measurement round against the "seconds, not tens" design goal.
2. **Root-arm compile cost upstream**: helm recompiles per invocation
   and santhosh-tekuri compile scales superlinearly with root-arm count
   (444 arms ≈ 49 of 58 s). Both v1 upstream reports stand, strengthened
   by the per-arm evidence.
3. **Guard-grouping canonicalization**: identical fragments already
   disjoin their guards at emission (`overlay_lowering.rs:1933` area,
   three coalescing stages). The follow-up is NOT a new grouping pass —
   it is rerunning/canonicalizing the existing grouping after canonical
   emission changes repetition structure, and measuring the incremental
   root-arm reduction. Root-arm count is the proven cost driver, so this
   may be the highest-leverage full-profile compile fix. Measure first.
4. **`kind-partitions` weakened substitution**: the provable weakening
   `sel ∉ {K1,K2} ∨ S1 ∨ S2` (or `if sel ∈ known-kinds then
   anyOf(S1,S2)`) with its algebraic obligation and exhaustive selector
   tests — only if deletion proves too coarse in practice.
5. **`scalar-spellings: plain`**: designed, unexposed; revisit with
   step-5 measurements.
6. **`assume-typed-scalars`**: not offered. Reopening requires all of:
   demonstrated demand, an unmistakably unsafe name, a generation-time
   diagnostic listing every narrowed path, full resolved-policy
   annotation, and CLI-only activation (never discovered config).
7. **`jv` meta-validation** in the downstream lint task pays a second
   compile; if downstream lint time still matters after lean adoption,
   propose caching or dropping it there.
