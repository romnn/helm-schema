# Performance review v1 progress

## Decision register

- Frozen plan: `plan/performance-review-v1.md` at `1ce9e660`.
- Frozen-plan policy: the plan remains byte-identical to `1ce9e660` for the whole campaign. This
  ledger is the only campaign prose surface, including measurements that disagree with the frozen
  estimates.
- Wave scope and order: round 0, then C1, A1, A2, A2b, B2, B1, E1, A3a, A3, R1, A6, A4, E2,
  E3, A7, and the A5 study. Each item is adopted or rejected only by its frozen criterion.
- Starting tree: clean `main` at `1d20fb66`. While round 0 was being measured, the concurrent bug
  hunt landed documentation-only commits through `9abc724f`; executable inputs and all ten
  reference chart trees are byte-identical across that range, and a rebuild at `9abc724f`
  reproduced the measured binary byte-for-byte. The separately authorized baseline repairs landed
  in `6ceaf9bc` and `8fbcc732`; `8fbcc732` is therefore the acceptance baseline for C1.
- Starting production Rust LOC: 66,064 (`task tokei:core`).
- Corpus state: 163 chart directories, 156 schema artifacts, and 18 symbolic-IR artifacts. The
  authoritative battery remains
  `round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced` in
  `crates/helm-schema/tests/schema_emission_profiles.rs`.
- Helm adjudicator: Helm 4.2.3 (`v4.2.3`, Git commit
  `43e8b7feece8beb0fcba47059ec9b522fd929a64`, Go 1.26.5, Kube client 1.36).
- Measurement host: Apple M3 Pro, macOS, private campaign root
  `/private/tmp/helm-schema-performance-v1.LSEe9Y`. The round-0 decision window stayed below
  load1 4.0; later rounds must record every invocation's load independently.
- Cache snapshot: 186 K8s files / 3.8 MiB, aggregate SHA-256
  `91075ab6af3b13c23b56a793d3281479b00e4bfa79f7b160bdf92d35a53345c2`; 25 CRD files /
  2.1 MiB, aggregate SHA-256
  `ae659b0e8e5b6f23b1827450d28bcb6710006fc318b9c837f22378a48d14fb08`.
- Downstream gate: `task -t
  /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml check:local`, after installing
  the exact candidate from `./crates/helm-schema-cli/`. The campaign never pushes.

### Re-baselined ten-chart decision table

The reference chart directories have no diff from the frozen review tree `b3475dec`, so the
structural columns are retained from that plan. Output byte sizes and JSON node counts were
independently recomputed from the round-0 artifacts and match all ten frozen rows. CPU is
`user + sys`; all timing runs are offline, compact, and use Kubernetes v1.35.0.

| Chart | n | CPU s median (min–max) | Wall s median (min–max) | load1 per run | templates | actions | defines | `.Values` paths | subcharts | output MB | output nodes | µs CPU / action |
|---|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| coredns | 5 | 0.33 (0.33–0.33) | 0.34 (0.34–0.35) | 3.40, 3.40, 3.21, 3.21, 3.21 | 17 | 707 | 9 | 120 | 0 | 0.36 | 9,552 | 467 |
| metrics-server | 5 | 0.16 (0.16–0.17) | 0.17 (0.17–0.18) | 3.40, 3.40, 3.21, 3.21, 3.21 | 20 | 404 | 11 | 82 | 0 | 0.18 | 5,125 | 396 |
| istiod | 5 | 0.57 (0.57–0.57) | 0.58 (0.57–0.59) | 3.40, 3.40, 3.21, 3.21, 3.03 | 28 | 948 | 8 | 126 | 0 | 0.29 | 14,018 | 601 |
| cert-manager | 5 | 0.67 (0.67–0.69) | 0.69 (0.68–0.72) | 3.40, 3.21, 3.21, 3.21, 3.03 | 48 | 1,521 | 19 | 226 | 0 | 0.70 | 16,249 | 440 |
| argo-cd | 3 | 5.59 (5.54–5.86) | 5.66 (5.61–5.92) | 2.78, 3.37, 2.13 | 166 | 5,536 | 63 | 1,291 | 1 | 2.08 | 77,624 | 1,010 |
| grafana | 3 | 4.37 (4.35–4.81) | 4.39 (4.37–5.10) | 2.59, 3.50, 2.20 | 39 | 2,677 | 29 | 425 | 0 | 1.26 | 49,452 | 1,632 |
| cilium | 3 | 7.88 (7.88–9.21) | 7.95 (7.95–9.74) | 3.34, 3.54, 2.43 | 148 | 5,861 | 49 | 1,126 | 0 | 1.70 | 78,339 | 1,344 |
| datadog | 3 | 63.06 (63.06–66.46) | 63.24 (63.23–69.06) | 3.60, 3.33, 2.39 | 143 | 6,534 | 203 | 838 | 4 | 2.10 | 118,295 | 9,651 |
| airflow | 3 | 88.45 (88.25–90.74) | 88.63 (88.45–91.10) | 3.80, 2.67, 2.45 | 160 | 8,612 | 223 | 1,333 | 1 | 3.90 | 175,792 | 10,271 |
| kube-prometheus-stack | 3 | 119.77 (119.58–124.44) | 120.00 (119.80–126.42) | 3.72, 1.84, 2.05 | 265 | 21,004 | 112 | 1,985 | 5 | 6.99 | 294,793 | 5,702 |

The quiet-host re-baseline contradicts the frozen table most strongly on the three large charts:
datadog is 22.2% faster than 81.1 s, airflow is 20.7% faster than 111.5 s, and
kube-prometheus-stack is 22.3% faster than 154.2 s. This confirms the plan's own warning that its
loaded-host large-chart numbers were inflated by roughly 20%. The absolute wave-1 targets do not
move; the required reductions from this baseline are now 52.4% for datadog to get below 30 s,
43.5% for airflow to get below 50 s, and 8.2% for kube-prometheus-stack to get below 110 s. The
mid-chart target below 4 s requires 28.4% from argo-cd, 8.5% from grafana, and 49.2% from cilium.

## Round 0 — current-tree re-baseline

- Status: landed in `6988a949`; two reproducible pre-existing integration-gate failures initially
  blocked closure and were resolved by the separately authorized prerequisite below.
- Contract: measurement and campaign-prose infrastructure only. Build and preserve the exact
  starting binary, establish one private cache snapshot, prove online/offline identity on all ten
  reference charts, re-measure the decision baseline, and record current corpus/LOC/tool state.
  No production, test, fixture, corpus, or frozen-plan byte may change.
- Acceptance baseline: `9abc724f`. Measurement began at `1d20fb66`; the only intervening paths
  are bug-hunt prose, and the rebuilt `9abc724f` executable is byte-identical.
- Baseline production Rust LOC: 66,064.
- Pre-registered acceptance expectations:
  - Online and warm-offline schema, stdout, JSON diagnostic, and exit bytes are identical for all
    ten charts.
  - Every repeat of a chart is deterministic on those same four channels.
  - The copied binaries built at `1d20fb66` and the documentation-only successor `9abc724f` are
    byte-identical.
  - No schema or IR fixture changes; no corpus acceptance flips; frozen plan unchanged.
- Performance baseline: the decision-register table above is the campaign baseline. It replaces
  the frozen plan's loaded-host table for every later paired decision.
- Measured results:
  - The initial clean release build took 60.10 s wall, 434.99 s user, and 28.06 s sys. The binary
    was copied immediately; size 16,061,424 bytes, SHA-256
    `89c242c64d55357032e7dce45749bf3cc0cc0428d1986662ea434a83ae3db06b`.
  - All ten online warm runs and all ten warm-offline runs exited 0. Schema, stdout, JSON
    diagnostics, and exit-status files are byte-identical online versus offline for every chart.
  - Every baseline repetition exited 0 and produced one unique hash per chart for schema, stdout,
    and diagnostics. CPU/wall medians, ranges, and invocation-order load values are preserved in
    the decision-register table.
  - `helm version` reports v4.2.3. Current inventory is 163 chart directories, 156 schema
    artifacts, 18 IR artifacts, and the round-74 battery named above. Production Rust LOC is
    66,064.
  - Rebuilding after the documentation-only concurrent advance took 0.51 s and emitted a binary
    byte-identical to the measured copy, validating the retained measurements against the actual
    first implementation baseline.
- Deviations:
  - `ps -Ao ...` was denied by the sandbox (`operation not permitted`), so external build-process
    observation was unavailable. No campaign build overlapped a decision run; the load and
    wall/CPU evidence is retained so third parties can judge host contention.
  - The first online-loop wrapper used zsh's read-only `status` parameter and exited 1 immediately
    after the first coredns invocation. No resulting artifact was used. The corrected wrapper used
    `rc` and restarted the complete ten-chart pass.
  - Load1 was 4.30 during inventory and 4.08 immediately before the decision window. The first
    actual timing invocation began at 3.40; every retained timing start was below 4.0. No retained
    wall/CPU ratio exceeds 1.10.
  - `main` advanced from `1d20fb66` to `9abc724f` during the measurements. `git diff` proves the
    range changes only `plan/schema-bug-hunt-v1.md` and its reports; executable inputs and all ten
    charts are unchanged. The required rebuild produced identical bytes, so the samples were not
    invalidated.
  - The first `task test:integration` final-tree gate ran all 665 tests in 933.616 s and exited 201:
    663 passed, 2 failed, and 24 were skipped. The failures were
    `lean_profile_schemas_match_their_separate_fixture_lane`, whose
    `schema-emission-temporal-wrapper` fixture lacks generated VPA `controlledResources` and
    `controlledValues` fields and has a corresponding conditional-order difference, and
    `corpus_charts_vendor_every_locked_dependency`, which reports unvendored
    `openldap-stack-ha` locks for `ltb-passwd 0.1.x` and `phpldapadmin 0.1.x`.
  - The exact integration command was rerun as the second honest attempt. It completed all 665
    tests in 931.056 s and exited 201 with the same two failures (663 passed, 24 skipped). HEAD
    remained `9abc724f`; the only worktree path was this new ledger. Because neither failure is
    attributable to the performance campaign and repairing either is outside the frozen item list,
    autonomy contract stop condition 4 fired before `task test:all` or the round-0 commit.
- Adjudication evidence: Helm v4.2.3. Round 0 has no candidate schema and therefore no flips or
  candidate-accepts/Helm-aborts cells. The cache identity check is 10/10 on all four output
  channels.
- Public/wire decision: none. This round changes only this ledger.

### Review dossier

