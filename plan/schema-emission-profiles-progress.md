# Schema emission profiles implementation progress

Reference: `plan/schema-emission-profiles.md` v2.6 (frozen).

## Open decision points

### Lean contract

- Default: proceed with the recommended middle-point contract: mandatory facts and local
  conditionals remain enabled; root-anchored conditionals, kind partitions, and terminal clauses
  are disabled.
- Status: preliminary default adopted in Step 2. The exact Temporal preset is
  below the size budget, its median strict-lint time is 19.31 seconds, and no
  monotonicity-law failure occurred. Step 3 must reconfirm this decision after
  canonical emission and the late reachability prune.
- Veto window: open through the Step 3 reconfirmation.

### Temporal migration

- Default: migrate the chart-local integration to `helm-schema.yaml` with `profile: lean` and
  `emission.local-conditionals: off`, removing the CLI profile flag.
- Status: default selected by the Step 2 measurements. The downstream
  `HELM_SCHEMA_OPTIONS`/`helm-schema.yaml` mutation is intentionally deferred
  to Step 4, when the config loader exists; applying it earlier would remove
  the only operative selector while the new file is still ignored.
- Veto window: open until the Step 4 Temporal config fixture ships.

## Step -1 — round-58 review-findings closure

- Status: landed.
- Measured results:
  - Ten focused IR/generator/chart regressions pass.
  - Eight Helm 4.2.3 microchart adjudications pass with
    `--skip-schema-validation`.
  - One clean integration-profile dump wrote all 55 corpus schemas; 54
    fixtures changed and `common` remained byte-identical.
  - One clean generator-fixture dump wrote all 20 schema candidates; 17
    fixtures changed for the same shared integer-token grammar.
  - The compiled Rust prober checked 17,401 coalesced documents at the three
    required granularities and found two flips: one loosening and one
    tightening.
  - Every adopted corpus fixture is byte-identical to the dump used by the
    prober.
  - Production Rust LOC moved from 58,057 to 58,107 (`+50`) across the
    prerequisite fixes and shared escaper consolidation.
- Deviations:
  - The review shorthand “`else with` is live” was narrowed to the grammar
    Helm actually accepts: `else with` continues a `with` chain. The
    regression exercises that implementation path with live syntax under
    Helm 4.2.3 / Go 1.26.5.
  - The NATS tightening does not make `helm template` exit nonzero. Helm's
    nested `fromYaml` conversion renders an `Error` object into the container
    slot; the provider schema rejects that rendered object for missing
    `name` and carrying the unexpected `Error` member.
  - The first whole-global watch-item implementation also copied sole-path
    absence/fail captures to every source. The downstream cert-manager
    wrapper proved that unsound: it rejected an absent parent `global` even
    though the dependency's declared `global` default supplies the operand.
    The final patch projects range modes only; the wrapper defaults render
    and validate again.
- Adjudication evidence:
  - Jenkins `controller.jenkinsRef` null-deletion changes reject → accept;
    Helm renders because the selected sidecar-folder value makes the
    eagerly evaluated `printf` fallback dormant.
  - NATS `container.image: {}` changes accept → reject; deleting every
    declared image member makes the selected formatter output fail nested
    YAML conversion and produces a provider-invalid container.
  - The other fixture changes are shared definition encodings for the
    Go-compatible regex escaper and unified integer-token grammar; the
    three-granularity battery found no further acceptance flips.
