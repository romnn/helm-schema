# Audit: original from-scratch architecture (`a3a5209`) vs current implementation

This compares the original `plan/from-scratch-architecture.md` as it existed in commit
`a3a5209dd1487f1456fe04a6e79f0b439fe6c8b6` with the current codebase.

The short version:

- The current implementation has made real structural progress toward the plan. The strongest areas are chart discovery/modeling, chart-local schema sources, typed capability lookup, reusable library extraction, and self-contained bundled output.
- The biggest remaining architecture gaps are still the public seam and API shape, the contract representation, guarded schema lowering, end-to-end provenance/spans, and the security/budget model.
- There are also a few places where the code has gone beyond the original plan in useful ways, especially source-aware provider bundling and some resource-identity edge cases.

## 1. What has been implemented faithfully

### 1.1 Chart discovery and chart program loading are much closer to the target model

The original architecture wanted chart loading to understand real Helm packaging, not just `templates/` plus a root `values.yaml`.

Current code does this well:

- `crates/helm-schema/src/chart/discovery.rs` discovers nested charts, aliasing, `Chart.template.yaml`, and vendored `.tgz` / `.tar.gz` subcharts.
- `crates/helm-schema/src/chart/file_roles.rs` classifies files structurally into manifest templates, helper/index sources, static CRDs, and `.Files.Get` sources.
- `crates/helm-schema/src/chart/values.rs` composes subchart values, hoists and mirrors `global`, and scopes values descriptions.
- `crates/helm-schema/src/chart/tests.rs` pins `condition:` / `tags:` scoping from `Chart.yaml`.

This is a substantial improvement over the pre-plan implementation that had much of this logic trapped in the CLI.

### 1.2 A real chart-local schema universe now exists

The original plan wanted chart-local schemas to become a first-class source instead of an ad hoc side path.

That has happened:

- `crates/helm-schema-k8s/src/local_schema_universe/universe.rs` defines `LocalSchemaUniverse` and `LocalResourceSchema`.
- `crates/helm-schema/src/chart/static_crds.rs` loads static `crds/` content into that universe.
- `crates/helm-schema/src/analysis/local_crd_projection.rs` also projects template-rendered CRDs when the CRD shape is structurally literal enough.
- `crates/helm-schema/src/analysis/collection.rs` threads the universe into provider construction.
- `crates/helm-schema-k8s/src/local_schema_universe/provider.rs` and `crates/helm-schema/src/provider_builder.rs` treat chart-local schemas as a proper provider in the lookup stack.

This is one of the clearest cases where the current implementation matches the spirit of the plan.

### 1.3 Resource identity is now structurally much stronger

The original plan explicitly wanted structural detection of resource identity, including tricky cases like:

- `kind` before `apiVersion`
- helper-resolved `apiVersion`
- capability-guarded `apiVersion` branches
- `kind: List` envelopes whose `items` contain the real resources

Current code delivers those:

- `crates/helm-schema-ir/src/resource_identity/tests/detector.rs` covers `kind` before `apiVersion`, helper-returned `apiVersion`, and preserved capability branches.
- `crates/helm-schema-ir/src/resource_identity/tests/locator.rs` covers transparent `kind: List` envelopes, including ranged list items.
- `crates/helm-schema-ir/src/resource_identity/api_version.rs` preserves guarded `apiVersion` alternatives rather than collapsing them.

This is very much in line with the "static analyzer first" design goal from `AGENTS.md`.

### 1.4 Typed capability presence lookup and offline-safe tri-state behavior are in place

The architecture put a lot of weight on a typed capability oracle that is upstream-first and does not treat cache state as truth.

Current code matches that well:

- `crates/helm-schema-core/src/capability.rs` defines typed `ApiPresenceQuery`.
- `crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs` implements the canonical-kind probe table for group/version queries.
- `crates/helm-schema-k8s/src/kubernetes_openapi/provider.rs` implements `capability_has_query_at_primary_version` with a real tri-state `Option<bool>` contract.
- `crates/helm-schema-k8s/tests/capability_oracle_offline.rs` pins the cold-cache / partial-cache / negative-cache cases the architecture doc called out.

This is one of the best-aligned parts of the current implementation.

### 1.5 Lookup planning/execution is partially refactored the way the plan wanted

The architecture wanted provider lookup to move toward planner/executor logic with explicit traces.

Current code has clearly moved in that direction:

