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

- Status: in-progress.
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
- Commit: pending; Step 0a records the resulting hash after this commit is
  created.

## Step 0a — harness and relax-host preflight

- Status: pending.
- Measured results: pending.
- Deviations: none.
- Adjudication evidence: pending.
- Review dossier: pending.
- Gates: pending.
- Commit: pending.

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