- Review dossier:
  - Focused regressions and exact scalar partition:
    `cargo nextest run -p helm-schema-ir -p helm-schema-gen -E
    'test(/(dependency_global_projection_keeps_whole_global_range_modes|printf_plain_slot_contract_follows_default_operand_selection|invalid_kind_over_ternary_does_not_claim_one_arm_is_absent|encoded_printf_result_clears_plain_slot_operand_contracts|invalid_kind_requires_one_exact_subject_identity|else_with_local_join_does_not_treat_the_arm_as_unconditional|go_regex_literal_escaping_leaves_re2_hyphens_bare|quoted_negative_zero_keeps_falsy_pattern_truth_unknown|int_or_string_preimage_partitions_numeric_string_spellings|subchart_values_are_scoped_to_the_coalesced_child_view)/)'`.
  - Finding-specific live Helm controls:
    `cargo nextest run -P integration -p helm-schema-gen --test
    round_58_review`.
  - Whole-chart flip pins:
    `cargo nextest run -P integration -p helm-schema-cli --test
    chart_reaudit -E
    'test(/(jenkins_dormant_printf_fallback_allows_a_deleted_operand|nats_selected_printf_operands_require_an_image_spelling)/)'`.
  - Clean corpus dump:
    `SCHEMA_DUMP=1 cargo nextest run -P integration -p helm-schema-cli
    --no-fail-fast -E 'binary(chart_corpus)'`. Before adoption this exits 100
    on expected equality mismatches after writing all 55 candidates.
  - Clean generator-fixture dump:
    `SCHEMA_DUMP=1 cargo nextest run -P integration -p helm-schema-gen
    --test corpus -E 'test(schema_fixtures_match)'`.
  - Compiled old/new prober setup:
    `mkdir -p /tmp/round57-fixtures-final`, then
    `git archive --format=tar
    --output=/tmp/round57-fixtures-final.tar HEAD
    testdata/chart-corpus-schemas`, then
    `tar -xf /tmp/round57-fixtures-final.tar -C
    /tmp/round57-fixtures-final`.
  - Compiled old/new prober:
    `cargo run --release --manifest-path
    /tmp/round58-schema-prober/Cargo.toml --
    /tmp/round57-fixtures-final/testdata/chart-corpus-schemas /tmp
    testdata/charts`. It reports `probes=17401 flips=2`.
  - Jenkins live flip:
    `helm template round58-jenkins testdata/charts/jenkins
    --skip-schema-validation --set controller.jenkinsRef=null`.
  - NATS live flip:
    `helm template round58-nats testdata/charts/nats
    --skip-schema-validation --set
    container.image.repository=null,container.image.tag=null,container.image.digest=null,container.image.fullImageName=null,container.image.pullPolicy=null,container.image.registry=null`.
  - Dependency-global fallback and downstream reproduction:
    `cargo nextest run -P integration -p helm-schema-cli --test cli -E
    'test(subchart_values_are_scoped_to_the_coalesced_child_view)'`, then
    `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    certmanager:check:local`.
  - Residual Rust-regex escapers:
    `rg -n 'regex::escape' crates` exits 1 with no matches.
  - Dump/adopt identity:
    `bash -c 'for dump in /tmp/helm-schema.cli.chart-corpus.*.schema.json;
    do chart=${dump#/tmp/helm-schema.cli.chart-corpus.};
    chart=${chart%.schema.json}; cmp -s "$dump"
    "testdata/chart-corpus-schemas/$chart.schema.json" || exit 1; done'`.
  - Generator-fixture adoption identity:
    `bash -c 'for dump in /tmp/helm-schema.*.schema.json; do
    stem=${dump#/tmp/helm-schema.}; stem=${stem%.schema.json};
    fixture_stem=$(echo "$stem" | tr ".-" "__");
    fixture="crates/helm-schema-gen/tests/fixtures/$fixture_stem.schema.json";
    if [ -f "$fixture" ]; then cmp -s "$dump" "$fixture" || exit 1;
    fi; done'`.
- Gates on this final pre-commit tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0.
  - `cargo nextest run --workspace`: exit 0.
  - `task test:integration`: exit 0.
  - `task test:all`: exit 0.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0.
  - `task tokei:core`: exit 0.
- Commit: `0164d77` (`fix(schema): close round-58 review findings`).

## Step 0a — harness and relax-host preflight

- Status: landed.
- Measured results:
  - The compiled Rust harness generates full and lean independently, compiles
    each schema once, and composes every sparse probe over defaults with
    null-deletion semantics. Its explicit coalesced `{}` probe deletes every
    declared key rather than standing for defaults.
  - The microchart lane checks 206 documents: defaults, the empty coalesced
    document, every JSON-type replacement at top and second level, empty
    member/item shapes, guard boundaries, pattern near-misses, and the 12
    semantic controls. The temporal lane checks defaults plus a 40-cell
    pairwise matrix. Both satisfy `accepts(full) ⊆ accepts(lean)`.
  - The hermetic controls cover retained tooth, intentionally removed tooth,
    and positive-control outcomes with `ValuesFileJson`, `Set`, and
    `SetString` transports. The live replay confirms all 12 against Helm and
    rendered-sink validation; seven validator-parity cases agree between the
    Rust validator and Helm's embedded validator.
  - The relax-host preflight passes: deleting the nil-safe member host is
    accepted by full, accepted by lean, and renders with Helm. No production
    patch was needed.
  - The pinned Temporal wrapper's compact, description-free current outputs
    measure 3,996,667 bytes / 142,903 objects for full and 43,626 bytes /
    2,166 objects for today's lean. Warm release generation took 16.52 s and
    11.26 s respectively; these are generation measurements, not the Step 2
    validator-compile veto measurement.
  - One clean 55-chart dump and one clean 20-candidate generator dump are
    byte-identical to the round-58 fixtures: no fixture flips.