- `crates/helm-schema-k8s/src/lookup/resource_lookup_plan.rs`
- `crates/helm-schema-k8s/src/lookup/resource_lookup_executor.rs`
- `crates/helm-schema-k8s/src/lookup/api_presence_executor.rs`
- `crates/helm-schema-k8s/src/lookup/orchestrator.rs`
- `crates/helm-schema-k8s/src/lookup/trace.rs`

This is not yet the final "sources as pure data" model from the plan, but it is no longer a monolithic provider chain with everything interleaved.

### 1.6 Bundled self-contained output is now the default

The architecture wanted bundled draft-07 output with internal `$defs` to become the default, with full flattening demoted to an explicit export mode.

Current code does that:

- `crates/helm-schema/src/output_pipeline/options.rs` defines `ReferenceMode::SelfContained` as the default.
- `crates/helm-schema/src/output_pipeline/transforms.rs` bundles by default and fully inlines only for `FullyInlinedExport`.
- `crates/helm-schema/src/flatten.rs` implements both bundling and full inlining.
- `crates/helm-schema-cli/src/lib.rs` uses `load_policy_inputs(...)` before the pure output transforms.

This is one of the most visible user-facing pieces of the original plan that is now shipped.

### 1.7 A lot of CLI-only logic has already been moved into library code

The original plan criticized the old architecture for trapping important logic in the CLI crate.

Current code has already corrected much of that:

- Chart discovery and archive extraction now live in `crates/helm-schema/src/chart/*`.
- Provider construction lives in `crates/helm-schema/src/provider_builder.rs`.
- Output reference preparation and final transforms live in `crates/helm-schema/src/output_pipeline/*`.
- The CLI in `crates/helm-schema-cli/src/lib.rs` is now a relatively thin shell around library calls.

This is not "overdelivery"; it is faithful implementation of a real design goal.

## 2. Where the current implementation is still not faithful to the original plan

### 2.1 The canonical product API is still not the planned `AnalysisSession`

This is the single clearest architecture miss.

The plan wanted:

- an `AnalysisSession`
- lazy memoized queries like `contract()`, `local_schema_universe()`, `resolved_contract(policy)`, `emit(mode)`, `explain(path)`
- one canonical product API object over pure stage functions

Current code still exposes a staged function pipeline instead:

- `crates/helm-schema/src/generation/pipeline.rs` does `discover -> analyze -> build provider -> generate -> required post-pass`
- `crates/helm-schema-cli/src/lib.rs` separately does `load_policy_inputs -> apply_schema_output_pipeline -> write`
- `crates/helm-schema/src/lib.rs` exports function-style entry points, not a session object

I found no current implementation of `AnalysisSession`, `Analysis { contract, local_schemas }`, `resolved_contract(...)`, or `explain(...)` outside the plan documents.

### 2.2 The public seam is still not the final planned contract artifact

The plan wanted the stable seam to be a guarded `ContractIR`, later normalized all the way to `Vec<Guarded<Witness>>`.

Current code only gets part of the way there:

- `crates/helm-schema-ir/src/contract/graph.rs` defines `ContractIr`
- but it is still just `uses: Vec<ContractUse>`
- `crates/helm-schema-ir/src/contract/use_claim.rs` still describes `ContractUse` as a "migration-era claim shape"
- `crates/helm-schema/src/analysis/collection.rs` immediately converts `ContractIr` into `ContractSchemaSignals`
- `crates/helm-schema-gen/src/lib.rs` consumes `ContractSchemaSignals`, not a resolved/normalized contract object

The result is that the product seam is still effectively:

- symbolic IR -> signal bundle -> generator

rather than:

- engine-private interpretation -> public contract artifact -> resolve/lower

### 2.3 The witness algebra from the plan is not implemented

The plan's final semantic normalization was:

- one witness algebra
- `Vec<Guarded<Witness>>`
- path and scope views derived from that
- explicit abort subjects

Current code does not have that model:

- there is no `Witness` type
- there is no `Guarded<Witness>` public artifact
- there is no `ScopeConstraint`
- there is no abort/`fail` witness channel

Instead, the code still relies on:

- `ContractUse`
- `ContractSchemaSignals`
- `RequiredInferenceSignals`
- path-level fact bundles like `ContractValuePathFacts`

That is useful migration scaffolding, but it is not the architecture the original document committed to.

### 2.4 Guarded JSON Schema lowering is substantially improved, but not the final precision ladder

The plan's major semantic promise was not merely "track guards", but:

- keep a richer predicate algebra
- classify values-decidable predicates
- lower those to draft-07 `if` / `then` / `dependencies` when sound
- widen only for `Env` or opaque predicates

