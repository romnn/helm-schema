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
  pairwise monotonicity all pass. A target-local Helm 4.2.3 chart independently
  replays the five Step 4 malformed-arity spellings: piped `fromYaml`,
  `fromJson`, and `fromJsonArray` with one extra explicit argument, plus piped
  `join` with zero and two explicit arguments. Each exits 1 with Helm's
  wrong-number-of-arguments diagnostic for the named function.

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

- Status: landed.
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
- Commit: `0df0748` (`docs(ir): restore semantic invariants`).

## Wave 2 R7 — ledger corrections and LOC re-forecast

- Status: landed.
- Contract: documentation-only; append the independent Wave 1 review's
  corrections without rewriting the historical step records.
- Acceptance baseline: `0df0748`.
- Baseline production Rust LOC: 61,308.
- Measured results:
  - All five Wave 1 review corrections are recorded below, including the
    like-for-like Step 3 miss, the reproducible capability band, the Step 4/5
    semantic deltas, remaining AST semantic evaluators, and an evidence-based
    campaign re-forecast.
  - One clean schema dump writes 84 artifacts and one clean IR dump writes 18
    artifacts, all byte-identical. The compiled comparison checks 60 lanes and
    121,059 probes against `0df0748` with zero flips, zero dropped base probes,
    and zero dropped third-level probes.
  - Production Rust remains 61,308 LOC, delta 0.
- Deviations: none. Historical step sections remain unchanged; this section
  explicitly supersedes only the reviewed claims and estimates named below.
- Adjudication evidence: this is documentation-only. There is no acceptance
  delta to adjudicate. Hermetic monotonicity, semantic controls,
  guard/composite synthesis, falsifiable truncation accounting, and Temporal
  pairwise monotonicity all pass.

### Wave 1 review corrections

1. The Step 3 estimate was a whole-step estimate, not a Step 3b estimate.
   Step 3 moved from 61,293 to 61,336 production Rust LOC, a measured net +43
   against the frozen -100..-40 band. The result misses that band by 83 lines
   at its upper edge and 143 at its lower edge. Step 3b's isolated -40 does
   not satisfy the whole-step estimate.
2. The D5 capability-oracle row's original 850..1,050 band is superseded by
   700..900. The cited directly owned files measure 723 production Rust LOC;
   the narrower band retains only a modest allowance for co-owned call sites
   rather than presenting those sites as reproduced by the file-only command.
3. Step 4 also changed malformed-arity semantics for piped `fromYaml`,
   `fromJson`/`fromJsonArray`, and `join`: the former pipeline-specific arms
   decoded or erased input shape regardless of explicit-argument arity, while
   the unified invocation routes malformed arities through passthrough or
   widening. Helm 4.2.3 aborts every such malformed spelling, so the delta
   does not change a renderable input, but it is nevertheless a semantic
   delta and the earlier disclosure was incomplete.
4. Step 5 also carried three micro-deltas. `urlquery` began reporting string
   operand indices; `mustUniq` and `mustDeepCopy` gained provenance
   preservation; and `mustDateModify` began claiming its first string operand
   without the former arity-at-least-two guard. The clean fixtures and
   full-depth battery did not expose an acceptance flip, but these changes
   belong in the semantic record.
5. The statement that AST retains only parsing and syntax classification was
   too broad. `TemplateExpr::renders_yaml_fragment` and
   `TemplateExpr::fragment_indent_width` remain public semantic evaluators in
   `expr.rs:138-166`; the public printf renderers remain in
   `printf_eval.rs:8-115`; and the public semver constraint evaluators remain
   in `semver_constraint.rs:49-179`. They are known ownership debt for a later
   wave, not part of this remediation.

### Campaign LOC re-forecast

- Wave 1 measured +46 LOC against its like-for-like frozen aggregate band of
  -910..-390. The miss came from typed carrier/catalog seams and preserved
  measured semantic documentation, while the deletion steps also delivered
  less removal than forecast. The frozen estimates remain the immutable plan
  contract, but they are no longer used as the execution forecast.
- From the current 61,308-LOC tree, the execution forecast for the remaining
  remediation plus Steps 6b through 11 is -1,120..+580 LOC, placing campaign
  completion at roughly 60,188..61,888 LOC (a total -1,074..+626 from the
  61,262 campaign baseline). The corresponding Wave 2 remainder through Step
  8 is -620..+380 LOC. These bands deliberately include adapter retention and
  typed-seam growth observed in Wave 1; feature pruning remains excluded by
  D5.
- The remaining-step working bands are: unfinished remediation +50..+200;
  Step 6b -250..+50; Step 7a +80..+180; Step 7b -400..-100; Step 8
  -100..+50; Step 9 -350..-50; Step 10 -150..+50; and Step 11 0..+200.
  They are a ledger forecast, not amendments to the frozen plan.

### Review dossier

- Step 3 like-for-like arithmetic: `sed -n '322,501p'
  plan/architecture-review-v3-progress.md`; the recorded baseline is 61,293
  and the final Step 3b tree is 61,336, so the whole-step delta is +43.
- Capability lower bound: `tokei
  crates/helm-schema-core/src/capability.rs
  crates/helm-schema-core/src/capability_liveness.rs
  crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs
  crates/helm-schema-k8s/src/kubernetes_openapi/provider.rs --exclude tests`;
  reports 723 production Rust LOC.
