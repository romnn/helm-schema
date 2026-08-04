# Schema emission profiles implementation progress

Reference: `plan/schema-emission-profiles.md` v2.6 (frozen).

## Open decision points

### Lean contract

- Default: proceed with the recommended middle-point contract: mandatory facts and local
  conditionals remain enabled; root-anchored conditionals, kind partitions, and terminal clauses
  are disabled.
- Status: preliminary default adopted in Step 2. The exact Temporal preset is
  below the size budget, its final-phase strict-lint median is 15.00 seconds,
  and no monotonicity-law failure occurred. Step 3 reconfirms the middle point
  after canonical emission and the late reachability prune.
- Veto window: closed by the Step 3 fixture commit after no measurement trigger
  and no user veto. Changing the preset now requires an explicit follow-up.

### Temporal migration

- Default: migrate the chart-local integration to `helm-schema.yaml` with `profile: lean` and
  `emission.local-conditionals: off`, removing the CLI profile flag.
- Status: executed in Step 4. The downstream Temporal chart now carries root
  `helm-schema.yaml` version 1 with `profile: lean` and
  `emission.local-conditionals: off`; `HELM_SCHEMA_OPTIONS` no longer carries
  `--profile lean`.
- Veto window: closed when the Step 4 config surface and exact Temporal
  precedence tests shipped without a measurement trigger or user veto.

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
    The final patch projects both whole-global range modes and `RangeInput`
    fail captures to every live source. The current iterable-domain fail
    requirement admits absent/null inputs, so the wrapper's child default
    still renders and validates. The regression must remain green before any
    future whole-global capture kind becomes absence-strict.
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

- Status: landed.
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
- Commit: `d67020f` (`feat(schema): adopt middle lean emission policy`).

## Step 3 — canonical emission and final output metrics

- Status: landed.
- Measured results:
  - Canonicalization has the total top-level result required by the frozen
    plan: `Applied(Emitted | Redundant) | NotApplicable`; the caller retains
    the original Mandatory carrier on `NotApplicable`. On the Temporal anchor,
    full and lean each apply 6 Mandatory constraints directly, prove 0
    redundant, and retain 4 fallback carriers. Mandatory accounting is
    emitted 6 / equivalent 0 / redundant 0 / fallback 4 under both policies.
  - Constructor-owned kind unions now serialize as deterministic JSON Schema
    type arrays. The exhaustive seven-kind power-set test proves them
    acceptance-equivalent to their former `anyOf` arms, while the provider
    payload test proves foreign type unions are not rewritten.
  - The final Temporal full document is 5,631,055 bytes / 147,135 objects /
    1,541 condition nodes / 1,504 unique conditions / 828 unique `then`
    payloads. Middle lean is 3,057,894 bytes / 59,358 objects / 422 condition
    nodes / 403 unique conditions / 410 unique `then` payloads. Bytes include
    the trailing newline from the exact compact `write_schema_json` output.
  - The lean output is below the 4,718,592-byte budget. Strict Helm lint over
    that exact file took 23.36, 15.00, and 14.39 seconds (median 15.00), so the
    final-phase lean veto does not trigger.
  - One final-build dump produced 84 artifacts: 56 full corpus, 20 generator,
    4 lean, and 4 final-output fixtures. Every dump artifact is byte-identical
    to its adopted fixture. Across the 60 full/lean semantic schemas, total
    serialized fixture bytes move 124,622,225 → 124,605,827 (`-16,398`) and
    the net root definition count moves 26,563 → 26,519 (`-44`) after all
    canonicalization, minification, and ownership-aware pruning effects.
  - The compiled baseline/worktree prober checks 114,684 default-composed,
    null-deletion documents across the 60 full and lean schemas and reports
    zero acceptance flips. The seven-test hermetic law/control lane and the
    three-test live Helm/provider/parity lane also pass.
- Deviations:
  - No frozen-plan contract was reinterpreted. Redundancy is represented as an
    `Applied` disposition rather than a third top-level canonicalization
    result, preserving the plan's exact `Applied | NotApplicable(original)`
    boundary while retaining separate report counts.
  - Canonical object conjunctions exposed an existing phase interaction:
    missing-default backfill treated the direct object conjunct as an
    alternative carrier and could accept a scalar Temporal replica. The
    insertion path now recognizes only a proven object-only schema containing
    an exact generator object conjunct and fills that same lane. Ordinary
    foreign object roots retain their legacy openness; the focused Temporal
    regression and zero-flip battery pin the distinction.
  - The integration gate found four full-equality expectations outside the
    84-artifact dump filter: three inline packaged/child-view fixtures and the
    CLI full-fixture schema. Their changes are only the exhaustively proven
    constructor/canonical rewrites. The CLI candidate was produced in its own
    clean dump, all four complete equality tests pass, and their existing
    semantic controls remain green. No corpus artifact or prober verdict
    changed.
- Adjudication evidence:
  - No TIGHTEN or LOOSEN exists in the 114,684-probe fixture battery, so Step 3
    has no new per-flip Helm verdict to adopt. The full live control replay
    nevertheless passes under Helm 4.2.3 and the pinned Kubernetes 1.29
    provider boundary, including the unconditional-fail control and all
    touched-validator constructs.
  - Exhaustive compiled-validator tests prove the canonical property-slot,
    presence, not-null, and type-union rewrites. Missing closed-root slots
    return `NotApplicable` without mutation; already non-null slots are
    proven redundant; provider-owned payloads remain byte-structural inputs.
  - Late reachability closes transitive `$defs` and `definitions` references,
    decodes JSON Pointer names, and conservatively follows nested scopes. It
    removes only unchanged definitions captured from generator output;
    caller-added and caller-modified definitions survive even when dead.
  - Final override replacement and fully inlined transport each have an
    end-to-end regression proving that their orphaned generator definition is
    removed after the final transform ordering.
