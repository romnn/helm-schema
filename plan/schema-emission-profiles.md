# Schema emission profiles — fast `helm lint` opt-out (2026-07-17)

Goal: during live validation (`task -t
…/luup2/deployment/charts/taskfile.yaml check:local`), helm-schema
*generation* is no longer the slow part — `helm lint` itself is, because
Helm 4 recompiles our large generated `values.schema.json` on every
invocation. This plan (a) records the measured root cause, (b) designs an
opt-in way to generate a leaner schema, and (c) gives implementation steps
for a cheaper model. **Default behavior must not change**: full-fidelity
schemas stay the default, all existing fixtures stay byte-identical, and
the opt-out may only *widen* acceptance (drop constraints), never narrow it.

## Measured evidence (temporal chart, 2026-07-17)

Setup: temporal chart from `~/dev/branches/luup2/deployment/charts/temporal`
(copied to a scratch dir), generated schema 4.37 MB (`helm-schema
--strip-descriptions --compact`), helm v4.2.3 (mise), `jv` v0.7.0
(santhosh-tekuri/jsonschema v6.0.1 — the same library helm 4 embeds,
verified via `go version -m` on the helm binary: `santhosh-tekuri/jsonschema/v6
v6.0.2`). Timings via `/usr/bin/time`; variance across runs was large
(±20%), so treat numbers as magnitudes.

| Experiment | Time |
|---|---|
| `helm lint --strict -f values.yaml ./` with full generated schema | **86–104 s** (~430 MB peak RSS) |
| same, `values.schema.json` removed | **0.08 s** |
| same, schema minus the 645 root `allOf` if/then arms | **6.9 s** |
| same, schema minus every `if`/`then`/`else` | 0.23 s |
| `jv <schema> empty.json` (pure compile, trivial instance) full | 74–106 s |
| `jv` compile, minus root arms | ~7 s |
| `jv` compile, minus all `anyOf` | ~1.4 s |
| `jv` compile, all 14,690 `helm-truthy` refs **inlined** (235 K nodes) | **255–296 s** |
| `jv` compile, minus all `pattern` keywords | ≈ full (no win) |
| helm-schema generation of the same schema | **7.1 s** |

Schema census (compact JSON, 4,367,988 bytes, **146,837 schema objects**):
645 root `allOf` arms (all `if`/`then`; 2.23 MB = 51% of bytes), 1,895 `if`
total, 8,271 `anyOf` (7,352 of them 2-branch), 25,161 `$ref` sites of which
**14,690 point at `#/$defs/helm-truthy`**, 994 `pattern` occurrences but
only **21 distinct** regexes, `$defs` only ~484 KB total (interning works).

Conclusions, each directly load-bearing for the design:

1. **The cost is schema *compilation*, not validation.** `jv` against an
   *empty* instance costs the same as against real values. Helm 4's
   validator (santhosh-tekuri v6) compile time scales roughly
   **quadratically with subschema count** (147 K nodes → ~90 s; 74 K → 7 s;
   31 K → 1.4 s; 235 K → ~275 s), and helm recompiles the schema on every
   `helm lint` / `helm template` invocation. `check:local` pays it several
   times per chart (lint runs twice, template/validate/score again).
2. **The dominant node mass is the conditional machinery**: root guard
   arms plus the `anyOf` unions inside them. Removing the root arms alone
   is a **13× speedup** (90 s → 7 s) and halves the schema.
3. **`$ref` interning is a mitigation, not a cost** — inlining the truthy
   def made compile 3× *slower*. Never trade refs for inlining here;
   `--no-minimize`/`--inline-refs` would make lint worse, not better.
4. **Patterns are irrelevant to compile cost** (21 distinct regexes). No
   pattern-related opt-out is justified by this data.
5. **The arm-stripped schema keeps its teeth.** The base tree (properties,
   types, provider payloads, nested path-scoped conditionals, preimage
   patterns) survives: a wrong-typed `temporal.server.replicaCount:
   "three"` is rejected by the arm-stripped schema with the *identical*
   error the full schema produces. Dropping root arms only loses the
   guard-conditional refinements (branch-scoped overlays, fail-branch
   implications, terminal clauses) — and dropping an `if/then` arm can only
   ever *widen* acceptance, so it can never introduce a false rejection.