- Deviations:
  - The downstream `common` dependency is intentionally absent from the
    minimal Temporal wrapper. It is a library consumed only by downstream
    wrapper-owned templates; this anchor has none, so the dependency would
    contribute no lowered facts. The omission and rationale are pinned in
    `corpus-integrity.yaml`.
  - Per the frozen representation/producer ordering, the unconditional-fail
    semantic control lands with the Step 1a.1 producer. Step 0a does not pin
    today's known omission as an expected full verdict.
- Adjudication evidence:
  - Wrong unconditional Deployment replicas render but fail the pinned
    Kubernetes provider; integer and coercible-string transports render and
    validate.
  - Deleting `requiredText` and the version pattern near-miss abort Helm.
  - A wrong dependency replica is dormant while the dependency condition is
    false and provider-invalid when true.
  - The ConfigMap branch rejects a Service spelling for `immutable`; the
    adjacent Service branch accepts the same spelling. An unknown dynamic
    kind remains `Unresolved` instead of becoming an accept/reject oracle.
  - The Temporal archive is exactly 0.62.0 with SHA-256
    `c2f01baeef60ed96335948640a8ac30fb49a10b906e20c259b92f81f2cba5c04`;
    its dependency lock and Helm-produced coalesced defaults carry their own
    recorded checksums.
- Review dossier:
  - Hermetic monotonicity and semantic controls:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles`.
  - Live Helm/provider replay and validator parity:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profile_live --run-ignored ignored-only`.
  - Relax-host preflight alone:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(lean_profile_keeps_nil_safe_host_relaxation)'`.
  - Temporal anchor render:
    `helm template profile-temporal
    testdata/charts/schema-emission-temporal-wrapper
    --skip-schema-validation`.
  - Archive/default integrity:
    `sha256sum
    testdata/charts/schema-emission-temporal-wrapper/charts/temporal-0.62.0.tgz
    testdata/charts/schema-emission-temporal-wrapper/Chart.lock
    testdata/charts/schema-emission-temporal-wrapper/coalesced-defaults.json`,
    then `cargo nextest run -P integration -p helm-schema-cli --test
    corpus_integrity`.
  - Pinned full baseline:
    `/usr/bin/time -p helm-schema
    testdata/charts/schema-emission-temporal-wrapper --profile full
    --strip-descriptions --compact --k8s-version
    v1.29.0-standalone-strict --strict-k8s-version
    --k8s-schema-cache-dir
    testdata/provider-bundle/kubernetes-json-schema-cache
    --crd-catalog-cache-dir
    testdata/provider-bundle/crds-catalog-cache --offline --output
    /tmp/schema-emission-temporal-full-pinned.json`.
  - Pinned lean baseline: the preceding command with `--profile lean` and
    output `/tmp/schema-emission-temporal-lean-pinned.json`.
  - Baseline byte/object counts: `wc -c
    /tmp/schema-emission-temporal-{full,lean}-pinned.json`, then
    `jq '[.. | objects] | length'` on each file.
  - Clean corpus dump: `SCHEMA_DUMP=1 cargo nextest run -P integration -p
    helm-schema-cli --no-fail-fast -E 'binary(chart_corpus)'`.
  - Clean generator dump: `SCHEMA_DUMP=1 cargo nextest run -P integration
    -p helm-schema-gen --test corpus -E 'test(schema_fixtures_match)'`.
  - No production change: `git diff --exit-code 0164d77 -- 'crates/*/src'`.
- Gates on the final Step 0a tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0.
  - `cargo nextest run --workspace`: exit 0.
  - `task test:integration`: exit 0.
  - `task test:all`: exit 0.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0.
  - `task tokei:core`: exit 0; production Rust remains 58,107 LOC (delta
    zero from the prerequisite commit).
- Commit: `286ac44` (`test(schema): add emission profile harness`).

## Step 1a — fact model, phase split, and reporting

- Status: landed.
- Measured results:
  - The fact-model, total selection table, report accounting, host/constraint
    phase split, local kind-partition chart, and eight-stage completion-pass
    monotonicity test are implemented on the working tree.
  - Five focused generator tests pass. The local kind-partition integration
    control passes and Helm 4.2.3 renders its default Deployment arm.
  - The 20 committed generator fixtures and all 55 committed chart-corpus
    fixtures remain byte-identical.
  - Legacy lean remains the operative Step 1a selector. The shadow
    decision-table projection reports 9 retained facts for the controls
    chart, 4 for the local-kind audit chart, and 6,859 for the Temporal
    wrapper. Every disagreement is projection-only; there are no facts legacy
    lean retains that the decision table drops. Ordered full-diff SHA-256 pins
    are asserted for all three artifacts.
  - One clean final-build dump wrote all 20 generator candidates and all 55
    chart-corpus schemas; every candidate was byte-identical to its committed
    fixture, so Step 1a has zero fixture flips.
  - The workspace unit gate ran 1,142 tests; the integration gate ran 539
    tests; the live-inclusive gate ran 1,685 tests. The downstream luup2 gate
    checked all 32 charts successfully.
  - Production Rust LOC moved from 58,107 to 58,850 (`+743`) for the fact
    model, reporting surface, completion-stage seam, and tests of private
    policy behavior.
