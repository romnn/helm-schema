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
- Resource detection has been unified around the AST-driven detector; the old
  line-oriented detector and `DefaultResourceDetector` split are gone.
- Parser ownership is unified around the tree-sitter-backed parser. The older
  fused Rust/yaml-rust parser and its workspace crate are gone.
- Inferred schema descriptions still mostly come from upstream Kubernetes / CRD
  schemas. Chart-authored comments in `values.yaml` and layered values files
  are not yet carried into generated JSON Schema descriptions.

That means the highest-value work is no longer "basic correctness for one more
chart". The bottleneck has moved.

## Recommended order

1. **Carry values-file YAML comments into schema descriptions**
2. **Continue performance work for Temporal-class charts**
3. **Implement `kind: List` items[*] structural descent**
4. **Targeted architecture cleanup in the hot IR / generator path**
5. **Broader refactor / abstraction work only after the above is stable**

## Why this order

### 1. Parser unification is complete

After detector unification, the next duplicated structural layer was parsing.
That split is now removed: production and tests use the tree-sitter-backed
parser, and the old yaml-rust fused parser crate is no longer part of the
workspace.

### 2. Values-file comments become schema descriptions

Many Helm chart authors treat `values.yaml` comments as the documentation source
for generated `values.schema.json` descriptions. helm-schema should support that
directly.

This is now the next natural feature because the remaining parser foundation is
tree-sitter-backed and comment-preserving. Normal YAML deserializers discard
comments, so this metadata pass should be built from source ranges and AST
structure, not from deserialized YAML values.

The values-file comment layer should stay separate from template inference:

- templates answer which values are accepted and what types/nullability/shape
  they have;
- values files provide chart-authored documentation metadata for those paths.

Descriptions from values-file comments should generally take precedence over
upstream Kubernetes / CRD descriptions when they attach to the same values path.
The chart author is documenting the chart input contract, while upstream
descriptions document the rendered Kubernetes field. In practice conflicts
should be rare, but chart-authored comments are usually the better user-facing
description for `.Values.*`.

Commented-out values need a careful policy. They may represent supported but
unset inputs, examples, or just prose. The first pass should attach comments to
existing inferred/defaulted values paths; later work can decide whether and how
to infer optional properties from commented-out examples.

### 3. Performance remains important

The current Temporal run no longer blows up memory indefinitely, but it is still
slow enough to distort local iteration and CI ergonomics. We already know:

- small and medium charts like `inbucket` are fast enough
- Temporal is the outlier
- the generated Temporal schema is extremely large
- the remaining time is no longer explained by the original runaway helper bug

This should stay on the roadmap, but it no longer blocks values-comment work.
Parser unification has created a cleaner base for comment-aware values parsing.

### 4. Detector unification is complete

`./unify-resource-detector.md` has landed.

The production symbolic path and detector tests now share the same resource
identity logic. That removed the largest duplicated interpretation path and
created a cleaner foundation for the next cleanup work.

### 5. JSON Schema minimization is complete

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

### 6. `kind: List` descent after parser/comment cleanup

`./list-envelope-items-descent.md` should come after detector unification, not
before it.

The plan is structurally correct: proper list-envelope descent now belongs on
top of the unified AST detector. It can unlock real correctness improvements for
charts that currently treat `List` wrappers as validation black holes.

### 7. Architecture cleanup after comments/list work

There is real cleanup value in the current `symbolic.rs` / generator code, but
it should be constrained and follow the performance, comments, and list descent
work.

Reasons:

- performance work needs profiling-driven edits, not premature abstractions
- comment extraction will likely delete or move code anyway
- broad modularization before those changes would churn files without reducing
  long-term complexity much

So cleanup should be real and targeted, not a style exercise.

### 8. Broad refactor last

"More modular, more DRY, more trait-based" is only high-value if it reduces
proven complexity. It should not come before:

- fixing the remaining painful performance problem
- adding the values-comment metadata layer

Otherwise it risks becoming motion without leverage.

## Additional workstreams worth tracking

These are important enough to name explicitly. They are not all "do now", but
they should stay visible.

### A. Performance observability and benchmarks

Perfetto tracing is now the source of truth for runtime analysis. The old
hand-maintained `ProfilePhase` / `--profile-phases` path has been removed, with
the useful phase boundaries represented by `tracing::instrument` annotations
instead.

The remaining gap is a stable benchmark harness and acceptance target.

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
  memory during merge/build as a performance optimization

The output-size side of this work is complete: the independent JSON Schema
minimizer can already move repeated output subtrees under `$defs` with `$ref`
reuse. The remaining question is whether similar sharing should happen earlier
inside schema construction for runtime and memory efficiency.

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

### E. Values-file comment extraction

Use tree-sitter YAML as a lossless-enough parser for values files:

- preserve comment nodes and byte ranges
- associate leading comments with the nearest following YAML key by path and
  indentation
- support layered values files, not just root `values.yaml`
- surface attached comments as JSON Schema `description`
- prefer chart-authored values comments over upstream K8s / CRD descriptions
  for the same generated values path
- keep commented-out keys out of type inference until an explicit policy exists

## Concrete next work items

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

### Completed — Detector unification

`./unify-resource-detector.md` has landed.

Completed criteria:

- one production resource detector path
- IR tests and production CLI share the same identity logic
- dead line-oriented detector code removed

### Completed — Parser unification and dead-code cleanup

Completed criteria:

- production and tests converge on the tree-sitter parser as the single
  Helm/YAML template parser
- `FusedRustParser` and the old yaml-rust fused parser path are deleted
- parser fixture tests that remain assert production behavior, not a second
  parser implementation
- dead code left from the old resource detector, parser split, and abandoned
  optimization scaffolding is removed
- tree-sitter YAML grammar support remains available for future values-file
  comment extraction

### Priority 1 — Values-file comments as schema descriptions

Success criteria:

- parse `values.yaml` and additional values files with tree-sitter YAML while
  preserving comments and source ranges
- associate leading and inline comments with stable `.Values.*` paths
- feed those comments into schema generation as `description`
- chart-authored values-file descriptions take precedence over upstream K8s /
  CRD descriptions for the same values path
- no type/shape inference from commented-out keys until there is an explicit,
  tested policy for that behavior

### Priority 2 — Temporal performance pass

1. Use Perfetto traces from `tracing::instrument` spans as the primary
   profiling signal.
2. Keep a repeatable release benchmark task for a small set of representative
   charts:
   - `inbucket`
   - `temporal`
   - `signoz`
   - `minio`
3. Identify the largest remaining Temporal hot path.
4. Optimize that path with a measured before/after benchmark.
5. Repeat until the chart is substantially closer to acceptable local
   iteration time.

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

- A broad trait-heavy redesign before the comments and list-descent boundaries
  settle.
- Generic abstraction work without a measured performance, correctness, or
  maintenance payoff.
- More chart-specific correctness hunts before the migration diff classification
  says they are still needed.

## Recommendation

If we only pick one thing next, it should be:

**values-file comments as schema descriptions.**

That has the highest practical value right now:

- parser unification just landed, so the source-range parser foundation is fresh
  in context
- values comments are a user-visible quality improvement that chart authors
  already expect from schema generation tools
- keeping this as metadata avoids mixing documentation extraction into template
  type inference
