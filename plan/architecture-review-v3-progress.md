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

- Status: landed; commit pending.
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
- Commit: pending.

## Step 3a — pure builder split

- Status: pending.
- Acceptance baseline: pending.
- Measured results: pending.
- Deviations: pending.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 3b — builder wrapper deletion

- Status: pending.
- Acceptance baseline: pending.
- Measured results: pending.
- Deviations: pending.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 4 — unified call invocation

- Status: pending.
- Acceptance baseline: pending.
- Measured results: pending.
- Deviations: pending.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 5 — typed function semantics

- Status: pending.
- Acceptance baseline: pending.
- Measured results: pending.
- Deviations: pending.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 6a — typed selection reachability with adapters

- Status: pending.
- Acceptance baseline: pending.
- Measured results: pending.
- Deviations: pending.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.