- Deviations:
  - Disposition from Roman: there is no plan contradiction. `Mandatory ->
    always emit` is a projection invariant, and the projection does not become
    lean's operative selector until Step 2. Step 1a keeps the legacy lean gate
    authoritative, carries classification as data, and runs the decision
    table in shadow mode. The operative and projected accounting, plus every
    fact-level disagreement, travel in the same `EmissionReport`.
  - The Step 0a version-pattern control remains byte-for-byte and
    verdict-for-verdict unchanged. Its shadow-projection disagreement is
    pre-registered for Step 2 rather than adopted in this representation step.
- Adjudication evidence:
  - Helm 4.2.3 exits 1 for `version: v1` at the chart's explicit fail, so the
    unguarded pattern is not a false rejection. Step 2 may retain it as a
    Mandatory constraint after recategorizing the control.
  - The new local kind-partition defaults render as Deployment and the
    compiled schema controls prove Deployment/StatefulSet provider payloads
    stay anchored below `workload`.
- Review dossier:
  - Focused fact/report, policy-validity, local-anchor, and completion-stage
    tests: `cargo nextest run -p helm-schema-gen -E
    'test(emission_profiles::)'`.
  - Local kind-partition semantic control: `cargo nextest run -P integration
    -p helm-schema --test schema_emission_profiles -E
    'test(local_kind_partition_is_a_local_policy_fact)'`.
  - Local kind Helm render: `helm template step1a-local-kind
    testdata/charts/schema-emission-local-kind
    --skip-schema-validation`.
  - Byte-identical generator fixtures: `cargo nextest run -P integration -p
    helm-schema-gen --test corpus -E 'test(schema_fixtures_match)'`.
  - Byte-identical chart corpus: `cargo nextest run -P integration -p
    helm-schema-cli --test chart_corpus`.
  - Single clean final-build dump: `SCHEMA_DUMP=1 cargo nextest run -P
    integration -p helm-schema-gen --test corpus -E
    'test(schema_fixtures_match)'`, then `SCHEMA_DUMP=1 cargo nextest run -P
    integration -p helm-schema-cli --no-fail-fast -E
    'binary(chart_corpus)'`. Both report only exact matches.
  - Exact shadow-diff registry: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profiles -E
    'test(legacy_lean_reports_step_2_projection_differences)'`.
  - Current-behavior pin: `cargo nextest run -P integration -p helm-schema
    --test schema_emission_profiles -E
    'test(current_profiles_obey_monotonicity_and_semantic_controls)'`.
  - Version-pattern live adjudication: `helm template
    step2-preregister-pattern testdata/charts/schema-emission-controls
    --skip-schema-validation --set-string version=v1`; expected exit 1 with
    `version must be vMAJOR.MINOR`.
  - Exhaustive completion-stage law: `cargo nextest run -p helm-schema-gen -E
    'test(emission_profiles::every_completion_stage_is_monotone)'`.
- Gates on the final Step 1a tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,142 passed.
  - `task test:integration`: exit 0; 539 passed, 2 skipped.
  - `task test:all`: exit 0; 1,685 passed, 2 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 58,850 LOC (`+743` from
    Step 0a).
- Commit: `5a1b52e` (`refactor(schema): add emission fact model`).

## Step 1a.1 — unconditional termination producer

- Status: landed.
- Measured results:
  - An empty direct-fail conjunction now produces exactly one
    `Terminal::Always` fact. Full selects it and rejects all seven JSON-kind
    representatives; legacy lean and the shadow middle-point policy drop it.
  - The unconditional-fail chart is the corpus's 56th chart and is listed in
    the oracle-conditioned defaults rejection set. Its full schema is pinned
    by a complete equality fixture.
  - The full six-test hermetic profile suite passes both the monotonicity law
    and its retained-tooth / removed-tooth / positive-control assertions.
  - One clean final-build dump reports all 20 generator candidates and all 56
    chart schemas exact. No existing fixture changes; the only new fixture is
    the unconditional-fail chart.
  - Production Rust LOC moved from 58,850 to 58,853 (`+3`); the remaining
    implementation is test and fixture evidence.
- Deviations: none.
- Adjudication evidence:
  - Helm 4.2.3 exits 1 before rendering any manifest and reports `schema
    emission unconditional fail`. Because the chart itself rejects every
    values document, the full always-false schema is the correct contract.
  - Turning terminal clauses off accepts the declared defaults, which is a
    sound widening of the empty accepted language.
- Review dossier:
  - IR producer and full/lean semantic fixture:
    `cargo nextest run -p helm-schema-ir -E
    'test(unconditional_fail_produces_an_always_terminal_clause)'`, then
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(unconditional_fail_is_an_independent_terminal_tooth)'`.
  - Both harness obligations on the behavior-changing tree:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles`.
  - Live Helm replay of the new control: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profile_live --run-ignored ignored-only
    -E 'test(replay_unconditional_fail_against_helm)'`.
  - Direct Helm reproducer: `helm template unconditional-fail
    testdata/charts/schema-emission-unconditional-fail
    --skip-schema-validation`; expected exit 1 with the pinned control
    message.
  - Oracle-conditioned corpus floor and full-schema equality:
    `cargo nextest run -P integration -p helm-schema-cli --test chart_corpus
    -E 'test(schema_emission_unconditional_fail)'`.
  - Clean generator dump: `SCHEMA_DUMP=1 cargo nextest run -P integration -p
    helm-schema-gen --test corpus -E 'test(schema_fixtures_match)'`.
  - Clean 56-chart dump: `SCHEMA_DUMP=1 cargo nextest run -P integration -p
    helm-schema-cli --no-fail-fast -E 'binary(chart_corpus)'`.