- Clean build: `/usr/bin/time -p cargo build -p helm-schema-cli --release`; exit 0. Immediate copy
  to `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-1d20fb66`, followed by
  SHA-256 and byte comparison after the `9abc724f` rebuild.
- Cache snapshot: copy `~/.cache/helm-schema/kubernetes-json-schema` to
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s` and `crds-catalog` to the sibling
  `crd` directory. Aggregate hashes are computed from sorted per-file SHA-256 rows.
- Identity environment:
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s
  HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`.
  Online runs use the copied binary with `<chart> --compact --k8s-version v1.35.0 --diag-format
  json --output <online.schema.json>`; offline runs add `--offline`. Program stdout/stderr and
  status are separate files. All 40 channel comparisons pass for ten charts.
- Timing command shape: record `uptime` and UTC start, then
  `DIAG=<stderr.jsonl> /usr/bin/time -p sh -c 'exec "$@" 2>"$DIAG"' sh <copied-binary>
  testdata/charts/<chart> --compact --offline --k8s-version v1.35.0 --diag-format json --output
  <schema.json> > <stdout> 2> <time.txt>`. Five runs were used for the four charts below 2 s;
  three for the other six.
- Artifact proof: each chart's baseline schema, stdout, and diagnostic files have one unique
  SHA-256 across repetitions; every status is 0. `jq '[paths] | length + 1'` recomputed output
  node counts, and `wc -c` recomputed compact byte sizes.
- Scope proof: `git diff --quiet 1d20fb66..9abc724f -- Cargo.toml Cargo.lock mise.toml
  Taskfile.yml crates <ten chart paths>` exits 0. `git diff --exit-code 1ce9e660 --
  plan/performance-review-v1.md` exits 0.

### Self-adversarial pass

- Differently built binary: the baseline never uses an installed executable. Both relevant HEADs
  emit the exact preserved bytes, so the concurrent documentation commit cannot contaminate the
  comparison.
- Cache state: every timed call is warm and offline against one private snapshot. The preceding
  online/offline check includes diagnostics and statuses, not schema alone.
- Host state: per-run load is disclosed in invocation order. Every retained load is below the
  frozen protocol's loaded threshold, and CPU/wall ratios remain below 1.10. The denied process
  listing prevents a stronger claim than that.
- Statistic integrity: raw `real`, `user`, and `sys` files remain in the private campaign root;
  medians are not mixed across online, traced, symbolized, or differently built runs.
- Static table reuse: only structural columns are carried from the frozen plan, and the ten chart
  inputs are byte-identical to its reviewed tree. Output sizes/nodes are independently reproduced.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 0.961 s.
- `task lint`: exit 0; the whole workspace compiles under Clippy and all three AST-grep policy
  tests pass. The two existing escaped-newline findings remain informational.
- `task lint:fc`: exit 0; 48 feature combinations for 13 packages across three targets pass in
  174.99 s with zero errors and warnings.
- `cargo nextest run --workspace`: exit 0; 1,338/1,338 tests pass in 104.522 s.
- `task test:integration`, attempt 1: exit 201; 663/665 pass in 933.616 s, 2 fail, 24 skip.
- `task test:integration`, attempt 2: exit 201; 663/665 pass in 931.056 s, the same 2 fail, 24
  skip. This reproducible pre-existing failure blocks the round.
- `task test:all`: not run because autonomy contract stop condition 4 fired after the second
  integration attempt.
- Downstream luup2: not required; round 0 changes no schema semantics.
- `task tokei:core`: exit 0; 66,064 production Rust LOC.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0.
- `git diff --check`: exit 0 after recording the blocker.

- Measured production LOC delta: 0 (66,064 to 66,064).

## Baseline prerequisite — restore the expanded corpus gates

- Status: landed as `6ceaf9bc` (`test(corpus): match local dependency constraints`) and
  `8fbcc732` (`fix(k8s): preserve third-party capability uncertainty`).
- Contract: repair only the two pre-existing failures that blocked round 0. Keep corpus integrity
  strict, preserve structural ambiguity for third-party capability queries, and update a fixture
  only after full-depth acceptance adjudication. Do not begin a frozen performance item.
- Acceptance baseline: `9abc724f`; the round-0 ledger-only commit `6988a949` changes no executable
  input.
- Baseline production Rust LOC: 66,064.
- Pre-registered acceptance expectations:
  - Existing unpacked dependencies satisfy exact lock versions or Helm-compatible version
    requirements, including Helm's optional leading `v`; missing or out-of-range dependencies
    still fail.
  - The Kubernetes bundle abstains for third-party resource-qualified capability queries because
    absence from the built-in bundle cannot prove a CRD is absent from a cluster.
  - The temporal-wrapper lean fixture returns to byte identity without regeneration.
  - Any corpus fixture change is accepted only with zero candidate-accepts/Helm-aborts cells.
  - The ten reference charts remain byte-identical on schema, stdout, JSON diagnostics, and exit
    status.
- Performance baseline: not a performance round. The round-0 decision table remains authoritative.
- Measured results:
  - `Chart.lock` preserves `0.1.x` for openldap's two local dependencies while their vendored
    charts identify versions 0.1.0 and 0.1.2. Two other dependencies use `v0.6.0` and `v1.8.0`
    locks against unprefixed vendored versions. Exact version parsing remains exact; only entries
    that are not concrete versions are interpreted as semver requirements.
  - `build_capability_probe` now reuses the existing built-in group classifier before constructing
    a resource-qualified Kubernetes probe. `autoscaling.k8s.io/v1/VerticalPodAutoscaler` therefore
    abstains instead of promoting a Kubernetes negative-cache record into evidence about an
    installed CRD.
  - The temporal-wrapper lean schema again matches its existing fixture. The corpus has one byte
    change, oncall, because it embeds the same Prometheus VPA helper and its expansion-era fixture
    had captured the false negative capability answer.
  - The candidate release binary is 16,061,440 bytes with SHA-256
    `73ec9a2fc51f4fd8796e4dc4e2b492d61888ebfcebe3b8be732376790e64313e`.
  - All ten reference charts are byte-identical to round 0 on schema, stdout, JSON diagnostics,
    and exit status; every exit is 0.
- Deviations:
  - After local version requirements were handled, the integrity test exposed the two leading-`v`
    lock spellings. Both dependencies were already vendored at the correct concrete versions; the
    matcher was extended rather than weakening or quarantining the gate.
  - A focused candidate generation first appeared to suggest refreshing the temporal lean fixture.
    Source tracing instead showed that the expanded provider bundle's negative-cache record had
    exposed a cache-law defect for third-party capabilities. No lean fixture was changed.
  - The first full repaired-tree integration run identified oncall as the sole corpus byte change
    and exited 201 before fixture adoption. A clean single-chart dump from the final build replaced
    only that fixture after the battery reported zero flips.
  - The instructed Linux luup2 taskfile path does not exist on this macOS host and exited 100. The
    checkout is `/Volumes/T7/branches/luup2`. Its first run there exited 201 because BSD `xargs`
    lacks `-a`; the documented repository-external `xargs`/`flock` shims then ran the unchanged
    downstream tree successfully.
- Adjudication evidence:
  - Helm v4.2.3 was verified immediately before adjudication.
  - The round-74 full-depth battery compared oncall at `9abc724f` with the clean candidate dump:
    one chart, 1,947 probes, zero acceptance flips, and zero candidate-accepts/Helm-aborts cells.
    The oncall fixture change is therefore an acceptance-preserving re-spelling under the complete
    bounded battery.
  - Schema fixtures are exact after adopting the one clean oncall dump; all 18 IR artifacts remain
    byte-identical.
- Public/wire decision: none. The capability query type and provider trait are unchanged. The
  existing built-in/third-party group classifier now also guards the concrete Kubernetes probe
  boundary.

### Review dossier

- Focused oracle proof: `cargo nextest run -p helm-schema-k8s -E
  'test(third_party_resource_qualified_probe_abstains)'`; exit 0, one test passes.
- Focused blocker proof: `cargo nextest run --workspace --profile integration -E
  'test(unpacked_dependency_with_wrong_version_is_not_vendored) |
  test(corpus_charts_vendor_every_locked_dependency) |
  test(lean_profile_schemas_match_their_separate_fixture_lane)'`; exit 0, three tests pass.
- Candidate build: `cargo build -p helm-schema-cli --release`; exit 0, followed immediately by
  copying `target/release/helm-schema` to
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-prerequisite`.
- Ten-chart byte gate: the candidate and preserved round-0 binary use
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s` and
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, with
  `--compact --offline --k8s-version v1.35.0 --diag-format json`; all 40 channel comparisons pass.