Current code now implements an important slice of that:

- `crates/helm-schema-ir/src/predicate.rs` keeps an internal predicate algebra with `Not`, `And`, and `Or`.
- `crates/helm-schema-ir/src/contract_signal_builder/builder.rs` projects lowerable guard sets into `ConditionalPathOverlay` records.
- `crates/helm-schema-gen/src/lib.rs` lowers those overlays into draft-07 `if` / `then` conditionals, including default-aware truthiness when `values.yaml` makes an omitted guard path active.
- `crates/helm-schema/src/analysis/manifest_contract.rs` now applies `Chart.yaml` dependency activation as real guard branches instead of just boolean type hints.

That is a real implementation of values-decidable guarded lowering, and it is materially more faithful to the original plan than the previous state.

The remaining gap is that this is still a compatibility projection rather than the plan's final witness model:

- but `crates/helm-schema-ir/src/contract/use_claim.rs` still stores compatibility `Vec<Guard>`
- `crates/helm-schema-ir/src/contract_signals.rs` still carries both path-local `GuardConstraint` facts and separate conditional overlay DTOs
- opaque/environment predicates still do not have the planned widen/abstain classification boundary
- the lowering does not yet cover every predicate shape the internal algebra can represent

So the project has implemented the central `if` / `then` mechanism, but not the original plan's complete guarded-typing precision ladder.

### 2.5 The engine is not consolidated; `helm-schema-engine` is still a facade, not the engine

The original architecture wanted parse, interpretation, projection, resolution, and lowering to co-evolve inside one engine crate.

That is not the current crate shape:

- `crates/helm-schema-ast`
- `crates/helm-schema-ir`
- `crates/helm-schema-gen`
- `crates/helm-schema-engine`

And `crates/helm-schema-engine/src/lib.rs` is mostly a re-export facade over the other crates, not the place where the semantic core actually lives.

Likewise, the actual top-level product orchestration is still split between:

- `helm-schema`
- `helm-schema-k8s`
- `helm-schema-cli`

This matters because the original architecture explicitly treated the previous public seam and arity ladder as evidence that the crates were split in the wrong place.

### 2.6 Parsing is better, but still not "typed and spanned everywhere"

The parser situation is improved, but it is still well short of the planned end state.

What is better:

- `crates/helm-schema-ast/src/expr.rs` gives typed `TemplateExpr`
- `crates/helm-schema-ast/src/tree_sitter_parser.rs` gives a fused tree-sitter-backed `HelmAst`

What is still missing:

- `crates/helm-schema-ast/src/lib.rs` still stores control-flow headers as raw strings (`If { cond: String }`, `Range { header: String }`, `With { header: String }`)
- `HelmAst` nodes still do not carry source spans
- diagnostics in `crates/helm-schema-k8s/src/diagnostic/diagnostic.rs` are still not span-based
- document attribution still depends on the stateful `DocumentTracker` shape machinery in `crates/helm-schema-ir/src/document_projection/tracker/*`

So the project has replaced some string parsing with AST parsing, but it has not reached the "typed and spanned everywhere" bar.

### 2.7 The plan's provenance model is still missing end-to-end

The original architecture wanted every fact to be traceable to a span and helper chain.

Current code has fragments of provenance:

- provider fragments can carry source filename/pointer (`crates/helm-schema-core/src/provider_schema_fragment.rs`)
- local CRD sources preserve filename/source ids
- resource identity uses internal spans for attribution

But the main contract data does not carry the planned provenance model:

- `ContractUse` has `source_expr`, path, kind, guards, and resource
- there is no span payload on `ContractUse`
- there is no helper-chain provenance on the public contract artifact

This is a major remaining gap between "better implementation" and the actual architecture in the plan.

### 2.8 Superseded: shipped `values.schema.json` is not analyzer evidence

This audit originally treated shipped `values.schema.json` enforcement as a
missing architecture item. That premise has been rejected.

From first principles, helm-schema's inference engine should recover accepted
values from the chart's render program: templates, helpers, `.Values` control
flow, composed values defaults/descriptions, and rendered Kubernetes/CRD sink
schemas. A sibling or dependency `values.schema.json` is an external author
assertion. It can be useful to Helm or to humans, but it is not structural
evidence from the templates.

Current architecture direction:

- do not automatically read chart/dependency `values.schema.json` files
- do not intersect generated output with shipped schema files
- do not infer type, shape, nullability, requiredness, or guards from them
- keep explicit user override schemas as policy inputs outside inference