- Step 4 edge-arity diff: `git diff a0a9e3e 02e8e4a --
  crates/helm-schema-ir/src/expr_call_eval/mod.rs
  crates/helm-schema-ir/src/expr_call_eval/serialization.rs | rg -n -C 5
  'fromYaml|fromJson|join'`; shows the former unconditional pipeline arms and
  the unified operand-count gates.
- Step 4 live edge-arity adjudication: after creating the target-local chart
  recorded in this dossier, `helm version --short` reports
  `v4.2.3+g43e8b7f`. The exact replay commands are `helm template wave2-r7
  target/arch-v3-wave2-r7-live-arity --show-only
  templates/from-yaml-extra.yaml --set-string case=from-yaml-extra
  --skip-schema-validation`, with the same command using
  `templates/from-json-extra.yaml` and `case=from-json-extra`,
  `templates/from-json-array-extra.yaml` and
  `case=from-json-array-extra`, `templates/join-missing.yaml` and
  `case=join-missing`, then `templates/join-extra.yaml` and
  `case=join-extra`. Their exit codes are respectively 1, 1, 1, 1, and 1;
  each diagnostic names the selected function and its actual versus required
  argument count.
- Step 5 micro-deltas: `git diff 02e8e4a 888f274 --
  crates/helm-schema-ir/src | rg -n -C 4
  'urlquery|mustUniq|mustDeepCopy|mustDateModify'`.
- Remaining AST semantics: `rg -n '^pub fn|pub fn
  (renders_yaml_fragment|fragment_indent_width)'
  crates/helm-schema-ast/src/{expr.rs,printf_eval.rs,semver_constraint.rs}`.
- Forecast arithmetic: `jq -n '[[50,200],[-250,50],[80,180],[-400,-100],[-100,50],[-350,-50],[-150,50],[0,200]]
  | [map(.[0]) | add, map(.[1]) | add]'`; reports `[-1120,580]`.
- Clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r7-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written.
- Clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r7-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r7-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=0df0748
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r7-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r7-probe-coverage.json
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
  target/arch-v3-wave2-r7-probe-coverage.json`; reports 112,260 base, 7,465
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
- Gates on the final R7 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,224 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 566 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,794 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because R7 changes campaign documentation
    only.
  - `task tokei:core`: exit 0; 61,308 production Rust LOC, delta 0.
- Commit: `78cb7c7` (`docs(plan): correct wave 1 review record`).

## Wave 2 R4 — restore battery exactness

- Status: landed.
- Contract: test infrastructure only; make candidate-accepts/Helm-aborts
  acceptance flips an explicitly counted machine-report category and reject a
  count above the source-controlled, pre-registered per-step allowance, which
  defaults to zero.
- Acceptance baseline: `78cb7c7`.
- Baseline production Rust LOC: 61,308.
- Measured results:
  - `ProbeCoverageReport` now carries a `helm_adjudication` object with whether
    live replay was enabled, the number of flip cells replayed, the count and
    identities of candidate-accepts/Helm-aborts cells, and the pre-registered
    allowance.
  - The allowance is a source-controlled constant and defaults to zero. The
    report is written before validation, then count-to-case accounting and the
    allowance are enforced. A synthetic one-cell/zero-allowance test proves
    the enforcement fails rather than merely round-tripping its own values.
  - The authoritative full-depth run checks 60 lanes and 121,059 probes
    against `78cb7c7` with zero flips. Its machine report records live Helm
    adjudication enabled, zero replayed flip cells, zero
    candidate-accepts/Helm-aborts cells, allowance zero, and an empty case
    list.
  - One clean schema dump writes 84 artifacts and one clean IR dump writes 18
    artifacts, all byte-identical. Production Rust remains 61,308 LOC.
- Deviations:
  - No contract deviation. The first schema-dump invocation omitted creation
    of its target-local `TMPDIR`; it exited nonzero before writing any
    artifacts. After creating the empty directory, the single authoritative
    dump ran from that clean destination and passed. This was command setup,
    not a fixture or implementation failure.
- Adjudication evidence:
  - R4 changes only test accounting, not schema acceptance. The full-depth
    comparison has zero flips, so no Helm document verdict is available or
    required. The live lane is nevertheless enabled in the report, which
    prevents a zero-flip run from being mislabeled as an adjudication-disabled
    run.
  - Hermetic monotonicity, three-category semantic controls, guard/composite
    synthesis, both falsifiable accounting checks, and Temporal pairwise
    monotonicity pass in a five-test run.
  - Self-adversarial pass: the inherited `accepted || !rendered` outcome was
    not trusted. Candidate-accepts/Helm-aborts now returns a distinct verdict;
    every such verdict increments a count and records the exact chart/probe
    identity; mismatched count and case-list length fails; count above the
    compile-time allowance fails; and the report is persisted before either
    failure. No unaccounted path or favorable measurement framing was found.

### Review dossier

- Accounting implementation: `sed -n '1,130p;980,1100p;1260,1390p'
  crates/helm-schema/tests/schema_emission_profiles.rs`; shows the report
  fields, source-controlled zero allowance, falsifiable unit test, report
  write-before-validation order, distinct adjudication verdict, and counting
  path.
- Falsifiable enforcement: `cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(helm_adjudication_validation_rejects_unregistered_accepted_abort)'`;
  exit 0 and the deliberately over-budget synthetic value is rejected.
