# Architecture Review v3 Campaign Progress

Frozen reference: `plan/architecture-review-v3.md` at `44aa758`.

Campaign baseline: 61,262 production Rust LOC (`task tokei:core` at
`44aa758`).

## Decision register

Roman has resolved the decision points as follows:

- **D1 = option 1**: reject chart declarations containing unquoted YAML-1.1
  boolean-alias keys with one aggregated diagnostic (implemented in Step
  11a, Wave 2 — not now).
- **D2 = option 3**: measure Helm's installed-entry-to-alias association
  first, then model the measured identity (Step 11b, Wave 2 — not now).
- **D3 = option 1**: one typed selection-reachability carrier owned beside
  `EvalResult` — this unblocks Step 6a in this wave.
- **D4 = option 1**: typed keyword subset plus lossless ordered
  `extra_keywords` (Step 9, Wave 2 — not now).
- **D5**: retain every vertical; produce the Step 1 feature-cost table so
  Roman can decide per row. No pruning in this campaign wave.

The frozen plan's independent-review cadence for behavior-bearing steps is
batched at the Wave 1 boundary by Roman's authority. Every per-step fixture,
acceptance, semantic-control, gate, and evidence contract remains in force.

## Step 1 — dead surfaces, shared utilities, and feature costs

- Status: landed.
- Contract: representation-only; every IR, schema, diagnostic, final-output,
  and acceptance artifact must remain byte-identical.
- Acceptance baseline: `44aa758`.
- Baseline production Rust LOC: 61,262.
- Measured results:
  - Production Rust is 61,212 LOC, a 50-LOC reduction from the 61,262
    baseline.
  - Three unused public guard-algebra entry points and three constructors
    used only by test support were deleted. The remaining normalization
    implementation is crate-private and still has production callers.
  - Regex literal escaping has one implementation in `helm-schema-core`;
    IR forwards its compatibility export and gen now depends on IR only for
    tests. `.Files.Get` recognition has one parsed-function-name classifier
    in IR.
  - The clean final-build schema dump contains all 84 artifacts and the clean
    IR dump contains all 18 artifacts; every fixture equality test passes.
    The compiled full-depth comparison checks 60 lanes and 121,059 probes
    with zero flips. Coverage accounting reports zero undisclosed base or
    third-level truncation.
- Deviations:
  - The frozen estimate was -280..-180 production LOC, but the post-Round-74
    production-use audit found only 50 safely deletable lines. The functions
    at `crates/helm-schema-core/src/guard_dnf.rs:57-76` and the callers at
    `crates/helm-schema-ir/src/fragment_eval/project.rs:440-518` show that the
    remaining DNF constructors are live semantic APIs; the fail/guard
    predicates at `crates/helm-schema-gen/src/overlay_lowering.rs:1315-1471`
    have different domains. They remain rather than forcing the estimate by
    deleting live or semantically distinct code.
  - D5 asks for attributable production LOC in shared semantic code. The
    table therefore reports audited bands, not false point precision: its
    lower edge is the directly owned modules/functions and its upper edge
    includes co-owned carriers and lowering arms. Test LOC is excluded by the
    same rule as `task tokei:core`.
- Adjudication evidence:
  - This is representation-only. The full-depth comparison reports
    `charts_checked=60 probes_checked=121059 flips=0`, so there is no Helm
    acceptance flip to adopt. Hermetic monotonicity, three-category semantic
    controls, composite guard probes, and the Temporal pairwise matrix pass.

### D5 feature-cost table

Every row is retained. The fixture-teeth column is a lower-bound inventory of
committed behavior fixtures whose rejection depends on the vertical; it does
not count re-encodings. The luup2 column records current exposure; the
final-wave 32-chart gate validates the retained set, while an optional Step 12
deletion would still need an isolated counterfactual dump and Helm
adjudication.

| Vertical | Attributable production LOC | Unique committed teeth | luup2 exposure | Direction class |
|---|---:|---|---|---|
| Kind partitioning | 330-430 | Airflow scheduler strategy provider partition; dynamic-kind ranged-provider matrix | workload charts with values-selected Pod strategy, especially `signoz` and `temporal` | tooth-adding |
| Terminal clauses | 850-1,050 | unconditional-fail chart; cross-path and ranged fail controls; Airflow/Datadog validator teeth | validation-heavy `signoz`, `temporal`, and operator charts | tooth-adding |
| String-consumer contracts plus selection scope | 2,600-3,100 | Promtail range-key consumers; printf/default and encoder matrices; strict `tpl`/hash/quote controls | `minio`, `nats`, `oauth2proxy`, `signoz`, `temporal` | false-rejection-preventing; inseparable from its tooth-adding consumers |
| Scalar spelling and provider preimage models | 1,250-1,550 | radix integer preimages, quote-falsy spellings, named-port and serialized-scalar controls | `nats`, `signoz`, `minio`, and charts with provider scalar unions | false-rejection-preventing |
| Range-domain and dependency-global projection | 1,100-1,450 | Promtail map-key/range teeth, integer-range controls, cert-manager-wrapper global refill | `signoz`, `nats`, `temporal`, `cert-manager` | false-rejection-preventing; range fail teeth are co-owned with terminals |
| Capability oracle and probe table | 850-1,050 | capability-selected API-version branches and cold/partial-cache uncertainty controls | Kubernetes-resource charts broadly; operator and cert-manager charts exercise the highest density | false-rejection-preventing |
| Local CRD projection and CRD catalog | 700-900 | pruning/non-pruning Widget controls and CRD-backed operator field rejection | `postgres-operator`, `spicedb-operator`, `cert-manager` | tooth-adding |