- Clean oncall dump: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/prerequisite-dump
  SCHEMA_DUMP=1 cargo nextest run -p helm-schema-cli --profile integration --test chart_corpus -E
  'test(oncall)'`; exit 0. The generated artifact SHA-256 is
  `d626f561c94e038f1a1f625df8eec2ba21a7529615c68f4e0fbd042f76ce5160`.
- Acceptance battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/prerequisite-adjudication
  SCHEMA_ACCEPTANCE_BASELINE_REF=9abc724f
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/private/tmp/helm-schema-performance-v1.LSEe9Y/prerequisite-dump
  SCHEMA_ACCEPTANCE_CHART=oncall
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/prerequisite-adjudication/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 1,947 probes and zero flips.

### Self-adversarial pass

- A permissive semver parser could turn an exact lock into a range. The matcher first parses a
  concrete version and requires equality; it falls back to `VersionReq` only when the lock is not a
  concrete version.
- Stripping `v` indiscriminately could accept malformed versions. Both normalized strings still
  have to parse as semver, and the exact/range relation is checked afterward.
- Asking the CRD catalog whether a capability exists would make a public catalog stand in for the
  target cluster. The correction instead preserves `None`, so typed branch analysis keeps the
  chart's exact alternatives without claiming cluster state.
- A fixture-order change can conceal a semantic change. The full-depth battery checked 1,947
  coalesced oncall probes and found zero flips; no schema-only assertion was used as adjudication.
- The architecture stays on the right hill: the existing built-in classifier owns the distinction,
  and no new capability list, provider facade, or cache representation was introduced.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 0.857 s.
- `task lint`: exit 0; whole-workspace Clippy and three AST-grep policy tests pass. The two existing
  escaped-newline findings remain informational.
- `task lint:fc`: exit 0; 48 feature combinations for 13 packages across three targets pass in
  51.65 s with zero errors or warnings.
- `cargo nextest run --workspace`: exit 0; 1,339/1,339 tests pass in 104.726 s.
- `task test:integration`: exit 0; 665/665 tests pass in 951.994 s, with 24 profile skips.
- `task test:all`: exit 0; 2,008/2,008 tests pass in 1,065.080 s, with 24 profile skips and live
  network tests included.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; release build and replacement complete
  in 49.64 s.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 66,067 production Rust LOC.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +3 (66,064 to 66,067).

## Round C1 — key condition fragments by selected values document

- Status: landed in `7cee5c6c`.
- Contract: make `ConditionFragmentCache` include the identity of the exact guarded root-values
  document selected by `RootValuesDocuments::condition_context`. Do not change guard lowering,
  document selection, absence semantics, ordering, public API, or wire formats.
- Acceptance baseline: `acd226a4` for prose identity and `8fbcc732` for executable/schema identity.
- Baseline production Rust LOC: 66,067.
- Pre-registered acceptance expectations:
  - Most likely outcome: all ten reference charts, all 156 schema artifacts, all 18 IR artifacts,
    diagnostics, stdout, and exit statuses remain byte-identical because guarded values-default
    sources are rare and the collision is latent.
  - If schema bytes change, every change must be attributable to two guard sets previously sharing
    `(ancestor, guard)` while selecting different values documents. Such changes are correctness
    fixes, not performance wins, and require the full-depth Helm 4.2.3 battery with zero
    candidate-accepts/Helm-aborts cells before adoption.
  - A focused regression must prove that identical ancestor/guard pairs under distinct selected
    document identities cannot reuse each other's condition fragment.
  - Empty-cache online, warm-online, and warm-offline schema, stdout, JSON diagnostics, and status
    must agree for all ten reference charts.
- Performance baseline: paired against the preserved `8fbcc732` executable. CPU medians and
  ranges for A are: coredns 0.33 s (0.33–0.35), metrics-server 0.17 s (0.16–0.17), istiod
  0.57 s (0.57–0.57), cert-manager 0.68 s (0.68–0.70), argo-cd 5.70 s (5.67–5.78), grafana
  4.69 s (4.33–4.83), cilium 8.44 s (8.39–8.46), datadog 62.77 s (62.61–65.66), airflow
  90.30 s (90.06–90.77), and kube-prometheus-stack 121.95 s (118.68–122.36).
- Measured results:
  - The selected guarded document is now returned with its stable index from the same
    `condition_context` selection. The base document is `None`; guarded documents are
    `Some(index)`. Both condition-fragment cache call paths include that identity in their key.
  - A private regression primes the cache from the base document, requests the same ancestor and
    guard from a distinct guarded document, and proves the cached result equals an uncached
    construction while differing from the base fragment.
  - The candidate release binary is 16,061,440 bytes with SHA-256
    `4d36dbb79dbc066e7f16ba2995a1cdbe41d371c555d0edc9b8d55400e9151c13`.
  - All ten reference charts are byte-identical to the acceptance executable on schema, stdout,
    JSON diagnostics, and exit status. The empty-cache-online, warm-online, and warm-offline
    triple is also exact on all four channels for every chart.
  - All 156 schema artifacts and all 18 IR artifacts remain byte-identical. The 160-chart battery
    ran 284,869 coalesced probes and found zero acceptance flips.
  - Candidate B CPU medians and ranges are: coredns 0.33 s (0.33–0.34), metrics-server 0.17 s
    (0.16–0.18), istiod 0.57 s (0.57–0.58), cert-manager 0.68 s (0.68–0.69), argo-cd 5.71 s
    (5.67–5.82), grafana 4.65 s (4.38–5.03), cilium 8.48 s (8.44–8.99), datadog 63.10 s
    (62.68–65.01), airflow 90.24 s (90.22–90.36), and kube-prometheus-stack 122.02 s
    (120.69–122.33).
  - Median paired gains, with paired ranges, are: coredns 0.000% (-3.030%–+2.941%),
    metrics-server -5.882% (-6.250%–+5.882%), istiod 0.000% (-1.754%–0.000%), cert-manager
    0.000% (-1.471%–+1.429%), argo-cd -0.175% (-0.692%–0.000%), grafana -1.155%
    (-7.249%–+3.727%), cilium -0.596% (-6.517%–-0.236%), datadog -0.064%
    (-3.833%–+1.997%), airflow +0.089% (-0.333%–+0.584%), and kube-prometheus-stack -0.057%
    (-1.694%–+0.025%). Every paired interval except cilium crosses or touches zero; no gain is
    claimed. C1 is adopted because it repairs a cache-law violation and its frozen criterion says
    correctness fixes are never rejected for flat timing.
- Deviations:
  - The first `task lint` attempt exited 201 because the new regression used `assert!(left !=
    right)`, which Clippy correctly identified as a manual equality assertion. The test was changed
    to return `eyre::Result` and use `eyre::ensure!`; the final lint rerun completed the whole
    workspace successfully. No suppression was added.
  - The full battery completed before that test-only assertion spelling changed. The release
    executable, production code, fixture inputs, and candidate bytes were unchanged, so the
    284,869-probe result remains evidence for the exact candidate later gated in full.
  - The first committed dossier named the byte artifacts as `c1/byte-gate`; the actual retained
    directory is `c1/byte`. This ledger correction changes no evidence or repository artifact.
  - The host became loaded during the final integration gates and selected A/B pairs: retained
    load1 ranges are 2.36–2.48 for coredns, 2.36 for metrics-server, 2.36–2.41 for istiod,
    2.41–2.54 for cert-manager, 2.25–2.94 for argo-cd, 2.72–3.31 for grafana, 3.61–4.52 for
    cilium, 1.97–4.27 for datadog, 3.62–6.19 for airflow, and 2.44–4.19 for
    kube-prometheus-stack. The loaded samples remain disclosed, but flat/cross-zero results are
    not promoted into speed claims.
- Adjudication evidence: Helm v4.2.3. There are no schema byte changes and no acceptance flips;
  the battery's candidate-accepts/Helm-aborts cell is zero. Cache-state identity is 10/10 charts
  across empty online, warm online, and warm offline on all four captured channels.
- Public/wire decision: none. The cache key and selected-document identity are crate-private
  emission details; schema and diagnostic wire bytes are unchanged.

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-gen -E
  'test(condition_cache_separates_selected_values_documents)'`; exit 0, one test passes.
- Candidate build: `cargo build -p helm-schema-cli --release`; exit 0, followed immediately by
  copying `target/release/helm-schema` to
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-c1`.
- Ten-chart byte gate: preserved `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-prerequisite`
  and the C1 candidate use the round-0 K8s and CRD snapshot with `--compact --offline
  --k8s-version v1.35.0 --diag-format json`; schema, stdout, stderr, and status comparisons all
  pass under `/private/tmp/helm-schema-performance-v1.LSEe9Y/c1/byte`.
- Cache-law gate: for each reference chart, begin with distinct empty K8s and CRD directories,
  invoke the candidate online twice and offline once, and compare all four channels. Artifacts are
  under `/private/tmp/helm-schema-performance-v1.LSEe9Y/c1/cache-law`; every comparison passes.
- Acceptance battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/c1/battery
  SCHEMA_ACCEPTANCE_BASELINE_REF=acd226a4
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/c1/battery/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 160 charts, 284,869 probes, zero flips.
- Timing environment: both preserved binaries use
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s` and
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, with
  `--compact --offline --k8s-version v1.35.0 --diag-format json`. Randomized AB/BA pairs and each
  pre-invocation load record are under `/private/tmp/helm-schema-performance-v1.LSEe9Y/c1/measure`.
  Five pairs were retained for coredns, metrics-server, istiod, cert-manager, and datadog; three
  for the remaining charts.

### Self-adversarial pass

- A hash or document contents would be the wrong identity: mutable construction and equality cost
  are unnecessary. The stable index in the immutable `guarded` vector, with `None` for base
  documents, names exactly the selected input.
- Passing the identity only at one cache call site would leave the other reuse path under-keyed.
  Every `build_condition_clauses_cached` call must receive the identity returned with the exact
  document and `AbsenceDefaults` it uses.
- The index must come from the same selection operation as the document references; recomputing it
  separately could drift under ties.
- C1 is a correctness prerequisite, so a flat timing result cannot reject it and a speed movement
  cannot excuse a byte change.
- The guarded-vector index is stable only within one immutable `RootValuesDocuments`; the cache is
  created and consumed inside one emission pass over that exact owner, so the identity cannot
  cross owners.
- Small timing granularity makes several paired percentages look larger than their absolute
  movement. Decisions use CPU seconds, paired intervals, and the frozen correctness exception,
  not the percentage label alone.

### Gates on the final tree

- `cargo fmt --check`: exit 0; the final `cargo fmt --all` plus check sequence took 1.555 s (the
  check duration was not retained separately).
- `task lint`: first attempt exit 201 for the manual-equality test assertion described above;
  final attempt exit 0, with the whole workspace compiling under Clippy and all three AST-grep
  policies passing. The two existing escaped-newline findings remain informational.