- Clean schema dump setup and proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r4-final-dump`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r4-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written.
- Clean IR dump setup and proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r4-final-ir`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r4-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written.
- Full-depth acceptance proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r4-prober`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r4-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=78cb7c7
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r4-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r4-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Machine accounting: `jq '{baseline_ref, helm_adjudication, totals: {base:
  (.charts | map(.base_emitted) | add), third_level: (.charts |
  map(.third_level_emitted) | add), guard_pairs: (.charts |
  map(.guard_pairs_emitted) | add), composite_pairs: (.charts |
  map(.composite_pairs_emitted) | add), base_dropped: (.charts |
  map(.base_dropped) | add), third_level_dropped: (.charts |
  map(.third_level_dropped) | add)}}'
  target/arch-v3-wave2-r4-probe-coverage.json`; reports baseline `78cb7c7`,
  live adjudication enabled, zero adjudicated flips, zero accepted-abort cells,
  allowance zero, 112,260 base probes, 7,465 third-level probes, 427 guard
  pairs, 240 composite pairs, and no base or third-level drops.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(helm_adjudication_validation_rejects_unregistered_accepted_abort) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; exit
  0, five tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa -- plan/architecture-review-v3-wave2.md`;
  exit 0.
- Gates on the final R4 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,224 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,795 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because R4 changes only test
    infrastructure, not schema semantics.
  - `task tokei:core`: exit 0; 61,308 production Rust LOC, delta 0.
- Measured production LOC delta: 0 (61,308 to 61,308).
- Commit: `9fdd812` (`test(harness): account helm-aborting widenings`).

## Wave 2 R2 — harden the selection carrier

- Status: landed.
- Contract: representation-only; harden the Step 6a carrier's exactness,
  complement, truth-source ownership, and forgotten-producer state without
  migrating a producer or changing schema/IR fixture bytes.
- Acceptance baseline: `9fdd812`.
- Baseline production Rust LOC: 61,308.
- Measured results:
  - `SelectionReachability::exact` now routes approximation-containing
    predicates through the `TruthCondition` exactness boundary, yielding an
    `Approximate` carrier with only its sound subset instead of minting an
    invertible `Exact` state.
  - The carrier owns its private state and truth source. Its `complement`
    operation is the sole inversion spelling: Always/Never swap, Exact
    negates, and Approximate deliberately discards its one-way sound subset.
    `approximate(Some(True))` canonicalizes to Always, and the unused polarity
    negation implementation is deleted.
  - `DefaultPrimarySelection` owns the truth source for every arm. Literal,
    ValuesPath, JsonDecodedPath, OutputPath, and FirstTruthy arms are labeled
    raw-input truth; printf-identity arms are labeled rendered-scalar truth.
    The adapter no longer accepts a caller-supplied source that could mislabel
    an arm.
  - `EvalResult.selection_reachability` is now optional. Both construction
    paths initialize it to `None`, making a forgotten producer distinguishable
    from a proved Always selection before the Step 6b family migrations.
  - Six focused tests cover approximation demotion, true-subset
    canonicalization, all complement states, all default-selection states and
    truth sources, and the forgotten-producer/eager-effect boundary.
  - One clean schema dump writes 84 artifacts and one clean IR dump writes 18
    artifacts, all byte-identical. The full-depth comparison checks 60 lanes
    and 121,059 probes against `9fdd812` with zero flips and no undisclosed
    truncation. Production Rust is 61,376 LOC, delta +68.
- Deviations:
  - Process-only: the baseline commit and 61,308-LOC measurement were verified
    before editing, but this section was appended after the focused carrier
    edits and tests rather than before the first edit. The acceptance baseline
    itself remained `9fdd812` throughout.
- Adjudication evidence:
  - R2 changes a dormant representation and its adapters only; no producer or
    consumer is migrated. The authoritative schema and IR fixture dumps are
    byte-identical, and the compiled battery reports zero acceptance flips, so
    no Helm document disposition or fixture update is available or required.
  - Hermetic monotonicity, three-category semantic controls,
    guard/composite synthesis, Temporal pairwise monotonicity, and both
    falsifiable accounting checks pass together.
  - Self-adversarial pass: `rg` finds no production write of
    `selection_reachability`; the only constructor values are `None` and the
    only explicit `Some` is in the focused forgotten-producer test. Private
    state prevents field-level sound-subset inversion, `complement` drops
    approximate subsets, and the default adapter obtains its source from the
    classified arm rather than its caller. The inherited raw/rendered labels
    were checked against each classifier branch rather than copied from the
    old adapter. No contract-in-letter-only result, acceptance delta, or
    favorable measurement framing was found.

### Review dossier

- Carrier exactness and forgotten-producer proof: `sed -n '800,1030p'
  crates/helm-schema-ir/src/eval_effect.rs`; shows private carrier state,
  approximation demotion, canonicalization, the sole complement operation,
  optional `EvalResult` storage, and `None` initialization.
- Truth-source ownership: `sed -n '220,375p'
  crates/helm-schema-ir/src/expr_call_eval/collections.rs`; shows each
  default-selection arm carrying its own source and the source-free adapter,
  including rendered truth for printf identity and raw truth for identities
  and FirstTruthy.
- Producer audit: `rg -n 'selection_reachability' crates/helm-schema-ir/src
  --glob '*.rs'`; finds the field, its two `None` constructors, the focused
  test assignment, and no migrated production producer.
- Focused state/source proof: `cargo nextest run -p helm-schema-ir -E
  'test(selection_reachability)'`; exit 0, six tests pass.