### Review dossier

- Frozen reference and baseline: `test "$(git rev-parse 44aa758)" =
  "44aa758b41c01859429cc1867fdaa6fb98b6739f" && git diff --exit-code
  44aa758 -- plan/architecture-review-v3.md`.
- Dead-surface proof: `! rg -n
  'pub fn (key_is_strict_subset|minimize_key_disjunction|resolve_complementary_keys)|from_contract_predicate'
  crates --glob '*.rs'`; none of the removed public names remains.
- Single regex implementation and dependency boundary: `rg -n
  'fn escape_regex_literal|helm_schema_ir::escape_regex_literal|helm_schema_ir::ConditionalGuard'
  crates/helm-schema-{core,gen}/src --glob '*.rs' && cargo tree -p
  helm-schema-gen -e normal`.
- Single `.Files.Get` classifier: `rg -n
  'fn is_files_get|Files.Get.*ends_with|ends_with.*Files.Get'
  crates/helm-schema-ir/src --glob '*.rs'`.
- D5 LOC basis: `task tokei:core`; dedicated lower-bound measurements are
  reproduced with `tokei crates/helm-schema-ir/src/expr_call_eval/strict_operands.rs
  crates/helm-schema-ir/src/expr_call_eval/serialization.rs --exclude tests`,
  `tokei crates/helm-schema-gen/src/quoted_serialization.rs
  crates/helm-schema-ir/src/scalar_value.rs --exclude tests`, `tokei
  crates/helm-schema-ir/src/range_modes.rs
  crates/helm-schema-gen/src/required_source_backprojection.rs --exclude tests`,
  `tokei crates/helm-schema-core/src/capability.rs
  crates/helm-schema-core/src/capability_liveness.rs
  crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs
  crates/helm-schema-k8s/src/kubernetes_openapi/provider.rs --exclude tests`,
  and `tokei crates/helm-schema-k8s/src/crds_catalog
  crates/helm-schema/src/analysis/local_crd_projection.rs
  crates/helm-schema-k8s/src/builtin_groups.rs --exclude tests`. Co-owned
  spans are enumerated by `rg -n
  'kind_branches|kind_candidates|kind_partition|terminal_clauses|FailValueRequirement|RangeInput|string_contract'
  crates/helm-schema-{core,ir,gen}/src --glob '*.rs'`.
- Feature-teeth unit controls: `cargo nextest run -p helm-schema-ir -p
  helm-schema-gen -p helm-schema-cli -E
  'test(cross_path_fail_formulas_lower_as_terminal_clauses) |
  test(int_cast_string_preimages_cover_radix_and_complement_lanes) |
  test(range_domains_compose_with_body_and_sibling_contracts) |
  test(capabilities_defaulted_semver_gates_decode_against_the_policy_version) |
  test(chart_shipped_crds_close_the_fields_their_schema_prunes)'`; five tests
  pass. The chart teeth are reproduced separately with `cargo nextest run -P
  integration -p helm-schema-cli -E
  'test(airflow_scheduler_kind_partition_scopes_strategy_providers) |
  test(promtail_string_consumers_and_range_keys_keep_their_domains)'`; two
  tests pass.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step1-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=44aa758
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step1-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step1-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes, 121,059 probes, zero
  flips.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step1-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; all 18
  artifacts remain fixture-identical.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Gates on the final Step 1 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; the workspace Clippy and all three ast-grep checks
    complete without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets complete with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,212 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 565 tests pass and 21 are skipped by the
    integration profile.
  - `task test:all`: exit 0; 1,781 tests pass and 21 are skipped by the CI
    profile, including the live-network lane.
  - Downstream luup2: not required because this step is representation-only
    and has zero schema-semantic change; it remains mandatory on the final
    Wave 1 tree.
  - `task tokei:core`: exit 0; 61,212 production Rust LOC, delta -50.
- Commit: `44472dd` (`refactor(core): collapse shared semantic utilities`).

## Step 2 — structural bounded widening

- Status: landed.
- Acceptance baseline: `44472dd`.
- Baseline production Rust LOC: 61,212.
- Measured results:
  - Bound-helper `config` values now widen from their typed `AbstractValue`
    shape at a structural width greater than 32, independently of chart,
    helper, and source names. The metric counts alternative leaves while
    ignoring depth, so a deeply nested single path stays precise and every
    cloned alternative pays one unit (`abstract_value.rs:96-122`).
  - The OpenTelemetry Collector chart v0.166.0 supplies a nearest observed
    under-bound helper at width 31 and a nearest observed over-bound helper at
    width 58. Synthetic boundary controls keep width 32 exact, widen width 33,
    preserve an Otel-named width-32 value, and produce the same result under a
    renamed helper (`crates/helm-schema-ir/src/tests/abstract_value.rs:38-73`).
  - On the pinned chart, the old name-selected analysis takes 34.85 seconds
    and 1,069,640 KiB maximum RSS. The final structural implementation takes
    2.72 seconds and 84,224 KiB. An over-large 256-leaf preflight took 64.63
    seconds and 2,388,640 KiB, proving that the chosen limit must precede that
    shape. The plan's bounded-runtime stop branch does not trigger.
  - Widening preserves the eager argument's discovered values paths as
    total-shape dependency reads while discarding member-to-path consumer
    correspondence (`analysis_db.rs:1079-1100`). This keeps closed roots from
    rejecting an admitted path without pretending the helper's body consumed
    its value.
  - The authoritative clean schema dump contains all 84 artifacts and the IR
    dump all 18 artifacts. Both remain byte-identical to committed fixtures.
    The compiled corpus comparison checks 60 lanes and 121,059 probes with
    zero flips and zero undisclosed base or third-level truncation.
  - The external OpenTelemetry old/new comparison checks 1,942 probes. It
    reports five widenings, no tightenings: Boolean and numeric
    `namespaceOverride`, an unknown member, and a guarded two-path composite
    all render under Helm; an empty object item aborts and is the disclosed
    resource-bound completeness loss.