- `task lint:fc`: exit 0; 48 feature combinations pass in 51.29 s.
- `cargo nextest run --workspace`: exit 0; 1,340/1,340 tests pass in 104.204 s.
- `task test:integration`: exit 0; 665/665 tests pass in 1,185.723 s, with 24 profile skips.
- `task test:all`: exit 0; 2,009/2,009 tests pass in 1,102.885 s, with 24 profile skips and live
  network tests included.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; exact candidate installed in 16.74 s.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`: exit 0; 32/32 charts
  pass in 53.44 s.
- `task tokei:core`: exit 0; 66,079 production Rust LOC in 0.848 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +12 (66,067 to 66,079).

## Round A1 — allocation-free `ValuesPath` and `Segment` ordering

- Status: landed in `0926805c`.
- Contract: replace comparison-time encoded `String` allocation with a lazy byte stream that is
  exactly equivalent to the current `encode()`/`encode_component()` order. Preserve derived
  segment equality, hashing, constructors, path syntax, public signatures, emitted ordering, and
  every wire byte. Measure the frozen cached-spelling variant independently and adopt it only if
  it beats the lazy candidate by at least 5% on the isolated `kubernetes-apps.yaml` chart.
- Acceptance baseline: `7cee5c6c` for executable/schema identity and `5cea0a47` for the complete C1
  ledger state.
- Baseline production Rust LOC: 66,079.
- Pre-registered acceptance expectations:
  - Property tests must prove `ValuesPath::cmp(a, b) == a.encode().cmp(&b.encode())` and that
    comparison equality agrees with structural equality over the root, prefixes, `EachMember`,
    literal `*`, ASCII punctuation including `.`, `\\`, `-`, `/`, and space, and multi-byte UTF-8.
  - The corresponding `Segment` property must match `encode_component()` exactly across the same
    literal and wildcard domain.
  - The lazy variant must keep all ten reference schemas, stdout, JSON diagnostics, statuses, all
    156 schema artifacts, and all 18 IR artifacts byte-identical, with zero corpus acceptance
    flips.
  - The frozen 6–25% hypothesis is not assumed to survive the expanded tree. Reject A1 if the
    paired gain is below 5% on both grafana and cilium, even if microbenchmarks or other charts
    improve.
  - The cached-spelling variant is rejected unless its paired gain over the lazy variant is at
    least 5% on the isolated Kubernetes applications template. A rejected variant is fully
    restored before final A1 gates.
- Performance baseline: the landed C1 candidate medians are coredns 0.33 s, metrics-server
  0.17 s, istiod 0.57 s, cert-manager 0.68 s, argo-cd 5.71 s, grafana 4.65 s, cilium 8.48 s,
  datadog 63.10 s, airflow 90.24 s, and kube-prometheus-stack 122.02 s. Final decisions use fresh
  interleaved pairs against the preserved C1 executable, not these unpaired row values.
- Measured results:
  - A lazy byte-stream comparator passes the generated path and segment ordering properties. Its
    preserved release binary is 16,078,064 bytes with SHA-256
    `27e9dd3a32c82eba1c11331f4e24613086af43f9482944ea64d43578c84d8797` and is byte-exact to C1
    on all ten reference charts across schema, stdout, JSON diagnostics, and status.
  - The frozen cached-spelling alternative stores a `Box<str>` maintained by every path
    constructor and mutation. The retained five-pair isolated-chart comparison reports lazy
    18.89 s CPU median (18.63–19.69) versus cached 9.84 s (9.40–10.59), with paired gains
    48.10% median (45.21%–50.24%). All retained pairs favor cached by far more than the 5%
    criterion, so cached is the selected final representation. Every load1 was above 4 and remains
    marked loaded.
  - The exact final cached candidate is 16,177,136 bytes with SHA-256
    `ff96f5453c5be509e930abeda0b68551ab76090c76dd7c810cebf95c1d37af38`. It is byte-exact to C1
    on all ten reference charts and to the lazy variant on the isolated chart, across all four
    channels.
  - The full-depth round-74 battery checks 160 charts and 284,869 coalesced probes with zero
    acceptance flips. All 156 schema fixtures and all 18 IR fixtures are byte-identical.
  - The final compiler-free curve is below. `A` is the preserved C1 binary and `B` is the exact
    cached-spelling A1 binary. CPU and wall columns are median (min–max); gain is the median and
    range of the paired CPU percentages.

    | Chart | n | A CPU s | B CPU s | A wall s | B wall s | paired gain | load1 range |
    |---|---:|---:|---:|---:|---:|---:|---:|
    | coredns | 5 | 0.35 (0.33–0.35) | 0.29 (0.28–0.30) | 0.35 (0.34–0.38) | 0.29 (0.29–0.34) | 17.14% (9.09–20.00%) | 3.65–3.65 |
    | metrics-server | 5 | 0.17 (0.17–0.17) | 0.15 (0.15–0.17) | 0.18 (0.18–0.18) | 0.16 (0.16–0.19) | 11.77% (0.00–11.77%) | 3.60–3.65 |
    | istiod | 5 | 0.58 (0.57–0.58) | 0.45 (0.45–0.45) | 0.59 (0.58–0.60) | 0.46 (0.45–0.47) | 22.41% (21.05–22.41%) | 3.47–3.60 |
    | cert-manager | 5 | 0.69 (0.68–0.71) | 0.57 (0.57–0.58) | 0.70 (0.70–0.71) | 0.58 (0.58–0.62) | 17.39% (14.71–18.57%) | 3.35–3.47 |
    | argo-cd | 3 | 5.86 (5.76–5.91) | 4.59 (4.55–4.60) | 5.90 (5.79–5.97) | 4.61 (4.59–4.70) | 22.17% (20.31–22.36%) | 3.19–3.43 |
    | grafana | 3 | 4.77 (4.62–5.24) | 3.14 (3.08–3.15) | 4.86 (4.73–5.82) | 3.25 (3.17–3.29) | 34.17% (31.82–41.22%) | 4.43–4.63 |
    | cilium | 3 | 7.97 (7.95–7.98) | 5.29 (5.27–5.31) | 8.00 (8.00–8.01) | 5.33 (5.28–5.40) | 33.46% (33.46–33.88%) | 3.46–4.06 |
    | datadog | 5 | 63.85 (63.70–64.16) | 36.29 (36.17–36.30) | 64.04 (63.93–64.42) | 36.48 (36.37–36.53) | 43.22% (43.04–43.42%) | 2.37–4.83 |
    | airflow | 3 | 89.20 (89.20–89.20) | 40.40 (40.35–40.51) | 89.47 (89.44–89.53) | 40.63 (40.54–40.75) | 54.71% (54.59–54.77%) | 2.79–8.39 |
    | kube-prometheus-stack | 3 | 120.76 (120.54–120.90) | 45.61 (45.60–45.67) | 121.16 (121.09–121.36) | 45.95 (45.94–45.99) | 62.23% (62.16–62.24%) | 2.30–3.86 |

  - No candidate median is slower. Metrics-server's paired range touches zero, so it is not
    claimed as a proven gain on that chart; every other paired range is strictly positive.
    Grafana and cilium both exceed the frozen 5% criterion by more than 30 points, so A1 lands.
  - The frozen 6–25% expectation reproduces on the six smaller/mid charts except grafana and
    cilium, which exceed it; the three large charts exceed it substantially. After A1, grafana,
    airflow, and kube-prometheus-stack already meet their absolute wave-1 targets. Argo-cd,
    cilium, and datadog remain above theirs at 4.59 s, 5.29 s, and 36.29 s respectively.
- Deviations:
  - The first focused nextest filter used the default profile, which excludes the integration-test
    binary, and exited 4 with zero tests. Rerunning with `--profile integration --test value_path`
    passed all six tests.
  - The first early `task lint` exited 201 because `[b'\\', b'*']` triggered
    `clippy::byte_char_slices`. Re-spelling it as `b"\\*"` made the next whole-workspace lint pass;
    no suppression was added.
  - The first battery attempt exited 101 before testing because its declared `TMPDIR` did not yet
    exist and clang could not create a temporary `ring` object. After creating that exact
    directory, the unchanged command passed in 217.853 s.
  - The first final timing wrapper produced one coredns invocation, then exited 2 because BSD awk
    treats `system` as a built-in rather than a writable variable. The fresh wrapper uses
    `sys_time`; the single sample under `a1/measure` is invalid.
  - A restarted whole-table window produced 52 invocations through the third grafana pair. A
    process snapshot then found an unrelated `cargo test` and multiple active `rustc` processes;
    grafana candidate CPU had simultaneously risen from about 2.9 s to 4.6–4.9 s. The command was
    interrupted with exit 130 and every sample under `a1/measure-final` was invalidated.
  - A five-pair grafana-only retry completed, but the immediate post-window snapshot found newly
    launched Clippy drivers. A three-pair cilium retry likewise overlapped an unrelated `cargo`
    check and `task lint:fc`. Both complete windows are invalid; none of their favorable pairs is
    used for acceptance.
  - More than two honest decision windows have therefore failed the non-negotiable
    no-concurrent-build condition for reasons outside this tree. External compilers continued to
    launch after cooldown periods, reaching load1 27.16. Autonomy stop condition 4 applies until
    the host can provide a compiler-free timing window.
  - While locating dossier lines, a shell diagnostic used Markdown backticks inside a
    double-quoted pattern and unintentionally launched `task lint`. Its output is not a gate and it
    further delayed host cooldown; no result from that interval is retained.
  - On the user-requested retry, two short external Cargo builds delayed the first clean start.
    Four isolated variant pairs then completed before another compiler began during pair 5; that
    fifth pair alone was discarded. A compiler-free replacement pair produced the retained
    five-pair result above.
  - Five grafana pairs followed. Process ages prove the first three completed before a new Clippy
    process began; pair 5 and its attempted replacement overlapped later cross-target checks and
    are discarded, while pair 4 is unused because the frozen repeat count was already met.
  - The interfering parent was then identified as PID 95163, an external `cargo-fc fc lint` that
    remained active beyond 10 minutes and repeatedly launched cross-target Clippy waves. The
    resumed run therefore hit the same non-concurrent-build blocker before cilium.
  - On the next user-requested retry, the host began at load1 3.76 with no compiler. Process
    snapshots at every chart boundary remained compiler-free through cilium, the four small
    charts, argo-cd, datadog, airflow, and kube-prometheus-stack. Those are the only samples in the
    final table.
  - Some retained invocations are still marked loaded: all grafana runs, one edge of cilium and
    datadog, and airflow up to load1 8.39. CPU intervals remain tight and wall/CPU stays close to
    one on the decision charts; wall is decorative where a sub-second run exceeds the 1.10 ratio.
  - The user proposed a lightweight follow-up with Criterion cases and opt-in Perfetto trace
    writing. That is good wave-2 infrastructure but is outside A1 and the frozen item list, so it
    is deferred to the campaign handoff rather than mixed into this performance commit.
- Adjudication evidence: Helm v4.2.3. The representation-only full-depth battery reports zero
  flips, hence zero candidate-accepts/Helm-aborts cells; no schema semantics are proposed.
- Public/wire decision: pre-registered as none. Ordering and encoding remain the existing public
  semantics. `ValuesPath` caches that spelling privately and `Segment` compares a lazy equivalent
  byte stream; every observed schema and diagnostic byte remains unchanged.

### Review dossier

- Property proof: `cargo nextest run -p helm-schema-core --profile integration --test value_path`;
  exit 0, all six tests pass, including generated all-pairs comparisons for the root and every
  one- and two-segment path over the frozen punctuation/UTF-8/wildcard domain.
- Lazy build: `cargo build -p helm-schema-cli --release`; exit 0 in 37.33 s, followed immediately
  by the preserved `helm-schema-a1-lazy` copy named above.
- Isolated chart: `/private/tmp/helm-schema-performance-v1.LSEe9Y/a1/kubernetes-apps-chart`
  contains the parent `Chart.yaml`, `Chart.lock`, and `values.yaml`, plus only `_helpers.tpl` and
  `templates/prometheus/rules-1.14/kubernetes-apps.yaml`; there is no `charts/` directory.
- Variant artifacts: lazy/cached isolated byte outputs are under `a1/variant-byte`; five randomized
  pairs and their per-invocation load records are under `a1/variant-measure`.
- Retained variant decision evidence is under `a1/variant-measure-retry`: pairs 1–4 plus replacement
  pair 5b. Original pair 5 is explicitly invalidated by the post-window compiler snapshot.
- Final build: `cargo build -p helm-schema-cli --release`; exit 0 in 29.16 s, followed immediately
  by the preserved `helm-schema-a1` copy named above.
- Ten-chart byte gate: preserved `helm-schema-c1` versus `helm-schema-a1`, using the round-0 private
  K8s/CRD cache and `--compact --offline --k8s-version v1.35.0 --diag-format json`; all comparisons
  pass under `/private/tmp/helm-schema-performance-v1.LSEe9Y/a1/byte`.
- Battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/a1/battery
  SCHEMA_ACCEPTANCE_BASELINE_REF=7cee5c6c
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/a1/battery/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, one test passes in 217.853 s.
- Invalid timing evidence is retained under `a1/measure`, `a1/measure-final`,
  `a1/measure-valid/grafana`, and `a1/measure-retained/cilium`. The process evidence includes
  external Cargo parents 58027, 72045, 2031, 19838, 65788, 66101, 57971, and 88097, spanning other
  workspaces and rust-analyzer.
- The resumed grafana window is under `a1/measure-retry/grafana`; pairs 1–3 are retained, while the
  later pair directories remain available as explicitly invalid/unused evidence.
- Final timing evidence is under `a1/measure-quiet` for every chart except grafana. Five pairs are
  retained for coredns, metrics-server, istiod, cert-manager, and datadog; three for argo-cd,
  cilium, airflow, and kube-prometheus-stack. Every invocation records UTC start, `uptime`, schema,
  stdout, JSON stderr, status, and `/usr/bin/time -p` output separately.

### Self-adversarial pass

- Segment-wise order is not encoded-path order. The comparator must emit separator and escape
  bytes lazily, not compare the stored segment vector or rely on derived ordering.
- `Segment` cannot reuse path-component encoding blindly because its legacy comparator uses
  `encode_component()`, whose treatment of `.` differs from a component embedded in a path.
- UTF-8 continuation bytes cannot alias the ASCII separator or escape bytes, but property cases
  still include multi-byte text so this assumption is exercised rather than merely asserted.
- A cached spelling can make comparison faster while making clones, construction, or memory
  traffic worse. Only an end-to-end isolated-chart pair can authorize that representation.
- Output identity is sensitive to `BTreeMap` iteration order; a comparator that merely appears
  reasonable but differs on one punctuation edge case must be rejected even if fixtures happen
  not to contain that case.
- The cached-spelling result remains a loaded-host number, but all five compiler-free paired gains
  are 45% or greater and agree with the earlier provisional window. It selects the variant without
  substituting for the still-required final C1-versus-A1 ten-chart curve.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 1.097 s.
- `task lint`: exit 0; 1.469 s. Whole-workspace Clippy and all three AST-grep policies pass; the
  two existing escaped-newline findings remain informational.
- `task lint:fc`: exit 0; 48 feature combinations for 13 packages across three targets pass in
  85.69 s with zero errors or warnings.
- `cargo nextest run --workspace`: exit 0; 1,340/1,340 tests pass in 47.991 s after a 1m15s build.
- `task test:integration`: exit 0; 666/666 tests pass in 508.844 s, with 24 profile skips.
- `task test:all`: exit 0; 2,010/2,010 tests pass in 540.974 s, with 24 profile skips and live
  network tests included.
- Downstream luup2: not triggered because A1 is representation-only and all public schema,
  diagnostic, and status bytes are exact; the campaign's installed C1 gate remains 32/32 green.
- `task tokei:core`: exit 0; 66,154 production Rust LOC in 0.990 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +75 (66,079 to 66,154).

## Round A2 — per-run predicate BDD memo

- Status: landed in `39b44aaf`.
- Contract: memoize `predicate_bdd::normalize` and `exact_implies` for one analysis session using
  full structural predicate keys. Clear both maps at the session boundary, never evict, and release
  every interior-mutability borrow before a miss computes recursively. Do not change BDD caps,
  normalization choices, approximation handling, predicate equality/order/hash semantics, public
  schema APIs, or wire formats.
- Acceptance baseline: `0926805c` for executable/schema identity and `f6a139bb` for the completed
  A1 ledger state.
- Baseline production Rust LOC: 66,154.
- Pre-registered acceptance expectations:
  - Memoized and deliberately unmemoized normalization/implication results agree over generated
    exact predicates, approximate predicates with and without sound subsets, and formulas that
    reach each BDD/normal-form abstention cap.
  - Two complete analysis sessions in one process emit identical schema and diagnostics, proving
    that session reset neither leaks nor changes semantics.
  - Keys contain the complete `Predicate` or complete predicate pair, never only a 64-bit hash;
    misses compute with no live `RefCell` borrow, and the maps have no eviction policy.
  - All ten reference outputs, all 156 schema fixtures, all 18 IR fixtures, diagnostics, stdout,
    and statuses remain byte-identical, with zero corpus acceptance flips.
  - Empty-cache online, warm-online, and warm-offline schema, stdout, JSON diagnostics, and status
    agree for all ten charts even though the new memo is process-local rather than persistent.
  - Kube-prometheus-stack peak RSS may not exceed 1.5× its paired A1 baseline. Reject A2 if it
    does, if any fixture byte differs, or if paired gains are below 20% on datadog and below 10%
    on kube-prometheus-stack.
- Performance baseline: the final A1 row is coredns 0.29 s, metrics-server 0.15 s, istiod 0.45 s,
  cert-manager 0.57 s, argo-cd 4.59 s, grafana 3.14 s, cilium 5.29 s, datadog 36.29 s, airflow
  40.40 s, and kube-prometheus-stack 45.61 s CPU median. Fresh randomized A/B pairs against the
  preserved A1 binary decide A2.
- Performance baseline and measured results (five randomized interleaved pairs per chart; CPU is
  user time, and the range on gain is the range of the five paired deltas):

  | Chart | A1 CPU s median (min–max) | A2 CPU s median (min–max) | Paired gain median (min–max) | load1 range |
  |---|---:|---:|---:|---:|
  | coredns | 0.28 (0.28–0.29) | 0.24 (0.24–0.24) | 14.29% (14.29–17.24%) | 6.77–6.77 |
  | metrics-server | 0.15 (0.15–0.16) | 0.14 (0.14–0.14) | 6.67% (6.67–12.50%) | 6.63–6.63 |
  | istiod | 0.44 (0.44–0.44) | 0.36 (0.36–0.37) | 18.18% (15.91–18.18%) | 6.50–6.63 |
  | cert-manager | 0.57 (0.57–0.58) | 0.51 (0.51–0.52) | 10.53% (8.77–12.07%) | 6.38–6.50 |
  | argo-cd | 4.46 (4.42–4.49) | 3.76 (3.71–3.76) | 16.06% (15.51–16.59%) | 5.27–6.38 |
  | grafana | 2.77 (2.77–2.80) | 2.04 (2.04–2.07) | 26.35% (26.07–26.88%) | 4.97–5.43 |
  | cilium | 5.22 (5.17–5.24) | 3.17 (3.15–3.18) | 39.08% (38.68–39.89%) | 4.18–4.97 |
  | datadog | 35.94 (35.77–35.98) | 17.84 (17.82–17.88) | 50.36% (50.07–50.42%) | 4.15–5.39 |
  | airflow | 40.04 (39.87–40.09) | 19.84 (19.75–19.91) | 50.46% (50.11–50.67%) | 3.04–5.46 |
  | kube-prometheus-stack | 44.90 (44.80–45.52) | 38.33 (38.10–38.68) | 14.62% (14.56–15.14%) | 3.00–16.95 |

  Every paired range is strictly positive. Datadog clears the frozen 20% threshold by 30.36
  points and kube-prometheus-stack clears its 10% threshold by 4.62 points. The plan's expected
  55–70% datadog gain does not fully reproduce on the expanded tree, while its 10–25%
  kube-prometheus-stack expectation does.
- Peak RSS on the exact final binaries, measured by `/usr/bin/time -lp` on
  kube-prometheus-stack, is 1,825,865,728 bytes for A1 and 2,055,782,400 bytes for A2: 1.126×,
  below the independent 1.5× rejection limit. Peak memory footprint is 1,597,900,720 versus
  1,676,675,352 bytes (1.049×).
- Corroborating suite wall time also falls: `task test:integration` ran 667 tests in 396.773 s
  versus A1's 666 in 508.844 s (22.0% faster), and `task test:all` ran 2,013 tests in 391.247 s
  versus A1's 2,010 in 540.974 s (27.7% faster). These are not randomized paired benchmarks and
  do not drive the decision, but they measure the practical feedback-loop improvement.
- Deviations:
  - The first implementation spike used a reset-at-session-start thread-local because the frozen
    design allowed it. It was removed before preserving a binary or taking a decision measurement:
    the user correctly identified that hidden shared state makes concurrent library sessions
    compete and makes tests order-dependent and reset-dependent. The landed design instead gives
    `IrAnalysisDb` the analysis memo, contract finalization a local phase memo, and
    `LoweredEmissionPlan` the generation memo. Lifetime, reset, and isolation are ordinary
    ownership; no global or thread-local state exists.
  - A compiler-driven attempt to require the memo at every existing semantic helper produced 98
    test call-site errors. The final API keeps test-facing pure adapters only where tests actually
    need them and gives production hot paths explicit `_with_memo` operations. This is more
    plumbing than the frozen S estimate anticipated; production Rust grows by 777 LOC.
  - The user's later decision rule would permit a modest reproducible non-regression because the
    ownership model is independently valuable. It does not alter this round's outcome: the frozen
    performance thresholds are both exceeded.
  - The first `/usr/bin/time -lp` attempt ran inside the sandbox. macOS could not read
    `kern.clockrate`, the wrapper returned 1 despite both child schemas completing, and its stderr
    polluted the diagnostic channel. Those runs under `a2/measure` are invalidated. RSS was rerun
    outside the sandbox and ordinary CPU timing used `/usr/bin/time -p`.
  - A complete provisional curve under `a2/measure-valid` was invalidated after lint-driven
    extraction changed the executable. The final release was rebuilt and copied immediately, and
    all decision numbers above come only from `a2/measure-final` against that exact copy.
- Adjudication evidence: Helm v4.2.3. The final ten-chart byte gate is exact for schema, stdout,
  JSON diagnostics, and status. Schema and IR artifact paths differ by zero files from `0926805c`.
  The final corpus battery checks 160 charts and 284,869 composed probes with zero flips, hence
  zero candidate-accepts/Helm-aborts cells.
- Public/wire decision: none. Memo state does not participate in semantic equality or emitted
  ordering, is never serialized, and is dropped with its owner. Pure `Predicate` operations remain
  available; hot analysis and emission paths opt into the owned memo explicitly.

### Review dossier

- Differential tests: `cargo nextest run -p helm-schema-core predicate_bdd`; exit 0, two tests
  pass. The generated set includes exact formulas, approximations with sound subsets, the
  256-normal-form-path abstention, and the 4,096-BDD-node abstention; a second identical lookup
  proves neither memo map grows on a hit.
- Session isolation: `cargo nextest run -p helm-schema --profile integration --test public_surface
  -E 'test(successive_sessions_keep_schema_and_diagnostics_identical)' --no-capture`; exit 0. Two
  separately owned public sessions in one process produce identical schema and diagnostic vectors.
- Final binary: `cargo build --release -p helm-schema-cli`; exit 0 in 22.17 s, copied immediately
  to `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-a2`, SHA-256
  `3c6f0027988f641b21b5e04ed353387d059a41c05403946c6c8cba545bc38172`, 16,194,176 bytes.
- Final ten-chart byte gate: preserved `helm-schema-a1` versus `helm-schema-a2`, with
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s`,
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, and
  `--compact --offline --k8s-version v1.35.0 --diag-format json`; all four channels are exact under
  `a2/byte-final`.