- Review dossier:
  - Canonical algebra and constructor-owned type-union exhaustive tests:
    `cargo nextest run -p helm-schema-gen canonical`.
  - Late-prune ownership, reachability, and validation-equivalence tests:
    `cargo nextest run -p helm-schema -E
    'test(/(reachability|fully_inline|caller_overwrite)/)'`.
  - Exact final-output and fact measurements:
    `TMPDIR=/home/roman/dev/helm-schema-step3-prober-scratch cargo nextest
    run -P integration -p helm-schema --test schema_emission_profiles
    --run-ignored ignored-only --nocapture temporal_middle_policy_measurements`.
  - Baseline-to-adopted compiled acceptance battery:
    `SCHEMA_ACCEPTANCE_BASELINE_REF=d67020f
    TMPDIR=/home/roman/dev/helm-schema-step3-prober-scratch cargo nextest run
    -P integration -p helm-schema --test schema_emission_profiles
    --run-ignored ignored-only --nocapture
    early_provider_definition_pruning_is_acceptance_equivalent`; it reports
    `charts_checked=60 probes_checked=114684 flips=0`.
  - Hermetic monotonicity and three-category controls:
    `TMPDIR=/home/roman/dev/helm-schema-step3-prober-scratch cargo nextest run
    -P integration -p helm-schema --test schema_emission_profiles -E
    'not test(/(early_provider_definition_pruning_is_acceptance_equivalent|middle_lean_transition_has_only_preregistered_tightenings|temporal_middle_policy_measurements)/)'`.
  - Live Helm/provider controls and validator parity:
    `TMPDIR=/home/roman/dev/helm-schema-step3-prober-scratch cargo nextest run
    -P integration -p helm-schema --test schema_emission_profile_live
    --run-ignored ignored-only`.
  - Single clean final-build dump:
    `mkdir -p /home/roman/dev/helm-schema-step3-final-dump-20260803-d`, then
    `TMPDIR=/home/roman/dev/helm-schema-step3-final-dump-20260803-d
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and the directory contains
    exactly 84 JSON artifacts.
  - Exact final lean output: `cargo run --quiet -p helm-schema-cli --
    testdata/charts/schema-emission-temporal-wrapper --profile lean
    --exclude-tests --compact --k8s-version v1.29.0-standalone-strict
    --strict-k8s-version --k8s-schema-cache-dir
    testdata/provider-bundle/kubernetes-json-schema-cache
    --crd-catalog-cache-dir
    testdata/provider-bundle/crds-catalog-cache --offline --output
    /home/roman/dev/helm-schema-step3-lint/chart/values.schema.json`, followed
    by `wc -c` and `jq '[.. | objects] | length'` on that file.
  - Final Helm compile timing: copy the pinned wrapper to
    `/home/roman/dev/helm-schema-step3-lint/chart`, place the exact preceding
    output at `values.schema.json`, create its otherwise-empty `templates/`
    directory, then run `/usr/bin/time -f
    'elapsed=%e user=%U system=%S max_rss_kb=%M exit=%x' helm lint
    /home/roman/dev/helm-schema-step3-lint/chart --strict` three times.
  - Supplemental full-equality candidates and semantic controls:
    `TMPDIR=/home/roman/dev/helm-schema-step3-cli-dump SCHEMA_DUMP=1 cargo
    nextest run -P integration -p helm-schema-cli --test cli
    generates_schema_for_fixture_chart_without_k8s_provider`, then `cargo
    nextest run -P integration -p helm-schema-cli -E
    'test(/(wrapper_chart_with_subchart_tarball_containing_dir_entries|generates_schema_for_fixture_chart_without_k8s_provider|nested_printf_around_common_fullname_keeps_name_overrides_nullable|subchart_values_are_scoped_to_the_coalesced_child_view)/)'`;
    the final focused run reports 4 passed.
- Gates on the final Step 3 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations checked.
  - `cargo nextest run --workspace`: exit 0; 1,169 passed.
  - `task test:integration`: exit 0; 545 passed, 6 skipped.
  - `task test:all`: exit 0; 1,718 passed, 6 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked.
  - `task tokei:core`: exit 0; production Rust is 59,669 LOC (`+427` from
    Step 2).
- Commit: `35f4134` (`refactor(schema): canonicalize schema emission`).

## Step 4 — configuration surface

- Status: landed.
- Measured results:
  - The checked public matrix exposes `EmissionSelection = Preset { profile,
    delta } | Explicit(EmissionPolicy)`, retains requested-profile provenance,
    and rejects only kind partitions with both anchor lanes disabled.
  - Root directory and `.tgz` config resolution agree. Packaged and directory
    generation produce byte-identical final schemas for the same chart.
  - The exact Temporal config resolves to requested profile `lean` with local
    conditionals off. An explicit CLI `--profile lean` resets the file delta
    to standard lean, while an explicit CLI knob wins over file and preset.
  - Unsupported config versions 0 and 2 report supported range `1..=1` and
    the update-config/update-binary remediation. Unknown fields, malformed
    YAML, X-class settings, and contradictory knob matrices fail hard.
  - The aggregated weakening diagnostic is one typed event in both text and
    JSON modes. Effective output reports built-in/profile/file/CLI provenance
    for every field, and print mode succeeds without `Chart.yaml`, analysis,
    provider construction, or network access.
  - One clean final-build dump under
    `/home/roman/dev/helm-schema-step4-final-dump-20260803-b` ran 61 tests and
    wrote 84 artifacts. All 84 are byte-identical to the Step 3 final dump;
    no semantic corpus, lean-lane, generator, or final-output fixture changed.
- Deviations: none.
- Adjudication evidence:
  - No fixture acceptance flip exists: the complete 84-artifact clean dump is
    byte-identical to Step 3. Step 4 changes selection/config composition and
    reference-policy ownership without changing the default full projection,
    so no new Helm flip verdict is required.
  - The downstream Temporal file carries the already measured fast config
    (`lean` plus local conditionals off); the default CLI invocation remains
    full when no config is present.
- Review dossier:
  - Strict config trust, precedence, exact Temporal combination, root-only
    discovery, and X-class exclusion: `cargo nextest run -p helm-schema-cli
    -E 'test(config)'`.
  - Directory/archive agreement, relative explicit config, early print exit,
    aggregated JSON diagnostic, explicit-profile reset, and generated-policy
    annotation: `cargo test -p helm-schema-cli --test config_surface`.
  - Checked public policy and requested-profile contract: `cargo nextest run
    -p helm-schema --test public_surface
    emission_selection_resolves_checked_public_policy_once`.
  - Single reference-policy ownership through override preparation, transport,
    and annotation: `cargo nextest run -p helm-schema
    caller_authored_ref_replace_keys_do_not_collide_with_merge_intent`; source
    audit: `rg -n 'ReferencePolicy' crates/helm-schema/src/output_pipeline` and
    verify it appears in `EmitRequest`, not `PolicyInputOptions` or
    `OutputPipelineOptions`.
  - Single clean final-build dump: `TMPDIR=/home/roman/dev/helm-schema-step4-final-dump-20260803-b
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 passed and 84 JSON artifacts were
    written. `diff -rq /home/roman/dev/helm-schema-step3-final-dump-20260803-d
    /home/roman/dev/helm-schema-step4-final-dump-20260803-b` exits 0.
- Gates on the final Step 4 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 42 feature combinations and 3 structural lint
    tests checked. The superseded first attempt failed only because Zig tried
    its read-only default cache; the final run used the writable project cache.
  - `cargo nextest run --workspace`: exit 0; 1,176 passed.
  - `task test:integration`: exit 0; 553 passed, 6 skipped.
  - `task test:all`: exit 0; 1,733 passed, 6 skipped.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked, including Temporal with the
    chart-local config.
  - `task tokei:core`: exit 0; production Rust is 60,363 LOC (`+694` from
    Step 3).
- Commit: `20e31e8` (`feat(schema): add emission policy configuration`).

## Step 5 — benchmark and documentation

