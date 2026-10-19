# Architecture Decision Records

These records capture the design requirements and decisions helm-schema has made,
so they live next to the code instead of only in `CLAUDE.md`, the `plan/` ledgers
and hand-offs. Each record states one decision in the project's own terms and
cites where it was made (a `CLAUDE.md` section, a `plan/` file or ledger entry,
a commit). `CLAUDE.md` remains the working agreement for contributors; where the
two overlap, an ADR records why and when, and `CLAUDE.md` says what to do today.
The coding conventions in `CLAUDE.md` (test layout, equality assertions, result
types) are summarised in a single ADR rather than one record each.

## Adding an ADR

1. Copy `0000-template.md` to `NNNN-kebab-title.md`, using the next free number.
2. Fill in the title, `Status` (`Proposed`, `Accepted`, or `Superseded by NNNN`)
   and `Date` (when the decision was made; earliest evidence).
3. Keep it to one decision and roughly 60 lines; every claim cites a reference.
4. To revise a decision, write a new ADR and mark the old one
   `Superseded by NNNN`; do not rewrite history in place.
5. Add a row to the index below.

## Index

The initial records were numbered by theme: 0001–0019 analysis principles,
parsing and frontend; 0020–0040 IR and abstract interpreter; 0041–0054
Kubernetes and CRD schema providers; 0055–0072 schema emission and output
policy; 0073–0087 corpus, fixtures, acceptance battery and the Helm oracle;
0088–0100 landing gates, tooling, performance and conventions. New records
take the next free number regardless of theme.