- Artifact gate: `git diff --exit-code 0926805c -- testdata/chart-corpus-schemas
  crates/helm-schema-ir/tests/fixtures`; exit 0, covering 156 schema and 18 IR artifacts.
- Corpus battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/a2/battery-final
  SCHEMA_ACCEPTANCE_BASELINE_REF=0926805c
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/a2/battery-final/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, one test passes in 197.142 s with 160 charts, 284,869 probes,
  and zero flips.
- Cache-law evidence is under `a2/cache-law-final`: each chart has its own initially empty K8s/CRD
  pair and empty-online, warm-online, and warm-offline invocations of the exact final binary. All
  ten charts are byte-identical across schema, stdout, JSON diagnostics, and status for all three
  cache states; every child status is 0.
- Final randomized interleaved pairs and per-invocation UTC start/load records are under
  `a2/measure-final`. Each of all ten charts has five pairs in BA/AB/BA/AB/BA order. The earlier
  `a2/measure-valid` directory is explicitly provisional and not decision evidence.
- Final unsandboxed resource pair is under `a2/rss-final`; both child statuses are 0 and all four
  output channels remain exact.

### Self-adversarial pass

- Holding a `RefCell` borrow while computing a normalization miss would panic when structural
  simplification calls implication recursively. Lookup, compute, and insertion must be three
  separate borrow scopes.