- Status: landed.
- Measured results:
  - The pinned Temporal wrapper remains dependency version 0.62.0. The
    vendored archive SHA-256 is
    `c2f01baeef60ed96335948640a8ac30fb49a10b906e20c259b92f81f2cba5c04`
    and the dependency-lock SHA-256 is
    `e401fb6aebdc95368cce83f2785d52e0e359298bb8be649fcbec637b914e2748`.
  - Three analysis samples build one immutable plan per sample and project
    full, standard lean, and the chart-local Temporal-fast policy from that
    plan. Warm plan construction is 3,564.75 ms median
    (3,545.89–3,583.62); retained-plan RSS delta is 221,659,136 bytes,
    unique retained provider-candidate payloads occupy 295,740 canonical
    bytes, and process peak RSS is 1,403,068 KiB.
  - The plan lowers 12,193 facts: 10 Mandatory, 4,641 OrdinaryRoot, 6,849
    OrdinaryLocal, and 693 TerminalGuarded. Full retains all facts and has
    418 root / 1,183 local carriers / 1,601 condition nodes. Standard lean
    retains Mandatory plus all OrdinaryLocal facts, has 0 root / 450 local
    carriers / 450 condition nodes, and drops exactly 4,641 OrdinaryRoot plus
    693 TerminalGuarded facts. Temporal-fast retains only the 10 Mandatory
    facts and has no conditional carriers.
  - Exact compact final outputs are: full 3,981,595 bytes / 142,287 objects /
    1,538 conditions; lean 1,600,557 bytes / 55,618 objects / 424 conditions;
    Temporal-fast 47,196 bytes / 2,322 objects / 0 conditions. Full and lean
    have 1,504/403 unique conditions and 828/410 unique `then` payloads.
  - Helm 4.2.3 strict-lint cold/warm-median times are baseline 0.06/0.05 s,
    full 102.26/101.98 s, standard lean 10.69/10.99 s, and Temporal-fast
    0.10/0.10 s. Each warm median covers three fresh Helm processes; lean's
    range is 10.86–11.36 s. The exact-preset lint and size thresholds do not
    trigger, and no monotonicity failure exists, so the middle lean contract
    remains final.
  - Sequential compiled Rust validators take 341.65 ms for full, 126.53 ms
    for lean, and 3.49 ms for Temporal-fast. Keeping full while compiling,
    using, and dropping each comparison raises process high-water RSS from
    255,836 to 641,384 KiB.
  - The measurement-only scalar-plain transform removes 34 scalar spelling
    unions. It saves only 2,389 bytes and 22 objects from full (3,979,206
    bytes / 142,265 objects, approximately 0.06% by bytes). Its Rust validator
    compiles in 291.99 ms, while Helm cold/warm-median lint is 98.60/105.45 s
    with a 103.75–115.27 s warm range. It has no material size or compile-cost
    benefit, so `scalar-spellings` remains unexposed; `assume-typed-scalars`
    remains absent.
  - The dedicated `task bench:emission-profiles` command and persistent-output
    script record report-derived facts/carriers/final metrics, shared-plan
    timing, sequential validator timing, baseline and per-policy Helm samples,
    RSS, machine/tool versions, and anchor checksums. Default benchmark and
    trace outputs also moved from `/tmp` into the repository `target` tree.
  - README, long CLI help, CLI reference, and the new configuration reference
    state the exact full/lean retention contract, strict root-source trust
    boundary, explicit-profile reset, and CLI > file > preset > built-in
    precedence. The documentation build regenerated its examples with the
    authoritative policy annotation.
- Deviations:
  - No frozen-plan contract was reinterpreted. The plan-authorized
    `bench-support` fallback is used because the end-to-end Temporal session
    and the immutable generator plan are in different crates. It is a
    non-default feature; the runtime session entry point remains crate-private
    and test-only, and no public CLI/config policy knob was added.
- Adjudication evidence:
  - Step 5 changes benchmark support and documentation, not schema selection
    or emission. One clean final-build dump ran 61 tests and wrote 84
    artifacts; all 84 are byte-identical to the Step 4 final dump. There is no
    TIGHTEN or LOOSEN to adjudicate against Helm.
  - The benchmark itself uses real `helm lint --strict` in fresh processes for
    every baseline and schema sample, and the Rust validator confirms the
    coalesced Temporal defaults validate under full, lean, Temporal-fast, and
    the measurement-only scalar output.