Reproduction (for re-measuring after implementation):

```sh
cp -r ~/dev/branches/luup2/deployment/charts/temporal /tmp/t && cd /tmp/t
/usr/bin/time -f "%es" helm lint --strict --kube-version 1.33 -f ./values.yaml ./
jq 'del(.allOf)' values.schema.json > lean.json   # the manual approximation of the lean profile
echo '{}' > empty.json && /usr/bin/time -f "%es" jv lean.json empty.json
```

## Design

### One knob, gated at emission, compiler-style

The compiler analogy the project already lives by gives the clean shape:
**same front-end and analysis, a backend emission policy that emits less**
— like `-g0` dropping debug info without changing codegen semantics. The
IR pipeline stays single-model and untouched; only `helm-schema-gen`
consults a policy when *lowering signals into schema*. That keeps:

- one semantic model (no second inference mode to keep correct),
- determinism (policy is an explicit input, not ambient state),
- tests asserting full schemas in both profiles,
- the accuracy default unchanged.

The data says only one class of emission is worth a knob today: the
conditional machinery. So v1 ships **one profile switch**, backed by an
internal policy struct that can grow classes later *if measurements ever
justify them* (they currently don't for patterns):

```rust
// helm-schema-gen (new file: src/emission_policy.rs)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemaProfile {
    /// Everything the analysis can prove. The default.
    #[default]
    Full,
    /// Omit document-level conditional validation (guard-overlay `if/then`
    /// arms, fail-branch implication arms, terminal clauses, synthesized
    /// required-source arms). Only ever widens acceptance. Exists because
    /// Helm 4's validator compile time is superlinear in schema size;
    /// measured 13x faster `helm lint` on large charts.
    Lean,
}
```

CLI: `--profile <full|lean>` (clap `ValueEnum`, default `full`). No
granular `--exclude` flags in v1 — one measured lever, one flag; the enum
is the extension point if a second lever is ever measured. Keep the
`--help` text short but state the measured motivation (validator compile
cost) so users can decide.

**The soundness law (also goes into the policy type's doc comment):** an
emission profile may only *remove constraints* from the emitted schema. It
must never move a branch-scoped schema to an unconditional position
(applying a branch schema without its guard can reject values the chart
accepts), never drop `anyOf` escape branches (falsy escapes, nullability —
removing a union *branch* narrows), and never change what the analysis
computes. Lean ⊇ Full in accepted instances, always.

### What lean drops, precisely

Everything that lowers through the conditional channel of
`ContractSchemaSignals`:

- conditional overlay arms (guarded `if/then`),
- fail-implication arms (from `fail`/abort captures),
- terminal clauses (`if G then false`),
- required-source backprojection arms (they ride fail-implication
  lowering),
- kind-partitioned overlay variants (they are conditional arms).

Everything else stays: base per-path schemas, provider (K8s/CRD) payloads,
values-default backfill, type hints, nullability/falsy unions, scalar
preimage and parser-language patterns, `$defs` interning, requiredness.
The `helm-truthy` def disappears automatically when nothing references it
(the def is inserted by a scan over the emitted document).

Expected lean output for temporal: ~2.1 MB, ~74 K nodes, `helm lint` ~7 s.

### What NOT to do (each rejected on measurement or architecture)

- **No IR/analysis gating.** Skipping analysis passes would fork the
  semantic model, change diagnostics, and save little (generation is 7 s).
- **No post-hoc JSON stripping** (`del(.allOf)` as a real implementation).
  Phase 4 interns provider payloads referenced from arms; post-hoc
  stripping would strand unused `$defs`, and the openness sentinels the
  phases currently communicate through make output surgery fragile. Gate
  at lowering, so unused defs are never created.
- **No `--inline-refs`-style "simplification" for lint** — measured 3×
  worse. If anything, document on `--no-minimize` that disabling interning
  makes downstream validation slower.
- **No pattern opt-out** — measured no compile win, and patterns carry real
  checks (the `replicaCount` control was caught by one).
- **No arm-count caps or "drop only big arms" cleverness** — a cap makes
  output depend on incidental arm ordering and makes accuracy
  discontinuous. One total on/off class is predictable and explainable.

## Implementation steps

Ground rules are the same as `plan/architecture-review-v3.md` (gates:
`cargo check -q --workspace --all-targets`, `cargo nextest run --workspace`,
`cargo fmt --check`, `task lint`; `sim_assert_eq!`; one commit per step;
comments explain why). Steps 1–3 are the feature; 4 is verification.

### Step 1 — policy type and plumbing (fixture-identical)

1. New `crates/helm-schema-gen/src/emission_policy.rs` with
   `SchemaProfile` as above (re-export from gen's `lib.rs`).
2. Add `profile: SchemaProfile` to gen's options struct (the one
   `generate_values_schema` receives; default `Full` via `Default`).
3. Thread it into `build_root_schema` (`gen/src/lib.rs:129-246`).
4. Engine: add the field to `GenerateOptions` in `crates/helm-schema`
   (default `Full`), pass through `session.rs` where gen is invoked
   (`ResolvedContract` build, session.rs:298 area). Re-export
   `SchemaProfile` through the engine's options module so the CLI does not
   import gen directly.
5. Cache contract: the session's stage memoization
   (`SessionCache`) must treat the profile as part of the generation
   input. Inspect how `resolved_contract`/schema stages are keyed; if the
   cache is per-`AnalysisSession` with options fixed at construction,
   state that in a comment; if options can vary per call, include the
   profile in the key. A stale full-profile artifact must never be served
   for a lean request or vice versa.
6. Update `crates/helm-schema/tests/public_surface.rs` for the new field.

Gate: zero fixture changes; default construction compiles everywhere.

### Step 2 — gate the conditional channel in gen (fixture-identical for the default profile)

All anchors are pre-`architecture-review-v3` step 11a paths; if that split
has landed first, the functions keep their names in the new modules
(`overlay_lowering` lowering/emission halves, `fail_requirements.rs`) —
grep by function name.

1. **Phase 3 gate:** in `build_root_schema`, when `profile == Lean`, skip
   `overlay_lowering::collect_conditional_schemas` (`overlay_lowering.rs:38-311`)
   entirely and use an empty conditional set. This transitively removes
   overlay arms, fail-implication arms, kind partitions
   (`kind_partitioned_overlays`), and the synthesized required-source arms
   (`required_source_backprojection.rs` rides the same lowering — verify
   its call site is inside/after the gate and gate it too if it is
   invoked separately).
2. **Terminal clauses gate:** `append_terminal_clauses`
   (`overlay_lowering.rs:732-766`) reads the *signals* directly, not
   phase-3 output — it must be gated explicitly.
3. `append_conditional_schemas` (`overlay_lowering.rs:810-920`) then
   no-ops on the empty set; add no special casing there.
4. **Degenerate-case check (read, don't guess):** base classification
   (`base_schema.rs:56-94` + `ConditionalTargetIndex` use in
   `lib.rs:157-176`) and the default-backfill skip-set
   (`lib.rs:184-202`) both consume the conditional set. With an empty set
   they must behave exactly like a chart that produced no overlays — read
   both sites and confirm no hidden assumption (e.g. an index built from
   signals rather than from the lowered set). Fix only if an assumption
   exists.
5. **Accepted widening, documented:** paths whose *only* typing lives in
   overlay arms (resolve policy defers some evidence to overlays) emit
   their base schema, possibly `{}`, under lean. That is the intended
   trade. Do NOT "rescue" them by folding branch schemas into the base
   unconditionally — that narrows and is unsound (see the soundness law).
6. `helm-truthy`: no action — the def-insertion scan over the emitted
   document only fires when refs exist.

Gate: with `Full`, every fixture byte-identical (this is the release
safety proof). Lean output is exercised in step 3.

### Step 3 — CLI flag and tests

1. CLI: `--profile <full|lean>` on the generate command
   (`crates/helm-schema-cli`), mapped to `GenerateOptions.profile`.
   Help text: one sentence on what lean omits + one on why (Helm 4
   validator compile time superlinear in schema size; ~13× faster lint on
   large charts, only ever accepts more, never less).
2. **Lean fixtures with full-schema equality** (project rule: no selective
   assertions). New `crates/helm-schema-cli/tests/chart_lean_profile.rs`
   generating with `profile = Lean` for two corpus charts — one small
   (e.g. `oauth2proxy` or `inbucket`) and one arm-heavy (e.g. `velero` or
   `cilium`) — compared against new explicit
   `testdata/chart-corpus-schemas/<chart>.lean.schema.json` fixtures via
   `sim_assert_eq!`. Generate the initial fixtures with the dump flow
   (`SCHEMA_DUMP=1` pattern), inspect them (they must contain no root
   `allOf` arms, no `then:false`, no `#/$defs/helm-truthy`), then commit.
3. **Widening guarantee test:** in the same test file, for each of the two
   charts, validate the chart's composed default values (the corpus
   harness already builds these) against both the full and the lean
   schema with the `jsonschema` crate (already a gen dependency):
   assert that whenever the full schema accepts an instance, the lean one
   does too, and specifically that both accept the chart defaults. Also
   assert node-count sanity: the lean document contains no `"if"` key at
   the root `allOf` (cheap structural probe that the gate held).
4. **Default-profile no-churn:** no new test needed — the existing corpus
   suite is the assertion; it must pass untouched.
5. README/flag docs: document the flag next to `--no-minimize`, including
   the note that `--no-minimize`/`--inline-refs` make downstream
   validation slower (measured), so they should not be combined with a
   lint-speed motive.

### Step 4 — end-to-end verification (manual, downstream)

1. Rebuild, then against the scratch temporal copy:
   `helm-schema --strip-descriptions --compact --profile lean -o values.schema.json .`
   Expect ≈ 2.0–2.3 MB output.
2. `helm lint --strict --kube-version 1.33 -f ./values.yaml ./` — expect
   single-digit seconds (baseline table above).
3. Wrong-type control: set a provider-typed leaf (e.g.
   `temporal.server.replicaCount: "three"`) and confirm lint still rejects.
4. Downstream adoption is Roman's call, per chart, in luup2:
   `HELM_SCHEMA_OPTIONS: "--strip-descriptions --compact --profile lean"`
   for temporal (and other slow charts), leaving small charts on full.
   Not part of this repo's changes.

## Ordering relative to plan/architecture-review-v3.md

**Do this plan first.** Rationale:

- It addresses an active downstream pain (multi-minute `check:local` per
  large chart) and is small: additive policy struct + two gates + one flag
  + tests, roughly a day.
- It is independent of v3 steps 1–6 (different files) and *benefits* from
  landing before v3's gen campaign: step 11a will relocate the gated
  functions (pure moves — the gates move with them), and step 11c's
  typed-tree work is easier to fixture-gate when the lean profile already
  exists as a second full-equality corpus lane.
- The only textual overlap is v3 step 4 (`condition_encoding` pattern
  abstention) and step 12a (`kind_partitioned_overlays` moving to the
  contract layer); both compose cleanly — 12a moves where partitions are
  *produced*, this plan gates whether gen *lowers* them.

If v3 (or part of it) happens to land first anyway, nothing here changes
except file paths in step 2 — resolve by function name.

## Recorded follow-ups (not scheduled)

1. **Lossless node-count compaction in `json-schema-minify`.** The
   remaining lean-profile mass is 2-branch `anyOf` unions; many are
   expressible as `type: [t, "null"]` arrays or merged sibling keywords
   with identical semantics. A semantics-preserving compaction pass would
   shrink *both* profiles' compile cost and belongs in the
   Helm-independent minify crate. Only worth doing with before/after `jv`
   compile measurements.
2. **Upstream reports.** (a) helm: `helm lint`/`template` recompile the
   schema per invocation and compile time dominates; (b)
   santhosh-tekuri/jsonschema v6: compile scales superlinearly with
   subschema count (repro: any ~4 MB schema with ~150 K nodes vs an empty
   instance — the temporal schema demonstrates 90 s compile / ms
   validate). Either fix upstream would benefit the full profile too.
3. **Arm grouping by identical `then`** (emit one arm with `anyOf` of the
   `if`s) — reduces arm count but barely reduces node count; only worth
   revisiting if upstream fixes make node count sublinear but per-arm
   overhead remains.
4. **`jv` meta-validation in the downstream lint task** (`jv
   'http://json-schema.org/draft-07/schema#' values.schema.json`) also
   pays a compile of the 4.4 MB document; if downstream lint time still
   matters after lean, that line is a candidate for the chart taskfile to
   skip on lean charts. Downstream-only; no repo change.