### 2.9 `Chart.yaml` `condition:` / `tags:` now drive chart-level guards, but through compatibility claims

The original plan wanted `Chart.yaml` dependency activation to provide real declarative guard evidence.

Current code now uses that information semantically:

- `crates/helm-schema/src/chart/discovery.rs` extracts dependency activation paths correctly
- `crates/helm-schema/src/analysis/manifest_contract.rs` applies dependency activation as branch guard sets on subchart contract uses
- condition order follows Helm precedence: the first present condition decides, tags are considered only after all conditions are absent, and a fully absent activation set preserves Helm's default-active dependency behavior

The remaining gap is architectural rather than basic semantics: activation is still projected as compatibility guards on duplicated `ContractUse` rows instead of becoming first-class guarded witness data.

### 2.10 Requiredness is still explicitly heuristic and outside the core pipeline

The original plan treated requiredness as the most dangerous narrowing move and wanted it handled with much more care.

Current code still keeps required inference as a removable heuristic:

- `crates/helm-schema-gen/src/required_inference.rs` says so directly in its module docs
- `crates/helm-schema/src/required_inference.rs` keeps the CLI orchestration isolated so the feature can be deleted cleanly
- `crates/helm-schema-gen/src/lib.rs` keeps core generation free of required inference

This is a sensible current implementation choice, but it means the project has not reached the plan's intended contract-level handling of aborts, positivity, and scope facts.

### 2.11 The output policy input model is only partially realized

One part of the plan is implemented well:

- override loading and override `$ref` preparation moved ahead of the pure output transform path via `PolicyInputs`

But the broader planned picture is not there yet:

- `PolicyInputs` in `crates/helm-schema/src/output_pipeline/overrides.rs` currently holds prepared override schemas only
- there is no session-level assembled policy/input object spanning the whole analysis run

So this is a partial implementation of the idea, not the final architecture.

### 2.12 Security hardening is only partially implemented

The original design wanted explicit `FetchPolicy`, `LoadBudget`, and a validated `RelPath` boundary.

Current code improves some things, but not to that level:

- output ref handling is at least gated by `allow_net` in `crates/helm-schema/src/flatten.rs`
- capability lookup is upstream-first but cache-safe

Still missing relative to the plan:

- no `FetchPolicy` allowlist / host policy object
- no `LoadBudget`
- no validated `RelPath` newtype
- archive extraction in `crates/helm-schema/src/chart/discovery.rs` reads the full archive into memory and unpacks without the planned explicit budgets
- output ref retrieval still accepts arbitrary `file://` paths and arbitrary `http(s)://` hosts whenever `allow_net` is true

So the architecture's security model is not yet in place.

### 2.13 The project still relies heavily on `serde_yaml`, contrary to the planned parser direction

The architecture explicitly called out the need for a maintained, span-preserving YAML parser instead of `serde_yaml`.

Current code still uses `serde_yaml` in many core paths:

- values composition in `crates/helm-schema/src/chart/values.rs`
- top-level values seeding in `crates/helm-schema/src/values_roots.rs`
- template CRD literal extraction in `crates/helm-schema/src/analysis/local_crd_projection.rs`
- generator values-file parsing in `crates/helm-schema-gen/src/lib.rs`

This does not mean the current code is wrong, but it does mean the original architecture's parser policy has not yet been carried through.

### 2.14 Testing is strong in some areas, but the plan's full validation harness is not here yet

What exists and is good:

- many full-schema fixture equality tests using `similar_asserts::assert_eq!`
- targeted `helm template`-based tests in `crates/helm-schema-gen/tests/*`
- chart-level validation helpers in `crates/helm-schema-cli/tests/common/mod.rs`
- strong capability-oracle and lookup tests in `crates/helm-schema-k8s/tests/*`

What I did not find as a unified architecture feature:

- a full differential harness that systematically explores guard-flipping samples across the corpus
- a general `helm lint` acceptance gate
- a lockfile-backed reproducibility story
- the 100-run contract DTO determinism gate described in the plan

So the tests are already valuable, but the exact architecture-level acceptance model is still incomplete.

### 2.15 The serde boundary and stability-ring story are not implemented as designed

The original plan wanted:

- internal graphs not to derive `Serialize`
- DTO projections for dumps/fixtures
- an explicit versioned contract DTO
- a narrow semver-stable facade ring

Current code is looser than that:

- `crates/helm-schema-ir/src/contract/use_claim.rs` derives `Serialize` / `Deserialize` on `ContractUse`
- `crates/helm-schema-ir/src/contract_types.rs` derives `Serialize` / `Deserialize` on `Guard`
- `crates/helm-schema-ir/src/contract/projection.rs` is a transparent serialized wrapper, not a versioned envelope