- Review dossier:
  - Complete release benchmark and environment capture:
    `HELM_SCHEMA_BENCH_DIR=/home/roman/dev/helm-schema-step5-final-benchmark-20260803-c
    HELM_SCHEMA_BENCH_RUNS=3 HELM_SCHEMA_LINT_WARM_RUNS=3 task
    bench:emission-profiles`; the command exits 0 and writes
    `metrics.final.json`.
  - Fact, carrier, final-output, timing, memory, tool, and scalar decision
    evidence: `jq '{anchor, runs, generation, scalar_spellings_plain,
    validators, helm_lint, environment}'
    /home/roman/dev/helm-schema-step5-final-benchmark-20260803-c/metrics.final.json`.
  - Feature-gated shared-plan benchmark compilation and API-boundary audit:
    `cargo test -p helm-schema --features bench-support --lib --no-run`, then
    `rg -n 'benchmark_emission_policies|bench_support' crates/helm-schema-gen
    crates/helm-schema`; default builds omit the feature and the session method
    is `pub(crate)` under `cfg(all(feature = "bench-support", test))`.
  - Exact CLI help contract: `cargo test -p helm-schema-cli --test cli_flags
    long_help_states_the_profile_retention_contract -- --exact`, then
    `cargo run --quiet -p helm-schema-cli -- --help`.
  - Configuration/reference site and regenerated examples: `task docs:build`.
  - Single clean final-build dump:
    `mkdir -p /home/roman/dev/helm-schema-step5-final-dump-20260803-c`, then
    `TMPDIR=/home/roman/dev/helm-schema-step5-final-dump-20260803-c
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and 84 files are written.
  - No semantic drift: `diff -rq
    /home/roman/dev/helm-schema-step4-final-dump-20260803-b
    /home/roman/dev/helm-schema-step5-final-dump-20260803-c`; exit 0.
- Gates on the final Step 5 implementation tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; workspace Clippy and all 3 structural lint tests
    passed.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and 3
    targets passed, including `bench-support` off/on for `helm-schema-gen` and
    `helm-schema`.
  - `cargo nextest run --workspace`: exit 0; 1,176 passed.
  - `task test:integration`: exit 0; 554 passed, 6 skipped.
  - `task test:all`: exit 0; 1,734 passed, 6 skipped, including the live
    network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts checked, including Temporal under its
    chart-local fast config.
  - `task tokei:core`: exit 0; production Rust is 60,495 LOC (`+132` from
    Step 4).
  - Supplemental documentation gate `task docs:build`: exit 0; 32 pages built
    after regenerating all four documentation schema examples.
- Commit: `b731c30` (`feat(schema): add emission profile benchmark`).

## Round 68 — emission-review findings closure

- Status: landed.
- Measured results:
  - Chained `default` selection now stamps a final fallback with the negated
    truthiness of every exact identity in a composed `FirstTruthy` primary.
    Pipeline and call-order three-deep controls accept a dormant or selected
    final fallback and retain the true-tooth rejection when every operand is
    null-deleted.
  - Mandatory required-entry canonicalization now types an untyped object
    host. Completion backfill recognizes every semantically object-only
    canonical conjunct, including the not-null form, so a later inserted
    member cannot bypass the dropped carrier.
  - Kind partitioning separates truly selector-independent provider uses onto
    an Ordinary conjunct. Candidate-bearing and exact branch-selected uses
    retain their typed selector provenance and stay in the kind partitions;
    unrelated overlay evidence remains with those partitions.
  - Late reachability starts from every caller-retained definition, decodes
    percent-encoded local fragments before JSON Pointer matching, and keeps
    every definition conservatively when a local fragment cannot decode.
  - The full-side semantic oracle is explicit for every control, including
    full-accepts for an unknown provider kind. The structural battery retains
    Helm 4 dependency roots instead of probing parent null-deletion states
    that Helm refills from the dependency chart.
  - One clean final-build dump ran 61 tests and wrote exactly 84 artifacts.
    Twenty-one full corpus fixtures, the Temporal lean fixture, and one
    generator fixture changed; all four final-output fixtures remained
    byte-identical. Across the 23 changed files, serialized fixture bytes
    moved from 85,546,740 to 85,477,530 (`-69,210`).
  - One clean IR dump ran the complete 18-case corpus. Two IR fixtures
    changed: Bitnami Redis retains the chained-default selection predicate,
    while Zalando Postgres Operator drops conditions derived from a
    meta-selected `kindIs` subject. The other 16 IR fixtures are
    byte-identical.
  - The compiled Rust prober checked 112,356 coalesced documents at top-level
    deletion, second-level deletion, and empty member/item granularities. It
    found 16 loosenings, zero tightenings: eight Airflow
    `fullnameOverride`, four MetalLB `fullnameOverride`, and four Traefik
    `namespaceOverride` probes.
  - Helm 4.2.3 renders all 12 falsy/empty loosenings. Four truthy non-string
    Airflow shapes still abort in a string-consuming helper; full accepts
    those states because the corrected `kindIs "invalid"` decoder abstains
    when output selection is conditional instead of asserting one raw input
    identity. This is a recorded loss of completeness, not a false rejection
    or an invented exact guard.
  - Production Rust is 60,586 LOC (`+91` from Step 5).
- Deviations:
  - The review's “empty `kind_candidates`” shorthand was insufficient for an
    exact kind arm whose candidates had already been cleared by IR
    concretization. Non-serialized `kind_branches` now remains as typed
    provenance through emission, preventing a selected ConfigMap constraint
    from becoming an unconditional ordinary constraint on the Service arm.
  - The round-58 whole-global note is corrected rather than changing the
    working projection: both range modes and `RangeInput` fail captures fan
    out. A full-schema dependency regression pins that the current iterable
    domain admits absent/null parent globals supplied by the child default.
  - Every low-severity item was fixed; none was deferred or blocked. The
    frozen plan was not edited.
- Adjudication evidence:
  - Helm 4.2.3 renders chained-default `printf` with a dormant deleted final
    fallback and with that fallback selected, while the all-null raw scalar
    spelling still makes Helm reject the rendered YAML.
  - An `else with` successor control renders when either preceding arm is
    selected and reaches the invalid final operand only when both are falsy.
  - The 16 fixture flips are pinned in one live Helm table with the exact 12
    render / four abort verdicts described above. Every adopted fixture is
    byte-identical to the candidate used by the compiled prober.
- Review dossier:
  - Formatter/default selection and sibling forms: `cargo nextest run -p
    helm-schema-ir -p helm-schema-gen -E
    'test(/(chained_default|printf_plain_slot_contract_follows_chained_default_selection)/)'`.
  - Canonical required host and completion-order backfill: `cargo nextest run
    -p helm-schema-gen -E
    'test(/(canonical_required_entries_type_an_untyped_object_host|canonical_not_null_conjunction_survives_completion_default_backfill|conditional_duplicate_of_nested_range_does_not_restore_declared_leaf_types)/)'`.
  - Partition separation and full semantic controls: `cargo nextest run -p
    helm-schema-gen -E
    'test(selector_independent_provider_uses_stay_on_an_ordinary_conjunct)'`,
    then `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(/(current_profiles_obey_monotonicity_and_semantic_controls|local_kind_partition_is_a_local_policy_fact|temporal_wrapper_pairwise_matrix_is_monotone)/)'`.
  - Reachability closure and encoded fragments: `cargo nextest run -p
    helm-schema -E
    'test(/(late_prune|retained_caller_definition_keeps_owned_transitive_references)/)'`.
  - Config provenance and failure-path diagnostics: `cargo nextest run -p
    helm-schema-cli -E
    'test(temporal_combination_keeps_profile_provenance_and_cli_profile_resets_file_delta)'`,
    then `cargo nextest run -P integration -p helm-schema-cli -E
    'test(/(config_weakening_diagnostic_survives_downstream_failure|relative_explicit_config)/)'`.
  - New live controls: `TMPDIR=/home/roman/dev/helm-schema/target/round68-tmp
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profile_live -E
    'test(/(replay_chained_default_printf_against_helm|replay_else_with_successor_against_helm|replay_round68_corpus_loosenings_against_helm|replay_semantic_controls_against_helm_and_provider)/)'
    --run-ignored ignored-only --no-capture`.
  - Compiled three-granularity flip proof: `TMPDIR=/home/roman/dev/helm-schema/target/round68-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=34e58cc
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round68-final-dump
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round68_fixture_flips_match_the_helm_adjudicated_list)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=112356 flips=16`.
  - Single clean dump: `TMPDIR=/home/roman/dev/helm-schema/target/round68-final-dump
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and 84 artifacts are written.
  - Complete clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/round68-ir-final-dump
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
    helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; the
    18-case corpus passes and writes 18 artifacts.
- Gates on the final Round 68 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 48 feature combinations for 13 packages across
    three targets. The successful invocation places both Zig caches under
    `target` because the sandbox denies Zig's default cache path.
  - `cargo nextest run --workspace`: exit 0; 1,187 passed, zero skipped.
  - `task test:integration`: exit 0; 557 passed, 10 skipped by the profile.
  - `task test:all`: exit 0; 1,748 passed, 10 skipped by the profile,
    including the live network tests.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 downstream charts passed.
  - `task tokei:core`: exit 0; 60,586 production Rust LOC.
- Commit: `8ab98fc` (`fix(schema): close round-68 emission findings`).

## Round 69 — shared override-bundling namespace

- Status: landed.
- Measured results:
  - Self-contained override preparation reserves generated definition names
    across the generated base and every override's authored `$defs` before it
    bundles any external reference. Each override still carries the
    definitions it references, while generated names are unique across the
    complete application-ordered merge.
  - A private output-pipeline regression reserves base `schema1` and a later
    override's authored `schema2`; the two external targets become `schema3`
    and `schema4`, resolve to their distinct contents, and validate the
    intended combined instance. Repeating the same order produces the same
    override digest; reversing the order produces a different digest.
  - A command-line end-to-end equality regression passes two
    `--override-schema` documents whose external refs would previously both
    become `schema1`. The final output instead contains distinct `schema1`
    and `schema2` definitions with the correct ref targets and a pinned
    application-ordered policy digest.
  - One clean final-build dump ran 61 tests and wrote exactly 84 artifacts.
    Every artifact is byte-identical to the Round 68 dump.
  - The compiled Rust prober checked 112,356 coalesced documents at top-level
    deletion, second-level deletion, and empty member/item granularities. It
    found zero acceptance flips across all 60 full and lean schemas.
  - Production Rust is 60,637 LOC (`+51` from Round 68).
- Deviations:
  - This closes a verified pre-existing output-pipeline defect rather than an
    emission-profile regression. The frozen plan was not edited.
  - The session constructs its memoized generated schema before loading
    overrides so preparation can reserve the base definition namespace.
    Override file and external-ref IO remains in the preparation phase; the
    pure output transform still consumes prepared documents only.
- Adjudication evidence:
  - No committed fixture changes direction: the complete 84-artifact dump is
    byte-identical and the independent acceptance prober reports zero flips.
    Therefore there is no fixture TIGHTEN or LOOSEN to adjudicate with Helm.
  - The collision itself is adjudicated at the final CLI boundary by exact
    output equality: both refs resolve to their own external document, and
    neither definition is silently deep-merged with the other.
- Review dossier:
  - Shared base/override namespace and ordered digest: `cargo nextest run -p
    helm-schema -E
    'test(bundled_overrides_allocate_names_across_the_base_and_every_override)'`.
  - Command-line two-override reproducer: `cargo nextest run -P integration
    -p helm-schema-cli --test override_bundling -E
    'test(multiple_override_external_refs_use_distinct_bundled_definitions)'`.
  - Single clean dump: `TMPDIR=/home/roman/dev/helm-schema/target/round69-final-dump
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and 84 artifacts are written.
  - Compiled three-granularity zero-flip proof:
    `TMPDIR=/home/roman/dev/helm-schema/target/round69-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=8ab98fc
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round69-final-dump
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round69_override_bundling_is_corpus_acceptance_equivalent)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=112356 flips=0`.
  - Hermetic monotonicity and semantic controls: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profiles -E
    'test(/(current_profiles_obey_monotonicity_and_semantic_controls|local_kind_partition_is_a_local_policy_fact|temporal_wrapper_pairwise_matrix_is_monotone)/)'`.
  - Live Helm/provider replay: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profile_live -E
    'test(replay_semantic_controls_against_helm_and_provider)'
    --run-ignored ignored-only --no-capture`.
