# Schema emission profiles implementation progress

Reference: `plan/schema-emission-profiles.md` v2.6 (frozen).

## Open decision points

### Lean contract

- Default: proceed with the recommended middle-point contract: mandatory facts and local
  conditionals remain enabled; root-anchored conditionals, kind partitions, and terminal clauses
  are disabled.
- Status: pending Step 2 measurements and the Step 3 reconfirmation.
- Veto window: open until the affected lean fixtures ship.

### Temporal migration

- Default: migrate the chart-local integration to `helm-schema.yaml` with `profile: lean` and
  `emission.local-conditionals: off`, removing the CLI profile flag.
- Status: pending Step 2 implementation and measurements.
- Veto window: open until the affected temporal fixture/config ships.

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

- Status: in-progress.
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
- Commit: pending; Step 1a records the resulting hash after this commit is
  created.

## Step 1a — fact model, phase split, and reporting

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 1a.1 — unconditional termination producer

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 1b — immutable lowering plan and multi-policy projection

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

## Step 2 — lean policy and annotations

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
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