- Clean schema dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r2-final-dump`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r2-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written byte-identically.
- Clean IR dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r2-final-ir`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r2-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written byte-identically.
- Full-depth acceptance proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r2-prober`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=9fdd812
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r2-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r2-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Machine accounting: `jq '{baseline_ref, helm_adjudication, totals: {base:
  (.charts | map(.base_emitted) | add), third_level: (.charts |
  map(.third_level_emitted) | add), guard_pairs: (.charts |
  map(.guard_pairs_emitted) | add), composite_pairs: (.charts |
  map(.composite_pairs_emitted) | add), base_dropped: (.charts |
  map(.base_dropped) | add), third_level_dropped: (.charts |
  map(.third_level_dropped) | add)}}'
  target/arch-v3-wave2-r2-probe-coverage.json`; reports baseline `9fdd812`,
  live adjudication enabled, zero adjudicated flips, zero accepted-abort cells,
  allowance zero, 112,260 base probes, 7,465 third-level probes, 427 guard
  pairs, 240 composite pairs, and no base or third-level drops.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(helm_adjudication_validation_rejects_unregistered_accepted_abort) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; exit
  0, five tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Gates on the final R2 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,226 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,797 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because R2 is representation-only and all
    schema/IR bytes and acceptance verdicts are unchanged.
  - `task tokei:core`: exit 0; 61,376 production Rust LOC, delta +68.
- Measured production LOC delta: +68 (61,308 to 61,376).
- Commit: `70bc42d` (`refactor(ir): harden selection carrier`).

## Wave 2 R6 — catalog and hygiene cleanups

- Status: landed.
- Contract: reconcile or remove unused catalog shape facts; add a checked
  dispatcher-to-catalog boundary; use existing semantic helpers; correct and
  pin sequence nil-abort directness; and move the test-support inline module
  to the repository's mandated test layout. Adjudicate any behavior-bearing
  delta.
- Acceptance baseline: `70bc42d`.
- Baseline production Rust LOC: 61,376.
- Measured results:
  - The unconsumed `CollectionShape::Sequence` and `Mapping` variants and all
    writes to them are deleted. The retained `Merge` and `StringSplit` facets
    are both consumed by direct and pipeline evaluation; sequence routing
    remains operational dispatch rather than a second catalog partition.
  - A dispatcher entry boundary now permits special-form evaluation only for
    catalogued functions or names in a maintained intentional-exceptions
    table. The table contains evaluation-order forms that own no shared
    semantic facet; focused tests require each exception to remain outside the
    catalog and prove arbitrary unknown names take the generic path.
  - Assignment-derived-text classification uses the catalog's
    `is_string_transform` helper. Sequence nil behavior has no `uniq`
    special-case: a pipeline result is explicitly non-direct, while a direct
    call retains directness only for an actual values path. Focused tests pin
    all three direct/piped states.
  - The sparse-override round-trip test moved from an inline module in test
    support to `schema_emission_profiles.rs`; setup now returns
    `eyre::Result` and uses `ok_or_eyre`.
  - The repaired authoritative schema dump writes 84 artifacts and the IR
    dump writes 18 artifacts, all byte-identical. The full-depth comparison
    checks 60 lanes and 121,059 probes against `70bc42d` with zero flips and
    zero undisclosed truncation. Production Rust is 61,408 LOC, delta +32.
- Deviations:
  - Failed preflight, discarded rather than adopted: the first dispatcher
    exception table omitted the `required` special form. The first battery run
    exposed 18 Kyverno acceptance widenings; live Helm replay showed 16
    candidate-accepts/Helm-aborts cells on the two `sigstoreVolume` paths and
    only the object-member cells rendered. The zero allowance rejected the
    run with exit 100. `required` was added to the intentional exceptions,
    the invalid dump/IR/prober batch was deleted, and the single authoritative
    batch was regenerated from the repaired tree. The focused Kyverno corpus
    test and final battery then returned to byte-identical/zero-flip results.
  - No final-tree contract deviation or semantic delta. The executable
    dispatcher boundary adds +32 LOC rather than deleting code, but it makes
    catalog drift behaviorally test-visible and leaves no unconsumed catalog
    facet.
- Adjudication evidence:
  - The failed preflight Helm-adjudicated both directions of the accidental
    `required` widening: boolean, numeric, string, array, and empty-object
    replacements aborted; a populated object rendered. The widening was not
    registered or retained.
  - On the repaired tree, fixture bytes and the full-depth 121,059-probe
    acceptance surface match `70bc42d`, so no final fixture flip or Helm
    disposition is available or required. Hermetic monotonicity, semantic
    controls, guard/composite synthesis, Temporal monotonicity, and both
    falsifiable accounting checks pass.
  - Self-adversarial pass: the inherited exception inventory was not trusted;
    its first omission was caught and repaired. The final boundary makes any
    non-catalog/non-exception name return through generic evaluation before a
    special arm can run. The catalog facet deletion was audited by `rg`, the
    pipeline directness rule has an explicit three-state test, and the final
    measurements disclose the failed first design and positive LOC delta. No
    remaining contract-in-letter-only result or undisclosed semantic change
    was found.

### Review dossier

- Catalog deletion and retained consumers: `rg -n
  'CollectionShape|with_collection' crates/helm-schema-ir/src --glob '*.rs'`;
  shows only None, Merge, and StringSplit, with the latter two consumed in
  direct/pipeline dispatch and expression evaluation.
- Executable dispatcher boundary: `sed -n '45,150p'
  crates/helm-schema-ir/src/expr_call_eval/mod.rs`; shows the maintained
  exceptions, known-or-exception classifier, early generic return, and piped
  final-argument handling.
- Focused catalog/directness proof: `cargo nextest run -p helm-schema-ir -E
  'test(function_semantics) |
  test(piped_sequence_operands_are_never_direct_accesses) |
  test(dispatcher_special_forms_are_catalogued_or_intentional_exceptions)'`;
  exit 0, four tests pass.
