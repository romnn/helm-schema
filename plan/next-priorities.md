# helm-schema next priorities

This plan tracks the high-level work after the luup3 migration and the recent
correctness, output-size, parser, comments, and performance passes.

## Current state

- The luup3 migration is complete: active charts use generated schemas by
  default, and the remaining override layer is limited to non-inferable
  deployment-pipeline values.
- The large correctness bugs we chased in chart inference are fixed:
  helper-bound objects, open string maps, nullable helper defaults, wrapper
  chart projection issues, and the `inbucket` drift class.
- `helm-schema` test coverage is much stronger than before, including large
  real-chart fixtures plus focused regression tests.
- Opt-in JSON Schema minimization exists as a Helm-independent output transform
  that deduplicates repeated schema-position subtrees into root-level `$defs`.
- Resource detection has been unified around the AST-driven detector; the old
  line-oriented detector and `DefaultResourceDetector` split are gone.
- Parser ownership is unified around the tree-sitter-backed parser. The older
  fused Rust/yaml-rust parser and its workspace crate are gone.
- Values-file comments are carried into generated JSON Schema descriptions as
  metadata only. They do not create values paths or influence inferred types,
  nullability, requiredness, or object shape.
- The Temporal runaway bug is fixed. Generation now finishes reliably instead
  of exhausting swap and RAM.
- The current Temporal-class performance/RSS pass is good enough for local
  iteration. Further performance work should be profiling-driven, but it is no
  longer the next priority.

## Recommended order

1. **Implement `kind: List` items[*] structural descent**
2. **Targeted architecture cleanup in the hot IR / generator path**
3. **Broader refactor / abstraction work only after the above is stable**

## Completed work

### JSON Schema minimization

The minimizer is implemented as a self-contained crate that:

- accepts `serde_json::Value`
- canonicalizes schema subtrees deterministically
- extracts repeated, worthwhile schema-position subtrees into `$defs`
- rewrites occurrences to `$ref`
- stays free of Helm concepts, K8s providers, template inference, and
  luup3-specific policy

The CLI exposes this as `--minimize`, after override merging and reference
flattening and before final writing. Description stripping remains a simpler,
orthogonal output transform.

### Resource detector unification

`./unify-resource-detector.md` has landed.

Completed criteria:

- one production resource detector path
- IR tests and production CLI share the same identity logic
- dead line-oriented detector code removed

### Parser unification and dead-code cleanup

Completed criteria:

- production and tests converge on the tree-sitter parser as the single
  Helm/YAML template parser
- `FusedRustParser` and the old yaml-rust fused parser path are deleted
- parser fixture tests that remain assert production behavior, not a second
  parser implementation
- dead code left from the old resource detector, parser split, and abandoned
  optimization scaffolding is removed

Tree-sitter YAML comment support is now used by values-file description
extraction.

### Values-file comments as schema descriptions

Completed criteria:

- parse `values.yaml` and additional values files with tree-sitter YAML while
  preserving comments and source ranges
- associate leading, inline, trailing-example, and Helm-docs `@param` comments
  with stable `.Values.*` paths
- feed those comments into schema generation as `description`
- chart-authored values-file descriptions take precedence over upstream K8s /
  CRD descriptions for the same values path
- commented-out keys do not create schema paths and do not participate in
  type/shape/nullability/requiredness inference

This stays separate from template inference: templates answer which values are
accepted and what schema they have; values files provide documentation metadata
for paths that already exist in the inferred schema.

### Temporal performance/RSS pass

Completed criteria:

- use Perfetto traces from `tracing::instrument` spans as the primary profiling
  signal
- measure representative release-mode charts including `inbucket`, `minio`,
  `signoz`, and `temporal`
- track peak RSS alongside wall time
- identify the remaining Temporal hot path around output minimization
- reduce minimizer peak RSS by deferring cloned schema subtrees until a planned
  definition is actually used
- keep minimized output byte-for-byte unchanged for the selected optimization

Representative latest measurements from this pass:

- `inbucket`: about 70 ms wall, 23 MB peak RSS
- `minio`: about 640 ms wall, 34 MB peak RSS
- `signoz`: about 950 ms wall, 61 MB peak RSS
- `temporal` with luup3 minimization flags: about 2 seconds wall, 177 MB peak
  RSS after the deferred-clone minimizer fix

The remaining Temporal cost is acceptable for now. Any future pass should stay
profiling-driven and preserve RSS as a first-class metric.

## Next active priority

### List-envelope descent

Follow `./list-envelope-items-descent.md`.

Success criteria:

- `kind: List` wrappers no longer suppress inner validation
- inner resources resolve against their actual `apiVersion` / `kind`
- existing suppression logic in the provider chain is deleted
- focused tests cover mixed lists, templated list items, and non-list resources
  so normal resource detection does not regress

This is the next structural correctness item because detector unification has
landed and there is now one resource identity path to extend.

## Later work

### Focused architecture cleanup

After list descent:

1. Split oversized files only where there is a stable ownership boundary.
2. Remove dead helper-analysis scaffolding that did not survive the
   performance pass.
3. Consolidate duplicated schema-merge and path-attribution helpers where the
   behavior is already stable.
4. Keep refactors test-backed and benchmark-checked.

### Broader refactor last

"More modular, more DRY, more trait-based" is only high-value if it reduces
proven complexity. It should not come before list-envelope descent, because that
work may still clarify the stable module boundaries.

Avoid:

- broad trait-heavy redesign before the list-descent boundary settles
- generic abstraction work without a measured performance, correctness, or
  maintenance payoff
- more chart-specific correctness hunts before a migration diff classification
  says they are needed

## Maintenance contracts

### Completed luup3 migration contract

Keep this visible so future override additions do not become hidden generator
bug workarounds:

- chart-specific overrides should be justified as application logic,
  deployment-pipeline injected values, or genuinely non-inferable policy
- structural inference gaps should be fixed upstream in helm-schema
- generated schemas should stay tracked and refreshed by the chart tasks
- CI should keep validating and linting charts against generated schemas

### Performance observability

Perfetto tracing is the source of truth for runtime analysis. The old
hand-maintained `ProfilePhase` / `--profile-phases` path has been removed, with
the useful phase boundaries represented by `tracing::instrument` annotations.

Future performance work should answer:

- which phase is slow: chart loading, IR extraction, provider lookup, schema
  merge/build, minimization, flattening, serialization
- which charts are the worst offenders
- whether a change improved release-mode runtime and peak RSS
