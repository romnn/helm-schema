# helm-schema — next priorities after items 1-3

This plan covers the next high-level work after the recent correctness push.

## Current state

- All active luup3 charts now validate against the generated
  `values.helm-schema.json` files.
- The large correctness bugs we chased in chart inference are fixed:
  helper-bound objects, open string maps, nullable helper defaults, wrapper
  chart projection issues, and the `inbucket` drift class.
- `helm-schema` test coverage is much stronger than before, including large
  real-chart fixtures plus focused regression tests.
- The Temporal runaway bug is fixed in the sense that generation now finishes
  reliably instead of exhausting swap and RAM.
- Temporal is still far too slow in release mode for its size and usage target.

That means the highest-value work is no longer "basic correctness for one more
chart". The bottleneck has moved.

## Recommended order

1. **Performance and output-size work for Temporal-class charts**
2. **Finish the migration in luup3 and cut over to generated schemas by default**
3. **Unify the dual resource detector**
4. **Implement `kind: List` items[*] structural descent**
5. **Targeted architecture cleanup in the hot IR / generator path**
6. **Broader refactor / abstraction work only after the above is stable**

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

### 2. Migration completion second

We are now in a good position to complete the practical luup3 migration:

- decide which remaining schemadiff lines are real generator bugs vs
  hand-written schema policy
- keep only the overrides that encode non-inferable domain constraints
- switch the repo to generated schemas by default once the remaining diffs are
  intentionally accounted for

This work is high-value because it converts the recent helm-schema improvements
into a simpler steady state for luup3.

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

### B. Output deduplication / structural sharing

The Temporal schema size strongly suggests repeated large subtrees are being
cloned into the final output. We should explicitly investigate:

- whether repeated Kubernetes-derived fragments can be shared structurally in
  memory during merge/build
- whether output should optionally preserve more `$ref`s for repeated subtrees
  instead of eagerly inlining everything

This is likely the biggest remaining performance lever after the recent fixes.

### C. Migration cutover mechanics in luup3

We already have generation, diffing, and validation tasks. The remaining work
is to define the cutover contract clearly:

- which chart-specific overrides are justified
- which hand-written schemas or shared schema refs can be deleted
- how generated schemas are tracked in git
- what CI checks become mandatory after the cutover

This is separate from core helm-schema code, but it is necessary to finish the
work.

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

### Priority 2 — Migration completion in luup3

1. Re-run schemadiff across active charts against the latest helm-schema.
2. Classify remaining diffs into:
   - generator bug
   - justified hand-written override
   - hand-written schema is stale or less accurate
3. Add only the justified override layer needed for cutover.
4. Switch the repo to generated schemas by default chart by chart.

### Priority 3 — Detector unification

Follow `./unify-resource-detector.md`.

Success criteria:

- one production resource detector path
- IR tests and production CLI share the same identity logic
- dead line-oriented detector code removed

### Priority 4 — List-envelope descent

Follow `./list-envelope-items-descent.md`.

Success criteria:

- `kind: List` wrappers no longer suppress inner validation
- inner resources resolve against their actual apiVersion/kind
- existing suppression logic in the chain is deleted

### Priority 5 — Focused architecture cleanup

After priorities 1-4:

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