- Gates on the final Step 1a.1 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,143 passed.
  - `task test:integration`: exit 0; 541 passed, 3 skipped.
  - `task test:all`: exit 0; 1,688 passed, 3 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 58,853 LOC (`+3` from Step
    1a).
- Commit: `6f207a2` (`feat(schema): emit unconditional terminals`).

## Step 1b — immutable lowering plan and multi-policy projection

- Status: landed.
- Measured results:
  - `LoweredEmissionPlan` now owns the cloned contract signals, composed and
    refill documents, descriptions, provider-resolved base paths, immutable
    tagged conjuncts, and the support plan. `ProjectedTree` retains the schema
    tree and fact accounting until the policy-free completion tail crosses to
    `CompletedGeneratedSchema`.
  - Full and standard-policy projections reuse one plan. Projection-order
    equality and the fact floors pass; a counting provider confirms that plan
    construction performs all provider access and neither projection re-enters
    it.
  - Host preparation, conditional base ownership, accepted-root refill, and
    default-fill exclusions are computed from all facts before projection.
    Every projection applies the same immutable support plan to fresh mutable
    state. Source-aware provider extraction now sees only selected cloned
    candidate facts and runs after the selection boundary.
  - Full remains byte-identical across all 20 generator fixtures and all 56
    chart-corpus fixtures. An old-vs-new replay of compact lean output across
    all 56 corpus charts also found zero byte differences. Equality satisfies
    the widening law; the existing dedicated nil-safe-host control remains
    accepted by full and lean and renders under Helm.
  - The unreachable-definition preflight found 66 generator-owned provider
    definitions with no incoming `$ref` in 14 committed full schemas. Pruning
    therefore cannot be byte-identical and is deferred to the plan's separate
    VE branch after this Step 1b commit; no pruning or fixture churn is hidden
    here.
- Deviations:
  - The plan anticipated lean changing where selected-only support mutations
    had vanished. The Step 0a preflight already showed the known host case was
    open under legacy lean, and the complete 56-chart old/new replay found no
    serialized delta. Step 1b still removes the architectural hazard by
    computing support from all facts; it does not manufacture a fixture flip.
  - Because the early reachability prune changes 14 full fixtures, the
    plan-prescribed separate VE branch is taken. The caller-private `$defs`
    ownership contract will land with that prune rather than preceding an
    implementation that does not yet remove definitions.
- Adjudication evidence:
  - There are no semantic fixture flips to adopt. The hermetic monotonicity
    battery and all three semantic-control categories pass, including the
    nil-safe host deletion. The live lane replays all controls under Helm
    4.2.3 and the pinned provider boundary with no disagreement.
  - The clean final-build dumps are exact: 20 generator candidates and 56
    chart-corpus candidates match their committed full fixtures.