- Gates on the final Round 69 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 48 feature combinations for 13 packages across
    three targets, with both Zig caches under `target`.
  - `cargo nextest run --workspace`: exit 0; 1,188 passed, zero skipped.
  - `task test:integration`: exit 0; 558 passed, 11 skipped by the profile.
  - `task test:all`: exit 0; 1,750 passed, 11 skipped by the profile,
    including the live network tests.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 downstream charts passed.
  - `task tokei:core`: exit 0; 60,637 production Rust LOC.
- Commit: `2a1d839` (`fix(schema): share override bundle namespace`).

## Round 70 — deeper probes and canonical partition boundaries

- Status: landed.
- Measured results:
  - The compiled acceptance battery retains the existing top-level deletion,
    second-level deletion, and empty member/item probes, and adds exact
    third-level deletions plus bounded root-guard witness pairs. Per chart it
    caps 50,000 total probes, 2,048 depth-three deletions, 24 attempted guards,
    eight guard-state pairs, and 128 guard-witness candidates. Every omitted
    path, guard, or witness candidate is reported on stderr.
  - Replaying the Round 68/69 fixture set against `34e58cc` at the new depth
    checks 120,538 coalesced documents and finds exactly the same 16
    Helm-adjudicated Round 68 loosenings. It finds no additional flip.
    Comparing the final Round 70 dump with `10f5231` checks 120,540 documents
    and reports zero acceptance flips.
  - Selector-independent provider uses remain on an Ordinary kind-partition
    conjunct without discarding the branch's range facts, type hints,
    metadata-field kinds, or base-preservation state. The new range control
    accepts both map defaults and a Helm `--set` integer in full and lean.
  - Descendant backfill now treats a multi-arm object union as branch-specific
    unless the descendant schemas are equal. The ambiguous lane abstains from
    default backfill instead of conjoining one descendant across every arm.
    A canonical mixed object/scalar not-null slot splits only its explicit
    type array, inserts the descendant into the object arm, and retains the
    outer not-null conjunct.
  - Exact input identity now rejects default-selection and derived-text
    channels as well as meta predicates. Chained direct `default` operands
    still carry their exact selection conjunction, while formatter-derived
    primaries explicitly abstain from making a dormant fallback mandatory.
  - One clean final-build dump runs 61 tests and writes exactly 84 artifacts.
    Only Argo CD and OAuth2 Proxy re-encode: 4,398,171 -> 4,397,889 bytes and
    1,982,205 -> 1,981,228 bytes, respectively. The 18-case clean IR dump is
    byte-identical to the committed IR fixtures.
  - Production Rust is 60,757 LOC (`+120` from Round 69).
- Deviations:
  - Multi-arm object unions disclose a deliberate completeness loss: when
    their descendant constraints differ, composed-default evidence is not
    added because the analyzer cannot attribute it to one arm without either
    conjoining alternatives or weakening the structural provider tooth.
    Equal descendants retain the canonical insertion path.
  - Helm 4.2.3 accepts an integer range source supplied by `--set` but rejects
    the corresponding `--set-json` number because the latter arrives as a
    float64. The live control therefore uses the measured `--set entries=2`
    transport; the existing input-channel diagnostic remains authoritative.
  - Guard sampling is intentionally bounded. Charts whose guards have no
    satisfying or violating witness among the bounded, default-composed
    structural candidates report `dropped_without_bounded_witness`; they are
    not silently counted as sampled controls.
  - The frozen plan is unchanged.
- Adjudication evidence:
  - Helm `v4.2.3` (`go1.26.5`) renders the selector-independent ranged-provider
    control with its map default and with `--set entries=2`. Full and lean
    accept both coalesced documents.
  - Helm renders both opaque formatter-default shapes with `z=null`; their
    schemas accept that dormant-fallback deletion. The existing chained
    default live control also retains both directions: deleting only `z`
    renders, selecting a present final fallback renders, and selecting an
    absent final fallback aborts.
  - The two corpus fixture rewrites produce no acceptance change in 120,540
    final-depth probes, so there is no new TIGHTEN or LOOSEN verdict requiring
    per-fixture Helm adjudication. The earlier 16 loosenings remain exactly the
    independently replayed and Helm-adjudicated Round 68 set.
