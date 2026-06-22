# Agents

## Design goal: structural static analysis first

helm-schema should recover a chart's meaning through typed, structural static analysis whenever that is possible, and only fall back to heuristics when the chart has genuinely run out of precise static signals.

This is a core design principle of the project, not an implementation detail. The point of helm-schema is not to approximate Helm templates with string tricks; it is to understand, as precisely as possible, what the chart structurally means and then generate JSON Schema from that understanding.

In practice, that means:

- If something can be known from the parsed Helm/YAML structure, helper bodies, control flow, or explicit manifest shape, helm-schema should derive it from that structure deterministically.
- If the chart is genuinely ambiguous, helm-schema should preserve that ambiguity instead of collapsing it into a convenient but potentially wrong guess.
- If the chart does not provide enough information for a precise answer, helm-schema may use bounded heuristics, but only as a last resort and never as the primary source of truth.

A good shorthand for this is:

> helm-schema is a static analyzer first and a heuristic inference engine second.

Or even more strictly:

> No heuristic should exist for a problem that can be solved by typed structural analysis.

### What this means concretely

helm-schema should prefer:

- typed Helm expression analysis over regexes or text scanning
- YAML / AST structure over line-shape heuristics
- helper expansion over filename guessing
- explicit candidate preservation over premature collapsing
- "unknown" or "ambiguous" over a wrong deterministic-looking answer

Examples of the kind of precision we want:

- If a manifest writes `kind:` before `apiVersion:`, that should still be detected correctly from structure.
- If `apiVersion` is chosen through `if` / `else` branches, helm-schema should analyze those branches structurally instead of guessing from nearby text.
- If `apiVersion` comes from a helper like `{{ include "grafana.hpa.apiVersion" . }}`, and that helper statically resolves to a finite set of literals, helm-schema should derive those exact candidates from helper analysis.
- If a template emits `kind: List` and then places real Kubernetes objects under `items:`, helm-schema should treat the contained objects as the meaningful resources rather than stopping at the wrapper.
- If a `.Values.*` path is guarded by `if`, `with`, `range`, `default`, `eq`, `not`, or `or`, the resulting schema should reflect those semantics because the typed control-flow analysis says so, not because a text pattern happened to match.

### When heuristics are appropriate

Heuristics are still useful, but only after structural analysis has gone as far as it can.

Good examples of acceptable heuristic fallback:

- inferring a likely apiVersion for a known kind only after the chart itself failed to statically reveal it
- scanning configured cache roots for candidate schemas only after exact structural resolution failed
- using a bounded shortlist for well-known resource kinds when no stronger signal exists
- using version fallback to reach older Kubernetes schema bundles for removed APIs

Even then, heuristics should be:

- bounded
- explicit
- lower-priority than structural facts
- willing to abstain

That means heuristics should never silently override a precise structural result, and they should never replace real ambiguity with a false sense of certainty. When a heuristic materially affects resolution, it should be diagnosable.

### Project standard

The standard for helm-schema should be:

- use precise static analysis wherever the chart makes precision possible
- preserve exact alternatives when the chart expresses alternatives
- use heuristics only for the residual cases that cannot be solved structurally
- prefer a principled "ambiguous" or "unknown" result over a wrong guess

That is the bar that keeps helm-schema aligned with its purpose: a smart, typed, template-aware static analyzer for Helm charts, not a pile of ad hoc text heuristics.

## Design goal: simplicity by deletion

Precision is the primary goal, but the preferred way to reach that precision is
through a design with fewer moving parts, fewer parallel representations, and
fewer compatibility layers.

This matters because helm-schema has historically accumulated multiple partial
models for the same idea: line-driven trackers beside parser-backed structure,
parallel helper/fragment value shapes, generator-side reassembly of facts that
the IR already knew, and fallback layers that survived long after the precise
path was available. That kind of architecture makes correctness harder to
reason about, not easier.

The standard should be:

- prefer one semantic model over multiple projections of the same fact
- prefer immutable precomputed structure over mutable incremental state where possible
- prefer parser-backed structural models over line-shape or text-shape recovery
- prefer deleting obsolete fallback paths once the structural path is good enough
- prefer a small, explicit bounded fallback over a stack of overlapping rescue heuristics

In practice, when choosing between two designs with similar correctness:

- choose the one with fewer representations to keep in sync
- choose the one that removes code rather than adding another layer
- choose the one whose invariants can be explained in terms of the parsed
  language structure rather than incidental source layout

If a new abstraction does not make the system both easier to reason about and
more structurally correct, it is probably the wrong abstraction.

### Rust guideline: do not be cleverer than necessary

For Rust code in this repo, follow the KISS principle strictly.

- Do not introduce generic helpers, iterator tricks, macros, wrapper
  functions, or tiny adapter layers unless they clearly remove real
  duplication or make the control flow easier to understand.
- Prefer the obvious local loop or direct `match` when it says the thing more
  plainly than a reusable helper.
- If an abstraction saves only a few repeated lines but makes the call site
  harder to read, do not add it.
- If a helper needs a closure or type parameter just to spell an otherwise
  obvious operation, that is a strong sign it may be cleverer than necessary.