- Moved harness proof: `cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(composed_probe_sparse_override_round_trips_null_deletion_and_replacement)'`;
  exit 0, one test passes through `eyre::Result` setup.
- Failed-preflight reproducer: run the full-depth command below on the
  pre-repair tree with `required` removed from
  `INTENTIONAL_DISPATCH_EXCEPTIONS`; exit 100 and its machine/live output
  reports 18 Kyverno flips and 16 candidate-accepts/Helm-aborts cells across
  `admissionController.sigstoreVolume` and
  `reportsController.sigstoreVolume`.
- Repaired Kyverno control: `cargo nextest run -P integration -p
  helm-schema-cli --test chart_corpus -E 'test(kyverno)'`; exit 0, one test
  passes.
- Clean schema dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r6-final-dump`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r6-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written byte-identically.
- Clean IR dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r6-final-ir`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r6-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written byte-identically.
- Full-depth acceptance proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r6-prober`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r6-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=70bc42d
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r6-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r6-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Machine accounting: `jq '{baseline_ref, helm_adjudication, totals: {base:
  (.charts | map(.base_emitted) | add), third_level: (.charts |
  map(.third_level_emitted) | add), guard_pairs: (.charts |
  map(.guard_pairs_emitted) | add), composite_pairs: (.charts |
  map(.composite_pairs_emitted) | add), base_dropped: (.charts |
  map(.base_dropped) | add), third_level_dropped: (.charts |
  map(.third_level_dropped) | add)}}'
  target/arch-v3-wave2-r6-probe-coverage.json`; reports baseline `70bc42d`,
  live adjudication enabled, zero adjudicated flips, zero accepted-abort cells,
  allowance zero, 112,260 base probes, 7,465 third-level probes, 427 guard
  pairs, 240 composite pairs, and no base or third-level drops.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(helm_adjudication_validation_rejects_unregistered_accepted_abort) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; exit
  0, five tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Gates on the final R6 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,228 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 567 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,799 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream luup2: not required because the final R6 tree is byte- and
    acceptance-identical to its baseline.
  - `task tokei:core`: exit 0; 61,408 production Rust LOC, delta +32.
- Measured production LOC delta: +32 (61,376 to 61,408).
- Commit: `eaeb7df` (`refactor(ir): reconcile function dispatch`).

## Wave 2 R1 — correct `unset` nil behavior

- Status: landed.
- Contract: behavior-bearing; replace the inherited `unset => AlwaysAborts`
  catalog row with the live-Helm `DirectAccessAborts` fact, pin direct and
  local-binding spellings, pre-register expected IR/schema effects, and run
  the full behavior-bearing protocol with live adjudication.
- Acceptance baseline: `eaeb7df`.
- Baseline production Rust LOC: 61,408.
- Measured results:
  - Pre-change Helm 4.2.3 measurement: direct `unset .Values.absent "k"`
    aborts with an expected-map/got-interface error (exit 1), while assigning
    the same missing value to `$x` and calling `unset $x "k"` renders a
    ConfigMap (exit 0). This pins `DirectAccessAborts`, not `AlwaysAborts`.
  - The catalog now classifies `unset` as `DirectAccessAborts`. A focused IR
    test pins the direct missing-values path's `AbsenceAborts` and null-aborting
    object type while the equivalent local binding keeps only a non-null-
    aborting object type.
  - Pre-registered change set: focused IR/effect fixtures for the local-binding
    spelling could lose only the unconditional nil-abort tooth; direct access
    had to remain strict. The initial corpus expectation was byte identity.
    The clean run instead found two schema fixture deltas, NACK and nats-kafka,
    because both pass a values subtree through a helper-local dot to `unset`.
    Both were stopped, inspected, and live-adjudicated before adoption.
  - NACK's adjacent-state matrix renders when `jetstream.pullPolicy` is
    deleted, numeric, or a consumed string, and aborts when the helper host
    `jetstream` is deleted or scalar. nats-kafka has the same verdict matrix
    for `image.tagOverride` and its `image` host. A compiled Rust chart test
    pins the generated schema to those Helm verdicts.
  - The authoritative schema dump writes 84 artifacts and the IR dump writes
    18 artifacts. The full-depth comparison checks 60 lanes and 121,059
    probes against `eaeb7df` with zero acceptance flips, zero accepted-abort
    cells, and zero undisclosed truncation. The fixture changes are therefore
    structural re-encoding under the sampled acceptance surface, not a
    registered acceptance change.
- Deviations:
  - The pre-registered assertion that no corpus chart exercised helper-local
    `unset` was false. The first final-tree integration gate stopped with exit
    100 after 565 passes and two fixture mismatches. File inspection traced
    those deltas to NACK's `.Values.jetstream` and nats-kafka's `.Values.image`
    helper contexts. The fixtures were adopted only after the five-cell live
    matrix for each chart passed and the compiled regression was added.
  - The first focused regression expected deletion of NACK's whole
    `jetstream` host to report `/jetstream`; the generated root terminal
    correctly rejects at the document root, matching Helm's nil-pointer
    abort. The test expectation was corrected before the final gates.
  - Production Rust grows by one LOC rather than deleting code. No malformed-
    arity or other semantic delta was observed; only the inherited `unset`
    nil classification changed.
- Adjudication evidence:
  - Helm 4.2.3 exits 1 for direct `unset .Values.absent "k"` and exits 0 for
    `$x := .Values.absent` followed by `unset $x "k"`. This is the corrected
    live fact used by the catalog.
  - NACK exits 0 for deleted/null, numeric, and present-string
    `jetstream.pullPolicy`; it exits 1 for null or numeric `jetstream` hosts.
    nats-kafka exits 0 for the corresponding three `image.tagOverride`
    states and exits 1 for null or numeric `image` hosts. All Helm calls used
    `--skip-schema-validation`; the compiled schema control matches every
    verdict.
  - Hermetic monotonicity, semantic controls, guard/composite synthesis,
    Temporal monotonicity, and both falsifiable accounting checks pass. The
    final full-depth live battery has no flip requiring an additional
    disposition.
  - Self-adversarial pass: the inherited catalog row was measured rather than
    trusted; the helper-local corpus uses invalidated the favorable initial
    byte-identity claim and are disclosed here. Deleted, present-wrong-type,
    truly-consumed, and invalid-host composite states were probed. No
    contract-in-letter-only result, unadjudicated acceptance change, or more
    favorable ledger framing remains.

### Review dossier

- Direct/local Helm fact: `helm template unset-nil-control
  target/arch-v3-wave2-r1-live --set mode=direct
  --skip-schema-validation` exits 1; the same command with `--set mode=local`
  exits 0.
- Focused IR proof: `cargo nextest run -p helm-schema-ir -E
  'test(unset_nil_behavior_distinguishes_direct_access_from_a_local_binding)'`;
  exit 0, one test passes.
- NACK live matrix: run `helm template nack testdata/charts/nack
  --skip-schema-validation` separately with `--set
  jetstream.pullPolicy=null`, `--set jetstream.pullPolicy=7`, `--set-string
  jetstream.pullPolicy=Always`, `--set jetstream=null`, and `--set
  jetstream=7`; exits are 0, 0, 0, 1, and 1.
- nats-kafka live matrix: run `helm template nats-kafka
  testdata/charts/nats-kafka --skip-schema-validation` separately with `--set
  image.tagOverride=null`, `--set image.tagOverride=7`, `--set-string
  image.tagOverride=changed`, `--set image=null`, and `--set image=7`; exits
  are 0, 0, 0, 1, and 1.
- Compiled schema matrix: `cargo nextest run -P integration -p
  helm-schema-cli --test chart_reaudit -E
  'test(unset_helper_contexts_preserve_guarded_values_and_reject_invalid_hosts)'`;
  exit 0, one test passes.
- Clean schema dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r1-final-dump`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r1-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written.
- Clean IR dump proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r1-final-ir`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r1-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written.
- Full-depth acceptance proof: `mkdir -p
  /home/roman/dev/helm-schema/target/arch-v3-wave2-r1-prober`, then
  `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=eaeb7df
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r1-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r1-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Machine accounting: `jq '{baseline_ref, helm_adjudication, totals: {base:
  (.charts | map(.base_emitted) | add), third_level: (.charts |
  map(.third_level_emitted) | add), guard_pairs: (.charts |
  map(.guard_pairs_emitted) | add), composite_pairs: (.charts |
  map(.composite_pairs_emitted) | add), base_dropped: (.charts |
  map(.base_dropped) | add), third_level_dropped: (.charts |
  map(.third_level_dropped) | add)}}'
  target/arch-v3-wave2-r1-probe-coverage.json`; reports baseline `eaeb7df`,
  live adjudication enabled, zero flips, zero accepted-abort cells against
  allowance zero, 112,260 base probes, 7,465 third-level probes, 427 guard
  pairs, 240 composite pairs, and no base or third-level drops.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(helm_adjudication_validation_rejects_unregistered_accepted_abort) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states)'`; exit
  0, five tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Gates on the final R1 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,229 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 568 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,801 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream install: `cargo install --path
    ./crates/helm-schema-cli/`; exit 0.
  - Downstream luup2: `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`; exit 0 across all 32 charts.
  - `task tokei:core`: exit 0; 61,409 production Rust LOC, delta +1.
