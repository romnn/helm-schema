# Architecture Decision Records

These records capture helm-schema's durable design choices and their rationale.
[AGENTS.md](../../AGENTS.md) and the linked skills hold current contributor
rules; style and routine procedures do not need separate ADRs.

`CLAUDE.md` is a symlink to `AGENTS.md`. Historical section citations must
name a commit and section: removed sections cannot be located in today's
shortened guide. Each historical-guide citation pins `<commit>:AGENTS.md`;
read it with `git show <commit>:AGENTS.md`, since `CLAUDE.md` itself stores
only the symlink. These citations use the full guide before shortening;
earlier decision dates and commits remain in each record's references.

## Adding an ADR

1. Copy [0000-template.md](0000-template.md) to `NNNN-kebab-title.md`, using
   the next free number.
2. State one decision, its context, consequences, status and earliest date.
3. Keep records to roughly 60 lines and support claims with references.
4. For a revised decision, add an ADR and mark its predecessor superseded.
5. Update the index; link related rationale instead of duplicating it.

## Index

The 66 records retain their thematic order after the value audit:
0001–0011 analysis principles, parsing and frontend; 0012–0027 IR and the
abstract interpreter; 0028–0037 Kubernetes/CRD providers; 0038–0050 schema
emission and output; 0051–0061 corpus, fixtures and the Helm oracle;
0062–0066 landing, performance and maintainer tooling. New records take the
next free number regardless of theme. Consolidated rationale and rejected
strategies live in their surviving decisions.

