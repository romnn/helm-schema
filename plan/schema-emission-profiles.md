# Schema emission policy: passes, profiles, and configuration — v2.1 (2026-08-02)

Goal unchanged from v1 (2026-07-17): during live validation, helm-schema
*generation* is not the slow part — `helm lint` is, because Helm 4
recompiles our large generated `values.schema.json` on every invocation.
v2 replaces v1's one opaque `full|lean` switch with **named,
individually reasoned emission policies**; the built-in profiles remain
exactly two — `full` and `lean` — as preset policy-sets, overridable per
knob through a `helm-schema.yaml` config file. Default behavior must not
change: full-fidelity schemas stay the default and every configuration
obeys the policy laws below.

v2.1 revises v2 after review. The load-bearing changes: the fact model
gains explicit **applicability and placement dimensions** (so policy
never filters by which producer happened to create a fact), the
producer-gate design is replaced by a **monotone policy projection**
(which also fixes a live lean soundness suspect), the harness moves to
**step 0**, normalization becomes **canonical emission** rather than a
knob, `infer-required` is reclassified as X, and the public knob surface
shrinks.

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
  the luup2 temporal chart adopted lean downstream (the only adopter).
- v1 steps 3–4 (lean fixtures, widening test, measurements) never ran.
  There are **no lean fixtures and no widening harness** — the procedural
  root cause of everything below, which is why the harness is now step 0.
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
  folds and host-type relaxations
  (`overlay_lowering.rs:1860-1881`). The relaxation comment states the
  coupling explicitly ("Only arms that actually emit may relax — a
  dropped arm would turn the relaxation into a plain widening"), but the
  lean gate drops arms *before* this point, so a nil-safe member host
  keeps the strict descendant-materialized `type: object` that full
  relaxes — lean would then reject a null/absent host state that full
  accepts and helm renders. Unconfirmed empirically; it is the
  priority-one step-0 control, and the policy projection below fixes the
  class by construction.
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
   Any spelling knob is motivated by size/readability, never speed — and
   the measured size at stake is small, which is why no spelling knob is
   exposed in this plan (see "deliberately not exposed").
3. **The teeth interleave with the cost ladder.** Temporal's
   `replicaCount` typing lives in *nested* (path-local) conditional
   context, so "drop root arms only" keeps it while "drop all
   conditionals" loses it. Placement must therefore be a first-class
   policy dimension.
4. **The doc-page complaint decomposes cleanly.** For the two-key
   Deployment example, full emits four unconditional `allOf` arms
   (per-fact presence + not-null carriers) beside an interned
   `anyOf[{type:integer,format:int32},{type:string,pattern:RADIX}]`
   union. The delta to the "human" schema is: (a) canonical emission of
   unconditional facts (fold into root `required`/property slots —
   validation-equivalent; the not-null conjunct is provably redundant
   against the union), and (b) the spelling union, which is genuine
   precision (`{type: integer}` alone would falsely reject
   `replicas: "3"`, which helm renders).
5. Generation itself now takes 15–26 s on temporal (v1 recorded 7.1 s).
   Emission policy cannot fix analysis cost — recorded as a follow-up.

## Design

### The fact model: applicability, placement, origin

Every lowered constraint carries three independent typed dimensions:

```text
Applicability = Unconditional | Guarded(guard scope)
Placement     = Local (path-local subtree)
              | RootAnchored (cross-path, bypass-proof root arm)
Origin        = Overlay | FailImplication | MergeShadow | OmittedMember
              | Backprojection | TerminalClause | KindPartition
              | ProviderPayload | BaseType | SpellingUnion | Presence | …
```

Policy filters on **Applicability × Placement only**. Origin exists for
diagnostics, provenance annotations, and the v3 provenance enum — never
for policy decisions. This is the type-level protection against another
round-50 drift: today the conditional vector also carries unguarded fail
implications and object-host requirements emitted as unconditional
fragments (`overlay_lowering.rs:1957-1966` drops the `if` wrapper for
empty guard sets), so "split the presence producer" would not have been
enough; *any* unconditional fact must survive every sound profile no
matter which producer created it.

### Policy classes and the two laws

- **VE — validation-equivalent.** Accepts exactly the same instances
  (canonical emission, `$defs` interning/minify, annotation handling).
  Not "semantics-preserving": description removal changes annotations
  and ref bundling has environment-dependent resolution, but instance
  acceptance is identical.
- **W — widen.** Drops a constraint class; accepts strictly more. The
  only class profiles may toggle. The invariant is retained verbatim
  from `emission_policy.rs:5-8`: a reduced configuration may remove
  constraints and therefore widen acceptance, but must never introduce a
  rejection that the full profile does not.
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
| `cross-path-conditionals` | W | Guarded ∧ RootAnchored | overlay, fail, merge-shadow, omitted-member, guarded-backprojection arms |
| `local-conditionals` | W | Guarded ∧ Local | dependency-gated and path-local `if/then` refinements |
| `terminal-clauses` | W | the `if G then false` class | independent of the above |
| `kind-partitions` | W | per-kind arm refinement; off = one union arm | **dependent**: ineffective when `cross-path-conditionals` is off — that combination is diagnosed, not silently accepted |

Deliberately **not** exposed:

- **Presence, not-null, values-default backfill, declared-default
  preservation, falsy escapes, program wrappers** — mandatory in every
  profile. Presence has no measured compile cost and unconditional facts
  are the soundness floor; the rest are X-class if dropped.
- **Normalization** — not a knob; it becomes canonical emission
  (below). A temporary internal flag exists only during the step-3
  rollout and is deleted with it.
- **`scalar-spellings: plain`** — designed (replace pattern'd string
  arms with a plain string type member; W-class at the preimage
  producers `resolve_policy.rs:832-1158`) but **not exposed** until a
  real size/readability benefit is measured; patterns have no compile
  cost, so today it would be a knob without a purpose.
- **`assume-typed-scalars`** (drop spelling-union string arms, leaving
  `{type: integer}`) — **not offered.** It violates the fidelity charter,
  hides the quoted-scalar checks that catch the most common helm values
  mistake, and would be unsafe in an automatically discovered chart
  config. Reopening conditions recorded in follow-ups.

Output-side options (`refs`, `descriptions`, `minify`) remain their own
`OutputPipelineOptions` (`output_pipeline/options.rs`), and
`infer-required` its own explicitly-X policy. One top-level CLI/config
resolver owns all three structs; there is deliberately no single
`EmissionConfig` that conflates validation policy with output transport
concerns.

### Profiles are presets — and the lean decision

```text
full: every W knob on.
lean: cross-path-conditionals off, kind-partitions off,
      terminal-clauses off, local-conditionals ON.
```

**This adopts the measured middle point as lean** (temporal: 17.3 s lint
vs 110 s full, `"three"` control retained), rather than the current
all-conditionals-off behavior. Rationale: the tooth loss was the
motivating regression, wrapper charts (the common downstream shape) keep
most of their typing value through local conditionals, and the size
argument is empirically void (3.84 MB vs 4.19 MB; both under Helm's
limit — today's 50 KB lean was never needed for size). The sub-second
point remains one config line away (`local-conditionals: off`), and the
one downstream lean adopter (temporal) can carry exactly that line in
its chart-local `helm-schema.yaml` if 17 s regresses the check:local
loop unacceptably — per-chart config is precisely the mechanism for it.
This is stated plainly as a **redefinition with a retention contract**,
not a "repair": lean's contract is *every unconditional fact plus every
path-local conditional refinement; only cross-path guard arms, kind
partitions, and terminal clauses are dropped*. Roman can veto toward the
fast variant; then the contract text swaps, the harness controls swap
their expected verdicts, and nothing else in this plan changes.

### The monotone policy projection (replaces producer gates)

Conditional emission is not a pure appender today: it folds object
constraints into the base, relaxes host object typing, reconciles
descendants, and extracts shared provider definitions
(`overlay_lowering.rs:1860-1881`, `provider_definitions.rs`). Filtering
arms before these operations changes the *base*, which is how the lean
relax-host suspect arises. The pipeline therefore becomes:

```text
lower ALL facts (policy-free)
→ compute base ownership, host preparation, default refill,
  and every support mutation from ALL facts
→ select emitted constraints by policy   ← the only policy-aware stage
→ canonical emission / normalization
→ output pipeline (refs → descriptions → minify)
```

The projection invariant: **a W policy may only delete tagged conjuncts
or replace one with a proven weaker conjunct; it must never affect
resolution, base ownership, host preparation, default refill, or
analysis.** Under this design lean still applies the host relaxations
(they are support mutations computed from all facts) while dropping the
arms — widen-only by construction. Before v3's total `SchemaNode`
exists, this is implemented by splitting `append_conditional_schemas`
into "prepare hosts from full facts" and "append selected constraints";
under v3 step 11c the selection becomes a tag-directed pass over one
typed tree, with no config or fixture change (that migration being
invisible is the acceptance test for the knob vocabulary).

### Canonical emission (was: normalize knobs)

Once facts carry Applicability, unconditional presence/not-null emit
**directly** as root `required` entries and property-slot conjuncts —
constructing a root arm and folding it back is an avoidable
representation. Slot conjuncts are conjoined via explicit `allOf` in the
existing slot, or dropped when `schema_excludes_type(null)`
(`schema_model.rs:158-206`) proves implication; never routed through
`merge_into_schema_slot` (the evidence-union combiner,
`path_resolver.rs:1157-1160`) and never creating new slots under the
closed root (that would widen the closure). Constraint-free single-type
`anyOf` arms collapse to `type` arrays. Both run **on generated schemas
only, before user overrides and ref bundling** — caller-owned override
schemas and bundled foreign definitions are never rewritten. After the
equivalence evidence lands (law 1, VE), canonical emission is permanent:
the doc-example output becomes the human schema modulo the spelling
union, in both profiles, with no toggle to keep alive.

### Configuration surface and trust

```yaml
# helm-schema.yaml — root chart directory only (never read from
# dependencies), discovered through the VFS or passed via --config.
version: 1
profile: lean
emission:
  local-conditionals: off      # per-knob override over the preset
```

- **Precedence**: explicit CLI > config file > profile preset > built-in
  default. CLI overrides are tri-state (unset / explicit value) — the
  current always-populated `--profile` default (`cli/mod.rs:62` area)
  cannot distinguish "not supplied", so the flags move to
  `Option<_>`-backed args. An explicit `--profile full` on the CLI
  resets file-level knob overrides (profile choice at a higher
  precedence level discards lower-level deltas against a different
  preset); the resolution table with concrete conflict examples ships in
  the config reference.
- **Trust**: unknown keys and invalid/ineffective combinations are hard
  errors with diagnostics, malformed discovered config is a hard
  failure (never silently ignored), `--no-config` ignores discovered
  chart policy, X-class policies are never activatable from discovered
  config. `--print-effective-config` prints the fully resolved policy
  and exits.
- **Packaged charts**: config discovery must work for `.tgz` inputs.
  Verification item: the CLI wraps the supplied path in a physical VFS
  (`cli/src/lib.rs:62`) while archive extraction demonstrably happens
  for vendored dependencies (`chart/discovery.rs:110`) — confirm or add
  root-archive extraction, with top-level packaged-chart tests.
- **Cache law** (unchanged): policy changes emission only, never
  analysis. `prepared`/`finalized_contract` stay policy-free; the
  policy value is immutable per session; any future
  re-emit-on-one-session API keys the emission stages by resolved
  policy.

### The resolved-policy annotation

The emitted schema records one versioned object — not a preset name,
because preset contents evolve (the exact drift being repaired):

```json
"x-helm-schema-policy": {
  "version": 1,
  "requested-profile": "lean",
  "resolved": { "cross-path-conditionals": false, "...": "..." },
  "narrowing": ["infer-required"],
  "fingerprint": "<hash of resolved policy>"
}
```

The fingerprint is a hash of the resolved policy, **not** the generator
version — embedding the generator version would churn every fixture on
every release with no semantic change; `x-helm-schema-generated` already
marks provenance. The annotation lands in the same step that first
creates lean fixtures, so no later step regenerates them for
annotation-only reasons.

## Verification design (step 0, before any behavior change)

1. **The harness** (compiled Rust, `jsonschema` crate). For any policy
   under test: generate full and policy schemas from one binary, assert
   law 1 over the probe set and law 2 over defaults + CI values. Probe
   classes: replacement with every JSON type, null deletion, key
   deletion, coercible and non-coercible strings, unknown object
   members, empty and non-empty collection elements, guard boundary
   values, pattern near-misses. Exhaustive combination coverage on
   focused microcharts; pairwise coverage on large anchors (random
   sampling alone is too weak for ownership interactions).
2. **Three-category semantic controls** per profile promise — this is
   what turns silent drift into a red test naming the lost check:
   - retained tooth: full rejects, lean rejects;
   - intentionally removed tooth: full rejects, lean accepts (documents
     the trade);
   - positive control: both accept.
   Coverage across: unconditional provider typing, scalar spellings,
   presence, not-null, object-host typing (the relax-host suspect is the
   priority-one control), patterns, dependency-gated typing. Include the
   **exact temporal control** — vendor the temporal chart (public
   temporalio chart) into `testdata/charts/`, not a surrogate.
3. **Lean fixture lane**: full-schema-equality fixtures
   (`testdata/chart-corpus-schemas/<chart>.lean.schema.json`) for anchor
   charts spanning the shapes that broke silently: the doc-example
   microchart, one arm-heavy chart (velero or kyverno), one
   wrapper-with-dependencies chart (temporal and/or signoz-signoz).
   Same dump/adjudication discipline as the full lane.
4. **Structural CI floors** (deterministic, gateable): e.g. zero emitted
   conditional keywords under the all-conditionals-off recipe; zero
   root-anchored arms under lean; presence facts present in every
   profile's output. Wall-clock stays non-gating.
5. **Benchmark script** (`plan/chart-corpus-scripts/emission-bench.sh`
   or similar), run per release, recording per design point: root vs
   local `if` counts, total condition AST nodes, unique conditions and
   unique `then` payloads, output bytes and object count, baseline
   `helm lint` without any schema, repeated median/range warm and cold,
   exact chart/tool versions and machine metadata. This is what turns
   conclusion 1's confounded four points into an actual cost model.

## Implementation steps

Ground rules as in `plan/architecture-review-v3.md` (per-step gates:
fmt, `task lint`, workspace nextest; full AGENTS.md gate list per
round; `sim_assert_eq!`; one commit per step). No public field ships
before its implementation works independently — an intermediate API
whose knobs share one combined behavior is a false contract.

### Step 0 — harness and semantic controls

Verification items 1, 2, 4 against the CURRENT binary (both current
profiles), plus vendoring the temporal anchor. Expected immediate
yield: an empirical verdict on the relax-host widen-only suspect and a
pinned record of today's lean behavior before anything changes. No
production code changes.

### Step 1 — fact tags and the policy projection (fixture-identical)

Introduce Applicability/Placement/Origin on lowered constraints and
split `append_conditional_schemas` into "prepare hosts from full facts"
/ "append selected constraints". `EmissionPolicy` (internal) replaces
the bare enum behind the existing `SchemaProfile` API; today's two
gates are re-expressed as a policy selection reproducing today's exact
behavior. Gate: both profiles byte-identical on every fixture; the
harness reports no acceptance change.

### Step 2 — lean per the decided contract (private policy)

Flip lean's preset to the adopted definition (middle point unless
vetoed). If step 0 confirmed the relax-host violation, this step also
closes it via the projection (support mutations now run under every
policy). Add the resolved-policy annotation. Bootstrap the lean fixture
lane and the three-category controls with the new behavior; adjudicate
with the harness. Full-profile fixtures byte-identical. Re-measure
temporal downstream; decide with Roman whether temporal's taskfile
keeps `--profile lean` or adds the chart-local sub-second recipe.

### Step 3 — canonical emission (one full+lean regeneration)

Emit unconditional presence/not-null canonically, collapse
constraint-free type unions, drop the arm-then-fold vestige. VE
evidence: algebraic rewrite spec + exhaustive small-domain property
tests per shape, plus a zero-flip-both-directions battery run. One
clean dump regenerates both lanes; luup2 gate.

### Step 4 — configuration surface

The `helm-schema.yaml` loader (root chart only, VFS, `--config`,
`--no-config`, hard-fail on malformed/unknown), tri-state CLI
overrides, precedence resolution + `--print-effective-config`, the
packaged-chart verification item, config parsing/precedence tests. The
knob matrix goes public here — each knob already works independently
(steps 1–2). No fixture churn expected (annotation landed in step 2);
any churn is a red flag, not an inconvenience.

### Step 5 — benchmark and documentation

Verification item 5; README/`--help` describing profiles as presets
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
   around a known false-rejection fix.
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
   (444 arms ≈ 49 of 58 s). Both v1 upstream reports stand,
   strengthened by the per-arm evidence.
3. **Guard-grouping canonicalization**: identical fragments already
   disjoin their guards at emission (`overlay_lowering.rs:1933` area,
   three coalescing stages). The follow-up is NOT a new grouping pass —
   it is rerunning/canonicalizing the existing grouping after canonical
   emission changes repetition structure, and measuring the incremental
   root-arm reduction. Root-arm count is the proven cost driver, so this
   may be the highest-leverage full-profile compile fix. Measure first.
4. **`scalar-spellings: plain`**: designed, unexposed; revisit with
   step-5 measurements.
5. **`assume-typed-scalars`**: not offered. Reopening requires all of:
   demonstrated demand, an unmistakably unsafe name, a generation-time
   diagnostic listing every narrowed path, full resolved-policy
   annotation, and CLI-only activation (never discovered config).
6. **`jv` meta-validation** in the downstream lint task pays a second
   compile; if downstream lint time still matters after lean adoption,
   propose caching or dropping it there.