- Measured production LOC delta: +1 (61,408 to 61,409).
- Commit: `b6d9c6f` (`fix(ir): correct unset nil behavior`).

## Wave 2 R3 — harden structural bound-helper widening

- Status: landed; commit pending.
- Contract: behavior-bearing; apply the structural width budget to every
  helper binding and helper/fragment dot value, keep the Step 2 bounded-runtime
  stop branch binding, separate widened-abstention dependency reads from real
  YAML serialization, and replace the name-only test with a non-`config`
  binding-level control.
- Acceptance baseline: `b6d9c6f`.
- Baseline production Rust LOC: 61,409.
- Measured results:
  - Every named helper binding and both helper/fragment dot values now pass
    through the same structural-width budget (`analysis_db.rs:1080-1086,
    1140-1178`). Width 32 remains exact; width 33 widens independently of the
    binding and helper names. The replacement test exercises a non-`config`
    `payload` binding and verifies every lost values path is retained.
  - Widening-preservation reads use the dedicated
    `ValueKind::WidenedDependency` (`types.rs:33`, `analysis_db.rs:1093`). The
    contract-signal builder consumes that kind only by marking the values path
    referenced (`contract_rows.rs:414-419`), so it cannot set
    `has_non_control_use`, `used_as_yaml_serialized`, provider facts, or a
    rendered-shape fact. A focused contract test pins this boundary.
  - On OpenTelemetry Collector 0.166.0, the R1 baseline takes 2.27 seconds and
    88,616 KiB maximum RSS; R3 takes 2.34 seconds and 91,616 KiB. Both remain
    below the Step 2 final measurement of 2.72 seconds / 84,224 KiB to normal
    run-to-run variance, and far below the 64.63-second / 2,388,640-KiB
    256-leaf blowup. The frozen stop branch does not trigger. The external
    compiled comparison checks 1,934 probes and finds zero acceptance flips.
  - The local structural-helper chart now carries a guard-only third-level
    `guard.deep.flag` with integer default beside the truly consumed `focus`
    path. With `focus=selected`, the R1 schema rejects a string flag as
    integer-only, R3 accepts it, and Helm renders it. The final six-cell matrix
    pins deleted/wrong/consumed focus plus deleted/string/integer guard states,
    with the required composite state in every guard cell.
  - The corpus preflight changes only Kyverno fixture bytes. Canonical
    definition-set comparison shows that R3 removes three vacuous schemas:
    open objects listing `enabled/mirror/root/rootRaw`, `eventTypes`, and
    `namespace`; there is no candidate-only definition. Definition interning
    consequently renumbers the file. The full-depth comparison checks 60
    lanes and 121,059 probes with zero acceptance flips, zero accepted-abort
    cells, and zero undisclosed truncation. IR fixtures remain byte-identical.
