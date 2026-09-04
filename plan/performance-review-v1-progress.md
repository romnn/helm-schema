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

- Status: implementation and byte/adjudication gates complete; the cached variant is selected,
  but the final ten-chart timing curve is blocked by recurring external compiler activity, so no
  implementation commit or round-level adoption decision exists.
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
    acceptance flips. Final schema/IR fixture and full verification gates remain pending until the
    ten-chart curve permits the round-level decision.
  - Three compiler-free grafana pairs compare C1 at 4.77 s CPU median (4.62–5.24) with A1 at
    3.14 s (3.08–3.15). Paired gains are 34.17% median (31.82%–41.22%), so the interval does not
    cross zero and the first half of the frozen rejection criterion is decisively satisfied.
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
- Adjudication evidence: Helm v4.2.3. The representation-only full-depth battery reports zero
  flips, hence zero candidate-accepts/Helm-aborts cells; no schema semantics are proposed.
- Public/wire decision: pre-registered as none. Ordering and encoding remain the existing public
  semantics; only comparison mechanics may change.

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

- Pending: `cargo fmt --check`.
- Early `task lint`: first attempt exit 201 for the byte-slice spelling; second and final-code
  early attempt exit 0. This is not the required final-tree gate.
- Pending: `task lint:fc`.
- Pending: `cargo nextest run --workspace`.
- Pending: `task test:integration`.
- Pending: `task test:all`.
- Pending: downstream luup2 decision and, if required, install plus `check:local`.
- Pending: `task tokei:core`.
- Pending: `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`.
- Pending: `git diff --check`.

- Measured production LOC delta: pending.
