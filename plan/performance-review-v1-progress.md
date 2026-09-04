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

- Status: pre-registered; implementation not started.
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
- Performance baseline: round-0 table; C1 has no speed target and will not be claimed as a
  performance gain.
- Measured results: pending.
- Deviations: none at pre-registration.
- Adjudication evidence: pending; Helm v4.2.3 remains mandatory if any acceptance flip appears.
- Public/wire decision: pre-registered as none; the cache and selected-document identity remain
  crate-private emission details.

### Review dossier

- Planned implementation proof: focused generator tests for document-key separation, followed by a
  release candidate copied immediately out of `target/`.
- Planned byte gate: preserved `8fbcc732` binary versus candidate on the ten reference charts, with
  schema, stdout, JSON diagnostics, and status captured separately under the round-0 private cache.
- Planned cache-law gate: a new empty private snapshot warmed online, then repeated warm online and
  warm offline, all four channels compared per chart.
- Planned artifact gate: exact schema and IR fixture tests; if a fixture differs, one clean dump and
  the round-74 full-depth battery against `8fbcc732` before adoption.

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

### Gates on the final tree

- Pending: `cargo fmt --check`.
- Pending: `task lint`.
- Pending: `task lint:fc`.
- Pending: `cargo nextest run --workspace`.
- Pending: `task test:integration`.
- Pending: `task test:all`.
- Pending if schema semantics change: `cargo install --path ./crates/helm-schema-cli/` and downstream
  luup2 `check:local`.
- Pending: `task tokei:core`.
- Pending: `git diff --exit-code 1ce9e660 -- plan/performance-review-v1.md`.
- Pending: `git diff --check`.

- Measured production LOC delta: pending.