| No. | Title | Status | Date |
| --- | --- | --- | --- |
| [0001](0001-structural-static-analysis-first.md) | Structural static analysis first, heuristics last | Accepted | 2026-05-18 |
| [0002](0002-preserve-ambiguity-prefer-unknown-over-a-wrong-guess.md) | Preserve ambiguity; prefer "unknown" over a wrong guess | Accepted | 2026-05-18 |
| [0003](0003-soundness-never-reject-a-working-configuration.md) | Soundness: never reject a working configuration | Accepted | 2026-06-11 |
| [0004](0004-simplicity-by-deletion.md) | Simplicity by deletion: one semantic model per fact | Accepted | 2026-06-20 |
| [0005](0005-compiler-style-phases.md) | Compiler-style phases, not clever plumbing | Accepted | 2026-06-22 |
| [0006](0006-kiss-rust-and-indoc-for-multiline-strings.md) | Plain Rust (KISS) and `indoc!` for multiline strings | Accepted | 2026-06-21 |
| [0007](0007-values-schema-json-is-output-not-evidence.md) | `values.schema.json` is output, not inference evidence | Accepted | 2026-06-19 |
| [0008](0008-values-comments-are-description-metadata-only.md) | Values-file comments are description metadata only | Accepted | 2026-06-10 |
| [0009](0009-parsers-over-string-heuristics.md) | Parsers over string heuristics; one tree-sitter parser path | Accepted | 2026-05-17 |
| [0010](0010-vendor-and-own-the-go-template-grammar.md) | Vendor and own the go-template grammar | Accepted | 2026-07-24 |
| [0011](0011-grammar-agrees-with-go-text-template.md) | The grammar agrees with Go's text/template | Accepted | 2026-08-31 |
| [0012](0012-one-ast-driven-resource-detector.md) | One AST-driven resource detector | Accepted | 2026-05-18 |
| [0013](0013-typed-apiversion-branches.md) | apiVersion alternatives are typed branches, not candidate lists | Accepted | 2026-05-18 |
| [0014](0014-kind-list-envelopes-descend-into-items.md) | `kind: List` envelopes: descend into `items` | Accepted | 2026-05-18 |
| [0015](0015-unified-templated-yaml-frontend.md) | One templated-YAML frontend CST replaces the line model | Accepted | 2026-07-03 |
| [0016](0016-no-fallback-deletion-before-the-structural-model.md) | Build the structural model before deleting fallbacks | Accepted | 2026-06-21 |
| [0017](0017-keep-separate-helper-runtimes-until-an-event-model-exists.md) | Keep separate helper runtimes until an explicit event model exists | Superseded by 0020 | 2026-06-21 |
| [0018](0018-rendered-layout-frontend-refactor.md) | YAML ownership from a rendered-layout skeleton, not source lines | Accepted | 2026-09-27 |
| [0019](0019-frontend-migrations-land-shadow-first.md) | Frontend migrations land shadow-first; deletions follow the last consumer | Accepted | 2026-09-29 |
| [0020](0020-one-abstract-interpreter-owns-helm-semantics.md) | One abstract interpreter owns Helm semantics | Accepted | 2026-06-09 |
| [0021](0021-evidence-error-polarity-uncertainty-widens.md) | Each evidence kind has a fixed error polarity; uncertainty widens | Accepted | 2026-06-11 |
| [0022](0022-evolve-the-pipeline-in-place-no-rewrite.md) | Evolve the pipeline in place; no clean-room rewrite or phase merge | Accepted | 2026-06-11 |
| [0023](0023-six-phase-pipeline-with-one-typed-artifact-per-phase.md) | Six-phase pipeline with one typed artifact and one owner per fact | Accepted | 2026-08-04 |
| [0024](0024-contract-ir-is-the-semantic-seam.md) | `ContractIr` is the semantic seam between analysis and emission | Accepted | 2026-06-13 |
| [0025](0025-helpers-are-evaluated-by-call-site-reguarded-summaries.md) | Helpers are evaluated by memoized summaries re-guarded at the call site | Accepted | 2026-06-11 |
| [0026](0026-render-conditions-are-preserved-as-a-guard-dnf.md) | Render conditions are preserved as a guard DNF | Accepted | 2026-07-11 |
| [0027](0027-chart-yaml-dependency-activation-is-a-guard.md) | `Chart.yaml` dependency activation is a structural guard | Accepted | 2026-06-13 |
| [0028](0028-analysis-bounds-are-structural-and-only-widen.md) | Analysis bounds are structural and may only widen | Accepted | 2026-08-05 |
| [0029](0029-direct-calls-and-pipeline-stages-share-one-invocation.md) | Direct calls and pipeline stages share one invocation model | Accepted | 2026-08-04 |
| [0030](0030-one-typed-function-semantics-catalog.md) | One typed function-semantics catalog owned by the IR | Accepted | 2026-08-04 |
| [0031](0031-one-typed-selection-reachability-carrier.md) | One typed selection-reachability carrier beside `EvalResult` | Accepted | 2026-08-05 |
| [0032](0032-compiler-enforced-exhaustiveness-for-semantic-carriers.md) | Compiler-enforced exhaustiveness for semantic carriers | Accepted | 2026-08-25 |
| [0033](0033-values-paths-are-a-segmented-type.md) | Values paths are a segmented type, not strings | Accepted | 2026-08-25 |
| [0034](0034-vocabulary-collapses-are-adopted-only-through-measured-spikes.md) | Vocabulary collapses are adopted only through measured E-gated spikes | Accepted | 2026-08-25 |
| [0035](0035-no-salsa-or-generic-query-framework.md) | No Salsa or generic query framework; phase artifacts instead | Accepted | 2026-06-24 |
| [0036](0036-retain-every-semantic-vertical.md) | Retain every semantic vertical; deleting one is an explicit feature decision | Accepted | 2026-07-03 |
| [0037](0037-reject-unquoted-yaml-1-1-boolean-alias-keys.md) | Reject unquoted YAML 1.1 Boolean-alias keys in chart declarations | Accepted | 2026-08-05 |
| [0038](0038-dependency-values-keys-and-installed-identity-are-separate.md) | Dependency values keys and installed-chart identity are validated separately | Accepted | 2026-08-05 |
| [0039](0039-typed-incomplete-analysis-result.md) | Typed incomplete-analysis result (deferred) | Proposed | 2026-09-26 |
| [0040](0040-generation-decisions-are-recorded-where-taken.md) | Generation decisions are recorded where they are taken and exposed by `explain` | Accepted | 2026-09-29 |
| [0041](0041-schema-cache-is-fetch-on-miss-never-an-oracle.md) | The schema cache is fetch-on-miss, never a correctness oracle | Accepted | 2026-05-18 |
| [0042](0042-resolve-capability-branches-by-first-successful-literal.md) | Resolve capability branches by iterating branch literals, first success wins | Superseded by 0043 | 2026-05-18 |
| [0043](0043-tri-state-upstream-first-capability-oracle.md) | A tri-state, upstream-first capability oracle | Accepted | 2026-05-18 |
| [0044](0044-declarative-capability-probe-table-as-bounded-debt.md) | Keep a declarative capability probe table as bounded, validated debt | Accepted | 2026-05-18 |
| [0045](0045-managed-cache-layout.md) | Managed cache layout: per-source namespaces and a version marker | Accepted | 2026-05-18 |
| [0046](0046-provider-chain-precedence-and-authoritative-overrides.md) | One provider chain with fixed precedence; local overrides are authoritative | Accepted | 2026-05-18 |
| [0047](0047-kubernetes-version-chain-with-bounded-fallback.md) | Kubernetes version chain: explicit versions plus a bounded lookup-only fallback | Accepted | 2026-05-18 |
| [0048](0048-crd-versions-are-never-substituted.md) | CRD versions are never substituted | Accepted | 2026-05-18 |
| [0049](0049-opt-in-tiered-apiversion-inference-that-abstains.md) | apiVersion inference is opt-in, tiered, bounded and abstains on ambiguity | Accepted | 2026-05-18 |
| [0050](0050-chart-local-crds-as-a-local-schema-universe.md) | Chart-local CRDs feed a source-agnostic local schema universe | Accepted | 2026-06-11 |
| [0051](0051-provider-schemas-resolved-once-as-a-phase-artifact.md) | Provider schemas are resolved once, into a phase artifact keyed by the complete use | Accepted | 2026-08-25 |
| [0052](0052-provider-uncertainty-is-carried-not-resolved-by-cache-contents.md) | Provider uncertainty is carried, never resolved by cache contents | Proposed | 2026-09-26 |
| [0053](0053-chart-kubeversion-range-as-the-analysis-domain.md) | The chart's admitted kubeVersion range as the analysis domain | Proposed | 2026-09-26 |
| [0054](0054-chart-crd-instances-judged-by-kubernetes-own-validator.md) | Chart CRD instances are judged by Kubernetes' own apiextensions validation | Proposed | 2026-10-19 |
| [0055](0055-emission-policy-classes-and-monotonicity-law.md) | Emission policies are classed, and only widening policies are selectable presets | Accepted | 2026-08-02 |
| [0056](0056-lean-profile-keeps-mandatory-and-local-facts.md) | The lean profile keeps mandatory facts and local conditionals | Accepted | 2026-08-02 |
| [0057](0057-versioned-chart-local-policy-config.md) | Chart-local policy lives in a strict, versioned `helm-schema.yaml` | Accepted | 2026-08-02 |
| [0058](0058-emitted-schemas-self-identify-their-policy.md) | Emitted schemas self-identify their resolved policy | Accepted | 2026-08-02 |
| [0059](0059-schema-must-accept-chart-defaults-and-lint-document.md) | A generated schema must accept the chart's own defaults and the `helm lint` document | Accepted | 2026-07-12 |
| [0060](0060-closed-values-root-by-default.md) | The values root is closed by default; `--open-root` opts out | Accepted | 2026-09-26 |
| [0061](0061-declared-defaults-assert-their-types-by-default.md) | Declared defaults assert their types by default; `--declared-types=annotate` opts out | Accepted | 2026-09-26 |
| [0062](0062-reserve-helm-global-at-every-chart-root.md) | Helm's injected `global` is reserved at every chart values root | Accepted | 2026-07-12 |
| [0063](0063-range-domain-takes-the-renderable-transport.md) | Integer `range` operands follow the renderable transport (the F5 transport union) | Accepted | 2026-09-26 |
| [0064](0064-content-hash-defs-keys.md) | Content-hash `$defs` keys for stable fixtures | Superseded by 0065 | 2026-09-27 |
| [0065](0065-defs-names-are-readable-source-paths.md) | `$defs` entries are named by readable source paths | Accepted | 2026-09-27 |
| [0066](0066-shorten-defs-only-on-explicit-request.md) | `$defs` shortening only on explicit request; never change output to fit Helm | Superseded by 0068 | 2026-09-27 |
| [0067](0067-helm-wrappers-run-helm-on-a-shortened-copy.md) | `helm-schema lint` / `template` run Helm on a shortened chart copy and translate diagnostics | Accepted | 2026-09-28 |
| [0068](0068-size-aware-output-layout.md) | Size-aware output layout; never fail on size | Accepted | 2026-10-19 |
| [0069](0069-minimization-interns-repeated-subtrees.md) | Repeated subtrees are interned into `$defs` by default, as an output-only transform | Accepted | 2026-07-15 |
| [0070](0070-emit-json-schema-draft-07.md) | Emit JSON Schema Draft-07, the dialect Helm validates against | Accepted | 2026-01-03 |
| [0071](0071-runtime-tpl-programs-stay-open.md) | Runtime `tpl` programs stay open; no allowlists or regex template parsers | Accepted | 2026-08-02 |
| [0072](0072-self-contained-output-by-default.md) | Output is self-contained by default; external `$ref`s move into `$defs` | Accepted | 2026-06-11 |
| [0073](0073-real-helm-is-the-correctness-oracle.md) | Real Helm is the correctness oracle; a tightening is a direction, not a verdict | Accepted | 2026-07-17 |
| [0074](0074-pinned-adjudication-environment.md) | Adjudicate in a pinned Helm and Kubernetes environment | Accepted | 2026-08-25 |
| [0075](0075-kubernetes-invalidity-needs-a-found-offline-schema.md) | Kubernetes invalidity is proven by a found offline schema, never by cache absence | Accepted | 2026-09-05 |
| [0076](0076-validate-the-coalesced-values-document.md) | Test instances validate the coalesced values document, composed by a port of Helm | Accepted | 2026-08-02 |
| [0077](0077-schema-tests-assert-full-schema-equality.md) | Schema tests assert full-schema equality against explicit fixtures | Accepted | 2026-02-18 |
| [0078](0078-fixture-change-contracts.md) | Every change declares its fixture contract; refactors are byte-identical | Accepted | 2026-07-12 |
| [0079](0079-one-verified-producer-one-clean-dump.md) | Fixtures come from one clean dump of one verified producer | Accepted | 2026-08-02 |
| [0080](0080-battery-requires-an-explicit-candidate-dump.md) | The acceptance battery always reads an explicit, fresh candidate dump | Accepted | 2026-09-05 |
| [0081](0081-compiled-rust-prober-for-acceptance-probing.md) | Acceptance probing uses a compiled Rust prober at three granularities | Accepted | 2026-08-02 |
| [0082](0082-exact-rosters-against-a-pinned-roster-baseline.md) | Known false and undecided acceptances live on exact rosters tied to a pinned baseline | Accepted | 2026-09-26 |
| [0083](0083-frozen-witness-gate.md) | A frozen-witness gate judges every adopted fixture absolutely | Accepted | 2026-09-25 |
| [0084](0084-vendored-real-world-chart-corpus.md) | A vendored real-world chart corpus, with adjudication state kept on rosters | Accepted | 2026-07-12 |
| [0085](0085-provider-bundle-is-a-committed-test-input.md) | Provider schemas are a committed, offline test input | Accepted | 2026-07-17 |
| [0086](0086-helmsweep-patches-a-pinned-helm-module.md) | The in-process Helm oracle builds from the pinned Helm module plus one patch; Helm is never vendored | Accepted | 2026-09-26 |
| [0087](0087-every-sweep-row-runs-with-a-sampled-cli-differential.md) | Every sweep row runs through the fast engine; the real Helm CLI checks a bounded sample | Accepted | 2026-09-26 |
| [0088](0088-verification-gates-run-on-the-final-tree.md) | Verification gates run individually on the final tree | Accepted | 2026-08-02 |
| [0089](0089-fail-closed-landing-chain-with-sealed-receipt.md) | Landings pass a fail-closed chain that seals a receipt | Accepted | 2026-09-26 |
| [0090](0090-every-fix-lands-with-a-red-then-green-test.md) | Every fix lands with a red-then-green regression test | Accepted | 2026-09-24 |
| [0091](0091-inherited-defects-stay-in-scope-and-blocking.md) | Inherited defects stay in scope and block landings | Accepted | 2026-10-18 |
| [0092](0092-fixes-simplify-the-architecture.md) | Fixes must simplify the architecture, not add special cases | Accepted | 2026-10-18 |
| [0093](0093-deterministic-output-and-the-performance-law.md) | Output is deterministic, ordered and fast by architecture | Accepted | 2026-06-26 |
| [0094](0094-harness-caches-never-replay-nondeterministic-renders.md) | Verification caches never replay nondeterministic renders | Accepted | 2026-09-27 |
| [0095](0095-correctness-may-land-with-a-measured-slowdown.md) | A structural correctness fix may land with a measured slowdown | Accepted | 2026-09-07 |
| [0096](0096-performance-measurement-protocol.md) | Performance claims follow one binding measurement protocol | Accepted | 2026-09-03 |
| [0097](0097-profile-with-tracing-spans-and-perfetto.md) | Profile with tracing spans and Perfetto, not custom timers | Accepted | 2026-06-09 |
| [0098](0098-user-maintained-forks-as-pinned-git-dependencies.md) | Fix upstream defects in user-maintained forks pinned as git dependencies | Accepted | 2026-10-18 |
| [0099](0099-drift-tooling-is-isolated-maintainer-tooling.md) | Helm/Kubernetes drift tooling is isolated maintainer tooling under tools/ | Accepted | 2026-10-19 |
| [0100](0100-test-and-error-handling-conventions.md) | Test and error-handling conventions | Accepted | 2026-02-18 |