- Review dossier:
  - One-artifact projection, order independence, hard fact floors, completion
    monotonicity, and provider-access boundary: `cargo nextest run -p
    helm-schema-gen -E
    'test(/(one_plan_projections_obey_floors_and_ignore_projection_order|completion_passes_preserve_profile_monotonicity|projections_never_reenter_the_provider)/)'`.
  - Hermetic monotonicity and three-category controls: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profiles`.
  - Live Helm/provider replay: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profile_live --run-ignored
    ignored-only`.
  - Clean generator dump: `SCHEMA_DUMP=1 cargo nextest run -P integration -p
    helm-schema-gen --test corpus -E 'test(schema_fixtures_match)'`.
  - Clean 56-chart dump: `SCHEMA_DUMP=1 cargo nextest run -P integration -p
    helm-schema-cli --no-fail-fast -E 'binary(chart_corpus)'`.
  - Full-fixture equality after the clean dump: `cargo nextest run -P
    integration -p helm-schema-gen --test corpus -E
    'test(schema_fixtures_match)'`, then `cargo nextest run -P integration -p
    helm-schema-cli --test chart_corpus`.
  - Old/new lean corpus replay: build `6f207a2` and the working tree's
    `helm-schema` binaries, emit `--offline --exclude-tests --profile lean
    --compact` for every basename in `testdata/chart-corpus-schemas`, then
    `bash -c 'for old in /tmp/schema-emission-step1b/lean-old/*.json; do
    name=${old##*/}; cmp -s "$old"
    "/tmp/schema-emission-step1b/lean-new/$name" || exit 1; done'`.
  - Early-prune failure branch: `bash -c 'unused=0; files=0; for schema in
    testdata/chart-corpus-schemas/*.schema.json; do count=$(jq -r '\''def
    refs: [.. | objects | .["$ref"]? | select(type == "string" and
    startswith("#/$defs/")) | sub("^#/\\$defs/"; "") | split("/")[0]];
    (."$defs" // {}) as $defs | (refs) as $refs | [$defs | keys[] |
    select(startswith("provider")) | select(. as $name | ($refs |
    index($name) | not))] | length'\'' "$schema"); unused=$((unused +
    count)); if [ "$count" -gt 0 ]; then files=$((files + 1)); fi; done;
    echo "schemas_with_unreferenced_provider_defs=$files
    unreferenced_provider_defs=$unused"'`; it reports `14` and `66`.
- Gates on the final Step 1b tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,145 passed.
  - `task test:integration`: exit 0; 541 passed, 3 skipped.
  - `task test:all`: exit 0; 1,690 passed, 3 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 59,029 LOC (`+176` from
    Step 1a.1).
- Commit: `5cebd97` (`refactor(schema): add immutable emission plan`).

## Step 1b VE — early provider-definition pruning

- Status: landed.
- Measured results:
  - The plan's Step 1b failure branch is implemented as a separate
    validation-equivalence round. Reachability starts from `$ref` sites in the
    projected typed tree and closes transitively through source-aware provider
    definition bodies; only the generator-owned definition map is eligible for
    removal.
  - The clean dump changes 15 fixtures: 14 chart-corpus schemas and the
    `signoz_zookeeper_statefulset` generator fixture. It removes 68 unreachable
    provider definitions and 933,939 serialized bytes. Every candidate's
    non-`$defs` document equals its baseline, its `$defs` keys are a strict
    subset, and every retained definition is equal.
  - The compiled Rust differential prober checked 114,208 coalesced documents
    across all 56 chart-corpus schemas at the required top-level deletion,
    second-level deletion, and empty member/item granularities. It found zero
    acceptance flips.
  - The public override-loading and CLI documentation now state the ownership
    boundary: caller overrides must carry their own definitions and may not
    reference generator-private `$defs` names.
- Deviations:
  - The Step 1b preflight counted 66 unreachable definitions in the final
    chart-corpus schemas. Applying the same early-prune boundary also removes
    two definitions from one lower-level generator fixture, so the complete VE
    round records 68 removals across 15 fixtures.
- Adjudication evidence:
  - The compiled old/new battery has zero verdict changes, so there is no
    TIGHTEN or LOOSEN fixture flip to adjudicate individually. The live lane
    replays the semantic controls under Helm 4.2.3 and the pinned provider
    boundary with no disagreement.
  - The graph audit independently proves the serialized churn consists only
    of unreachable generator-owned definition deletion; no validation keyword
    or reachable definition changes.
- Review dossier:
  - Transitive reachability unit control: `cargo nextest run -p
    helm-schema-gen -E
    'test(unreachable_provider_definitions_are_pruned_transitively)'`.
  - Exact old/new acceptance battery: `SCHEMA_ACCEPTANCE_BASELINE_REF=5cebd97
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(early_provider_definition_pruning_is_acceptance_equivalent)'
    --run-ignored ignored-only --no-capture`; it reports `charts_checked=56
    probes_checked=114208 flips=0`.
  - Hermetic monotonicity and three-category controls: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profiles`.
  - Live Helm/provider replay: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profile_live --run-ignored
    ignored-only`.
  - Clean generator dump: `SCHEMA_DUMP=1
    TMPDIR=/tmp/helm-schema-prune-dump-5cebd97 cargo nextest run -P
    integration -p helm-schema-gen --test corpus -E
    'test(schema_fixtures_match)'`.
  - Clean corpus dump: `SCHEMA_DUMP=1
    TMPDIR=/tmp/helm-schema-prune-dump-5cebd97 cargo nextest run -P
    integration -p helm-schema-cli --no-fail-fast -E
    'binary(chart_corpus)'`; before adoption it writes all 56 candidates and
    exits 100 on the 14 expected equality mismatches.
  - Adopted fixture equality: `cargo nextest run -P integration -p
    helm-schema-gen --test corpus -E 'test(schema_fixtures_match)'`, then
    `cargo nextest run -P integration -p helm-schema-cli --no-fail-fast -E
    'binary(chart_corpus)'`.