- Deviations:
  - Production Rust is 61,293 LOC, +81 from the step baseline and +31 from the
    campaign baseline. This exceeds the frozen +0..+30 estimate by 51 lines.
    The additional production code is the structural counter, typed
    path-preserving abstention, and its tracing event; removing the
    path-preserving lane reproduced a closed-root false rejection for the
    widened helper control. Test and battery infrastructure is excluded from
    this LOC measurement.
  - The repository corpus has no Step 2 fixture change, while the separately
    pinned upstream chart exposes five schema widenings. The plan explicitly
    permits reduced precision after the deterministic resource bound; the
    one Helm-aborting cell is retained as an explicit bounded false positive,
    not described as semantic parity.
- Adjudication evidence:
  - The local mechanism control covers the adjacent directions over chart
    defaults: deleting `focus` and setting `focus=7` both abort during the
    truly selected `b64enc`, while `focus=selected` renders. The widened
    schema accepts all three, so no false rejection remains and the two
    resource-bound false positives are visible.
  - The upstream comparison's five acceptance flips were each replayed with
    real Helm 4.2.3 and `--skip-schema-validation`. Helm renders four and
    aborts the empty-object-item probe. There are no tightening flips and no
    corpus fixture flip to adopt.

### Review dossier

- Structural ownership and limit: `rg -n
  'BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT|structural_width|widen_large_config_value|widening bound helper config'
  crates/helm-schema-ir/src/{abstract_value.rs,analysis_db.rs}`.
- Boundary and name-independence controls: `cargo nextest run -p
  helm-schema-ir -E
  'test(structural_width_ignores_depth_and_counts_alternatives) |
  test(structural_widening_keeps_the_limit_and_widens_the_next_leaf) |
  test(structural_widening_is_independent_of_helper_name)'`; three tests
  pass.
- Local schema matrix: `cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(structural_helper_widening_abstains_in_all_adjacent_input_states)'`;
  the generated schema accepts the deleted, present-number, and
  present-string documents.
- Local Helm matrix: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profile_live -E
  'test(replay_structural_helper_widening_matrix_against_helm)'
  --run-ignored ignored-only --no-capture`; Helm aborts the deleted and
  wrong-type states and renders the consumed-string state.
- Pinned upstream input: `helm repo add open-telemetry
  https://open-telemetry.github.io/opentelemetry-helm-charts
  --repository-config target/arch-v3-step2-preflight/repository/repositories.yaml
  --repository-cache target/arch-v3-step2-preflight/cache && helm pull
  open-telemetry/opentelemetry-collector --version 0.166.0 --untar
  --untardir target/arch-v3-step2-preflight/charts --repository-config
  target/arch-v3-step2-preflight/repository/repositories.yaml
  --repository-cache target/arch-v3-step2-preflight/cache`.
- External acceptance adjudication: after producing the baseline schema with
  `44472dd` and the candidate schema with the final tree, run
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step2-preflight/tmp
  SCHEMA_ACCEPTANCE_EXTERNAL_CHART=/home/roman/dev/helm-schema/target/arch-v3-step2-preflight/charts/opentelemetry-collector
  SCHEMA_ACCEPTANCE_BASELINE_SCHEMA=/home/roman/dev/helm-schema/target/arch-v3-step2-preflight/otel-current.schema.json
  SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA=/home/roman/dev/helm-schema/target/arch-v3-step2-preflight/otel-structural-final.schema.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(external_schema_pair_flips_are_helm_adjudicated)' --run-ignored
  ignored-only --no-capture`; 1,942 probes yield five widenings, every flip is
  printed with its Helm verdict, and there is no tightening.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step2-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Full-depth corpus acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=44472dd
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step2-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step2-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step2-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; all 18
  artifacts remain byte-identical.
- Hermetic and adjacent-state controls: `cargo nextest run -P integration -p
  helm-schema --test schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states) |
  test(composed_probe_sparse_override_round_trips_null_deletion_and_replacement) |
  test(structural_helper_widening_abstains_in_all_adjacent_input_states)'`;
  six tests pass.