- A thread-local that is never cleared is a cross-session global cache and violates both the
  memory bound and the per-run contract even if its values are pure.
- Hash-only keys are invalid despite the existing cached structural hash; collisions must still be
  resolved by full structural equality.
- Approximation and cap-abstaining results are part of the pure function's result space. Caching
  only successful exact BDD reductions could hide a semantic or memory asymmetry.
- CPU gains can buy excessive retained memory. The RSS rejection criterion is independent of the
  speed result and is measured on the exact copied binaries.
- Explicit ownership can still be the wrong trade if passing the memo obscures phase boundaries.
  Here the owners coincide with existing phase objects (`IrAnalysisDb`, contract finalization, and
  `LoweredEmissionPlan`), while leaf operations receive a plain shared reference; no cache reset,
  hidden singleton, or semantic wrapper was added.
- The final kube-prometheus-stack window briefly recorded load1 16.95. That would invalidate an
  unpaired absolute comparison, but its five paired gains are 14.56–15.14% and its A/B CPU ranges
  remain narrow. The decision rests on within-pair deltas, not comparison to round 0 or A1's older
  host window.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 1.000 s.
- `task lint`: exit 0; 11.854 s. Whole-workspace Clippy and all three AST-grep policies pass;
  the two existing escaped-newline findings remain informational.
- `task lint:fc`: exit 0; 48 feature combinations for 13 packages across three targets pass in
  90.34 s with zero errors or warnings.
- `cargo nextest run --workspace`: exit 0; 1,342/1,342 tests pass in 22.628 s after a 1m05s build.
- `task test:integration`: exit 0; 667/667 tests pass in 396.773 s, with 24 profile skips.
- `task test:all`: exit 0; 2,013/2,013 tests pass in 391.247 s, with 24 profile skips and live
  network tests included.
- Downstream luup2: not triggered because A2 is representation-only and every schema, diagnostic,
  stdout, status, fixture, and acceptance channel is exact; the campaign's installed C1 gate
  remains 32/32 green.
- `task tokei:core`: exit 0; 66,931 production Rust LOC in 0.763 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0; under 0.001 s.
- `git diff --check`: exit 0; under 0.001 s.

- Measured production LOC delta: +777 (66,154 to 66,931).

## Round A2b — cached predicate metadata and borrowed BDD atoms

- Status: landed in `30a5903f`.
- Contract: fold each predicate node's cached structural hash from its variant rank and child
  cached hashes rather than recursively hashing whole subtrees; cache whether the node contains an
  approximation; and let BDD atom collection/indexing borrow guards instead of deep-cloning them.
  Keep structural `Eq`, `Ord`, and public `Hash` behavior consistent, retain every BDD and
  normal-form cap, retain full-key A2 memo collision checking, and change no predicate choice,
  schema semantics, diagnostic, status, artifact, or wire representation.
- Acceptance baseline: `39b44aaf` for executable/schema identity and `d252d448` for the completed
  A2 ledger state.
- Baseline production Rust LOC: 66,931.
- Pre-registered acceptance expectations:
  - Hash equality is preserved for structurally equal predicates, and generated structural-order
    cases continue to compare and hash consistently after child hashes are folded.
  - Cached approximation flags agree with an explicit recursive walk over generated exact and
    approximate formulas, including nested negation and conjunction/disjunction shapes.
  - Borrowed BDD atoms produce the identical canonical guard order and identical bounded
    normalize/implication results as A2.
  - All ten reference outputs, all 156 schema fixtures, all 18 IR fixtures, stdout, JSON
    diagnostics, statuses, and the full-depth battery remain byte-identical with zero flips.
  - Reject if the paired CPU gain on the isolated kube-prometheus-stack
    `kubernetes-apps.yaml` chart is under 3%; a range crossing zero is not a gain.
- Performance baseline: fresh randomized interleaved pairs will compare the preserved
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-a2` binary with A2b on the
  isolated chart under `a1/kubernetes-apps-chart`. The ten-chart A2 curve is the round-wide
  reference: 0.24, 0.14, 0.36, 0.51, 3.76, 2.04, 3.17, 17.84, 19.84, and 38.33 s CPU median.
- Measured results: five randomized interleaved pairs on the isolated normalization-heavy
  kube-prometheus-stack chart put A2 at 6.35 s CPU median (6.27--6.88) and A2b at 3.26 s
  (3.14--3.47). Every pair is positive: the median paired gain is 49.06%, with a 45.98--49.92%
  range, at load1 4.00--5.71. This clears the pre-registered 3% threshold by a wide margin. The
  unpaired full-suite corroboration moved `test:integration` from A2's 396.773 s to 371.333 s
  (6.4% lower) and `test:all` from 391.247 s to 363.095 s (7.2% lower); compilation, harness,
  network, and small-test costs are outside the optimized path, so those suite figures are not
  treated as controlled A/B evidence.
- Deviations:
  - The frozen plan expected A2b to recover 10--20% of post-A1/A2 normalization time. The isolated
    end-to-end gain is 49.06%; the plan materially underestimated repeated recursive hashing and
    guard cloning after A2 made memo lookup frequent.
  - No measurement was invalidated and no implementation attempt was restored. The final release
    binary was copied before any byte gate or decision run, and no source changed afterward.
- Adjudication evidence: Helm v4.2.3. All ten reference charts are exact for schema, stdout, JSON
  diagnostics, and status. The artifact paths differ by zero files from `39b44aaf`; the battery
  covers 160 charts and 284,869 composed probes with zero flips, hence zero
  candidate-accepts/Helm-aborts cells.
- Public/wire decision: none. Cached metadata remains private and derived only from immutable
  structural predicate content. BDD atoms borrow guards only during one normalization call and
  clone a positive canonical guard solely when rebuilding a selected normal form.

### Review dossier

- Focused tests: `cargo nextest run -p helm-schema-core predicate`; exit 0, 46/46 tests pass in
  0.088 s. The added oracles prove cached approximation state matches a recursive walk, rebuilt
  equal predicates hash equally, negative and positive guards share one borrowed BDD atom, and
  the existing generated exact/approximate/cap cases remain unchanged.
- Final binary: `cargo build --release -p helm-schema-cli`; exit 0, copied immediately to
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-a2b`, SHA-256
  `2ef76927ca085a98d69e51650f169ec74a56ec100f7687f047935654136c90c1`, 16,161,232 bytes.