In short: the simpler Rust is usually the better Rust here. Prefer direct,
boring code over abstraction that does not materially improve correctness or
clarity.

### Rust test layout and source LOC hygiene

Keep production `src/` files focused on production code so source LOC remains a
useful simplicity metric.

- Public API and end-to-end behavior tests belong in the crate's `tests/`
  directory.
- Private API tests may live under `src/tests/` so they can access crate-private
  items without mixing test bodies into production modules.
- Do not add Rust sibling test files such as `foo_test.rs`, `foo_tests.rs`, or
  `foo.spec.rs` next to `foo.rs`. That is idiomatic in other ecosystems, but not
  the layout we want here. If an integration test mirrors one production file,
  it may use that production file's name inside `tests/`.
- Avoid inline `#[cfg(test)] mod tests { ... }` blocks in production source
  files. Move those tests to `src/tests/` when they need private access, or to
  `tests/` when they only need public APIs. A minimal `#[cfg(test)] mod tests;`
  declaration is acceptable only as a bridge to a `src/tests/` module tree.
- Do not put test-only helpers in production modules behind `#[cfg(test)]`.
  Shared test helpers belong in `tests/common.rs`, `tests/util.rs`, or the
  crate's `src/tests/` module tree when private access is required.
- `src/tests/**` is test code, not production source. Core LOC metrics should
  be able to exclude it along with crate-level `tests/` directories.

### `values.schema.json` is output, not inference evidence

helm-schema generates a `values.schema.json`-shaped artifact, but it must not
automatically read an existing chart or dependency `values.schema.json` as
input evidence for inference.

From first principles, the accepted input schema should be recovered from what
the chart actually does:

- Helm templates and helper bodies
- structural control flow over `.Values`
- composed `values.yaml` defaults and user-supplied values files
- comments/descriptions as metadata only
- resource schemas for rendered Kubernetes/CRD sinks

A `values.schema.json` file shipped by a chart dependency is an external
author assertion. It may be stale, incomplete, hand-written for a different
purpose, or generated by another tool. Treating it as analyzer evidence would
silently replace static analysis with trust in another author.

Therefore:

- Do not ingest chart/dependency `values.schema.json` files during inference.
- Do not intersect generated output with shipped `values.schema.json` files.
- Do not use shipped `values.schema.json` to infer types, shapes,
  nullability, requiredness, or guards.
- User-provided override schemas are allowed only as explicit caller policy
  inputs, not as discovered chart facts.

## Running tests

- Use `cargo nextest run --workspace` (debug mode) for the full suite. Do not use `--release`.

## Schema tests

- Schema integration tests must assert full JSON schema equality using diff-based assertions (e.g. `sim_assert_eq!(actual, expected)`; see "Equality assertions in tests" below).
- Do not replace full-schema equality with selective assertions of a few fields.
- Avoid snapshot testing / auto-regeneration; if output changes intentionally, update the full expected schema fixtures explicitly.

## Equality assertions in tests

- Tests must assert equality with `similar_asserts::assert_eq`, **not** the std `assert_eq!`. On failure it prints a readable line-by-line diff instead of dumping two large opaque values — essential for the big JSON schemas this project compares.
- Import the alias from the `test-util` prelude and call it `sim_assert_eq!`:

  ```rust
  use test_util::prelude::sim_assert_eq;
  // ...
  sim_assert_eq!(actual, expected);
  ```

  The alias is defined once in `test-util` (`pub use similar_asserts::assert_eq as sim_assert_eq;`) so every crate refers to the same macro. Add `test-util` as a `dev-dependency` (`test-util.workspace = true`) if the crate does not already have it.
- The import is **per-module**: a `use` in a parent module does not reach child `mod tests { … }` blocks, so each test module (and each integration-test file under `tests/`) needs its own `use test_util::prelude::sim_assert_eq;`.
- This is enforced by clippy: `clippy.toml` disallows the `std::assert_eq` macro via `disallowed-macros`. The lint only bans the std macro — `similar_asserts::assert_eq` / the `sim_assert_eq` alias is unaffected. It currently surfaces as a warning (it still "shouts" so violations are caught), so do not reintroduce bare `assert_eq!` in tests.

## Result types

- Prefer explicit result types and avoid `Result` aliases (do not import `Result` as a local alias), to avoid confusion with `std::result::Result`.
- Inside crates, prefer typed error enums (e.g. `std::result::Result<T, MyError>`) for precise variants.
- Convert typed errors to `color_eyre::eyre::Report` only at the outer boundary (e.g. `main`).

## Cache is a speed optimisation, not a correctness oracle

The K8s schema cache (and the CRD catalog cache) exists solely to make
repeat lookups fast. It is never the source of truth for what API
versions or kinds exist. **Always treat the upstream source as
authoritative; the cache is fetch-on-miss.**

Concretely:

- `has_resource(...)` / `cache_versions_holding(...)` reflect *what is
  currently on disk*, not *what exists upstream*. On a cold cache they
  return `false` / empty even for kinds that absolutely exist in the
  configured K8s version. Treating them as a capability oracle ties
  correctness to whether some previous run happened to warm the cache
  — which is non-deterministic.