- Gates on the final Step 2 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; the full workspace and three ast-grep checks finish
    without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets finish with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,215 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 23 are skipped.
  - `task test:all`: exit 0; 1,786 tests pass and 23 are skipped, including
    the live-network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - Downstream luup2 `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts complete.
  - `task tokei:core`: exit 0; 61,293 production Rust LOC, delta +81.
- Commit: `7817568` (`fix(ir): bound helper config analysis structurally`).

## Step 3a — pure builder split

- Status: landed.
- Acceptance baseline: `7817568`.
- Baseline production Rust LOC: 61,293.
- Measured results:
  - The 4,746-line builder is split into five directly named phase modules:
    input-channel ingestion (143 lines), contract-row/capture lowering (1,116
    lines), requirement/fail lowering (2,216 lines), conditional-overlay
    assembly (993 lines), and final signal construction (327 lines). The
    38-line root declares the phase graph and exposes only
    `derive_schema_signals_from_contract_parts` outside the directory
    (`contract_signal_builder/mod.rs:12-38`).
  - Function bodies and their comments moved without semantic edits. The only
    required source-level changes are sibling-module visibility bounded to
    `pub(super)` and explicit imports demanded by workspace Clippy; no wildcard
    lint suppression was added.
  - The final production count is 61,376 Rust LOC, +83 from the step baseline
    and +114 from the campaign baseline. This step intentionally pays the
    module/import seam before Step 3b deletes now-visible wrappers; the frozen
    -100..-40 estimate applies after 3b, not to this pure-move half.
  - The authoritative final-build schema dump contains all 84 artifacts and
    the IR dump all 18 artifacts, byte-identical to committed fixtures. The
    compiled comparison checks 60 lanes and 121,059 probes with zero flips and
    zero undisclosed base or third-level truncation.
- Deviations:
  - None from the Step 3a contract. The module root is 38 lines rather than a
    one-item facade because it owns the explicit sibling imports; its only
    outward item remains the requested derive function. No function body,
    fixture, or acceptance behavior changed.
- Adjudication evidence:
  - This is representation-only. The acceptance battery reports zero flips,
    so there is no fixture or Helm verdict to adopt. Hermetic monotonicity,
    three-category controls, guard/composite synthesis, and the Temporal
    pairwise matrix all pass on the final tree.

### Review dossier

- Phase boundaries and sole outward export: `wc -l
  crates/helm-schema-ir/src/contract_signal_builder/*.rs && rg -n
  '^mod |^pub\(crate\) use|^pub\(crate\) fn'
  crates/helm-schema-ir/src/contract_signal_builder/{mod.rs,input_channels.rs}`;
  the five modules and only the derive export are listed.
- Mechanical-move review: `git diff --find-renames=20% 7817568 --
  crates/helm-schema-ir/src/contract_signal_builder`; the diff consists of the
  old builder deletion, phase-file additions, sibling visibility, and explicit
  imports. Semantic identity is independently pinned by the dump and battery
  commands below.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3a-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3a-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; the 18
  artifacts remain byte-identical.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3a-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=7817568
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step3a-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step3a-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Frozen-plan check: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Gates on the final Step 3a tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; the workspace Clippy and all three ast-grep checks
    complete without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets complete with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,215 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 23 are skipped.
  - `task test:all`: exit 0; 1,786 tests pass and 23 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because the step is representation-only
    with byte-identical schemas and zero acceptance flips; it remains
    mandatory on the final Wave 1 tree.
  - `task tokei:core`: exit 0; 61,376 production Rust LOC, delta +83.
- Commit: `34e49a6` (`refactor(ir): split contract signal builder phases`).

## Step 3b — builder wrapper deletion

- Status: landed.
- Acceptance baseline: `34e49a6`.
- Baseline production Rust LOC: 61,376.
- Measured results:
  - Ten phase-local, single-caller wrappers are deleted from the split builder:
    compatible-hint partitioning, conditional-guard collection, wildcard
    collection extraction, ranged-member field extraction, metadata-field
    classification, guarded-range recording, three self/header predicate
    classifiers, and the unlowerable-output-selection classifier. Each body is
    now expressed directly at its sole use site
    (`contract_rows.rs:220-239,553-571,685-701,762-781,907-920,1065-1099`;
    `conditional_overlays.rs:120-136,862-871`; `final_signals.rs:266-294`;
    `requirements.rs:80-91`).
  - Explanatory comments moved with the logic they describe. The directory
    root no longer imports or re-exports the deleted wrappers
    (`contract_signal_builder/mod.rs:19-34`).
  - Production Rust is 61,336 LOC, -40 from the step baseline and +74 from the
    61,262 campaign baseline. This lands at the lower boundary of the frozen
    -100..-40 estimate.
  - The authoritative final-build schema dump contains all 84 artifacts and
    the IR dump all 18 artifacts, byte-identical to committed fixtures. The
    full-depth comparison checks 60 lanes and 121,059 probes with zero flips;
    every base and third-level candidate is emitted and the machine report
    discloses the bounded guard/composite sampling categories.
- Deviations:
  - None. This is the frozen representation-only wrapper-deletion contract;
    no fixture or acceptance behavior changes.
- Adjudication evidence:
  - The acceptance battery reports zero flips, so no Helm verdict or fixture
    update is eligible for adoption. The hermetic monotonicity, semantic,
    Temporal pairwise, falsifiable-cap, and guard/composite controls pass on
    the final tree.

### Review dossier

- Deleted-wrapper proof: `rg -n
  'fn (partition_compatible_hints|conditional_guard_predicates|wildcard_collection_path|member_relative_field|metadata_field_kind_from_yaml_path|record_guarded_range_requirement|predicate_is_self_guarding|predicate_is_self_presence|predicate_is_positive_header|predicate_is_unlowerable_output_selection)'
  crates/helm-schema-ir/src/contract_signal_builder`; exit 1 confirms no
  definitions remain. `git diff --stat 34e49a6 --
  crates/helm-schema-ir/src/contract_signal_builder` reports 138 insertions
  and 187 deletions across five files.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3b-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3b-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; one test
  passes and all 18 artifacts remain byte-identical.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step3b-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=34e49a6
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step3b-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step3b-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Frozen-plan check: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Gates on the final Step 3b tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks finish
    without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets finish with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,215 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 23 are skipped.
  - `task test:all`: exit 0; 1,786 tests pass and 23 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because the step is representation-only
    with byte-identical schemas and zero acceptance flips; it remains
    mandatory on the final Wave 1 tree.
  - `task tokei:core`: exit 0; 61,336 production Rust LOC, delta -40.
- Commit: `a0a9e3e` (`refactor(ir): inline phase-local signal helpers`).

## Step 4 — unified call invocation

- Status: landed.
- Acceptance baseline: `a0a9e3e`.
- Baseline production Rust LOC: 61,336.
- Measured results:
  - `CallInvocation` now carries one function name, explicit argument slice,
    and optional evaluated pipeline operand. Direct calls and pipeline stages
    enter the same semantic dispatcher after the pipeline driver has done only
    sequencing and primary evaluation (`expr_call_eval/mod.rs:52-96,866-956,
    1150-1195`).
  - Sequence, comparison, ternary, replacement, trim-affix, YAML/JSON decode,
    join, split, and generic string-consumer families use shared evaluators.
    The pipeline-specific evaluator twins and their duplicate operand-fact
    helpers are deleted from `collections.rs`, `comparisons.rs`,
    `serialization.rs`, and `strict_operands.rs`.
  - A table-driven IR regression compares the complete `EvalResult` for the
    direct and pipeline spellings of every migrated family
    (`src/tests/expr_eval.rs:997-1032`). A public-surface regression pins full
    schema equality for direct and pipeline `split`
    (`helm-schema/tests/public_surface.rs:162-264`).
  - The authoritative schema dump contains all 84 artifacts and the IR dump
    all 18 artifacts, byte-identical to committed fixtures. The full-depth
    comparison checks 60 lanes and 121,059 probes with zero flips and zero
    undisclosed base or third-level truncation.
  - Production Rust is 61,110 LOC, -226 from this step's baseline and -152
    from the 61,262 campaign baseline.
- Deviations:
  - The measured -226 LOC is 24 lines above the frozen -400..-250 estimate.
    The remaining direct-only special forms are intentionally explicit under
    the frozen contract; deleting them to meet an estimate would merge syntax
    recognition with semantic evaluation. The single dispatcher and all
    duplicated migrated-family evaluators are nevertheless landed.
  - The unified strict-parser evaluator exposed a pre-existing semantic
    mismatch: direct `split` recorded nil-strictness while pipeline `split`
    did not. This is behavior-bearing rather than a mechanical preservation,
    so it was preflighted in all three adjacent states and pinned explicitly.
- Adjudication evidence:
  - Helm 4.2.3 returns exit 1 for both direct and pipeline `split` when the
    selected operand is deleted/null, exit 1 for a present numeric operand,
    and exit 0 for a present string operand. The unified evaluator therefore
    preserves a real strict-consumption tooth instead of widening the pipeline
    spelling.
  - The ignored live family matrix exercises direct and pipeline spellings of
    nine families in deleted, present-wrong-type, and truly-consumed states
    (`schema_emission_profile_live.rs:130-294`). All 54 Helm cells match their
    pinned render/abort verdicts.
  - No corpus schema or IR fixture changes, and no full-depth acceptance flips,
    were eligible for adoption. Hermetic monotonicity, semantic controls,
    guard/composite synthesis, and the Temporal pairwise matrix pass.

### Review dossier

- Unified carrier and dispatcher: `rg -n 'struct CallInvocation|fn
  eval_invocation|fn eval_piped_invocation|fn eval_pipeline_with_helper_calls'
  crates/helm-schema-ir/src/expr_call_eval/mod.rs`; the carrier and three
  structural entry points are at lines 62, 85, 866, and 1150.
- Deleted twins: `rg -n
  'eval_pipeline_(comparison|replace|trim|from_yaml|from_json|join|split)|pipeline_string_operand_facts|record_operand_presence_operands'
  crates/helm-schema-ir/src/expr_call_eval`; exit 1 confirms the parallel
  family implementations are gone.
- Direct/pipeline IR identity: `cargo nextest run -p helm-schema-ir -E
  'test(migrated_invocation_families_match_between_call_and_pipeline_syntax)'`;
  one table-driven test passes for nine migrated families.
- `split` full-schema pin: `cargo nextest run -p helm-schema -E
  'test(split_call_and_pipeline_emit_the_same_nil_strict_schema)'`; one test
  passes.
- Live three-direction matrix: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step4-live
  cargo nextest run -P integration -p helm-schema --test
  schema_emission_profile_live -E
  'test(replay_call_and_pipeline_invocation_families_against_helm)'
  --run-ignored ignored-only --no-capture`; all 54 direct/pipeline Helm cells
  pass across nine families.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step4-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step4-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; one test
  passes and all 18 artifacts remain byte-identical.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step4-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=a0a9e3e
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step4-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step4-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Frozen-plan check: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Gates on the final Step 4 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks finish
    without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets finish with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,216 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 568 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,788 tests pass and 24 are skipped, including
    the live-network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - Downstream luup2 `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts complete.
  - `task tokei:core`: exit 0; 61,110 production Rust LOC, delta -226.
- Commit: `02e8e4a` (`refactor(ir): unify call and pipeline invocation`).

## Step 5 — typed function semantics

- Status: landed.
- Acceptance baseline: `02e8e4a`.
- Baseline production Rust LOC: 61,110.
- Measured results:
  - `FunctionSemantics` is now the single IR-owned row for a recognized
    Helm/Sprig function's string operand roles, nil behavior, output
    conversion, collection shape, provenance behavior, supported predicate
    semantics, strict parser, and strict collection-item language
    (`function_semantics.rs:8-277,399-598`). One exhaustive match owns every
    recognized name; unknown functions retain the explicit all-unknown row.
  - AST no longer exposes semantic function classifiers or expression output
    typing. The former URL/IP/parser tests moved beside the crate-private IR
    catalog; AST retains only parsing and syntax classification.
  - Every former `is_*_function`, nil table, operand-position table, parser
    pattern, and collection-item consumer now reads the typed catalog. Special
    evaluation remains in explicit match arms; no registry, macro DSL, or
    order-dependent production list was introduced.
  - The catalog test enumerates 97 recognized names exactly once and pins the
    intentional overlapping rows for total stringification, transformed
    provenance, numeric provenance, strict predicates, string splitting,
    merge nil behavior, and certificate item parsing
    (`src/tests/function_semantics.rs:10-155`).
  - The authoritative schema dump contains all 84 artifacts and the IR dump
    all 18 artifacts, byte-identical to committed fixtures. The compiled
    comparison checks 60 lanes and 121,059 probes with zero flips and zero
    undisclosed base or third-level truncation.
  - Production Rust is 61,179 LOC, +69 from this step's baseline and -83 from
    the 61,262 campaign baseline.
- Deviations:
  - The measured +69 LOC does not meet the frozen -180..-100 estimate. The
    estimate omitted the ownership transfer of the existing 532-line AST
    semantic catalog (including its fully measured URL, semver, duration,
    IPv4, and IPv6 languages) and the typed facet/coverage rows needed to make
    drift compiler-visible. Packing those facets into flags or deleting the
    measured parser documentation would reduce the metric at the cost of the
    plan's typed and auditability contracts, so the direct enum-based catalog
    is retained. No production test bodies moved into the LOC count.
  - The AST `expression_schema_type` helper had no remaining workspace caller
    except Step 5's literal-only `default` lane. It is deleted rather than
    moved; that lane now has a private literal-kind match at
    `expr_call_eval/collections.rs:142-150`. This preserves the exact existing
    scope and prevents the parser crate from retaining semantic output facts.
- Adjudication evidence:
  - Catalog migration is behavior-bearing by contract, but its authoritative
    schema and IR fixtures are byte-identical and the full-depth battery finds
    zero acceptance flips. There is therefore no corpus fixture or new Helm
    verdict to adopt.
  - The nine-family direct/pipeline live matrix replays 54 deleted,
    present-wrong-type, and truly-consumed Helm cells after catalog migration;
    every pinned render/abort verdict passes. Hermetic monotonicity, semantic,
    guard/composite, truncation, and Temporal controls also pass.

### Review dossier

- Single catalog: `rg -n 'struct FunctionSemantics|pub\(crate\) fn
  function_semantics|enum (StringOperands|NilBehavior|OutputSemantics|CollectionShape|ProvenanceBehavior|PredicateSemantics)'
  crates/helm-schema-ir/src/function_semantics.rs`; the typed facets and sole
  exhaustive match are listed.
- Deleted parallel predicates: `rg -n
  'expr_function_catalog|is_(checksum|coercing|merge|provenance_preserving|string_predicate|string_splitting|string_transform|total_numeric_cast|total_stringification)_function|strict_operand_nil_aborts'
  crates --glob '*.rs'`; exit 1 confirms no former independent classifier or
  parser-owned catalog remains.
- Row completeness and intentional overlaps: `cargo nextest run -p
  helm-schema-ir -E 'test(function_semantics)'`; two table-driven tests pass.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step5-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step5-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; one test
  passes and all 18 artifacts remain byte-identical.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step5-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=02e8e4a
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step5-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step5-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Live three-direction replay: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step5-live
  cargo nextest run -P integration -p helm-schema --test
  schema_emission_profile_live -E
  'test(replay_call_and_pipeline_invocation_families_against_helm)'
  --run-ignored ignored-only --no-capture`; all 54 Helm cells pass.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Frozen-plan check: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Gates on the final Step 5 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks finish
    without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets finish with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,220 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 566 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,790 tests pass and 24 are skipped, including
    the live-network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - Downstream luup2 `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts complete.
  - `task tokei:core`: exit 0; 61,179 production Rust LOC, delta +69.
- Commit: `888f274` (`refactor(ir): centralize function semantics`).

## Step 6a — typed selection reachability with adapters

- Status: landed.
- Contract: representation-only; no existing producer or consumer may change
  semantic behavior in this step.
- Acceptance baseline: `888f274`.
- Baseline production Rust LOC: 61,179.
- Measured results:
  - `SelectionReachability` beside `EvalResult` now represents always selected,
    never selected, exact-predicate selection, and approximate selection with
    an optional sound subset (`eval_effect.rs:806-948`). Its independent
    `SelectionTruthSource` distinguishes raw input truth from rendered scalar
    truth.
  - Standard adapters translate existing `TruthCondition` and
    `ScalarValueDispatch` facts without changing them
    (`eval_effect.rs:910-941`). The scalar-dispatch adapter owns the
    rendered-truth distinction. The existing local
    `DefaultPrimarySelection` classification has an adapter at
    `expr_call_eval/collections.rs:222-241`.
  - `EvalResult` constructors initialize the new carrier to always-selected
    raw-input truth. No producer writes it and no consumer reads it in this
    step; eager `Effects` remain independent even when a test sets output
    reachability to never selected (`src/tests/selection_reachability.rs`).
  - Four focused tests cover exact truth and complement polarity, partial
    sound subsets without invalid complement inversion, all four default
    selection states, raw-versus-rendered truth, and eager effects under dead
    output selection.
  - The authoritative clean dump contains 84 schema artifacts and 18 IR
    artifacts, all byte-identical to committed fixtures. The full-depth
    comparison covers 60 lanes and 121,059 probes with zero flips. Coverage
    reports zero dropped base probes and zero dropped third-level probes;
    bounded guard/composite omissions remain explicitly recorded.
  - Production Rust is 61,308 LOC, +129 from this step's baseline and +46 from
    the 61,262 Wave 1 campaign baseline. The result is inside the frozen
    +50..+150 Step 6a estimate.
- Deviations: none. D3 option 1 is implemented as the frozen plan specifies,
  and no producer migration from Step 6b is included.
- Adjudication evidence:
  - This is representation-only. The authoritative fixtures are byte-identical
    and the baseline battery reports `charts_checked=60
    probes_checked=121059 flips=0`; there is no acceptance flip requiring a
    new Helm verdict or fixture update.
  - The Round 73/74 opaque-formatter, literal-primary, and oauth2-proxy live
    matrices all replay successfully against Helm 4.2.3. Hermetic
    monotonicity, three-category semantic controls, guard/composite synthesis,
    falsifiable truncation accounting, and the Temporal pairwise matrix also
    pass.

### Review dossier

- Typed states and truth source: `nl -ba
  crates/helm-schema-ir/src/eval_effect.rs | sed -n '806,948p'`; the output
  shows the four states, optional sound subset, raw/rendered source, and the
  carrier beside `EvalResult`.
- Adapters: `nl -ba crates/helm-schema-ir/src/eval_effect.rs | sed -n
  '910,941p'; nl -ba
  crates/helm-schema-ir/src/expr_call_eval/collections.rs | sed -n
  '222,241p'`; the existing three fact shapes each map into the carrier.
- No producer migration: `rg -n 'selection_reachability'
  crates/helm-schema-ir/src --glob '*.rs' --glob '!**/tests/**'`; only the
  `EvalResult` field and its two default constructor initializations are
  listed.
- Focused invariants: `cargo nextest run -p helm-schema-ir -E
  'test(selection_reachability)'`; four tests pass.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step6a-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step6a-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; one test
  passes and all 18 artifacts remain byte-identical.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step6a-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=888f274
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-step6a-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-step6a-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Round 73/74 live matrices:
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-step6a-live cargo
  nextest run -P integration -p helm-schema --test
  schema_emission_profile_live -E
  'test(replay_opaque_formatter_default_against_helm) |
  test(replay_literal_default_primary_reachability_against_helm) |
  test(replay_oauth2_proxy_tpl_default_eagerness_against_helm)'
  --run-ignored ignored-only --no-capture`; all three matrices pass against
  Helm 4.2.3.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Frozen-plan check: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Gates on the final Step 6a tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks finish
    without warnings.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets finish with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,224 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 566 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,794 tests pass and 24 are skipped, including
    the live-network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - Downstream luup2 `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts complete.
  - `task tokei:core`: exit 0; 61,308 production Rust LOC, delta +129.
- Commit: `779b64d` (`refactor(ir): add selection reachability carrier`).

## Wave 1 summary

- Landed sequence: Step 1 `44472dd`; Step 2 `7817568`; Step 3a `34e49a6`;
  Step 3b `a0a9e3e`; Step 4 `02e8e4a`; Step 5 `888f274`; Step 6a
  `779b64d`.
- Final production Rust LOC: 61,308, a net +46 LOC from the 61,262 campaign
  baseline.
- Every representation-only step kept its schema and IR fixtures identical.
  The behavior-bearing steps adopted no corpus acceptance flip; Step 2's
  separately measured large-chart widening followed its frozen stop-branch
  protocol and live Helm adjudication.
- The final wave tree passes the full-depth 60-lane, 121,059-probe battery
  with zero flips, every repository gate, and the 32-chart downstream luup2
  gate. Wave 2 remains unstarted pending external review and D5 row decisions.

## Wave 2 R0 — freeze remediation contract

- Status: landed.
- Contract: documentation-only; freeze the complete R1–R7 remediation
  contract before implementation.
- Acceptance baseline: `0ed9bec`.
- Baseline production Rust LOC: 61,308.
- Frozen Wave 2 addendum: `plan/architecture-review-v3-wave2.md` at
  `5ef11aa`.
- Measured results:
  - The addendum records all seven remediation contracts, acceptance criteria,
    reporting-integrity rules, self-adversarial obligations, and stop
    conditions. No production or test file changes.
  - One clean schema dump writes 84 artifacts and one clean IR dump writes 18
    artifacts, all byte-identical to their fixtures.
  - The compiled comparison checks 60 lanes and 121,059 probes against
    `0ed9bec` with zero flips and zero undisclosed base or third-level
    truncation.
  - Production Rust remains 61,308 LOC, delta 0.
- Deviations: none.
- Adjudication evidence: this is documentation-only. There is no acceptance
  delta to adjudicate. Hermetic monotonicity, semantic controls,
  guard/composite synthesis, falsifiable truncation accounting, and Temporal
  pairwise monotonicity all pass.

### Review dossier

- Frozen addendum: `git show --stat --oneline 5ef11aa`; the commit contains
  only `plan/architecture-review-v3-wave2.md`.
- Existing frozen plans: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
- Wave 2 addendum: `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r0-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r0-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; one test
  passes and 18 artifacts are written.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r0-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=0ed9bec
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r0-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r0-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; 60 lanes and 121,059 probes yield
  zero flips.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; four
  tests pass.
- Gates on the final R0 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 48 feature combinations pass with no warnings.
  - `cargo nextest run --workspace`: exit 0; 1,224 tests pass.
  - `task test:integration`: exit 0; 566 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,794 tests pass and 24 are skipped.
  - Downstream luup2: not required because R0 changes documentation only.
  - `task tokei:core`: exit 0; 61,308 production Rust LOC, delta 0.
- Commit: `5ef11aa` (`chore(plan): freeze wave 2 remediation contract`).

## Wave 2 R5 — restore comments and documentation

- Status: landed; commit pending.
- Contract: representation-only; restore or re-home the nine documentation
  invariants listed in the frozen Wave 2 addendum without changing executable
  behavior or fixture bytes.
- Acceptance baseline: `d3d9ad5`.
- Baseline production Rust LOC: 61,308.
- Measured results:
  - The nil-behavior explanation now belongs to `NilBehavior` and
    `FunctionSemantics::nil_aborts`; `strict_parser_operand_pattern` again has
    only its parser-language contract (`function_semantics.rs:18-23`,
    `:266-332`, `:397-403`).
  - The catalog again states why total stringifiers participate in operand
    position selection, why division and modulo are outside coercing
    arithmetic, and that `argument_count` includes a pipeline input
    (`function_semantics.rs:162-166`, `:213-217`, `:303-309`).
  - `CallInvocation` and `PipedOperand` now state the already-evaluated,
    final-argument invariant. The unified ternary arm and sequence evaluator
    retain the piped-condition and `copystructure` rationales
    (`expr_call_eval/mod.rs:59-76`, `:130-137`, `:287-292`).
  - The stale string-consumer reference, guard-algebra module description,
    `.Files.Get` helper description, structural-width measurement, and
    metadata-field single-segment reliance are current at their authoritative
    definitions.
  - One clean schema dump writes 84 artifacts and one clean IR dump writes 18
    artifacts, all byte-identical to committed fixtures. The compiled
    comparison checks 60 lanes and 121,059 probes against `d3d9ad5` with zero
    flips, zero dropped base probes, and zero dropped third-level probes.
  - Production Rust remains 61,308 LOC, delta 0.
- Deviations: none. Every executable token remains unchanged; the metadata
  short-circuit equivalence is documented rather than rewritten, preserving
  the comments-only contract.
- Adjudication evidence: this is representation-only. There is no acceptance
  delta to adjudicate. Hermetic monotonicity, semantic controls,
  guard/composite synthesis, falsifiable truncation accounting, and Temporal
  pairwise monotonicity all pass.

### Review dossier

- Documentation-only diff: `git diff --word-diff=porcelain d3d9ad5 --
  crates/helm-schema-core/src/lib.rs crates/helm-schema-ir/src/analysis_db.rs
  crates/helm-schema-ir/src/contract_signal_builder/contract_rows.rs
  crates/helm-schema-ir/src/expr_call_eval/mod.rs
  crates/helm-schema-ir/src/fragment_eval/control.rs
  crates/helm-schema-ir/src/function_semantics.rs`; every addition or removal
  is a Rust comment or doc comment.
- Nil and function-catalog invariants: `nl -ba
  crates/helm-schema-ir/src/function_semantics.rs | sed -n '1,35p;155,225p;255,335p;390,410p'`.
- Invocation invariants: `nl -ba
  crates/helm-schema-ir/src/expr_call_eval/mod.rs | sed -n
  '55,80p;120,145p;275,300p'`.
- Restored boundary documentation: `nl -ba
  crates/helm-schema-core/src/lib.rs | sed -n '1,15p'; nl -ba
  crates/helm-schema-ir/src/analysis_db.rs | sed -n '1128,1148p'; nl -ba
  crates/helm-schema-ir/src/fragment_eval/control.rs | sed -n '638,652p';
  nl -ba
  crates/helm-schema-ir/src/contract_signal_builder/contract_rows.rs | sed -n
  '758,780p'`.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r5-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r5-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r5-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=d3d9ad5
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r5-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r5-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Coverage accounting: `jq '(.charts | map(.base_emitted) | add),
  (.charts | map(.third_level_emitted) | add),
  (.charts | map(.guard_pairs_emitted) | add),
  (.charts | map(.composite_pairs_emitted) | add),
  (.charts | map(.base_dropped) | add),
  (.charts | map(.third_level_dropped) | add)'
  target/arch-v3-wave2-r5-probe-coverage.json`; reports 112,260 base, 7,465
  third-level, 427 guard-pair, and 240 composite-pair probes, with both
  undisclosed-drop counts zero.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; exit
  0, four tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Gates on the final R5 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,224 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 566 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,794 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because R5 changes comments and
    documentation only.
  - `task tokei:core`: exit 0; 61,308 production Rust LOC, delta 0.
- Commit: pending.