- Gates on the final Step 1b VE tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,146 passed.
  - `task test:integration`: exit 0; 541 passed, 4 skipped.
  - `task test:all`: exit 0; 1,691 passed, 4 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 59,115 LOC (`+86` from
    Step 1b).
- Commit: `d0ab756` (`refactor(schema): prune unreachable provider definitions`).

## Step 2 — lean policy and annotations

- Status: landed; commit pending.
- Measured results:
  - Pre-registered shadow-projection input from Step 1a: controls 9 facts
    (`3594d70cd4304790641f1d5ee12a157da54a9215846bb181260f4f79b6f271e7`),
    local-kind 4 facts
    (`5b51fa618edae0059e8a9dfac20091d7228a4a6b76af01912ce2c6cfa27dd255`),
    Temporal 6,859 facts
    (`602e49d724de9477fb87081089e29a49e6e28b0b22474dd4ca5e6b93d85c38ae`).
    All are projection-only. The report contains the full ordered identity of
    every fact (index, class/origin, target, class digest, payload digest).
  - The decision-table selector is now operative. Full selects every fact;
    lean selects every Mandatory and OrdinaryLocal fact and selects zero
    OrdinaryRoot, kind-partition, or terminal facts. The temporary legacy
    selector, shadow accounting, and difference identities are deleted after
    serving their Step 1a pre-registration purpose.
  - On the Temporal anchor, full lowers 10 Mandatory, 4,641 OrdinaryRoot,
    6,849 OrdinaryLocal, and 693 TerminalGuarded facts. Middle lean retains
    the 10 Mandatory and all 6,849 OrdinaryLocal facts, dropping the other
    5,334. Its description-free compact final output is 1,599,019 shipped
    bytes and 55,589 objects; full is 3,961,838 bytes and 141,839 objects.
  - Strict Helm lint over the exact middle schema took 22.53, 19.31, and
    14.64 seconds (median 19.31); the same wrapper without a schema took 0.07
    seconds in all three runs. `jv` 0.7.0 compile took 6.59, 6.67, and 6.58
    seconds. The preset is below the 4.5 MiB budget and does not materially
    exceed the plan's 20–30 second veto envelope.
  - The compiled transition prober checks 476 default-composed/null-deletion
    documents across the four lean anchors. It finds 45 legacy-accepts /
    middle-rejects transitions and zero inverse flips. The ordinary and
    Temporal pairwise batteries prove `accepts(full) ⊆ accepts(lean)`.
  - Four complete lean equality fixtures now live only under
    `testdata/emission-profile-schemas/lean/`. The 56 full corpus schemas and
    all 20 generator fixtures remain byte-identical to Step 1b VE.
  - Final output carries a deterministic versioned policy annotation after
    overrides, reference transport, description stripping, and minification.
    `GeneratedSchema` remains unannotated. Four complete final-output fixtures
    cover full/lean metadata, caller-key overwrite, deterministic identity,
    and the draft-07 Boolean-root wrapper.
  - Override replacement intent is captured out of band on initial read and
    retained as normalized JSON pointers through every reference mode. Merge
    and the application-ordered SHA-256 override digest consume the same
    representation. Caller-authored `$ref-replace` is ordinary schema data;
    non-schema override roots are rejected before preparation.
- Deviations:
  - The per-knob Step 2 measurement is exact at the fact-selection boundary:
    the Temporal class counts above register each knob's fact delta. Serialized
    per-knob timing/size comparisons remain part of the dedicated Step 5
    benchmark because the multi-policy projection API remains crate-private,
    as required, and no public experimental knob was introduced for
    measurement.
  - The Temporal migration decision is executed as a selected default here,
    but its downstream file mutation waits for Step 4's config loader. Applying
    the file now would be inert while removing the operative CLI profile.