- Deviations:
  - The first corpus preflight stopped with 55 passes and one Kyverno fixture
    mismatch. The expected fixture-identical assumption was not adopted. A
    clean candidate dump, canonical definition-set comparison, and full-depth
    battery established the three removed definitions are vacuous and the
    sampled acceptance surface is unchanged before the fixture was copied.
  - A first generic external comparison of the local widening control failed
    the R4 zero allowance: single-path mutations replaced the `guard` or
    `guard.deep` host while leaving `focus` absent/non-string, so the candidate
    resource-bound schema accepted 18/26 documents that Helm aborted in the
    independent `b64enc` consumer. This was a probe-composition failure, not
    registered parity. The final control uses explicit `focus=selected`
    composites and pins the three adjacent guard states directly.
  - OpenTelemetry output bytes change by removal of a conditional `nodePort`
    definition from one interned union, but the definition's remaining union
    alternatives preserve all 1,934 sampled verdicts. This re-encoding is
    disclosed rather than described as byte identity.
  - Production Rust is 61,404 LOC, delta -5. R3 has no separate frozen LOC
    estimate; the frozen Step 2 estimate is not reused for this remediation
    step.
- Adjudication evidence:
  - The guard composite live matrix uses real Helm 4.2.3 with
    `--skip-schema-validation`: deleted flag, present string flag, and active
    integer flag all render when `focus=selected`. The compiled R3 schema
    accepts the same cells. A compiled `jv` comparison independently shows
    R1 rejects the string cell at `/guard/deep/flag` while R3 accepts it.
  - The original focus lane remains explicit: deleted and numeric focus abort
    in Helm while the resource-bound schema accepts them as disclosed
    completeness loss; present-string focus renders and is accepted. R3 does
    not claim parity for the bounded-abstention lane.
  - No corpus acceptance flip is available for Helm disposition. Kyverno's
    three removed definitions are JSON-Schema-vacuous, and both the 121,059-
    probe corpus comparison and luup2 remain green.
  - Self-adversarial pass: the inherited `config` scope was eliminated rather
    than renamed; small-value preservation was measured at the boundary and on
    the pinned chart; the false-positive results from non-composite generic
    probes are recorded above rather than hidden; and the Kyverno byte change
    was reduced to its canonical semantic difference before adoption. No
    inherited fact, intent-only contract, false-rejection direction, or
    favorable ledger framing remains unexamined.

### Review dossier

- Width ownership and marker path: `rg -n
  'WidenedDependency|widen_large_bound_values|widen_large_bound_value_ref|BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT'
  crates/helm-schema-core/src/types.rs crates/helm-schema-ir/src/analysis_db.rs
  crates/helm-schema-ir/src/contract_signal_builder/contract_rows.rs`; shows
  the generic binding/dot calls, path-preserving marker creation, and the
  admission-only consumer.
- Focused representation proof: `cargo nextest run -p helm-schema-ir -E
  'test(bound_helper_config_budget_keeps_the_boundary_and_widens_its_sibling) |
  test(bound_helper_budget_widens_a_non_config_binding) |
  test(widened_dependencies_only_admit_paths_beneath_closed_roots)'`; exit 0,
  three tests pass.
- Pinned upstream input: `helm repo add open-telemetry
  https://open-telemetry.github.io/opentelemetry-helm-charts
  --repository-config target/arch-v3-wave2-r3-preflight/repository/repositories.yaml
  --repository-cache target/arch-v3-wave2-r3-preflight/cache`, then `helm pull
  open-telemetry/opentelemetry-collector --version 0.166.0 --untar
  --untardir target/arch-v3-wave2-r3-preflight/charts
  --repository-config target/arch-v3-wave2-r3-preflight/repository/repositories.yaml
  --repository-cache target/arch-v3-wave2-r3-preflight/cache`; both exit 0.