- Review dossier:
  - Probe depth and guard controls: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profiles -E
    'test(structural_battery_samples_depth_three_and_guard_states)'`.
  - Expanded Round 68/69 replay: `TMPDIR=/home/roman/dev/helm-schema/target/round70-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=34e58cc
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round70-final-dump
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round68_fixture_flips_match_the_helm_adjudicated_list)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=120538 flips=16`.
  - Round 70 zero-flip proof: `TMPDIR=/home/roman/dev/helm-schema/target/round70-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=10f5231
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round70-final-dump
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round70_partition_and_canonicalization_changes_are_acceptance_equivalent)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=120540 flips=0`.
  - Kind-partition preservation: `cargo nextest run -p helm-schema-gen -E
    'test(selector_independent_provider_uses_stay_on_an_ordinary_conjunct)'`,
    then `cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(ordinary_kind_partition_evidence_keeps_the_complete_range_domain)'`.
  - Canonical boundary controls: `cargo nextest run -p helm-schema-gen -E
    'test(multi_arm_object_union_abstains_from_ambiguous_default_backfill) |
    test(mixed_type_not_null_conjunction_survives_default_backfill)'`.
  - Exact identity and opaque-default boundary: `cargo nextest run -p
    helm-schema-ir -p helm-schema-gen -E
    'test(invalid_kind_abstains_for_a_default_selected_subject_identity) |
    test(opaque_default_primary_does_not_scope_its_fallback_as_an_exact_arm) |
    test(printf_plain_slot_contract_follows_chained_default_selection) |
    test(opaque_formatter_default_primary_abstains_from_scoping_the_fallback)'`.
  - Hermetic monotonicity and semantic controls: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profiles -E
    'test(current_profiles_obey_monotonicity_and_semantic_controls) |
    test(temporal_wrapper_pairwise_matrix_is_monotone)'`.
  - Live Helm controls: `cargo nextest run -P integration -p helm-schema
    --test schema_emission_profile_live -E
    'test(replay_chained_default_printf_against_helm) |
    test(replay_opaque_formatter_default_against_helm) |
    test(replay_selector_independent_ranged_provider_use_against_helm)'
    --run-ignored ignored-only --no-capture`.
  - Single clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/round70-final-dump
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and 84 artifacts are written.
  - Complete clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/round70-ir-final-dump
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
    helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; 18
    artifacts are byte-identical.
- Gates on the final Round 70 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 48 feature combinations for 13 packages across
    three targets, with both Zig caches under `target`.
  - `cargo nextest run --workspace`: exit 0; 1,193 passed, zero skipped.
  - `task test:integration`: exit 0; 560 passed, 14 skipped by the profile.
  - `task test:all`: exit 0; 1,757 passed, 14 skipped by the profile,
    including the live network tests.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 downstream charts passed.
  - `task tokei:core`: exit 0; 60,757 production Rust LOC.
- Commit: `876ff34` (`fix(schema): harden canonical partition boundaries`).

## Round 71 — Helm YAML 1.1 boolean-key parity

- Status: blocked at the measurement veto; evidence landed, production
  behavior unchanged.
- Measured results:
  - Helm `v4.2.3` (`go1.26.5`) normalizes unquoted
    `y/n/yes/no/on/off`, including all measured case variants, to boolean
    map keys and exposes them in `.Values`/JSON as string keys `"true"` and
    `"false"`. The same behavior occurs at the root, in nested mappings,
    in chart defaults, and in `-f` values files. Among unquoted aliases for
    one boolean key, the last source value wins.
  - Quoted aliases such as `"y"`, `'no'`, and `"on"` remain literal string
    keys. `--set y=...` and `--set nested.no=...` also create literal keys;
    they do not enter the YAML decoder's boolean-key lane. Consequently
    `.Values.y` is nil for an unquoted default normalized to `"true"`, but
    resolves a quoted or `--set` key.
  - The veto condition appears when a quoted canonical key (`"true"` or
    `"false"`) coexists with an unquoted boolean spelling that normalizes to
    the same JSON key. Twenty identical Helm processes produced different
    winners for the same mappings. For example, `legacyThenQuoted` yielded
    both `legacy` and `quoted`, and `quotedThenCanonical` yielded both
    `canonical` and `quoted`. This is not a deterministic last-write rule a
    schema composition pass can reproduce.
  - The corpus and luup2 values sweep found zero unquoted legacy boolean-key
    declarations. One clean final-build dump ran 61 tests and wrote 84
    artifacts, all byte-identical to Round 70. The expanded compiled Rust
    battery checked 120,540 default-composed documents and found zero flips.
- Deviations:
  - The recommended normalization and aggregated authoring diagnostic are
    not adopted. The task's explicit stop branch applies because Helm's
    quoted-canonical collision class is nondeterministic even at one pinned
    Helm/Go version. Implementing a deterministic winner would claim parity
    Helm itself does not provide.
  - Two dispositions remain for the v3 reconciliation: reject ambiguous
    mixed boolean/string key collisions with a diagnostic, or define a
    deterministic helm-schema composition policy and disclose that it cannot
    reproduce every Helm run. Normalizing only the non-colliding subset would
    still need a structural preflight that detects and excludes the mixed
    collision class.
  - The frozen plan is unchanged. No corpus fixture is adopted and no schema
    semantics change lands in this round.
- Adjudication evidence:
  - The pinned live matrix covers top-level and nested defaults, quoted keys,
    a values file, `--set`, and dot/index selector results. Every stable cell
    matches the measured Helm result.
  - The live control accepts either documented winner for the mixed collision
    and prints the observed set. An independent 20-process replay observed
    both winners, establishing the veto rather than assigning an arbitrary
    acceptance direction.
  - With no schema fixture delta, the clean dump and expanded prober have no
    TIGHTEN or LOOSEN to adjudicate.
- Review dossier:
  - Pinned Helm matrix and mixed-collision observation: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profile_live -E
    'test(replay_yaml_boolean_key_composition_against_helm)'
    --run-ignored ignored-only --no-capture`.
  - Repeated-process ambiguity reproducer: `for i in {1..20}; do cargo test
    -p helm-schema --test schema_emission_profile_live
    replay_yaml_boolean_key_composition_against_helm -- --ignored --exact
    --nocapture; done`; inspect the printed `mixed boolean/string key
    winners` sets.
  - Corpus/downstream declaration sweep: `rg -n --glob 'values.yaml'
    '^[[:space:]]*(?:[yY]|[nN]|[yY][eE][sS]|[nN][oO]|[oO][nN]|[oO][fF][fF])[[:space:]]*:'
    testdata /home/roman/dev/branches/luup2/deployment/charts`; exit 1 with
    no matches.
  - Single clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/round71-tmp
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 61 tests pass and 84 artifacts are written.
  - Expanded zero-flip proof: `TMPDIR=/home/roman/dev/helm-schema/target/round71-final-dump
    SCHEMA_ACCEPTANCE_BASELINE_REF=876ff34
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round71-tmp
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round70_partition_and_canonicalization_changes_are_acceptance_equivalent)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=120540 flips=0`.
  - Hermetic controls: `cargo nextest run -P integration -p helm-schema
    --test schema_emission_profiles -E
    'test(current_profiles_obey_monotonicity_and_semantic_controls) |
    test(temporal_wrapper_pairwise_matrix_is_monotone)'`.
