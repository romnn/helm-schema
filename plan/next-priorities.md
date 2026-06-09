# helm-schema — next priorities after items 1-3

This plan covers the next high-level work after the recent correctness push.

## Current state

- The luup3 migration is complete: active charts use generated schemas by
  default and the remaining override layer is limited to non-inferable
  deployment-pipeline values.
- The large correctness bugs we chased in chart inference are fixed:
  helper-bound objects, open string maps, nullable helper defaults, wrapper
  chart projection issues, and the `inbucket` drift class.
- `helm-schema` test coverage is much stronger than before, including large
  real-chart fixtures plus focused regression tests.
- The Temporal runaway bug is fixed in the sense that generation now finishes
  reliably instead of exhausting swap and RAM.
- Temporal is still far too slow in release mode for its size and usage target.
- Opt-in JSON Schema minimization exists as a Helm-independent output transform
  that deduplicates repeated schema-position subtrees into root-level `$defs`.

That means the highest-value work is no longer "basic correctness for one more
chart". The bottleneck has moved.

## Recommended order

1. **Continue performance work for Temporal-class charts**
2. **Unify the dual resource detector**
3. **Implement `kind: List` items[*] structural descent**
4. **Targeted architecture cleanup in the hot IR / generator path**
5. **Broader refactor / abstraction work only after the above is stable**

## Why this order

### 1. Performance first

This is the most urgent user-facing problem left.

The current Temporal run no longer blows up memory indefinitely, but it is still
slow enough to distort local iteration and CI ergonomics. We already know:

- small and medium charts like `inbucket` are fast enough
- Temporal is the outlier
- the generated Temporal schema is extremely large
- the remaining time is no longer explained by the original runaway helper bug

So the next work should be a focused performance pass, not a broad correctness
hunt.

### 2. JSON Schema minimization is complete

The luup3 cutover is complete, so the output-size feature is implemented as a
general JSON Schema transform rather than another chart-specific migration
step.

This should stay independent from Helm template inference:

- the minimizer lives in a small helm-agnostic workspace crate
- input is an arbitrary JSON Schema document
- output preserves semantics while replacing repeated schema-position subtrees
  with `$defs` and `$ref`
- the CLI exposes it as opt-in `--minimize`
- description stripping remains a simpler orthogonal output transform

### 3. Detector unification before more deep feature work

`./unify-resource-detector.md` is still real architectural debt.

Right now the production symbolic path and the simpler AST-driven detector are
still separate concerns in the codebase. Even though recent work improved the
production path, unifying them remains the cleanest way to:

- reduce duplicated logic
- make IR tests exercise the real production logic more directly
- open the way for deeper structural features without splitting effort across
  two detector styles

This is more important than broad cleanup because it removes a real ownership
split in the core inference path.

### 4. `kind: List` descent right after detector unification

`./list-envelope-items-descent.md` should come after detector unification, not
before it.

The plan is structurally correct: proper list-envelope descent belongs on top of
an AST-driven detector, not a partially line-driven one. This can unlock real
correctness improvements for charts that currently treat `List` wrappers as
validation black holes.

### 5. Architecture cleanup after hot-path work is proven

There is real cleanup value in the current `symbolic.rs` / generator code, but
it should be constrained and follow the performance and detector work.

Reasons:

- performance work needs profiling-driven edits, not premature abstractions
- detector unification will likely delete or move code anyway
- broad modularization before those changes would churn files without reducing
  long-term complexity much

So cleanup should be real and targeted, not a style exercise.

### 6. Broad refactor last

"More modular, more DRY, more trait-based" is only high-value if it reduces
proven complexity. It should not come before:

- fixing the remaining painful performance problem
- shipping the luup3 migration
- removing the detector split

Otherwise it risks becoming motion without leverage.

## Additional workstreams worth tracking

These are important enough to name explicitly. They are not all "do now", but
they should stay visible.

### A. Performance observability and benchmarks

This is missing from the current repo shape and should be added as part of the
performance pass.

We need a stable way to answer:

- which phase is slow: chart loading, IR extraction, provider lookup, schema
  merge/build, flattening, serialization
- which charts are the worst offenders
- whether a change improved release-mode runtime and peak RSS

This is the enabling work for getting Temporal closer to sub-second output.

Now that Perfetto tracing is available, it should become the source of truth for
runtime analysis. The old hand-maintained `ProfilePhase` / `--profile-phases`
path should be removed after the current minimization work lands, with any
still-useful spans represented by `tracing::instrument` annotations instead.

### B. Output deduplication / structural sharing

The Temporal schema size strongly suggests repeated large subtrees are being
cloned into the final output. We should explicitly investigate:

- whether repeated Kubernetes-derived fragments can be shared structurally in
  memory during merge/build as a performance optimization
- whether an independent JSON Schema minimizer can identify repeated output
  subtrees and move them under `$defs` with `$ref` reuse

The first item belongs to the Temporal performance pass. The second item is a
general output-size feature and should be developed independently so it can be
used for any large generated schema.

### C. Completed luup3 migration contract

The practical cutover is complete. Keep this contract visible so future
override additions do not become hidden generator bug workarounds:

- which chart-specific overrides are justified
- which hand-written schemas or shared schema refs can be deleted
- how generated schemas are tracked in git
- what CI checks become mandatory after the cutover

This is separate from core helm-schema code; it is a maintenance contract for
future chart changes, not a blocker for the completed migration.

### D. Dead-complexity cleanup from exploratory optimization paths

Recent performance work added some scaffolding in the symbolic path. Before
shipping a large follow-up performance PR, we should make sure only the
measured wins remain. This is not a primary workstream, but it should be part
of the review bar for the next performance iteration.

## Concrete next work items

### Priority 1 — Temporal performance pass

1. Add lightweight phase timing around:
   - chart discovery / load
   - IR generation
   - provider resolution
   - schema generation / merge
   - flattening / serialization
2. Add a repeatable release benchmark task for a small set of representative
   charts:
   - `inbucket`
   - `temporal`
   - `signoz`
   - `minio`
3. Identify the largest remaining Temporal hot path.
4. Optimize that path with a measured before/after benchmark.
5. Repeat until the chart is substantially closer to acceptable local
   iteration time.

### Completed — JSON Schema minimization

The minimizer is implemented as a self-contained crate that:

- accepts `serde_json::Value`
- canonicalizes schema subtrees deterministically
- extracts repeated, worthwhile schema-position subtrees into `$defs`
- rewrites occurrences to `$ref`
- stays free of Helm concepts, K8s providers, template inference, and
  luup3-specific policy

The CLI exposes this as `--minimize`, after override merging and reference
flattening and before final writing.

### Priority 2 — Detector unification

Follow `./unify-resource-detector.md`.

Success criteria:

- one production resource detector path
- IR tests and production CLI share the same identity logic
- dead line-oriented detector code removed

### Priority 3 — List-envelope descent

Follow `./list-envelope-items-descent.md`.

Success criteria:

- `kind: List` wrappers no longer suppress inner validation
- inner resources resolve against their actual apiVersion/kind
- existing suppression logic in the chain is deleted

### Priority 4 — Focused architecture cleanup

After priorities 1-3:

1. Split oversized files only where there is a stable ownership boundary.
2. Remove dead helper-analysis scaffolding that did not survive the performance
   pass.
3. Consolidate duplicated schema-merge and path-attribution helpers where the
   behavior is already stable.
4. Keep refactors test-backed and benchmark-checked.

## What should not be prioritized next

- A broad trait-heavy redesign before the hot-path and detector work settle.
- Generic abstraction work without a measured performance, correctness, or
  maintenance payoff.
- More chart-specific correctness hunts before the migration diff classification
  says they are still needed.

## Recommendation

If we only pick one thing next, it should be:

**a measured Temporal performance pass, followed immediately by luup3 migration
completion.**

That sequence has the highest practical value:

- it addresses the remaining painful runtime issue
- it converts the recent inference work into a simpler steady state in luup3
- it sets up the architectural detector cleanup on a more stable base