- Bounded-runtime measurement: `/usr/bin/time -v target/release/helm-schema
  target/arch-v3-wave2-r3-preflight/charts/opentelemetry-collector --output
  target/arch-v3-wave2-r3-preflight/otel-r3-candidate.schema.json`; exit 0,
  elapsed 2.34 seconds and maximum RSS 91,616 KiB. The same command with the
  `b6d9c6f` binary writes `otel-r1-baseline.schema.json` in 2.27 seconds and
  88,616 KiB.
- Upstream acceptance comparison: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-preflight/tmp
  SCHEMA_ACCEPTANCE_EXTERNAL_CHART=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-preflight/charts/opentelemetry-collector
  SCHEMA_ACCEPTANCE_BASELINE_SCHEMA=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-preflight/otel-r1-baseline.schema.json
  SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-preflight/otel-r3-candidate.schema.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(external_schema_pair_flips_are_helm_adjudicated)' --run-ignored
  ignored-only --no-capture`; exit 0, 1,934 probes and zero flips.
- Composite schema control: `cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(structural_helper_widening_abstains_in_all_adjacent_input_states)'`;
  exit 0, six schema cells pass.
- Composite live control: `cargo nextest run -P integration -p helm-schema
  --test schema_emission_profile_live -E
  'test(replay_structural_helper_widening_matrix_against_helm)'
  --run-ignored ignored-only --no-capture`; exit 0, six Helm cells pass.
- Old/new guard tooth: after generating the structural-helper schema with the
  `b6d9c6f` and R3 binaries over defaults containing `focus: selected`, run
  `jv target/arch-v3-wave2-r3-preflight/structural-helper-focus-r1.schema.json
  target/arch-v3-wave2-r3-preflight/guard-string-instance.json`; exit 1 at
  `/guard/deep/flag` (want integer). The same command with
  `structural-helper-focus-r3.schema.json` exits 0.
- Kyverno semantic diff: `LC_ALL=C comm -3 <(jq -S -c '."$defs"[]'
  target/arch-v3-wave2-r3-preflight/kyverno-baseline.schema.json | LC_ALL=C
  sort) <(jq -S -c '."$defs"[]'
  testdata/chart-corpus-schemas/kyverno.schema.json | LC_ALL=C sort)`; prints
  only three baseline-side vacuous open-object definitions and no
  candidate-side definition.
- Clean schema dump proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-final-dump
  SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
  helm-schema-gen -p helm-schema-cli -p helm-schema -E
  'test(schema_fixtures_match) | binary(chart_corpus) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) |
  binary(final_output_policy)'`; exit 0, 62 tests pass and 84 artifacts are
  written.
- Clean IR dump proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-final-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
  helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; exit 0,
  one test passes and 18 artifacts are written byte-identically.
- Full-depth acceptance proof: `TMPDIR=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=b6d9c6f
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-final-dump
  SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/arch-v3-wave2-r3-probe-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
  --test schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`; exit 0, 60 lanes and 121,059
  probes yield zero flips.
- Machine accounting: `jq '{baseline_ref, helm_adjudication, totals: {base:
  (.charts | map(.base_emitted) | add), third_level: (.charts |
  map(.third_level_emitted) | add), guard_pairs: (.charts |
  map(.guard_pairs_emitted) | add), composite_pairs: (.charts |
  map(.composite_pairs_emitted) | add), base_dropped: (.charts |
  map(.base_dropped) | add), third_level_dropped: (.charts |
  map(.third_level_dropped) | add)}}'
  target/arch-v3-wave2-r3-probe-coverage.json`; reports baseline `b6d9c6f`,
  live adjudication enabled, zero flips, zero accepted-abort cells against
  allowance zero, 112,260 base probes, 7,465 third-level probes, 427 guard
  pairs, 240 composite pairs, and no base or third-level drops.
- Hermetic controls: `cargo nextest run -P integration -p helm-schema --test
  schema_emission_profiles -E
  'test(current_profiles_obey_monotonicity_and_semantic_controls) |
  test(temporal_wrapper_pairwise_matrix_is_monotone) |
  test(probe_coverage_validation_rejects_synthetic_truncation) |
  test(helm_adjudication_validation_rejects_unregistered_accepted_abort) |
  test(guard_battery_synthesizes_composite_guard_and_payload_states) |
  test(structural_helper_widening_abstains_in_all_adjacent_input_states)'`;
  exit 0, six tests pass.
- Frozen-reference checks: `git diff --exit-code 44aa758 --
  plan/architecture-review-v3.md plan/schema-emission-profiles.md`; exit 0.
  `git diff --exit-code 5ef11aa --
  plan/architecture-review-v3-wave2.md`; exit 0.
- Gates on the final R3 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all three ast-grep checks pass.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets pass with zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,230 tests pass and none are
    skipped.
  - `task test:integration`: exit 0; 568 tests pass and 24 are skipped.
  - `task test:all`: exit 0; 1,802 tests pass and 24 are skipped, including
    the live-network lane.
  - Downstream install: `cargo install --path
    ./crates/helm-schema-cli/`; exit 0.
  - Downstream luup2: `task -t
    /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`; exit 0 across all 32 charts.
  - `task tokei:core`: exit 0; 61,404 production Rust LOC, delta -5.
- Measured production LOC delta: -5 (61,409 to 61,404).
- Commit: pending.