- Gates on the final Round 71 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0.
  - `task lint:fc`: exit 0; 48 feature combinations for 13 packages across
    three targets.
  - `cargo nextest run --workspace`: exit 0; 1,193 passed, zero skipped.
  - `task test:integration`: exit 0; 560 passed, 15 skipped by the profile.
  - `task test:all`: exit 0; 1,757 passed, 15 skipped by the profile,
    including the live network tests.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 downstream charts passed.
  - `task tokei:core`: exit 0; 60,757 production Rust LOC (no production
    change from Round 70).
- Commit: `a2f9f00` (`test(schema): record yaml boolean key veto`).

## Round 72 — pipeline/session boundaries and harness hygiene

- Status: landed.
- Measured results:
  - Override documents are loaded and root-kind validated before chart
    generation. External-reference bundling remains after generation, where
    the generated base can seed the shared namespace. An end-to-end precedence
    control proves an invalid override root and a missing override path both
    fail before a simultaneously missing chart path.
  - The shared override namespace now deduplicates equal external target URIs
    across override documents. Two overrides targeting one URI both reference
    `#/$defs/schema1`, emit one generated definition, and produce the same
    application-ordered digest on a repeated preparation.
  - Late reachability splits a JSON Pointer before percent-decoding each
    segment. A definition named `provider/name` remains one segment when
    reached through `#/$defs/provider%2Fname`; malformed percent encodings
    continue to retain conservatively.
  - Applying an empty Mandatory `required` carrier to an untyped object host
    now preserves the carrier's `type: object` half. The exhaustive canonical
    equivalence lane covers empty and non-empty required sets.
  - Acceptance probes derive dependency roots from installed `charts/`
    directories and chart archives as well as manifest declarations. An
    unlisted vendored child is no longer null-deleted as an ordinary root.
    The harness explicitly records that it cannot synthesize child defaults
    missing from the already supplied coalesced input document.
  - One clean final-build dump runs 62 tests and writes exactly 84 artifacts.
    No fixture is modified. The expanded compiled Rust battery checks 120,435
    default-composed documents against `a2f9f00` and finds zero acceptance
    flips. The 105-probe reduction from Round 71 is the measured effect of
    excluding installed dependency roots, not an unreported cap.
  - Production Rust is 60,785 LOC (`+28` from Round 71).
- Deviations:
  - Round 69's note that override loading followed generation is superseded:
    file IO and root validation now precede generation, while bundling still
    follows it to preserve the base-plus-overrides namespace contract.
  - The generated-name reservation boundary intentionally reads root `$defs`
    only. Legacy `definitions` remains a distinct reference namespace and is
    documented at the reservation site.
  - No Round 72 item is deferred or blocked. The frozen plan is unchanged.
- Adjudication evidence:
  - The clean dump is fixture-identical, and the final-depth battery reports
    zero TIGHTEN and zero LOOSEN across 120,435 probes. There is therefore no
    corpus fixture flip to adopt or adjudicate individually with Helm.
  - The live semantic-control replay passes against Helm and the pinned
    provider. The hermetic monotonicity and Temporal pairwise controls also
    pass on the final Round 72 tree.
  - The lower probe count is pinned by the unlisted-vendored-dependency
    control: the harness now protects the child root that Helm obtains from
    the installed `charts/` tree.
- Review dossier:
  - Session precedence and shared-target bundling: `cargo nextest run -p
    helm-schema -E
    'test(override_loading_and_root_validation_precede_chart_generation) |
    test(bundled_overrides_share_one_definition_for_the_same_external_target)'`.
  - Reachability and degenerate canonical carrier: `cargo nextest run -p
    helm-schema -p helm-schema-gen -E
    'test(late_prune_decodes_percent_encoding_within_each_pointer_segment) |
    test(late_prune_preserves_definitions_for_undecodable_local_refs) |
    test(canonical_empty_required_entries_still_type_an_untyped_object_host)'`.
  - Vendored dependency roots and retained probe depth: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profiles -E
    'test(structural_battery_preserves_unlisted_vendored_dependency_roots) |
    test(structural_battery_preserves_helm_v4_dependency_roots) |
    test(structural_battery_samples_depth_three_and_guard_states)'`.
  - Single clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/round72-final-dump
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
  - Expanded zero-flip proof: `TMPDIR=/home/roman/dev/helm-schema/target/round72-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=a2f9f00
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round72-final-dump
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round72_pipeline_changes_are_acceptance_equivalent)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=120435 flips=0` and reports every
    guard/depth cap on stderr.
  - Hermetic controls: `cargo nextest run -P integration -p helm-schema
    --test schema_emission_profiles -E
    'test(current_profiles_obey_monotonicity_and_semantic_controls) |
    test(temporal_wrapper_pairwise_matrix_is_monotone)'`.
  - Live Helm/provider replay: `cargo nextest run -P integration -p
    helm-schema --test schema_emission_profile_live -E
    'test(replay_semantic_controls_against_helm_and_provider)'
    --run-ignored ignored-only --no-capture`.
- Gates on the final Round 72 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 after the final edit; the whole workspace and all
    three ast-grep rules pass.
  - `task lint:fc`: exit 0; 48 feature combinations for 13 packages across
    three targets.
  - `cargo nextest run --workspace`: exit 0; 1,196 passed, zero skipped.
  - `task test:integration`: exit 0; 562 passed, 16 skipped by the profile.
  - `task test:all`: exit 0; 1,762 passed, 16 skipped by the profile,
    including the live network tests.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 downstream charts passed.
  - `task tokei:core`: exit 0; 60,785 production Rust LOC.
- Commit: `ac36c7b` (`fix(schema): close round-72 pipeline findings`).

## Round 73 — second-review findings closure

- Status: landed.
- Measured results:
  - An opaque `default` primary now stamps its fallback with an unlowerable
    output-selection predicate. Downstream string consumers therefore retain
    conditional fail captures instead of promoting the fallback to a
    path-wide string contract. Eight expression shapes cover `b64enc`,
    `trunc`, `sha256sum`, `quote`, `trimSuffix`, parenthesized and call-form
    defaults, and a mixed exact/opaque chain.
  - The live Helm 4.2.3 matrix covers four states per expression: deleted
    dormant fallback, numeric dormant fallback, selected string fallback,
    and selected numeric fallback. Every dormant case and every selected
    string renders. Selected numeric values abort in the string-only
    consumers and render through `quote`; the schema deliberately stays open
    only where the opaque selection predicate has no sound lowerable subset.
    A separate exact-identity chain still rejects its selected numeric final
    fallback while accepting that value when dormant.
  - Multi-arm descendant equality now requires every arm to resolve the
    compared descendant. Wildcard segments and members hidden under nested
    `allOf` therefore abstain instead of treating `None == None` as an
    equivalence proof. Default-insertion abstentions are counted in
    `EmissionReport::canonicalization.default_backfill_abstentions`.
  - Guard probes are synthesized directly over null-deletion-composed chart
    defaults instead of relabeling existing single-path probes. Each sampled
    guard has a satisfying and violating witness where the bounded search can
    find both. A second bounded lane pairs those states with the same
    nonconforming payload on a different constrained path.
  - The asserted JSON coverage report records 121,059 emitted probes across
    60 full/lean fixture lanes: 112,260 base probes, 7,465 third-level
    deletions, 427 guard pairs, and 240 composite pairs. It also records
    13,828 discovered guards, 13,291 guard-cap skips, 110 guards without a
    bounded witness pair, 37,550 omitted witness candidates, 2,267 composite
    targets dropped by the pair cap, and 39 targets without a bounded
    nonconforming payload. No base or third-level probe was dropped.
  - The one clean final-build dump runs 62 tests and writes exactly 84
    artifacts. Airflow, Argo CD, Datadog, ingress-nginx, Jenkins, and Traefik
    re-encode; the compiled comparison against `d78aa30` finds zero acceptance
    flips at the expanded depth. All 18 clean IR dump artifacts are
    byte-identical.
  - Production Rust is 60,893 LOC (`+108` from Round 72). The larger battery
    and its coverage-report machinery live under `tests/` and are excluded
    from that production count.