- Final ten-chart byte gate: preserved `helm-schema-a2` versus `helm-schema-a2b`, with
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s`,
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, and
  `--compact --offline --k8s-version v1.35.0 --diag-format json`; all four channels are exact under
  `a2b/byte`.
- Artifact gate: `git diff --exit-code 39b44aaf -- testdata/chart-corpus-schemas
  crates/helm-schema-ir/tests/fixtures`; exit 0, covering 156 schema and 18 IR artifacts.
- Corpus battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/a2b/battery
  SCHEMA_ACCEPTANCE_BASELINE_REF=39b44aaf
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/a2b/battery/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, one test passes in 145.620 s with 160 charts, 284,869 probes,
  and zero flips.
- Final randomized interleaved pairs are under `a2b/measure`; A is the preserved A2 binary and B
  is the exact final A2b binary. The five pair orders are randomized, every invocation records UTC
  start and load1 beside `/usr/bin/time -p` output, and every pair's schema, stdout, JSON
  diagnostics, and status are exact.

### Self-adversarial pass

- Folding child hashes is valid only if the variant rank, list length/boundaries, and child order
  remain distinguishable exactly as `Hash` requires; cached hashes are not substitutes for
  structural equality in any map.
- Borrowed guards must outlive the BDD build and preserve the existing `BTreeSet` structural order;
  pointer identity or hash iteration order must not affect atom numbering.
- Caching approximation on a node must include approximate descendants beneath every variant,
  especially `Not`; a stale false flag could let opaque predicates enter exact BDD reasoning.
- The A2 memo can amplify a hashing regression because every lookup hashes full predicate keys.
  Only the post-A2 isolated paired measurement decides whether A2b earns its representation cost.
- Caching only child hashes intentionally permits additional structural-hash collisions. Full
  structural equality remains the collision resolver for predicates and memo keys; neither the
  cached hash nor borrowed atom identity can decide semantic equality.
- The suite reductions are directionally useful but were not run as randomized A/B pairs and can
  be affected by host load or network timing. The adoption decision therefore uses only the
  isolated five-pair CPU result.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 0.859 s.
- `task lint`: exit 0; 10.586 s. Whole-workspace Clippy and all three AST-grep policies pass.
- `task lint:fc`: exit 0; 48/48 feature combinations pass in 84.41 s.
- `cargo nextest run --workspace`: exit 0; 1,344/1,344 tests pass in 17.298 s after a 1m11s
  build.
- `task test:integration`: exit 0; 667/667 tests pass in 371.333 s, with 24 profile skips.
- `task test:all`: exit 0; 2,015/2,015 tests pass in 363.095 s, with 24 profile skips and live
  network tests included.
- Downstream luup2: not triggered because A2b is representation-only and every schema,
  diagnostic, stdout, status, fixture, and acceptance channel is exact; the campaign's installed
  C1 gate remains 32/32 green.
- `task tokei:core`: exit 0; 67,062 production Rust LOC in 0.697 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0; under 0.001 s.
- `git diff --check`: exit 0; under 0.001 s.

- Measured production LOC delta: +131 (66,931 to 67,062).

## Round B2 — `mimalloc` on every CLI target

- Status: landed in `a34859c0`.
- Contract: make the CLI's existing `mimalloc` dependency and global allocator declaration apply
  on macOS and the other supported non-musl targets, rather than only musl. Change no library
  ownership, allocation site, schema semantics, diagnostic, status, fixture, corpus input, or wire
  representation.
- Acceptance baseline: `30a5903f` for executable/schema identity and `85c67c0b` for the completed
  A2b ledger state.
- Baseline production Rust LOC: 67,062.
- Pre-registered acceptance expectations:
  - All ten reference outputs, all 156 schema fixtures, all 18 IR fixtures, stdout, JSON
    diagnostics, statuses, and the full-depth battery remain byte-identical with zero flips.
  - Five randomized interleaved A/B pairs each on grafana and datadog use the same private cache,
    exact preserved binaries, and per-invocation load records. Each chart must show at least 5%
    median paired CPU gain and a paired range wholly above zero; otherwise restore the spike.
  - Record release build time and executable size even though they are not B2 rejection criteria.
- Performance baseline: the preserved A2b binary is the direct A side. The latest round-wide A2
  medians were 2.04 s for grafana and 17.84 s for datadog; A2b's isolated predicate-heavy chart
  result does not substitute for fresh allocator pairs on either chart.
- Measured results:
  - Grafana: A2b 1.82 s CPU median (1.81--1.84), B2 1.48 s (1.44--1.52); median paired gain
    18.68%, range 16.02--21.74%.
  - Datadog: A2b 13.17 s CPU median (13.13--13.23), B2 11.25 s (11.24--11.30); median paired gain
    14.39%, range 14.32--14.97%.
  - All ten pairs preserve schema, stdout, JSON diagnostics, and status exactly. Load1 spans
    1.81--3.64. Both charts clear 5% and every paired sample is positive, so B2 is adopted.
  - The first incremental release build including `libmimalloc-sys` took 12.74 s wall, 22.91 s
    user, and 1.54 s sys. The final portability-only rebuild took 1.53 s wall. The final binary is
    16,280,000 bytes, 118,768 bytes larger than A2b.
  - Unpaired suite corroboration is mixed by target coverage: integration is effectively flat at
    372.423 s versus A2b's 371.333 s, while `test:all` falls from 363.095 s to 310.574 s. These
    gate timings are not used for adoption because test binaries do not uniformly inherit the CLI
    entry point's allocator and the runs were not randomized A/B pairs.
- Deviations:
  - The literal first implementation removed every target condition. `task lint:fc` attempt one
    then exited 201 after 45.71 s because `libmimalloc-sys`'s bundled C source failed under the
    workspace's Zig-based `x86_64-pc-windows-gnu` cross-check; the other 47 combinations passed.
    This was attributable to B2. The final implementation retains the system allocator on Windows
    and enables `mimalloc` on all supported non-Windows targets.
  - Rebuilding after that correction produced the exact same macOS executable as the measured
    candidate (matching SHA-256 below). The byte gate, corpus battery, and randomized measurements
    therefore exercise the exact final binary rather than an invalidated predecessor.
- Adjudication evidence: Helm v4.2.3. All ten reference charts are exact for schema, stdout, JSON
  diagnostics, and status. The artifact paths differ by zero files from `30a5903f`; the battery
  covers 160 charts and 284,869 composed probes with zero flips, hence zero
  candidate-accepts/Helm-aborts cells.
- Public/wire decision: none. The allocator is process-local implementation policy for the CLI
  binary on non-Windows targets and does not add shared semantic cache state to any library
  session. Windows retains its system allocator to satisfy the supported cross-target build.

### Review dossier

- Final implementation: `mimalloc` is a dependency under
  `cfg(not(target_os = "windows"))`; the existing global allocator declaration carries the same
  condition. Linux musl behavior is retained, macOS and Linux glibc gain the allocator, and the
  Windows cross-target remains buildable.
- Final binary: `/private/tmp/helm-schema-performance-v1.LSEe9Y/bin/helm-schema-b2-final`, SHA-256
  `c3e5a57fb4618e84fa4d331a3dadb7b98798534ef6efb9fb96fb35038b658a43`. It is byte-identical
  to the initially measured `helm-schema-b2` copy.
- Final ten-chart byte gate: preserved `helm-schema-a2b` versus `helm-schema-b2`, with
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s`,
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, and
  `--compact --offline --k8s-version v1.35.0 --diag-format json`; all four channels are exact under
  `b2/byte`.
- Artifact gate: `git diff --exit-code 30a5903f -- testdata/chart-corpus-schemas
  crates/helm-schema-ir/tests/fixtures`; exit 0, covering 156 schema and 18 IR artifacts.
- Corpus battery: `TMPDIR=/private/tmp/helm-schema-performance-v1.LSEe9Y/b2/battery
  SCHEMA_ACCEPTANCE_BASELINE_REF=30a5903f
  SCHEMA_PROBE_COVERAGE_REPORT=/private/tmp/helm-schema-performance-v1.LSEe9Y/b2/battery/coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, one test passes in 152.685 s with 160 charts, 284,869 probes,
  and zero flips.
- Final randomized interleaved pairs are under `b2/measure`. Odd pairs run B then A and even pairs
  A then B; every invocation records UTC start and load1 beside `/usr/bin/time -p` output. No build
  overlaps the measurement window.

### Self-adversarial pass

- An allocator can change timing without changing Rust-level semantics, but undefined behavior can
  become layout-sensitive. The full workspace, corpus battery, artifacts, and byte channels remain
  mandatory despite the one-line source change.
- Faster median CPU with any negative paired sample is not a stable gain under the campaign rule.
  Both named charts must clear the threshold on paired CPU, not wall time.
- The CLI allocator choice must not leak into the libraries as global cache or analysis state;
  concurrent library sessions remain isolated by ordinary ownership.
- The first cross-target failure demonstrates that allocator portability cannot be inferred from
  native builds. The final target predicate is covered directly by all 48 cargo-fc combinations.
- The lower `test:all` duration may include scheduling and host effects. It is not credited to B2;
  the paired preserved-binary CPU result is sufficient on its own.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 0.887 s.
- `task lint`: exit 0; 0.934 s. Whole-workspace Clippy and all three AST-grep policies pass.
- `task lint:fc`: exit 0 on the final tree; 48/48 feature combinations pass in 36.25 s. The failed
  first attempt is recorded under Deviations rather than hidden.
- `cargo nextest run --workspace`: exit 0; 1,344/1,344 tests pass in 19.196 s after a 26.53 s
  rebuild.
- `task test:integration`: exit 0; 667/667 tests pass in 372.423 s, with 24 profile skips.
- `task test:all`: exit 0; 2,015/2,015 tests pass in 310.574 s, with 24 profile skips and live
  network tests included.
- Downstream luup2: not triggered because B2 is representation-only and every schema, diagnostic,
  stdout, status, fixture, and acceptance channel is exact; the campaign's installed C1 gate
  remains 32/32 green.
- `task tokei:core`: exit 0; 67,062 production Rust LOC in 0.789 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0; under 0.001 s.
- `git diff --check`: exit 0; under 0.001 s.

- Measured production LOC delta: 0 (67,062 to 67,062).

## Round E1 — linear structural metadata for emission deduplication

- Status: pre-registered; implementation not started.
- Contract: add one shared post-order JSON metadata primitive that computes structural digest,
  exact canonical byte length, and unsafe reference-scope state once per node; use it to prefilter
  minifier candidates, logical-arm ordering, and provider-core candidates. Canonical strings and
  full structural equality remain the final collision checks. Preserve definition selection,
  savings thresholds, naming/ranking order, caps, schema semantics, diagnostics, statuses,
  fixtures, and wire bytes.
- Acceptance baseline: `a34859c0` for executable/schema identity and `81531769` for the completed
  B1 ledger state.
- Baseline production Rust LOC: 67,062.
- Pre-registered acceptance expectations:
  - A forced-digest-collision test proves unequal canonical values remain separate and cannot be
    substituted or deduplicated by hash alone.
  - Metadata canonical lengths equal actual canonical serialization byte lengths across nested
    objects, arrays, escaped strings, numbers, booleans, and null; unsafe-scope propagation follows
    JSON Schema positions and ignores ordinary data payloads.
  - All ten reference outputs, all 156 schema fixtures, all 18 IR fixtures, stdout, JSON
    diagnostics, statuses, and the full-depth battery remain byte-identical with zero flips.
  - Fresh Perfetto traces on airflow and kube-prometheus-stack compare exact preserved binaries.
    Reject E1 if either `minimize_schema` or `extract_repeated_provider_payloads` fails to drop by
    at least 50% on both charts.
  - The ten-chart curve uses randomized interleaved A/B pairs under one private cache. Large-chart
    decisions use at least five pairs; no paired range crossing zero is reported as a gain.
- Performance baseline: fresh traces and randomized pairs will establish the B2 side because B2
  was directly measured only on grafana and datadog. The round-0 large-chart medians were 63.06 s
  datadog, 88.45 s airflow, and 119.77 s kube-prometheus-stack; these are historical context, not
  substitutes for same-window B2 measurements.
- Measured results: pending.
- Deviations: none at pre-registration.
- Adjudication evidence: pending; E1 is representation-only and requires byte identity and zero
  acceptance flips.
- Public/wire decision: pre-registered as none. The metadata index is transient phase-local state,
  never a global cache, serialized representation, or semantic oracle.

### Review dossier

- Planned implementation seam: `helm-schema-json-schema-walk` owns canonical structural metadata;
  the minifier and generator consume that single definition rather than maintaining separate
  recursive digest/length walks.
- Planned collision evidence: exercise the digest-bucket path with an injected equal digest for
  distinct canonical subtrees, then assert exact canonical comparison preserves both identities.
- Planned phase evidence: emit `--trace-output` files under
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/e1/trace` for B2 and E1 on airflow and
  kube-prometheus-stack, using the same cache and output flags.