- A "capability present" check (e.g. evaluating
  `.Capabilities.APIVersions.Has "policy/v1"`) MUST NOT use cache
  state as its only signal. Either trigger a real fetch (upstream is
  the goto), or rely on a structural fact baked into the binary
  (e.g. a static "K8s 1.X supports api Y" table derived from the
  release notes).
- If a code path could give a different answer on a cold cache vs a
  warm cache, it is wrong. Make the behaviour cache-independent.

**Historical note (round 7 → round 8 → round 10).** An early
attempt at structural guard evaluation probed `has_resource` (which
is local-cache-only, fetch-free by contract) as a capability oracle.
On the first lookup of a cold cache the probe returned `false` for
*every* API version, collapsing the chart to its `else`-branch
fallback even for APIs that were present upstream — exactly the
"cache-as-oracle" antipattern this section warns against. The
short-lived round-7 fix removed capability evaluation entirely and
relied on the chain's "iterate branch literals, return first
success" semantics; round 8 reintroduced an explicit oracle
implemented the right way:

- `KubernetesJsonSchemaProvider::capability_has_at_primary_version`
  consults `probe_at`, which is upstream-first — local cache hit →
  `Found`; cache miss → fetch (if downloads are allowed); offline
  cache miss with no negative-cache record → `Uncertain`.
- The oracle returns a tri-state `Option<bool>`: `Some(true)` /
  `Some(false)` only when there's an authoritative signal (positive
  hit, or a confirmed upstream 404 recorded in the negative cache);
  `None` whenever the cache alone can't tell. The branch selector
  treats `None` as "potentially live" so uncertainty never silently
  drops a branch.
- Round 10 tightened the offline path: previously, an offline run
  with a partial cache (one unrelated file at the primary version)
  let the oracle promote "probe target absent" to `Some(false)`,
  which is exactly the cache-completeness-as-oracle bug this
  section forbids. The fix moved authoritative-vs-uncertain into
  the `ProbeOutcome` enum so the offline path only returns
  `Some(false)` when the negative cache says so, and `None`
  otherwise.

If you change the capability oracle: keep this contract intact.
The regression tests in
`crates/helm-schema-k8s/tests/capability_oracle_offline.rs` pin
the offline-safety cases (partial cache, empty cache, authoritative
404).

## Known architectural debt: capability probe table

The K8s capability oracle
(`KubernetesJsonSchemaProvider::capability_has_at_primary_version`)
needs to answer `.Capabilities.APIVersions.Has "group/version"` — the
*group/version* form without a Kind suffix — by probing whether any
kind exists at that api version. Since the upstream schema source is
per-file (one JSON per kind, fetched on demand) with no bundle
manifest we could enumerate, the probe needs a *canonical kind* to
target. That table lives in
`crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs`
and is manually maintained.

This is a defensible heuristic for the current cache architecture —
each entry is the kind that anchors a given api group/version (e.g.
`PodDisruptionBudget` for `policy/v1`, present from the version's
inception so its existence proves the api version exists) — but it
*is* a manual map, and that's structural debt:

- new K8s api versions (and new api groups) require a table edit;
- removing the table cleanly requires either an upstream bundle that
  ships a `_index.json`-style manifest of all kinds per api version,
  or a startup-time eager pre-fetch of the full primary-version
  bundle so we can answer from on-disk enumeration.

The `group/version/Kind` three-part form does *not* need the table —
it probes the kind directly, which is fully structural. Only the
two-part form has this dependency. Keep that in mind when extending
guard decoding: if new guard shapes need a similar lookup, prefer to
target the kind directly rather than grow the canonical-kind map.

## Parsers over string heuristics

- **Always prefer a proper parser over regex or `starts_with` / `strip_prefix` chains when parsing structured input** — Go template actions, YAML, JSON, schema-like text, etc. We have been deliberately migrating Go-template parsing from brittle regex to the tree-sitter Go-template grammar parser, and apply the same standard to new code.
- For Go-template / Helm action text, use `helm_schema_ast::parse_action_expressions` and pattern-match on `TemplateExpr` (`Call`, `Pipeline`, `Selector`, `Literal`, …). The tree-sitter parser correctly handles quoting, pipelines, nested calls, and trim modifiers that hand-rolled string code routinely gets wrong.
- For Helm template *structure* (define blocks, control flow, document boundaries), use `helm_schema_ast::HelmAst` rather than scanning lines for `{{- if … }}` / `{{- end }}` markers.
- For Helm/YAML template structure, prefer the tree-sitter-backed AST (`helm_schema_ast::TreeSitterParser` and `HelmAst`) over line-oriented heuristics whenever the consumer needs to understand nesting, sequences, comments, multi-document boundaries, or template control flow.
- Line-oriented detectors are acceptable for very narrow, local checks (single token on a single line), but the moment the logic needs to understand template control flow, pipelines, helper resolution, or multi-line composition, switch to the AST. "It works for the cases I tested" is not a sufficient argument — the brittle approach has bitten us repeatedly in the resource-detector / apiVersion-resolution path.