- Adjudication evidence:
  - Every one of the 45 transition tightenings was replayed at template level
    with Helm 4.2.3 and `--skip-schema-validation`: 34 fail during rendering
    and the remaining 11 render but fail strict Kubernetes 1.29 provider
    validation. No adopted tightening accepts at both oracle boundaries.
  - The pre-registered `version: v1` family is among the Helm failures; the
    chart reports `version must be vMAJOR.MINOR`, confirming the Mandatory
    pattern is not a false rejection.
  - The three-category hermetic controls and their live replay pass after the
    selector flip. Retained dependency-local replica typing rejects
    non-coercible spellings; removed root/kind/terminal teeth widen only their
    registered cases; positive controls continue to render and validate.
  - Helm's embedded validator and the compiled Rust validator agree on the
    Boolean wrappers, `if`/`then`, type arrays, internal refs, and extension
    annotations touched by this round.
- Review dossier:
  - Operative policy floors and ordinary/Temporal monotonicity:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E 'not test(/(early_provider_definition_pruning_is_acceptance_equivalent|middle_lean_transition_has_only_preregistered_tightenings|temporal_middle_policy_measurements)/)'`.
  - Exact transition prober plus all 45 live adjudications:
    `LEGACY_LEAN_SCHEMA_DIR=/tmp/helm-schema-step2-lean-old
    ADJUDICATE_WITH_HELM=1 cargo nextest run -P integration -p helm-schema
    --test schema_emission_profiles -E
    'test(middle_lean_transition_has_only_preregistered_tightenings)'
    --run-ignored ignored-only --no-capture`; it reports
    `probes_checked=476 tightenings=45 inverse=0` plus one `HELM_REJECT` or
    `PROVIDER_REJECT` line for every tightening.
  - Exact Temporal fact and generated-document deltas:
    `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E 'test(temporal_middle_policy_measurements)'
    --run-ignored ignored-only --no-capture`.
  - Version-pattern direct replay: `helm template
    step2-preregister-pattern testdata/charts/schema-emission-controls
    --skip-schema-validation --set-string version=v1`; expected exit 1.
  - Exact compact output and size: `target/debug/helm-schema
    testdata/charts/schema-emission-temporal-wrapper --profile lean
    --exclude-tests --strip-descriptions --compact --k8s-version
    v1.29.0-standalone-strict --strict-k8s-version
    --k8s-schema-cache-dir
    testdata/provider-bundle/kubernetes-json-schema-cache
    --crd-catalog-cache-dir
    testdata/provider-bundle/crds-catalog-cache --offline --output
    /tmp/helm-schema-step2-temporal-middle.schema.json`, then `wc -c` and
    `jq '[.. | objects] | length'` on the output.
  - Helm compile timing: copy the wrapper to
    `/tmp/helm-schema-step2-temporal-lint`, place the exact compact output at
    `values.schema.json`, create the otherwise-empty `templates/` directory,
    then run `/usr/bin/time -f
    'elapsed=%e user=%U system=%S max_rss_kb=%M exit=%x' helm lint
    /tmp/helm-schema-step2-temporal-lint --strict` three times. Move the schema
    aside and repeat for the no-schema baseline.
  - `jv` compile timing: `/usr/bin/time -f
    'elapsed=%e user=%U system=%S max_rss_kb=%M exit=%x' jv -q
    /tmp/helm-schema-step2-temporal-middle.schema.json` three times.
  - Override-intent and final-output contracts: `cargo nextest run -p
    helm-schema -E 'test(/(prepared_override_identity_includes_replacement_intent|caller_authored_ref_replace_keys_do_not_collide_with_merge_intent|override_loader_rejects_non_schema_roots|final_policy_annotation_is_deterministic_and_overwrites_caller_key|boolean_roots_are_wrapped_without_changing_acceptance)/)'`, then
    `cargo nextest run -P integration -p helm-schema --test
    final_output_policy`.
  - Single clean final-build dump for all semantic lanes:
    `TMPDIR=/tmp/helm-schema-step2-final-dump SCHEMA_DUMP=1 cargo nextest
    run -P integration --no-fail-fast -p helm-schema-gen -p
    helm-schema-cli -p helm-schema -E 'test(schema_fixtures_match) |
    binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and all 84 dump files are
    from the same final build.
  - Adopted separate-lane equality: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profiles -E
    'test(lean_profile_schemas_match_their_separate_fixture_lane)'`, then
    `cargo nextest run -P integration -p helm-schema --test
    final_output_policy`.
  - Full semantic lanes untouched: `git diff --exit-code d0ab756 --
    testdata/chart-corpus-schemas crates/helm-schema-gen/tests/fixtures`.
- Gates on the final Step 2 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,153 passed.
  - `task test:integration`: exit 0; 545 passed, 6 skipped.
  - `task test:all`: exit 0; 1,702 passed, 6 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 59,242 LOC (`+127` from
    Step 1b VE).
- Commit: pending.

## Step 3 — canonical emission and final output metrics

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 4 — configuration surface

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 5 — benchmark and documentation

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.