### Self-adversarial pass

- A digest is only a prefilter. Any digest-only candidate count, replacement, duplicate removal,
  or definition lookup would make collisions correctness-relevant and violates the contract.
- Canonical byte length must count serialized UTF-8 and escapes exactly; using character counts or
  approximate punctuation can alter the savings threshold and therefore fixture bytes.
- Unsafe reference-scope state must traverse schema children, not arbitrary JSON data. Treating an
  annotation/example containing `$id` as a schema keyword would silently suppress valid sharing.
- A transient node-address index is valid only while the document's container structure is stable.
  Any mutation pass must consult a complete index before replacing the current node and must never
  use stale entries as evidence after moving children.
- Post-order rewriting would change whether definitions contain separately deduplicated children.
  The existing pre-order replacement behavior and naming order must remain exact.

### Gates on the final tree

- Pending: `cargo fmt --check`.
- Pending: `task lint`.
- Pending: `task lint:fc`.
- Pending: `cargo nextest run --workspace`.
- Pending: `task test:integration`.
- Pending: `task test:all`.
- Pending: downstream luup2 decision and, if required, install plus `check:local`.
- Pending: `task tokei:core`.
- Pending: `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`.
- Pending: `git diff --check`.

- Measured production LOC delta: pending.

## Round B1 — release-profile LTO and codegen units

- Status: rejected and restored; neither release-profile option met the frozen criterion, so no
  production commit exists and the final tree remains B2 at `a34859c0`.
- Contract: evaluate fat LTO and `codegen-units = 1` as two isolated release-profile changes on
  top of B2, then preserve only a configuration whose final combined behavior satisfies the frozen
  criterion. Change no Rust source, dependency, schema semantics, diagnostic, status, fixture,
  corpus input, or wire representation.
- Acceptance baseline: `a34859c0` for executable/schema identity and `0be5d6c7` for the completed
  B2 ledger state.
- Baseline production Rust LOC: 67,062.
- Pre-registered acceptance expectations:
  - Build B2, LTO-only, and codegen-units-only release binaries in separate initially empty target
    directories. Record each clean build's wall/CPU cost and binary size.
  - For each isolated candidate, run three randomized interleaved pairs on grafana, cilium, and
    datadog under one cache snapshot, recording load and exact output channels.
  - An isolated option advances only if its median gain across the three chart medians is at least
    5%, every chart's paired range is wholly above zero, and its clean build wall time is no more
    than twice B2's. If both advance, build and measure their combination before landing it; the
    final profile must independently satisfy the same threshold.
  - All ten reference outputs, all 156 schema fixtures, all 18 IR fixtures, stdout, JSON
    diagnostics, statuses, and the full-depth battery remain byte-identical with zero flips.
- Performance baseline: the preserved B2 result is 1.48 s CPU median on grafana and 11.25 s on
  datadog. Cilium was not a B2 decision chart, so every B1 candidate uses a fresh preserved-binary
  B2 pair rather than borrowing an older curve row.
- Measured results:
  - Clean default B2 build: 47.62 s wall, 403.37 s user, 21.55 s sys; 16,280,000-byte binary.
  - Fat LTO-only build: 120.01 s wall, 294.00 s user, 18.83 s sys; 14,277,008-byte binary. Its
    2.52x wall-time ratio independently fires the more-than-double rejection criterion.
  - `codegen-units = 1`-only build: 69.67 s wall, 293.39 s user, 17.93 s sys;
    13,654,512-byte binary. Its 1.46x wall-time ratio remains eligible on build cost.
  - Fat LTO-only paired CPU: grafana 1.43 s default versus 1.38 s candidate, 3.50% median gain
    (3.50--3.50%); cilium 2.23 versus 2.14 s, 4.04% (3.59--4.04%); datadog 11.20 versus 10.87 s,
    2.94% (2.60--3.04%). The median of chart medians is 3.50%, below 5%.
  - Codegen-units-only paired CPU: grafana 1.43 s default versus 1.39 s candidate, 2.80% median
    gain (2.78--2.80%); cilium 2.23 versus 2.17 s, 2.69% (2.24--3.13%); datadog 11.20 versus
    11.10 s, 0.98% (0.27--1.25%). The median of chart medians is 2.69%, below 5%.
  - Every one of the 18 candidate pairs is positive and exact on all four output channels, but
    neither option clears the frozen 5% runtime threshold. No combined candidate is justified.
    Load1 spans 2.12--4.18.
- Deviations:
  - The preregistration said an isolated option would advance only when every chart's paired range
    stayed above zero. Both options satisfy that stability check, yet fail the independent 5%
    aggregate threshold. The positive-but-small results are recorded rather than rounded into an
    adoption.
  - The full corpus acceptance battery was not rerun under either rejected release binary. It is a
    test-profile process and cannot exercise release-only code-generation flags; all ten reference
    charts were instead generated by each exact candidate binary and compared on schema, stdout,
    JSON diagnostics, and status. The restored final tree is the already-zero-flip B2 tree.
- Adjudication evidence: Helm v4.2.3. Both isolated candidates are exact against the clean B2
  binary on all four channels for all ten charts, and schema/IR artifact paths differ by zero files
  from `a34859c0`. No candidate is shipped, so the final tree inherits B2's 160-chart,
  284,869-probe, zero-flip battery evidence and has zero candidate-accepts/Helm-aborts cells.
- Public/wire decision: none. The release profile remains unchanged; executable-size savings alone
  are insufficient to justify either option's build cost and sub-threshold runtime gain.

### Review dossier

- Clean builds are under `/private/tmp/helm-schema-performance-v1.LSEe9Y/b1/build/{default,lto,codegen}`.
  Commands set `CARGO_TARGET_DIR` to the corresponding absent directory; isolated options add
  either `CARGO_PROFILE_RELEASE_LTO=fat` or `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1` to
  `/usr/bin/time -p cargo build --release -p helm-schema-cli`.
- Preserved binaries are `helm-schema-b1-default`, `helm-schema-b1-lto`, and
  `helm-schema-b1-codegen` under the campaign `bin` directory. Their respective SHA-256 values are
  `7ef25a47dbe57fa427b1da6e821cc9acacf0bef3a8a5aeba98b9af5f537200ce`,
  `4edfb581afc72a3d0cf61ac012dd18162e791255b71a7b884f19d40b680bba0d`, and
  `7ad0c5d83cc2d6c722bdce61b4ecc1ef8591bdbc791f690cdc4834e5e7464922`.
- Randomized pairs are under `b1/measure/{lto,codegen}`. Odd pairs run candidate then default and
  even pairs default then candidate; each invocation records UTC start, load1, `/usr/bin/time -p`,
  and separate output channels. No build overlaps the measurement window.
- Ten-chart byte evidence is under `b1/byte`. Each preserved option and default use
  `HELM_SCHEMA_K8S_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/k8s`,
  `HELM_SCHEMA_CRD_SCHEMA_CACHE=/private/tmp/helm-schema-performance-v1.LSEe9Y/cache/crd`, and
  `--compact --offline --k8s-version v1.35.0 --diag-format json`; every comparison is exact.
- Artifact gate: `git diff --exit-code a34859c0 -- testdata/chart-corpus-schemas
  crates/helm-schema-ir/tests/fixtures`; exit 0, covering 156 schema and 18 IR artifacts.

### Self-adversarial pass

- Warm incremental build time cannot establish the clean-build cost. Each comparison must start
  from its own absent target directory and compile the same source/dependency graph.
- Measuring LTO and codegen units only together cannot identify whether either option pays for
  itself. The isolated candidates must be retained and measured independently.
- A smaller executable is not a performance gain, and a single fast chart cannot hide a negative
  paired range elsewhere. The decision uses the median of chart-level paired medians plus the
  per-chart non-regression check.
- Host load can drift during the longer Datadog windows. Interleaving, exact copied binaries, and
  per-invocation load records are mandatory; absolute comparisons to an older round are not.
- The default binary's hash differs from the earlier B2 copy because it was built in a distinct
  clean target directory. Every comparison uses this clean-build default, so target-directory
  provenance is symmetric within B1 rather than mixing clean and incremental artifacts.
- LTO's lower total compiler CPU does not negate its 2.52x wall cost: the user-facing clean release
  build latency is the pre-registered quantity, and the runtime gain independently misses 5%.

### Gates on the final tree

- `cargo fmt --check`: exit 0; 0.920 s.
- `task lint`: exit 0; 11.679 s. Whole-workspace Clippy and all three AST-grep policies pass.
- `task lint:fc`: exit 0; 48/48 feature combinations pass in 36.01 s.
- `cargo nextest run --workspace`: exit 0; 1,344/1,344 tests pass in 16.903 s after a 21.82 s
  rebuild.
- `task test:integration`: exit 0; 667/667 tests pass in 302.459 s, with 24 profile skips.
- `task test:all`: exit 0; 2,015/2,015 tests pass in 315.600 s, with 24 profile skips and live
  network tests included.
- Downstream luup2: not triggered because B1 restores the already-gated B2 code and both rejected
  candidates are byte-exact; the campaign's installed C1 gate remains 32/32 green.
- `task tokei:core`: exit 0; 67,062 production Rust LOC in 0.534 s.
- `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`: exit 0; under 0.001 s.
- `git diff --check`: exit 0; under 0.001 s.

- Measured production LOC delta: 0 (67,062 to 67,062).
