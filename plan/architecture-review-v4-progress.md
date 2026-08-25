# Architecture review v4 progress

## Decision register

- Frozen plan: `plan/architecture-review-v4.md` at `bb61a78f`.
- Frozen-plan policy: the plan remains byte-identical to `bb61a78f`; this ledger is the only
  campaign prose surface.
- Wave scope: G1, G3; Part A A1--A8 and S-A; the S-D IR-privacy item; B1 in three rounds; G2
  stage 1; B4a crate by crate; then B2/E1 with G2 stage 2, B4b, and E2 only while their adoption
  gates remain green.
- Starting tree: clean `main` at `0a31f95e`, one version-bump commit after the requested
  `bb61a78f` starting point.
- Starting production Rust LOC: 62,082.
- Helm adjudicator: Helm 4.2.3; zero candidate-accepts/Helm-aborts cells are allowed.
- Downstream gate: `/Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml`, using the
  repository-external macOS `xargs`/`flock` shims and the freshly installed
  `/Users/roman/.cargo/bin/helm-schema` binary.

## Tooling prerequisite — pin the Helm adjudicator

- Status: landed in `82f0ea11`.
- Contract: tooling-only; pin Helm 4.2.3 in `mise.toml` and its Linux, macOS, and Windows
  artifacts in `mise.lock` because acceptance adjudication depends on exact rendering behavior.
- Justification: the floating `latest` selector advanced to Helm 4.2.4 during the first round and
  correctly tripped the authoritative battery's version guard before any probe ran.
- Verification: `mise current helm` reports `4.2.3`; `helm version --template
  '{{.Version}}'` reports `v4.2.3`; `task lint:actions`, `git diff --check`, and the frozen-plan
  check each exit 0.
- Production Rust LOC delta: 0.

## Round G1+G3 — de-bias capped probes and validate capability rows

- Status: landed in this round's `test(infra)` commit.
- Contract: test infrastructure only; preserve production behavior and every schema/IR fixture
  byte while replacing DFS-prefix battery truncation with deterministic round-robin sampling over
  `(top-level key, replacement kind)` buckets, rejecting batteries where targeted probes consume
  the entire base budget, and validating every capability-probe table row against the pinned
  provider corpus.
- Acceptance baseline: `82f0ea11`.
- Baseline production Rust LOC: 62,082.
- Pre-registered acceptance expectations:
  - Generated schemas, IR documents, lean schemas, and final-output fixtures remain byte-identical.
  - The full-depth old-versus-new acceptance battery reports zero flips because the round changes
    probe selection and validation only, not analyzer or generator production code.
  - Mandatory base and third-level categories retain zero drops for the 60-chart authoritative
    battery.
  - A synthetic capped battery visits every available `(top-level key, replacement kind)` bucket
    once before taking a second probe from any bucket.
  - A synthetic battery with zero emitted base probes is rejected explicitly even when targeted
    probes fill the overall budget.
  - Every declarative capability-probe row resolves to a real resource in the pinned provider
    corpus; an incorrect api-version/kind association fails the test.
  - Any schema fixture change or acceptance flip stops the round before adoption.

- Measured results:
  - Base path replacements are grouped by the encoded top-level values key and the stable
    replacement-table index. A `BTreeMap` fixes bucket order; the sampler takes one item from every
    non-empty bucket per cycle. The two whole-document controls remain first, and DFS order remains
    deterministic only within a bucket.
  - The coverage validator now fails explicitly when `base_emitted == 0`, before ordinary drop and
    count accounting. A focused synthetic case pins the target-only failure, and another pins one
    complete bucket cycle before any bucket repeats.
  - The capability corpus now spans Kubernetes 1.15, 1.24, 1.29, and 1.35. The test reads the
    upstream `x-kubernetes-group-version-kind` metadata from 463 distinct resource identities and
    proves that all 41 declarative `(apiVersion, kind)` rows exist independently of the table's own
    spelling.
  - The immutable final3 archive contains 88 binaries and 126 files. One clean schema dump writes
    84 artifacts; one clean IR dump writes 18 artifacts. The integration gate confirms every
    tracked schema and IR fixture remains byte-identical.
  - The final3 full-depth comparison against `82f0ea11` checks 121,055 probes across 60 charts and
    reports zero acceptance flips. Mandatory coverage is 112,260/112,260 base probes and
    7,465/7,465 third-level probes, with zero drops in both categories.
  - Bounded accounting records 427 emitted guard pairs, 36,339 dropped guard-witness candidates,
    238 emitted composite pairs, 2,277 composite pairs dropped by cap, 121,055 emitted probes, and
    28,874 disclosed total drops.

- Deviations:
  - The requested starting point was `bb61a78f`, but the clean branch had advanced to `0a31f95e`
    through the 0.0.6 version bump. The frozen plan remains compared to `bb61a78f`; the first
    acceptance baseline was the actual clean head.
  - The first authoritative prober preflight exited nonzero before emitting a probe because
    `helm = "latest"` had advanced locally to Helm 4.2.4. The user authorized an exact pin. The
    separate tooling commit `82f0ea11` pins Helm 4.2.3 and becomes this round's actual acceptance
    baseline; no failed-preflight artifact was adopted.
  - The checked-in provider bundle initially lacked positive historical documents for every API
    deprecation window. Actual upstream `_definitions.json` documents for Kubernetes 1.15 and 1.29
    were added to the pinned test corpus. Their SHA-256 digests are
    `9ddc4c6b8a57e29e22c1eb0fd5abf564968f3de95a48732845dc62be57f47610` and
    `522b38c0a75cb25f9fc1672dc5e5ad7bdf418cbad3e9b5b9fadb57dde8dfea99`; the test consumes GVK
    metadata rather than a hand-copied table.
  - Final1 artifacts preceded the Helm pin and final2 artifacts preceded the lint repair. Both
    immutable batches were discarded. Only final3 schema, IR, and prober artifacts form the
    adopted dossier.
  - The first `task lint` gate exited 201 because the sampling code grew
    `structural_probe_battery_with_coverage` to 106 lines against the 100-line limit. Base-probe
    construction moved into one cohesive helper without a lint suppression; the gate restarted
    and passed.
  - The integration and full-battery gates reported slow notifications for existing whole-chart
    cases. Integration completed in 949.821 seconds and `task test:all` in 1,005.490 seconds; every
    test passed and no case approached the 600-second per-test kill threshold.