- Deviations and corrections:
  - The review's oauth2-proxy acceptance-widening claim does not reproduce.
    Helm aborts when `global.imageRegistry` is numeric both with an empty and
    a live `image.registry`, confirming eager `tpl` argument evaluation.
    However, compiled schemas from both `34e58cc` and the final tree reject
    both composite documents through the unconditional `tpl` program-string
    contract. The Round 70 conditional deletion was therefore structurally
    visible but acceptance-redundant; restoring it by treating formatter
    outputs as exact identities would reintroduce Round 73's confirmed false
    rejection. The live and two-validator controls pin this disposition.
  - Reachability now follows the consuming resolver rather than Round 72's
    incorrect split-before-percent-decode instruction. Measured with the
    repository's `jsonschema` version, `#/$defs/a%2Fb` resolves through the
    nested `a/b` pointer after whole-fragment decoding, while
    `#%2F$defs%2Fname` remains an unresolved anchor because its raw fragment
    does not start with `/`. Undecodable pointer fragments still preserve all
    generator-owned definitions conservatively.
  - Duplicate dependency names with multiple aliases remain a shared harness
    and production limitation: both maps key metadata by the dependency's
    chart name and keep the last alias. Correcting it requires measuring and
    modeling Helm's installed-entry-to-alias association, not replacing one
    last-write rule with an unverified guess. No corpus chart exercises the
    shape, so it is recorded rather than patched in this round.
  - The frozen emission plan and the Round 71 YAML boolean-key veto are
    unchanged.
- Adjudication evidence:
  - The eight-expression opaque-default live matrix and its schema matrix
    exercise deletion, dormant wrong type, selected valid type, and selected
    wrong type in both directions. The six re-encoded corpus fixtures produce
    zero TIGHTEN and zero LOOSEN at the full battery depth, so there is no
    corpus acceptance direction to adopt. The live mechanism controls pass
    before the fixture bytes are adopted.
  - oauth2-proxy's empty/live-primary composite states both abort Helm on a
    numeric eager fallback. Both the pre-Round-70 and final compiled schemas
    reject the same documents, while the selected string state renders.
  - The wildcard and nested-`allOf` canonical controls accept each original
    integer/Boolean branch and reject the unrelated inserted-string state;
    each reports one visible abstention instead of conjoining a leaf across
    branch-specific alternatives.
- Review dossier:
  - Opaque-selection structure and both-direction schema matrix: `cargo
    nextest run -p helm-schema-ir -p helm-schema-gen -E
    'test(opaque_default_primary_records_an_unlowerable_fallback_selection) |
    test(opaque_formatter_default_primary_keeps_fallback_consumers_conditional) |
    test(identity_default_chain_keeps_exact_final_fallback_selection)'`.
  - Opaque-default and oauth2-proxy Helm replay: `cargo nextest run -P
    integration -p helm-schema --test schema_emission_profile_live -E
    'test(/replay_(opaque_formatter_default|oauth2_proxy_tpl_default_eagerness)_against_helm/)'
    --run-ignored ignored-only --no-capture`.
  - oauth2-proxy two-validator retro-check: `cargo nextest run -P integration
    -p helm-schema --test schema_emission_profiles -E
    'test(round70_oauth2_proxy_tpl_change_kept_the_eager_string_tooth)'
    --run-ignored ignored-only --no-capture`.
  - Multi-arm boundaries, abstention accounting, and degenerate required
    carrier: `cargo nextest run -p helm-schema-gen -E
    'test(multi_arm_object_union_abstains) |
    test(canonical_empty_required_entries_leave_a_typed_foreign_host_untouched)'`.
  - Resolver measurement and reachability alignment: `cargo nextest run -p
    helm-schema -E 'test(output_pipeline::reachability)'`.
  - Targeted guard/composite controls and hermetic profile laws: `cargo
    nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(current_profiles_obey_monotonicity_and_semantic_controls) |
    test(temporal_wrapper_pairwise_matrix_is_monotone) |
    test(guard_battery_synthesizes_composite_guard_and_payload_states)'`.
  - Single clean schema dump: `TMPDIR=/home/roman/dev/helm-schema/target/round73-final-dump
    SCHEMA_DUMP=1 cargo nextest run -P integration --no-fail-fast -p
    helm-schema-gen -p helm-schema-cli -p helm-schema -E
    'test(schema_fixtures_match) | binary(chart_corpus) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) |
    binary(final_output_policy)'`; 62 tests pass and 84 artifacts are written.
  - Expanded zero-flip proof with asserted coverage disclosure:
    `TMPDIR=/home/roman/dev/helm-schema/target/round73-prober-tmp
    SCHEMA_ACCEPTANCE_BASELINE_REF=d78aa30
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/home/roman/dev/helm-schema/target/round73-final-dump
    SCHEMA_PROBE_COVERAGE_REPORT=/home/roman/dev/helm-schema/target/round73-probe-coverage.json
    cargo nextest run -P integration -p helm-schema --test
    schema_emission_profiles -E
    'test(round73_fixture_flips_are_adjudicated_and_probe_caps_are_disclosed)'
    --run-ignored ignored-only --no-capture`; it reports
    `charts_checked=60 probes_checked=121059 flips=0` and round-trips the JSON
    coverage report before passing.
  - Complete clean IR dump: `TMPDIR=/home/roman/dev/helm-schema/target/round73-ir-final-dump
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run -P integration -p
    helm-schema-ir --test corpus -E 'test(ir_corpus_fixtures_match)'`; all 18
    artifacts are byte-identical.
- Gates on the final Round 73 tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; the full workspace Clippy pass and all three
    ast-grep checks complete.
  - `task lint:fc`: exit 0; 48 feature combinations across 13 packages and
    three targets complete with zero errors and zero warnings.
  - `cargo nextest run --workspace`: exit 0; 1,201 tests pass.
  - `task test:integration`: exit 0; 564 tests pass and 19 are skipped by the
    integration profile.
  - `task test:all`: exit 0; 1,769 tests pass and 19 are skipped by the CI
    profile, including the live-network lane.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0.
  - `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml
    check:local`: exit 0; 32 charts complete.
  - `task tokei:core`: exit 0; 60,893 production Rust LOC.
- Commit: `f3939c3` (`fix(schema): close round-73 review findings`).