| No. | Title | Status | Date |
| --- | --- | --- | --- |
| [0001](0001-structural-static-analysis-first.md) | Structural static analysis first, heuristics last | Accepted | 2026-05-18 |
| [0002](0002-soundness-never-reject-a-working-configuration.md) | Soundness: never reject a working configuration | Accepted | 2026-06-11 |
| [0003](0003-simplicity-by-deletion.md) | Simplicity by deletion: one semantic model per fact | Accepted | 2026-06-20 |
| [0004](0004-values-schema-json-is-output-not-evidence.md) | `values.schema.json` is output, not inference evidence | Accepted | 2026-06-19 |
| [0005](0005-values-comments-are-description-metadata-only.md) | Values-file comments are description metadata only | Accepted | 2026-06-10 |
| [0006](0006-parsers-over-string-heuristics.md) | Parsers over string heuristics; one tree-sitter parser path | Accepted | 2026-05-17 |
| [0007](0007-vendor-and-own-the-go-template-grammar.md) | Vendor and own the go-template grammar | Accepted | 2026-07-24 |
| [0008](0008-grammar-agrees-with-go-text-template.md) | The grammar agrees with Go's text/template | Accepted | 2026-08-31 |
| [0009](0009-typed-apiversion-branches.md) | apiVersion alternatives are typed branches, not candidate lists | Accepted | 2026-05-18 |
| [0010](0010-unified-templated-yaml-frontend.md) | One templated-YAML frontend CST replaces the line model | Accepted | 2026-07-03 |
| [0011](0011-rendered-layout-frontend-refactor.md) | YAML ownership from a rendered-layout skeleton, not source lines | Accepted | 2026-09-27 |
| [0012](0012-one-abstract-interpreter-owns-helm-semantics.md) | One abstract interpreter owns Helm semantics | Accepted | 2026-06-09 |
| [0013](0013-evidence-error-polarity-uncertainty-widens.md) | Each evidence kind has a fixed error polarity; uncertainty widens | Accepted | 2026-06-11 |
| [0014](0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md) | Six analysis/output phases plus source preparation, one owner per fact | Accepted | 2026-08-04 |
| [0015](0015-contract-ir-is-the-semantic-seam.md) | `ContractIr` is the semantic seam between analysis and emission | Accepted | 2026-06-13 |
| [0016](0016-helpers-are-evaluated-by-call-site-reguarded-summaries.md) | Helpers are evaluated by memoized summaries re-guarded at the call site | Accepted | 2026-06-11 |
| [0017](0017-render-conditions-are-preserved-as-a-guard-dnf.md) | Render conditions are preserved as a guard DNF | Accepted | 2026-07-11 |
| [0018](0018-chart-yaml-dependency-activation-is-a-guard.md) | `Chart.yaml` dependency activation is a structural guard | Accepted | 2026-06-13 |
| [0019](0019-analysis-bounds-are-structural-and-only-widen.md) | Analysis bounds are structural and may only widen | Accepted | 2026-08-05 |
| [0020](0020-direct-calls-and-pipeline-stages-share-one-invocation.md) | Direct calls and pipeline stages share one invocation model | Accepted | 2026-08-04 |
| [0021](0021-one-typed-function-semantics-catalog.md) | One typed function-semantics catalog owned by the IR | Accepted | 2026-08-04 |
| [0022](0022-one-typed-selection-reachability-carrier.md) | One typed selection-reachability carrier beside `EvalResult` | Accepted | 2026-08-05 |
| [0023](0023-values-paths-are-a-segmented-type.md) | Values paths are a segmented type, not strings | Accepted | 2026-08-25 |
| [0024](0024-no-salsa-or-generic-query-framework.md) | No Salsa or generic query framework; phase artifacts instead | Accepted | 2026-06-24 |
| [0025](0025-reject-unquoted-yaml-1-1-boolean-alias-keys.md) | Reject unquoted YAML 1.1 Boolean-alias keys in chart declarations | Accepted | 2026-08-05 |
| [0026](0026-dependency-values-keys-and-installed-identity-are-separate.md) | Dependency values keys and installed-chart identity are validated separately | Accepted | 2026-08-05 |
| [0027](0027-generation-decisions-are-recorded-where-taken.md) | Generation decisions are recorded where they are taken and exposed by `explain` | Accepted | 2026-09-29 |
| [0028](0028-schema-cache-is-fetch-on-miss-never-an-oracle.md) | The schema cache is fetch-on-miss, never a correctness oracle | Accepted | 2026-05-18 |
| [0029](0029-tri-state-upstream-first-capability-oracle.md) | A tri-state, upstream-first capability oracle | Accepted | 2026-05-18 |
| [0030](0030-managed-cache-layout.md) | Managed cache layout: per-source namespaces and a version marker | Accepted | 2026-05-18 |
| [0031](0031-provider-chain-precedence-and-authoritative-overrides.md) | One provider chain with fixed precedence; local overrides are authoritative | Accepted | 2026-05-18 |
| [0032](0032-kubernetes-version-chain-with-bounded-fallback.md) | Kubernetes version chain: explicit versions plus a bounded lookup-only fallback | Accepted | 2026-05-18 |
| [0033](0033-crd-versions-are-never-substituted.md) | CRD versions are never substituted | Accepted | 2026-05-18 |
| [0034](0034-opt-in-tiered-apiversion-inference-that-abstains.md) | apiVersion inference is opt-in, tiered, bounded and abstains on ambiguity | Accepted | 2026-05-18 |
| [0035](0035-chart-local-crds-as-a-local-schema-universe.md) | Chart-local CRDs feed a source-agnostic local schema universe | Accepted | 2026-06-11 |
| [0036](0036-provider-schemas-resolved-once-as-a-phase-artifact.md) | Provider schemas are resolved once, into a phase artifact keyed by the complete use | Accepted | 2026-08-25 |
| [0037](0037-chart-crd-instances-judged-by-kubernetes-own-validator.md) | Chart CRD instances are judged by Kubernetes' own apiextensions validation | Proposed | 2026-10-19 |
| [0038](0038-emission-policy-classes-and-monotonicity-law.md) | Emission policies are classed, and only widening policies are selectable presets | Accepted | 2026-08-02 |
| [0039](0039-versioned-chart-local-policy-config.md) | Chart-local policy lives in a strict, versioned `helm-schema.yaml` | Accepted | 2026-08-02 |
| [0040](0040-emitted-schemas-self-identify-their-policy.md) | Emitted schemas self-identify their resolved policy | Accepted | 2026-08-02 |
| [0041](0041-schema-must-accept-chart-defaults-and-lint-document.md) | A generated schema must accept the chart's own defaults and the `helm lint` document | Accepted | 2026-07-12 |
| [0042](0042-closed-values-root-by-default.md) | The values root is closed by default; `--open-root` opts out | Accepted | 2026-09-26 |
| [0043](0043-declared-defaults-assert-their-types-by-default.md) | Declared defaults assert their types by default; `--declared-types=annotate` opts out | Accepted | 2026-09-26 |
| [0044](0044-range-domain-takes-the-renderable-transport.md) | Integer `range` operands follow the renderable transport (the F5 transport union) | Accepted | 2026-09-26 |
| [0045](0045-defs-names-are-readable-source-paths.md) | `$defs` entries are named by readable source paths | Accepted | 2026-09-27 |
| [0046](0046-helm-wrappers-run-helm-on-a-shortened-copy.md) | `helm-schema lint` / `template` run Helm on a shortened chart copy and translate diagnostics | Accepted | 2026-09-28 |
| [0047](0047-size-aware-output-layout.md) | Size-aware output layout; never fail on size | Accepted | 2026-10-19 |
| [0048](0048-emit-json-schema-draft-07.md) | Emit JSON Schema Draft-07, the dialect Helm validates against | Accepted | 2026-01-03 |
| [0049](0049-runtime-tpl-programs-stay-open.md) | Runtime `tpl` programs stay open; no allowlists or regex template parsers | Accepted | 2026-08-02 |
| [0050](0050-self-contained-output-by-default.md) | Output is self-contained by default; external `$ref`s move into `$defs` | Accepted | 2026-06-11 |
| [0051](0051-real-helm-is-the-correctness-oracle.md) | Real Helm is the correctness oracle; a tightening is a direction, not a verdict | Accepted | 2026-07-17 |
| [0052](0052-pinned-adjudication-environment.md) | Adjudicate in a pinned Helm and Kubernetes environment | Accepted | 2026-08-25 |
| [0053](0053-kubernetes-invalidity-needs-a-found-offline-schema.md) | Kubernetes invalidity is proven by a found offline schema, never by cache absence | Accepted | 2026-09-05 |
| [0054](0054-validate-the-coalesced-values-document.md) | Test instances validate the coalesced values document obtained from Helm | Accepted | 2026-08-02 |
| [0055](0055-fixture-change-contracts.md) | Every change declares its fixture contract; refactors are byte-identical | Accepted | 2026-07-12 |
| [0056](0056-one-verified-producer-one-clean-dump.md) | Fixtures come from one clean dump of one verified producer | Accepted | 2026-08-02 |
| [0057](0057-exact-rosters-against-a-pinned-roster-baseline.md) | Known false and undecided acceptances live on exact rosters tied to a pinned baseline | Accepted | 2026-09-26 |
| [0058](0058-frozen-witness-gate.md) | A frozen-witness gate judges every adopted fixture absolutely | Accepted | 2026-09-25 |
| [0059](0059-vendored-real-world-chart-corpus.md) | A vendored real-world chart corpus, with adjudication state kept on rosters | Accepted | 2026-07-12 |
| [0060](0060-provider-bundle-is-a-committed-test-input.md) | Provider schemas are a committed, offline test input | Accepted | 2026-07-17 |
| [0061](0061-helmsweep-patches-a-pinned-helm-module.md) | The in-process Helm oracle builds from the pinned Helm module plus one patch; Helm is never vendored | Accepted | 2026-09-26 |
| [0062](0062-fail-closed-landing-chain-with-sealed-receipt.md) | Landings pass a fail-closed chain that seals a receipt | Accepted | 2026-09-26 |
| [0063](0063-deterministic-output-and-the-performance-law.md) | Output is deterministic, ordered and fast by architecture | Accepted | 2026-06-26 |
| [0064](0064-performance-measurement-protocol.md) | Performance claims follow one binding measurement protocol | Accepted | 2026-09-03 |
| [0065](0065-drift-tooling-is-isolated-maintainer-tooling.md) | Helm/Kubernetes drift tooling is isolated maintainer tooling under tools/ | Accepted | 2026-10-19 |
| [0066](0066-landing-ratchet.md) | Landings ratchet: no new failure, strictly fewer known failures | Accepted | 2026-10-20 |