- Adjudication evidence:
  - Helm 4.2.3 is selected by the committed mise pin and confirmed by the battery's version guard.
  - The final3 comparison finds zero flips, so no individual fixture cell requires a render
    verdict. The fixed allowance remains zero and the report records zero
    candidate-accepts/Helm-aborts cells.

### Producer and route coverage

| Route | Final construction | Verification |
|---|---|---|
| Whole-document controls | Defaults and all-declared-keys-deleted remain the first two base probes. | Existing battery labels and 112,260/112,260 base accounting remain intact. |
| Path replacement probes | Each path enters the bucket keyed by its top-level key and replacement-table index. | The synthetic three-bucket test proves a full cycle precedes every repeat. |
| Guard and composite probes | Targeted probes retain their existing independent constructors and are appended after base and third-level lanes. | 427 guard pairs and 238 composite pairs emit; all bounded drops remain disclosed. |
| Exhausted base budget | Coverage validation rejects `base_emitted == 0` before accepting total targeted coverage. | The target-only synthetic battery is rejected. |
| Resource-qualified capability query | Exact `apiVersion/kind` queries continue to bypass the canonical-kind table. | Existing direct-resource tests remain byte- and behavior-exact. |
| Group/version capability query | Every one of the 41 table rows must match an upstream GVK identity across the four pinned release documents. | The corpus-gated row test passes; a wrong apiVersion or kind produces a non-empty missing set. |

### Review dossier

- Focused sampling proof: `cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E 'test(capped_base_probes_visit_every_bucket_before_repeating) or
  test(probe_coverage_validation_rejects_target_only_battery)'`; exit 0, two tests pass.
- Focused capability proof: `cargo nextest run -p helm-schema-k8s -E
  'test(capability_probe_rows_exist_in_pinned_provider_corpus)'`; exit 0, one test passes and all
  41 rows resolve within the 463-identity release corpus.
- Immutable build proof: `TMPDIR=target/arch-v4-g1g3-final3-build cargo nextest archive
  --workspace --archive-file /private/tmp/arch-v4-g1g3-final3.tar.zst`; exit 0, 88 binaries and 126
  files archived after the final test-code edit.
- Clean schema dump: `TMPDIR=target/arch-v4-g1g3-final3-schema SCHEMA_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-g1g3-final3.tar.zst --profile integration --no-fail-fast -E
  'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written in one batch.
- Clean IR dump: `TMPDIR=target/arch-v4-g1g3-final3-ir SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest
  run --archive-file /private/tmp/arch-v4-g1g3-final3.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written in one
  batch.
- Full-depth proof: `TMPDIR=target/arch-v4-g1g3-final3-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=82f0ea11
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=target/arch-v4-g1g3-final3-schema
  SCHEMA_PROBE_COVERAGE_REPORT=target/arch-v4-g1g3-final3-coverage.json ADJUDICATE_WITH_HELM=1
  cargo nextest run --archive-file /private/tmp/arch-v4-g1g3-final3.tar.zst --profile integration
  -E 'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Scope proof: `git diff --stat 82f0ea11` contains test harnesses, the pinned provider test corpus,
  and this ledger only; production Rust and fixture outputs are unchanged.
- Public/wire decision: none. This round changes private test infrastructure and vendored test
  inputs only.

### Self-adversarial pass

- Sampling order: a plain interleave by top-level key would still cluster replacement kinds. The
  chosen compound key makes both dimensions participate before any bucket repeats while preserving
  deterministic order.
- Control pressure: the two whole-document probes are intentionally outside the bucket cycle so
  they cannot disappear behind a large values tree.
- Budget pressure: round-robin order alone cannot help if targeted probes consume all capacity.
  The explicit zero-base failure prevents such a battery from being reported as covered.
- Corpus independence: duplicating the capability table into a manifest would only test hand-sync.
  The adopted test instead extracts standardized GVK metadata from upstream OpenAPI documents
  spanning both live and removed API versions.
- Cache law: G3 validates the retained declarative table without changing capability lookup,
  upstream-first probing, negative-cache authority, or the permissive `None` outcome.
- Comment audit: the one new Rust test summary explains why multiple releases are necessary; no
  implementation-history or decorative comments were added.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflight.
- `task lint:fc`; exit 0, 48 feature combinations across 13 packages and three targets.
- `cargo nextest run --workspace`; exit 0, 1,270 tests pass.
- `task test:integration`; exit 0, 564 tests pass in 949.821 seconds.
- `task test:all`; exit 0, 1,838 tests pass in 1,005.490 seconds, including four live network
  tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0, version 0.0.6 installed.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,082 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: 0 (62,082 to 62,082).