This is not catastrophic, but it is not the original architecture's intended stability discipline.

## 3. Where the code has overdelivered or improved on the original plan

These are not just "done"; they are places where the current implementation is stronger or more refined than the original document strictly required.

### 3.1 Source-aware provider bundling is more sophisticated than the original plan spelled out

The original architecture wanted bundled output and a two-tier schema model, but the current code goes further in a useful way:

- `crates/helm-schema-core/src/provider_schema_fragment.rs` carries provider source metadata plus optional source and definition schemas
- `crates/helm-schema-k8s/src/lookup/source_bundle.rs` can bundle provider-document-local refs into self-contained source fragments
- `crates/helm-schema-gen/src/provider_definitions.rs` reuses repeated provider-owned leaves, prefers stable source-derived definition names, and rewrites internal refs so the bundled definitions remain valid at their new root location

This is a genuinely better implementation story than a naive "bundle everything under `$defs`" approach.

### 3.2 `kind: List` handling is stronger than the plan's examples

The architecture explicitly wanted `kind: List` wrappers handled structurally.

The current implementation does that, but also handles:

- multi-document separation
- transparent list envelopes
- path rebasing from `items[*]`
- ranged list envelopes in templates

Those behaviors are pinned in `crates/helm-schema-ir/src/resource_identity/tests/locator.rs` and are better than a minimal list-wrapper implementation.

### 3.3 Template-rendered CRD extraction already preserves useful source identity

The original plan wanted template-rendered CRD extraction as a pipeline edge.

The current implementation not only does that for literal-enough CRDs, it also preserves source metadata:

- `chart-template-crd` as the source id
- the originating filename
- per-resource schema entries in `LocalSchemaUniverse`

That source tracking makes the chart-local schema path more operationally useful than a plain anonymous extracted blob.

### 3.4 Lookup tracing is richer than the original high-level sketch

The architecture wanted `LookupTrace`, but the current implementation adds useful detail:

- typed trace subjects for resource-path lookups and API-presence queries
- source-probe-level outcomes for capability checks
- final miss diagnostics projected from traces rather than emitted inline

This is visible across:

- `crates/helm-schema-k8s/src/lookup/trace.rs`
- `crates/helm-schema-k8s/src/lookup/miss_diagnostics.rs`
- `crates/helm-schema-k8s/tests/lookup_chain.rs`
- `crates/helm-schema-k8s/tests/capability_oracle_offline.rs`

That is a good refinement of the original idea.

## 4. The most important remaining work, in priority order

If the goal is to become faithful to the original architecture rather than just continue incremental feature work, the highest-value next steps are:

1. Introduce the real facade/session seam.
   - Add a public `AnalysisSession` or equivalent `Analysis { contract, local_schemas }` surface and make queries the canonical API.

2. Finish the contract representation migration.
   - Replace migration-era `ContractUse` / signal bundles with the planned public contract artifact, including provenance and object-scope facts.

3. Finish the guarded-lowering migration.
   - A strong `if` / `then` overlay path now exists. The next step is to remove compatibility projections, make the predicate/witness artifact the direct lowering input, and define the planned opaque-predicate widen/abstain boundary.

4. Harden and generalize `Chart.yaml` dependency activation.
   - Conditions and tags now affect liveness. The remaining work is broader corpus validation and ensuring the activation branch model feeds the final contract artifact rather than compatibility guards.

5. Finish the parsing/provenance story.
   - Typed control-flow headers, source spans, and less heuristic document tracking are still missing.

6. Implement the planned security/budget model.
   - `FetchPolicy`, `LoadBudget`, and the path-validation boundary are still absent.

7. Add the architecture-level validation harness.
   - Especially guard-flipping differentials and `helm lint` coverage.

## 5. Bottom line

The project has already implemented many of the hard, structurally meaningful parts of the original plan:

- chart modeling
- typed capability analysis
- chart-local schema sources
- resource identity preservation
- reusable library extraction
- bundled self-contained output

But the architectural center of gravity is still not where the original document wanted it:

- there is no canonical session API
- the semantic seam is still not the final contract artifact
- guard lowering still depends on compatibility projections instead of the final predicate/witness artifact
- provenance, security policy, and validation architecture are still incomplete

So the current state is best described as:

- strong progress toward the plan's semantics and chart model
- partial progress on the seam and API design
- incomplete progress on the final contract/lowering/security architecture
