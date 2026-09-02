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

- Status: landed in `0b2e7345` (`test(infra): de-bias probes and validate capability rows`).
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

## Round A1 — key provider memoization by stringification policy

- Status: landed in `1146a3d7` (`fix(gen): key provider memo by stringification policy`).
- Contract: behavior-bearing; make provider-schema memo identity include `stringified`, and make
  provider preimage lowering consume the same typed policy subkey embedded in the cache key so a
  future policy field cannot be read outside memo identity.
- Acceptance baseline: `0b2e7345`.
- Baseline production Rust LOC: 62,082.
- Pre-registered acceptance expectations:
  - Two uses of one values path at the same resource slot that differ only in `stringified` resolve
    independently and retain both provider preimages.
  - Reversing those two uses cannot change the generated schema; first-resolved cache order stops
    being semantic input.
  - A direct scalar use keeps its scalar-only provider restriction while a stringified use keeps
    the wider preimage allowed by total textual conversion; neither may pin the other.
  - No tracked fixture or 60-chart full-depth acceptance flip is expected. Any changed cell stops
    the round for individual Helm 4.2.3 adjudication before a fixture is changed.
  - The zero candidate-accepts/Helm-aborts allowance remains fixed.

- Measured results:
  - `ProviderValueUsePolicy` now owns every input read by provider preimage lowering: value kind,
    stringification, direct-range shape, template-supplied keys, split-segment identity, and omitted
    members. `ProviderSchemaLookupKey` embeds that policy beside resource/path identity and the two
    resolver-routing fields.
  - `ProviderSchemaLookupKey::from` exhaustively destructures `ProviderSchemaUse`, naming every
    deliberately ignored field. Preimage lowering receives only the embedded policy, so it cannot
    read a future use field without first adding that field to memo identity.
  - The regression sends the same values path to the same provider slot through stringified and
    direct scalar routes in both orders. The resulting full schemas are byte-identical; a plain
    string remains accepted, while a nested object admitted only by the stringified route is
    rejected because the direct route also executes.
  - The immutable final1 archive contains 88 binaries and 126 files. One schema dump writes 84
    artifacts and one IR dump writes 18 artifacts; all tracked fixtures remain exact.
  - The full-depth comparison against `0b2e7345` checks 121,055 probes across 60 charts and reports
    zero flips. Mandatory coverage is 112,260/112,260 base and 7,465/7,465 third-level probes, with
    28,874 bounded drops disclosed.

- Deviations:
  - No acceptance or fixture deviation occurred.
  - The frozen plan offered either a typed cache-key closure or hoisting all provider resolution to
    a phase artifact. This round takes the local typed subkey because it closes the live defect
    immediately without pre-empting C4's independently scheduled phase-artifact work.
  - Production Rust grows by 50 lines. The added enforcement type and exhaustive conversion replace
    a flat key literal rather than deleting a representation; no LOC promise was registered.

- Adjudication evidence:
  - The final comparison records zero flips and therefore requires no individual Helm render
    verdict. Helm 4.2.3 is selected by the pinned mise toolchain; the zero accepted-abort allowance
    remains unspent.

### Producer and route coverage

| Route | Final cache identity | Verification |
|---|---|---|
| Direct scalar | `kind = Scalar`, `stringified = false`, plus all shared policy fields. | Nested structured input admitted only through textual conversion stays rejected. |
| Stringified scalar | Same resource and slot but `stringified = true`. | Its wider provider preimage is resolved independently instead of reusing the direct candidate. |
| Multi-kind resource | Resource kind candidates remain lookup inputs while every concrete kind reuses the same typed value-use policy. | Existing candidate-union tests and full fixture battery remain exact. |
| Template-supplied/omitted members | Both collections remain inside `ProviderValueUsePolicy`. | Existing provider projection tests remain exact. |
| Split and direct-range modes | Split-segment and range-shape fields remain inside the policy key. | Existing scalar segment and range tests remain exact. |
| Merge-layer/range-key routing | These fields remain on the outer lookup key and are skipped or synthesized before ordinary lookup as before. | No merge/range fixture or acceptance cell changes. |

### Review dossier

- Focused regression: `cargo nextest run -p helm-schema-gen -E
  'test(provider_schema_cache_distinguishes_stringified_scalar_uses)'`; exit 0, one test passes.
- Enforcement proof: every production call to `provider_schema_for_value_use` passes a
  `ProviderValueUsePolicy`; no call can pass the wider `ProviderSchemaUse` carrier.
- Immutable build: `TMPDIR=target/arch-v4-a1-final1-build cargo nextest archive --workspace
  --archive-file /private/tmp/arch-v4-a1-final1.tar.zst`; exit 0, 88 binaries and 126 files.
- Clean schema dump: `TMPDIR=target/arch-v4-a1-final1-schema SCHEMA_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a1-final1.tar.zst --profile integration --no-fail-fast -E
  'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=target/arch-v4-a1-final1-ir SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a1-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written.
- Full-depth proof: `TMPDIR=target/arch-v4-a1-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=0b2e7345
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=target/arch-v4-a1-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=target/arch-v4-a1-final1-coverage.json ADJUDICATE_WITH_HELM=1
  cargo nextest run --archive-file /private/tmp/arch-v4-a1-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: none. The new type and cache key remain crate-private; serialized contract
  documents and public Rust APIs are unchanged.

### Self-adversarial pass

- Adding only `stringified: bool` to the flat key would fix today's collision but leave the policy
  free to read the wider carrier again. The nested policy subkey makes memo completeness the only
  route to a new lowering input.
- Moving the entire `ProviderSchemaUse` into the key would over-key on outer guards, value-path
  spelling, and presence-only facts that do not affect the cached candidate. The exhaustive
  conversion names those deliberate exclusions without turning them into cache misses.
- The regression reverses producer order because a single fixed order could pass while the first
  candidate still pins both uses. Full-schema equality proves order independence, and the nested
  object probe selects the restrictive direction rather than merely checking that outputs match.
- Multi-kind lookup changes only resource identity for each candidate; reusing the same typed
  value-use policy is correct because kind selection does not change stringification or source
  transforms.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,132 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +50 (62,082 to 62,132).

## Round A2/G4 — separate values-key uniqueness from installed-chart identity

- Status: landed in `3e9dbf8e` (`fix(engine): separate dependency key and installed identities`).
- Contract: behavior-bearing; validate declaration uniqueness through values keys, retain
  name-based installed-chart lookup, model one vendored chart under multiple unique aliases with
  `name -> Vec<metadata>`, and reject only duplicate installed entries sharing one internal name as
  the measured nondeterministic case.
- Acceptance baseline: `1146a3d7`.
- Baseline production Rust LOC: 62,132.
- Pre-registered acceptance expectations:
  - Two declarations of one dependency name with distinct aliases are accepted when exactly one
    vendored chart has that internal name; discovery publishes one chart context per alias with its
    own values prefix and activation metadata.
  - Distinct dependency names sharing one alias are rejected in one aggregated, deterministically
    ordered values-key diagnostic before discovery.
  - Repeating the same name and values key is rejected as a values-key collision, not described as
    Helm installed-chart ambiguity.
  - Two directory/tgz installed entries with one internal chart name are rejected in one
    deterministic installed-identity diagnostic because Helm's association is nondeterministic.
  - Ordinary `{name: redis, alias: cache}` lookup remains name-based and discovers `.Values.cache`;
    no alias-keyed sole lookup map is introduced.
  - Legacy `requirements.yaml` follows the same values-key policy as Helm v2 `Chart.yaml`.
  - No corpus or luup2 chart is expected to change: the prior manifest audit found no affected
    declaration. Any full-depth acceptance flip stops the round for Helm 4.2.3 adjudication before
    fixture adoption; the accepted-abort allowance remains zero.

- Measured results:
  - Manifest validation builds a deterministic values-key index from `alias.unwrap_or(name)` and
    rejects every key with multiple declarations in one aggregated diagnostic. The diagnostic names
    the values root and each declaring dependency; it makes no Helm-ambiguity claim.
  - Installed lookup remains keyed by the chart's internal name. Its value is now an ordered vector
    of dependency metadata, so one vendored chart produces one context per unique alias with that
    alias's activation facts.
  - Discovery first inventories all vendored entries, then rejects repeated internal names in a
    separate diagnostic whose wording names Helm's nondeterministic installed-entry association.
    Only after that check does it recurse through the name-indexed metadata contexts.
  - The former over-rejection tests are inverted: same-name/distinct-alias declarations pass for
    Helm v2 and legacy manifests, including nested charts; the former distinct-name/shared-alias
    acceptance case now rejects as a values-root collision.
  - The structural battery's dependency-root inventory now retains every declared values key for
    one installed name rather than only the last alias.
  - The immutable final1 archive holds 88 binaries and 126 files. The 84-schema and 18-IR dumps are
    fixture-exact. The full-depth comparison against `1146a3d7` checks 121,055 probes across 60
    charts with zero flips, 112,260/112,260 base probes, and 7,465/7,465 third-level probes.

- Deviations:
  - A first `task lint` preflight exited 201 on one needless borrow in the new installed-entry test.
    The borrow was removed without suppression; the repeated lint gate passed.
  - No fixture or acceptance deviation occurred. The earlier 158-manifest audit prediction holds
    for the committed corpus and the 32-chart downstream sweep.
  - Production Rust grows by 54 lines because the discovery phase now represents one-to-many alias
    metadata and performs a separate installed-entry identity check. No LOC promise was registered.

- Adjudication evidence:
  - The authoritative comparison records zero flips and zero candidate-accepts/Helm-aborts cells,
    so no individual Helm render verdict or fixture adoption is needed.
  - The behavior change is pinned by in-memory charts: one installed entry with two aliases is
    deterministic and accepted; two installed entries with one internal name are rejected before
    any order heuristic can choose an association.

### Producer and route coverage

| Declaration or installed shape | Final disposition | Verification |
|---|---|---|
| One name, two unique aliases, one vendored entry | One name-indexed metadata vector expands to two contexts. | Helm v2, legacy, and nested-chart tests pass. |
| Distinct names, one alias | Values-key index rejects the shared `.Values` root. | Aggregated and focused two-name tests pass. |
| Same name and same alias/key | Values-key index rejects the duplicate root before discovery. | The values-key diagnostic reports declaration multiplicity without claiming ambiguity. |
| Ordinary name plus alias | Installed chart is still found by internal name and receives the alias prefix. | Existing alias and activation tests plus the new multi-alias test pass. |
| Two installed directory entries, one internal name | Installed inventory rejects the repeated name with both source paths. | Deterministic duplicate-installed-name test passes. |
| Dependency probe battery | All declared aliases for the installed name are protected from parent null-deletion probes. | Full-depth mandatory accounting stays 112,260/112,260 and 7,465/7,465. |

### Review dossier

- Focused matrix: `cargo nextest run -p helm-schema -E
  'test(duplicate_dependency_values_keys_are_aggregated_before_discovery) or
  test(one_vendored_chart_expands_to_each_unique_alias) or
  test(legacy_requirements_support_one_chart_under_multiple_aliases) or
  test(nested_chart_supports_one_dependency_under_multiple_aliases) or
  test(distinct_dependency_names_cannot_share_one_values_key) or
  test(duplicate_installed_chart_names_are_aggregated)'`; exit 0, six tests pass.
- Immutable build: `TMPDIR=target/arch-v4-a2-final1-build cargo nextest archive --workspace
  --archive-file /private/tmp/arch-v4-a2-final1.tar.zst`; exit 0, 88 binaries and 126 files.
- Clean schema dump: `TMPDIR=target/arch-v4-a2-final1-schema SCHEMA_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a2-final1.tar.zst --profile integration --no-fail-fast -E
  'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=target/arch-v4-a2-final1-ir SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a2-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written.
- Full-depth proof: `TMPDIR=target/arch-v4-a2-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=1146a3d7
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=target/arch-v4-a2-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=target/arch-v4-a2-final1-coverage.json ADJUDICATE_WITH_HELM=1
  cargo nextest run --archive-file /private/tmp/arch-v4-a2-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: the typed `CliError` variants change from the over-broad
  `DuplicateDependencyNames` shape to separate declaration-key and installed-name failures. This is
  an intentional public error-enum narrowing within the unreleased 0.0.x API; no serialized wire
  document changes.

### Self-adversarial pass

- Re-keying the sole discovery map by alias would make the ordinary aliased chart undiscoverable.
  The final two-index design validates aliases separately while retaining internal-name lookup.
- Accepting same-name aliases without a vector would silently keep last-write-wins metadata. The
  vector is consumed into one complete context per alias, including per-alias activation.
- Expanding every installed entry across every same-name declaration would create a cross product
  when two archives share an internal name. The installed inventory rejects that state before
  expansion, so no filename or manifest order becomes policy.
- A values-key collision is deterministic data loss, not Helm nondeterminism. Its diagnostic names
  the shared `.Values` root; only the installed-name diagnostic mentions nondeterministic Helm
  association.
- Battery compatibility follows the same one-to-many mapping so the new supported aliases cannot be
  probed as ordinary deletable parent keys.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflight.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,186 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +54 (62,132 to 62,186).

## Round A3 — preserve escaped dot and backslash values-path segments

- Status: landed in `30e49d82` (`fix(ir): preserve escaped values path segments`).
- Contract: behavior-bearing; replace raw dot splitting, joining, prefix stripping, and path
  concatenation on the values-path currency with codec-aware helpers. This round repairs literal
  dots and backslashes only; the literal `*` versus range-member distinction remains deferred to
  B4b.
- Acceptance baseline: `3e9dbf8e`.
- Baseline production Rust LOC: 62,186.
- Pre-registered acceptance expectations:
  - Parent/member requirements for `dig`, strict presence, and ranged member fields attach to the
    encoded structural parent when a literal key contains `.` or `\`.
  - Merge-layer truthy collapse computes common suffixes, concrete roots, wildcard collection
    prefixes, and relative member paths from decoded segments; only guards involving escaped
    dot/backslash segments may change.
  - Root-overlay requirement projection prefixes encoded paths structurally, without turning a
    literal dotted segment into nested selectors.
  - Wildcard detection and member-scope matching decode segments but continue treating the segment
    string `"*"` as the existing range wildcard. No literal-star behavior is changed in this round.
  - Acceptance flips are expected only for probes whose affected values path contains an escaped
    dot or backslash and reaches one of the repaired routes. Every changed cell is adjudicated with
    Helm 4.2.3 before fixture adoption; any unrelated flip stops the round.
  - The zero candidate-accepts/Helm-aborts allowance remains fixed, and mandatory coverage permits
    zero drops.

- Measured results:
  - Required-presence and ranged-member absence lowering now re-encodes parents with
    `join_value_path`; raw `dig` subject captures do the same.
  - Merge-layer truthy collapse decodes every layer into segments before computing its common
    suffix, concrete roots, wildcard collection prefixes, and relative guard suffixes. Every
    derived path is re-encoded through `join_value_path`.
  - Root-overlay projection decodes and concatenates prefix/path segments in one private helper;
    neither projected targets nor their local guards use raw string interpolation.
  - Member requirement scopes use `append_value_path`; relative field requirements compare decoded
    segment slices. Immediate-member tests now use a one-segment match instead of searching the
    encoded suffix for a dot.
  - Range-key concretization, conditional decoding, helper range-shape checks, prepared-values
    descendant pruning, and provider member-presence synthesis all decode segments before wildcard
    or ancestry decisions.
  - The remaining production `split('.')` under the audited crates is Kubernetes-version parsing in
    `analysis_db.rs`; the only other matches are test-local parsing of emitted JSON property names.
  - The immutable final1 archive contains 88 binaries and 126 files. The clean schema and IR dumps
    write 84 and 18 artifacts and remain fixture-exact.
  - The full-depth comparison against `3e9dbf8e` checks 121,055 probes across 60 charts and reports
    zero flips, with 112,260/112,260 base and 7,465/7,465 third-level probes.

- Deviations:
  - No registered escaped-key acceptance cell changed in the current corpus. The repaired paths are
    live in focused dotted-key tests, but the 60-chart mutation battery did not synthesize a state
    that distinguishes the formerly phantom parent from the encoded parent.
  - A compile preflight failed at four slice-prefix calls because `Vec<String>` must be passed as a
    slice pattern. Each call now passes `.as_slice()`; no failed-state artifact was produced.
  - The first lint preflight then exited 201 on three needless borrows in wildcard decoding. The
    borrows were removed without suppression and lint passed before the immutable archive build.
  - Production Rust grows by 48 lines because decoded segment comparisons replace compressed raw
    string operations. No LOC promise was registered.

- Adjudication evidence:
  - The final comparison records zero flips and zero candidate-accepts/Helm-aborts cells. No fixture
    is adopted and no individual Helm replay is required.
  - Existing literal-dotted index/get and leaked-secret path tests pass; they keep dotted keys as
    single schema properties while exercising nested requirements beneath those keys.

### Producer and route coverage

| Path route | Codec-aware operation | Verification |
|---|---|---|
| Required presence / absence abort | Decode, pop leaf, `join_value_path` parent. | Existing fail/dig and full fixture suites remain exact. |
| Raw `dig` subject | Decode subject and re-encode its parent before `HasKey`. | Dotted-key validator tests pass. |
| Layered truthy collapse | Segment suffix comparison, encoded root/prefix/suffix reconstruction. | Merge/overlay fixtures and acceptance remain exact. |
| Root overlay projection | Decode prefix and effective-root path, concatenate segments, encode once. | Istiod root-overlay tests and corpus fixture remain exact. |
| Member scopes and fields | `append_value_path("*")`; segment-slice relative fields. | Range/fail/member suites remain exact. |
| Range-key concretization | Segment-prefix replacement followed by one encoded reconstruction. | Range-key equality tests remain exact. |
| Prepared-values pruning | Segment ancestry instead of encoded string prefix. | Values-default and fixture suites remain exact. |
| Wildcard checks | `split_value_path` then exact `"*"` segment comparison. | Literal `*` semantics intentionally remain unchanged for B4b. |

### Review dossier

- Raw-operation audit: the production search for `split('.')`, raw dot joins, raw member-scope
  formatting, and formatted-prefix stripping leaves only semver parsing; test-only JSON property
  inspection remains outside the values-path currency.
- Focused dotted-key proof: `cargo nextest run -p helm-schema-core -p helm-schema-gen -E
  'test(value_path_currency_preserves_literal_dots_and_backslashes) or
  test(literal_dotted_index_and_get_keys_generate_one_root_property) or test(/leaked_secret/)'`;
  exit 0 for the selected generator dotted-key test. The core public-surface integration test is
  profile-filtered here and runs in the complete integration gate.
- Immutable build: `TMPDIR=target/arch-v4-a3-final1-build cargo nextest archive --workspace
  --archive-file /private/tmp/arch-v4-a3-final1.tar.zst`; exit 0, 88 binaries and 126 files.
- Clean schema dump: `TMPDIR=target/arch-v4-a3-final1-schema SCHEMA_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a3-final1.tar.zst --profile integration --no-fail-fast -E
  'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=target/arch-v4-a3-final1-ir SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run
  --archive-file /private/tmp/arch-v4-a3-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written.
- Full-depth proof: `TMPDIR=target/arch-v4-a3-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=3e9dbf8e
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=target/arch-v4-a3-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=target/arch-v4-a3-final1-coverage.json ADJUDICATE_WITH_HELM=1
  cargo nextest run --archive-file /private/tmp/arch-v4-a3-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: none. Existing public codec helpers are reused; no public type, function,
  serialized path spelling, or ordering changes.

### Self-adversarial pass

- Raw interpolation of two already encoded paths can be byte-correct today but preserves no
  compiler tooth. Decoding and encoding once makes the structural intent explicit and prevents a
  future caller from supplying an unencoded segment.
- Relative field logic cannot safely inspect the encoded suffix for `.` or `*`; those characters
  may belong to one literal segment. Segment-slice matching distinguishes immediate members from
  nested fields while retaining the current wildcard convention.
- The range-key rewrite previously carried both `p.*` and `p.*.` string sentinels. The final form
  carries only the encoded member path and computes a structural relative suffix, removing the
  trailing-dot protocol.
- Literal `*` remains represented by the same segment spelling as range membership. Every new
  wildcard check intentionally preserves that collision; repairing it here would smuggle B4b
  behavior into the dot/backslash round.
- Semver `split('.')` and test-local emitted-property parsing are distinct domains and remain.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflight.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,234 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +48 (62,186 to 62,234).

## A4 — owned JSON-kind and integer-range requirement operations

- Status: landed in `90f16dcb` (`fix(gen): separate kind and integer range domains`).
- Contract: behavior-bearing. Replace the disagreeing stringly requirement interpreters with two
  distinct owned operations: `admitted_json_value_kinds` for set-valued JSON kind-domain reasoning,
  with disjoint integer and non-integer-number classes, and `integer_range_constraint` for the
  value-sensitive universal constraint over members emitted by Helm integer ranges. Schema
  spellings must be projections of the appropriate operation; kind membership must not stand in
  for the integer-count quantifier.
- Acceptance baseline: `30e49d82` (A3).
- Baseline production LOC: 62,234 Rust lines from `task tokei:core` on `30e49d82`.
- Pre-registered acceptance expectations:
  - WIDEN only the ranged-integer lane for member requirements whose JSON kind domain admits
    integers through `SchemaType("number")`, `SchemaTypeEvenNull("number")`, or the corresponding
    exact alternative. Positive integer counts render because each produced member is an integer.
  - TIGHTEN or WIDEN an integer-count lane only where its per-member requirement has a
    value-sensitive boundary. `HelmTruthy` admits no positive count because member zero is falsy;
    `HelmFalsy` admits count one but not count two; a supported integer `NotEquals(n)` admits counts
    only through `n` when `n` is nonnegative and every count when `n` is negative. A truthy-scoped
    nonnumeric type admits at most count one because member zero escapes the consumer.
  - Array-item and object-value member schemas remain byte-identical; only the separate integer
    range arm may change. Ordinary whole-value requirement domains may change spelling internally
    but not acceptance.
  - Every changed corpus cell is adjudicated individually with Helm 4.2.3 before fixture adoption.
    Any unrelated acceptance flip, fixture byte change outside the registered integer arms, or
    candidate-accepts/Helm-aborts cell stops the round. Mandatory base and third-level coverage
    permit zero drops.

- Measured results:
  - `JsonValueKind` is a disjoint seven-case domain. Integer and non-integer number are distinct,
    and the `number` schema spelling expands to both instead of relying on string comparison.
  - `admitted_json_value_kinds` owns exhaustive requirement-domain interpretation. Whole-value and
    per-member null semantics use the typed `RequirementPosition` rather than the former Boolean
    mode, and overlay-domain comparison consumes the same operation.
  - `integer_range_constraint` separately computes the universal bound over generated members. It
    returns an unbounded lane, an inclusive maximum count, or `None` to abstain; conjunction takes
    the tighter bound and `AnyOf` takes the wider representable bound.
  - Member and member-except-key integer arms project that constraint to `maximum`; ranged array
    keys project the identical index sequence to `maxItems`. The object-key domain remains the
    independent string-kind projection.
  - Focused full-schema tests prove the formerly rejected `SchemaType("number")` member lane is an
    unbounded integer count and that ranged keys use the value-sensitive bound. Operation tests pin
    integer/non-integer-number separation plus the truthy, falsy, and integer-not-equals boundaries.
  - The immutable final3 archive contains 88 binaries and 126 files. Its clean schema and IR dumps
    write 84 and 18 artifacts and are fixture-exact after the two adjudicated fixture updates.
  - The full-depth comparison against `30e49d82` checks 121,055 probes across 60 charts and reports
    zero flips, with 112,260/112,260 base and 7,465/7,465 third-level probes.

- Deviations:
  - The first lint preflight exited 201 because Clippy grouped `HelmTruthy` with other variants
    returning the same zero bound. The exhaustive patterns were combined without a suppression;
    no failed-state artifact was produced.
  - The first immutable-archive preflight exited 101 before archive creation because Clang requires
    the absolute step-local `TMPDIR` to exist. The directory was created explicitly and the
    unchanged tree was archived under the distinct final2 name.
  - The existing optional-leaf rustdoc sat immediately before the deleted kind helper only because
    the included file ended there; inserting the new projection helpers would have attached that
    comment to the wrong operation. It now sits beside `optional_leaf_object_path_schema`, the
    function it describes.
  - `NotEquals(Float)` returns `None`: integer/float comparison compatibility is not inferred from
    JSON kinds. The IR producer already drops float not-equals member arms, so this abstention has
    no live route or fixture effect.
  - No registered corpus cell flipped. The precise numeric-count defect is outside the current 60
    charts' generated probes and is held by a direct full-schema regression instead.
  - The first complete integration gate exited 100 on two registered byte deltas: Jenkins' primary
    and secondary ingress TLS integer arms became the shared zero-bound definition, and Traefik's
    `ports` integer arm gained `maximum: 1`. Old and candidate schemas both reject the constructed
    distinguishing documents through independent constraints, and Helm aborts them too. Only those
    two adjudicated fixture files were adopted; the final archive was rebuilt afterward.
  - Production Rust grows by 170 lines because the two formerly conflated questions become
    exhaustive operations with a disjoint kind type and focused tests. No LOC promise was
    registered.

- Adjudication evidence:
  - The final comparison records zero flips and zero candidate-accepts/Helm-aborts cells. No fixture
    acceptance changes are adopted.
  - Jenkins `controller.ingress.tls=1` with ingress enabled: old schema rejects, candidate rejects,
    and `helm template` exits 1 at `len .` with `len of type int64`. The same probe for
    `controller.secondaryingress.tls=1` with its required hostname exits 1 at the corresponding
    `len .` call. The fixture-only ref rewrite therefore adds a redundant zero bound.
  - Traefik `ports=2`: old schema rejects, candidate rejects, and `helm template
    --skip-schema-validation` exits 1 because `service.yaml` cannot range over integer 2. The
    fixture-only `maximum: 1` is likewise redundant to the chart's other iterable constraints.
  - Helm 4.2.3 is the pinned adjudicator. The registered generated-member sequence is the frozen
    plan's `0..N-1` integer-count model; no direct raw-integer `range` assumption was added.

### Producer and route coverage

| Requirement route | Owned operation | Verification |
|---|---|---|
| Whole-value overlay domain | `admitted_json_value_kinds(WholeValue)` | Existing overlay/full corpus schemas remain byte-exact. |
| Member array/object schema | Existing exact requirement-schema lowering with typed `Member` position | Focused full-schema and complete generator suites pass. |
| Integer-count member lane | `integer_range_constraint` projected to `maximum` | Number, truthy, falsy, and not-equals operation tests pass. |
| Ranged array-key lane | Same constraint projected to `maxItems` | Focused full-schema key test and range-key suite pass. |
| Object-key lane | String membership from `admitted_json_value_kinds(Member)` | Existing pattern/property-name and corpus schemas remain exact. |
| Alternative/conjunctive requirements | Union/intersection of typed constraints | Exhaustive operation match plus complete workspace suite. |

### Review dossier

- Interpreter audit: the deleted `requirements_allow_runtime_kind` and
  `requirement_admits_runtime_type` have no remaining call sites. Requirement kind membership is
  owned by `admitted_json_value_kinds`; integer count bounds are owned only by
  `integer_range_constraint`.
- Focused proof: `cargo nextest run -p helm-schema-gen -E 'test(requirement_domain)'`; exit 0,
  four tests pass. `cargo nextest run -p helm-schema-gen`; exit 0, 615 tests pass.
- Immutable build: after the rejected missing-directory final1 preflight and the superseded
  pre-adjudication final2 archive, `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-build
  cargo nextest archive --workspace --archive-file /private/tmp/arch-v4-a4-final3.tar.zst`; exit 0,
  88 binaries and 126
  files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a4-final3.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a4-final3.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=30e49d82
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-a4-final3-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a4-final3.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: none. Both new types and operations are crate-private; serialized schemas,
  path spellings, and ordering stay unchanged outside the corrected integer arm.

### Self-adversarial pass

- JSON Schema `number` cannot be represented as one runtime label without losing whether a value
  is an integer. The disjoint enum prevents both the former false rejection and a future accidental
  `number == integer` string shortcut.
- Kind membership is existential over inhabitants; an integer-count lane is universal over every
  generated member. Keeping separate operations makes it impossible for truthiness-sensitive
  requirements to inherit the existential answer.
- `HelmTruthy` rejects every positive count because zero is always generated first. `HelmFalsy`
  accepts count one because its sole member is zero, then rejects count two when member one appears.
  An excluded nonnegative integer `n` first appears when count exceeds `n`, hence inclusive
  `maximum: n`.
- Array keys and integer-count members share the same generated sequence but different JSON Schema
  hosts. One typed constraint projected to `maxItems` or `maximum` closes that hand-synced semantic
  pair without conflating their schemas.
- Field-host requirements still force a zero integer count at `MembersAt` before their leaf
  requirement is considered: generated integers cannot host the selected field. That separate
  structural rule is unchanged.
- The corpus's zero flips are not treated as proof that the defect was unreachable. The direct
  full-schema regression constructs the exact `Members { allow_integer: true }` carrier and pins
  the corrected emitted schema.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflight.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0 after the rejected exit-100 pre-adjudication sweep.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,404 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +170 (62,234 to 62,404).

## A5 — refuse unfaithful composite truthiness under negation

- Status: landed in `57277fb1` (`fix(ir): separate exact and control condition fidelity`).
- Contract: behavior-bearing. Make the existing faithfulness oracle reject the generic all-paths
  truthiness fallback for both `MergedLayers` and `FirstTruthy` whenever their exact decoders
  abstain. Cover field/selector projections as well as locals; do not begin B3's decoder/oracle
  consolidation in this round.
- Acceptance baseline: `90f16dcb` (A4).
- Baseline production LOC: 62,404 Rust lines from `task tokei:core` on `90f16dcb`.
- Pre-registered acceptance expectations:
  - Only fail-branch reachability inherited through an undecodable `MergedLayers` or `FirstTruthy`
    condition may change. An exactly decoded composite remains byte- and acceptance-identical.
  - The affected family may TIGHTEN where the former negated all-paths approximation weakened an
    abort requirement, or WIDEN where refusing the approximation makes the surrounding capture
    abstain. Every changed cell requires individual Helm 4.2.3 adjudication; direction alone is not
    a verdict.
  - Ordinary raw-path, literal, call, derived-text, and exact merge/selection conditions remain
    unchanged. Any fixture delta outside composite-truthiness fail reachability stops the round.
  - Candidate-accepts/Helm-aborts allowance remains zero. Mandatory base and third-level probe
    categories permit zero drops.

- Measured results:
  - `condition_lowering_is_faithful` now classifies `MergedLayers` and `FirstTruthy` as exact only
    when their owned truthiness decoder succeeds, for both field/selector projections and locals.
  - A distinct `ConditionFidelityUse::Control` mode preserves the established positive-polarity
    all-paths approximation for structural control analysis. Recursive `not` always switches back
    to exact use, so the approximation cannot cross a polarity inversion.
  - Fragment assignment, block/inline control, kind-source, and condition-carrier call sites use the
    control-grade query. Negation, exact default/merge decoding, De Morgan lowering, and literal
    dispatch continue to require the exact query.
  - Focused tests prove undecodable merged selectors and first-truthy locals are unfaithful for
    exact use but remain usable for positive control, while decodable composites stay exact.
  - The immutable final2 archive contains 88 binaries and 126 files. Its one clean schema dump and
    IR dump write 84 and 18 artifacts and remain fixture-exact.
  - The full-depth comparison against `90f16dcb` checks 121,055 probes across 60 charts and reports
    zero flips, with 112,260/112,260 base and 7,465/7,465 third-level probes.

- Deviations:
  - Rejected preflight final1 applied the stricter answer to every consumer of the historic
    `faithful` Boolean. The full-depth run exposed 98 flips and failed with 63
    candidate-accepts/Helm-aborts cells, concentrated in Airflow `workers.*` and Kyverno
    `*.imagePullSecrets`. Those positive structural consumers rely on a sound wider condition and
    must not be made to abstain merely because that condition cannot be negated. No final1 fixture
    or dump was adopted.
  - The corrected final2 design makes the consumer intent explicit with a two-case private enum and
    retains one recursive classification table. This is a temporary seam: B3 remains responsible
    for returning exactness with the decoded predicate and deleting the oracle/table duality.
  - The first lint preflight exited 201 when the direct patch crossed the 100-line limit. A local
    composite-classification operation removed duplicated matching without a suppression. Two
    subsequent lint preflights also exited 201 while the purpose-aware recursion remained one and
    then four lines over the limit; field/selector classification was extracted as its own direct
    method and lint passed.
  - No corpus acceptance cell or fixture byte changed. The confirmed asymmetric oracle defect is
    pinned by focused private tests; the current corpus has no mutation witness that reaches its
    negated undecodable shape after positive-control preservation.
  - Production Rust grows by 41 lines for the typed fidelity purpose and regression coverage. No
    LOC promise was registered.

- Adjudication evidence:
  - Rejected final1 was individually replayed by the battery with Helm 4.2.3: 63 of its 98 changed
    cells were candidate accepts where Helm aborted, so the design was discarded before fixture
    adoption.
  - Final2 reports zero flips and zero candidate-accepts/Helm-aborts cells. No fixture is adopted and
    no additional individual replay is required.

### Producer and route coverage

| Consumer route | Fidelity use | Verification |
|---|---|---|
| Fail-branch negation / De Morgan | Exact | Composite regression and zero-allowance battery. |
| `not` recursion | Exact regardless of enclosing use | Exhaustive call branch plus focused undecodable cases. |
| Exact default, merge, and dispatch decoding | Exact | Existing condition-predicate and corpus suites remain exact. |
| Block and inline structural control | Control | Final1 false-accept family disappears in final2; fixtures remain exact. |
| Assignment truthy reductions | Control | Airflow/Kyverno corpus and reaudit controls remain exact. |
| Kind/source branch carriers | Control at the positive boundary | Existing provider/kind partition fixtures remain exact. |

### Review dossier

- Oracle audit: every cross-module call site now names control-grade use; every remaining
  `condition_lowering_is_faithful` call is within the condition decoder and sits at a polarity or
  exactness boundary.
- Focused proof: `cargo nextest run -p helm-schema-ir -E 'test(composite_truthiness)'`; exit 0, two
  tests pass.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-a5-final2.tar.zst`; exit 0, 88
  binaries and 126 files. The final1 archive and dumps belong to the rejected design.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a5-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a5-final2.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=90f16dcb
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-a5-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a5-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: none. The fidelity purpose and both queries are crate-private; contract
  serialization, predicate wire shape, and schema ordering are unchanged.

### Self-adversarial pass

- A wider positive condition can be safe because it attributes fewer branch-specific guarantees;
  negating that same wider condition reverses containment and is unsound. The purpose enum records
  exactly that polarity boundary instead of treating one Boolean as universally meaningful.
- `MergedLayers` already had a variable-only exactness check. The selector route and
  `FirstTruthy` route now use the same exact composite classifier, closing both asymmetric holes.
- Control mode deliberately reproduces the old field/selector approximation and the old
  first-truthy-local fallback, but it does not relax merged locals, whose exact disjunction was
  already required. The rejected final1 battery proves this distinction is load-bearing.
- `not` ignores the caller's control purpose and recursively asks for exact fidelity. This prevents
  a future control caller from accidentally laundering an all-paths approximation through a nested
  negation.
- The new purpose split is not presented as the final architecture. B3's scheduled
  `Decoded::{Exact, Approximate}` result should make the distinction a property of one decode and
  delete both query methods.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflights.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,445 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +41 (62,404 to 62,445).

## A6 — branch-local range domain owned by IR

- Status: landed in `e99d74b6` (`fix(ir): publish branch-local range domains`).
- Contract: behavior-bearing. IR publishes one branch-scoped `RangeDomain` on conditional overlay
  evidence from the guarded range facts it already owns. Overlay finalization must not promote
  decoded/destructured modes from sibling branches into that carrier, and gen must render the
  carrier rather than recomputing integer admission from a different fact subset.
- Acceptance baseline: `57277fb1` (A5).
- Baseline production LOC: 62,445 Rust lines from `task tokei:core` on `57277fb1`.
- Pre-registered acceptance expectations:
  - TIGHTEN a live JSON-decoded guarded range from accepting integer counts to rejecting them; JSON
    round-tripping produces non-iterable `float64` values in that branch.
  - Preserve the integer-count lane in the complementary raw guarded branch. A decoded sibling may
    not suppress the raw branch through path-global fact promotion.
  - Preserve existing destructured-range, structured-member, string-member-contract, array, map,
    null, and dormant-branch behavior. Any fixture or acceptance delta outside the guarded range
    domain family stops the round for individual Helm 4.2.3 adjudication.
  - Candidate-accepts/Helm-aborts allowance remains zero. Mandatory base and third-level probe
    categories permit zero drops.

- Measured results:
  - Core now carries `RangeDomain::{CollectionOnly, CollectionOrIntegerCount}` on each
    `ConditionalOverlayEvidence`. The carrier owns the range header's integer-count admission and
    exposes one exhaustive `allows_integer` projection.
  - IR records the carrier at the guarded range capture, intersects multiple range headers sharing
    a branch, and completes it with existing structured/string member restrictions before
    publishing the overlay. Decoded/destructured modes are no longer ORed in from path-global
    sibling facts.
  - Gen deletes the three-fact integer-domain reconstruction and renders the carrier. Member and
    provider schemas continue to conjoin afterward as separate body constraints.
  - Complementary-guard coverage proves the same path accepts integer counts only in its raw branch
    and rejects them in its JSON-decoded branch. The existing isolated decoded/raw test remains
    green.
  - The immutable final7 archive contains 88 binaries and 126 files. Its clean schema and IR dumps
    write 84 and 18 artifacts and are fixture-exact after the adjudicated carrier-driven updates.
  - The full-depth comparison against `57277fb1` checks 121,055 probes across 60 charts and reports
    zero flips, with 112,260/112,260 base and 7,465/7,465 third-level probes.

- Deviations:
  - The first compile preflight exited 101 because the new public carrier type was not yet included
    in core's and IR's explicit facade re-export lists. Both lists now name `RangeDomain`; no failed
    artifact was produced.
  - The first complementary test rendered each member through `toYaml`, which independently
    constrains the member/body domain and correctly removed the raw integer lane. The probe was
    rewritten to a neutral whole-member render so it isolates only the range header distinction;
    the body constraint remains covered elsewhere.
  - The first lint preflight exited 201 on unnecessary hashes around that neutral raw string. The
    hashes were removed without suppression.
  - Rejected final1 published the header carrier without folding existing structured/string member
    restrictions into IR's complete branch domain. The battery found four WIDEN cells, all false
    accepts against Helm: Bitnami Redis `global.imagePullSecrets` and `image.pullSecrets`, Jenkins
    `agent.additionalContainers`, and SigNoz `global.imagePullSecrets` with integer inputs. No
    final1 fixture or dump was adopted.
  - Final2 performs that intersection in IR, not gen, while keeping decoded/destructured header mode
    branch-local. The four false accepts disappear and the complementary raw branch still passes.
  - The first full integration sweep then exposed 28 corpus fixture deltas plus the lean temporal
    wrapper. Explicit raw-branch carriers restore integer arms that the old path-global sibling
    mode erased; definition extraction consequently renumbers many refs. The final2/final3 sweep
    was stopped after the complete fixture list was audited rather than spending the remaining
    gate time on a known failing tree.
  - The full-depth prober reports zero acceptance flips across those byte deltas: independent body
    and requirement constraints keep the accepted documents unchanged. The 28 corpus fixtures and
    one lean fixture were adopted from one clean dump as the carrier was completed.
  - Final4's syntactic strict-subset propagation fixed Sealed Secrets' provider row under a raw
    range but missed SigNoz's logically equivalent De Morgan spelling of the same guard. Its full
    integration run therefore failed those two focused controls, and the state was rejected.
    Final5 converts each exact conditional guard set back to the shared `Predicate` vocabulary and
    uses bounded `exactly_implies`; both focused controls pass.
  - The first complete final5 integration run then reported 17 chart fixture mismatches plus the
    lean temporal wrapper. Exact implication removed stale integer lanes from rows whose enclosing
    range domain the earlier vector containment could not recognize; definition extraction also
    renumbered refs. The already-complete final5 prober reported zero acceptance flips. Only those
    18 files were copied from the single clean final5 dump. A final self-adversarial pass changed
    the focused fallible test to return `eyre::Result` instead of using `expect`; the immutable
    final7 archive and dumps therefore own the authoritative gates.
  - Production Rust grows by 145 lines for the typed carrier, its accumulator ownership, exact
    branch implication, and focused regression coverage. No LOC promise was registered.

- Adjudication evidence:
  - Rejected final1 was replayed by the battery with Helm 4.2.3: all four flips were candidate
    accepts where Helm aborted. The design was discarded before fixture adoption.
  - Final2/final3 reports zero flips and zero candidate-accepts/Helm-aborts cells. Because no
    acceptance cell changes, there is no individual Helm replay to perform before adopting the 29
    carrier-driven full-schema fixture bytes.

### Producer and route coverage

| Route | Range-domain owner | Verification |
|---|---|---|
| Guarded raw single-variable range | IR capture publishes `CollectionOrIntegerCount` | Complementary branch test accepts integer only here. |
| Guarded JSON-decoded range | IR capture publishes `CollectionOnly` | Same test rejects integer in the decoded branch. |
| Guarded destructured range | IR capture publishes `CollectionOnly` | Existing destructured-range suites remain exact. |
| Multiple range headers in one branch | Accumulator intersects carriers | Exhaustive two-variant merge operation. |
| Structured/string member body | IR completes carrier to `CollectionOnly` | Final1 false-accept family disappears; existing member suites pass. |
| Overlay schema emission | Gen calls `RangeDomain::allows_integer` | Deleted three-fact reconstruction; corpus remains exact. |

### Review dossier

- Ownership audit: `range_allows_integer` in gen now reads only the typed carrier. The two
  path-global decoded/destructured promotions in `conditional_overlay_evidence` are deleted;
  guarded range capture is the producer.
- Focused proof: `cargo nextest run -p helm-schema-gen`; exit 0, 616 tests pass, including the new
  complementary branch matrix and existing decoded/raw isolation.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-a6-final7.tar.zst`; exit 0, 88
  binaries and 126 files. The final1 archive belongs to the rejected false-accept design; final2 and
  final3 precede the adjudicated fixture adoption, and final4 precedes exact guard implication.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a6-final7.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a6-final7.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=57277fb1
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-a6-final7-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a6-final7.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: additive public API narrowing of ownership. `RangeDomain` and the
  `ConditionalOverlayEvidence::range_domain` field are public because the public contract signal
  tree exposes overlay evidence. No serialized wire format changes; these types do not derive
  serde. Gen is the sole production reader in this round.

### Self-adversarial pass

- Adding `has_json_decoded_range_use` to gen's old conjunction would make a decoded sibling kill a
  raw branch because overlay evidence previously ORed that fact path-wide. Recording the carrier at
  each guarded range capture prevents that cross-branch contamination.
- Header domain and body validity are distinct phases. IR owns the complete published range domain,
  but structured/string member facts narrow it after the header carrier is recorded; gen only
  renders the result and then conjoins concrete member schemas.
- Two range headers under the same normalized guard both execute, so their accepted input domain is
  the intersection: `CollectionOnly` absorbs `CollectionOrIntegerCount`. Union would recreate a
  false accept.
- A missing carrier on legacy ranged evidence falls back inside IR from that branch's own facts.
  This compatibility path is not a second gen interpretation and keeps construction total while
  every producer migrates through the accumulator.
- A nested render row can spell the header condition differently after Boolean normalization.
  Exact predicate implication, rather than vector containment, admits De Morgan-equivalent guards
  while remaining unable to borrow from a complementary sibling.
- The public enum names the input-channel distinction explicitly. It does not claim raw JSON
  integers always render; the existing diagnostic still documents Helm's values-file versus
  `--set` provenance difference.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected 201 preflight.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0.
- `task test:all`; exit 0.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 62,590 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +145 (62,445 to 62,590).

## A7 — activation-scoped overlay projection and default sources

- Status: landed in `c9967847`.
- Contract: behavior-bearing. Preserve the two distinct fact channels across conditional chart
  activation without pretending they share one consumption model. Root-overlay projection clones
  abort-grade implications and conjoins the chart activation predicate. Default sources stay out
  of unconditional values composition and instead participate in branch-aware prepared-values and
  absence lowering only while their activation predicate holds.
- Acceptance baseline: `e99d74b6` (A6).
- Baseline production LOC: 62,590 Rust lines from `task tokei:core` on `e99d74b6`.
- Pre-registered acceptance expectations:
  - TIGHTEN wrong-kind and missing-value probes reached through a dependency-local root overlay
    only while that dependency's `condition:`/`tags:` activation formula is true. Preserve the
    disabled branch and every unrelated root or sibling dependency.
  - WIDEN deletion probes only where an active dependency's runtime merge supplies the deleted
    effective target from its guarded source. Preserve rejection when the source is also absent,
    and do not apply those defaults while activation is false.
  - Nested activation is the conjunction of every ancestor level; multiple condition/tag
    alternatives remain alternatives rather than being flattened into one unconditional fact.
  - Preserve unconditional root overlays and default sources byte-for-byte. Any fixture or
    acceptance delta outside the two activation-scoped families stops the round for individual
    Helm 4.2.3 adjudication. Candidate-accepts/Helm-aborts allowance remains zero. Mandatory base
    and third-level probe categories permit zero drops.

- Measured results:
  - IR no longer clears either fact family. It moves default-source and root-overlay facts into
    separate activation-scoped carriers, rebases both the carrier payloads and their guards, and
    preserves them exhaustively through `ObservedFacts::absorb`.
  - Root overlays now retain both their effective target root and source subtree. Core projects
    each abort-grade implication from the target-relative suffix onto the source path and conjoins
    the normalized activation disjunction; dependency scoping therefore produces
    `child.profile.name`, never the duplicated `child.profile.child.name` spelling.
  - Guarded default sources have their own public signal type and never enter the unconditional
    composition set. Gen prepares an alternate composed/deeper/refill document for each exact
    activation domain, selects it only when the lowered consumer predicate proves that domain, and
    lowers the target/source absence alternative under the same activation.
  - Identical facts from condition/tag alternatives are grouped by payload and retain one explicit
    Boolean disjunction. Nested facts retain every ancestor guard. Multiple sources active under
    one branch are applied together, while the existing unique-source-per-target abstention stays
    intact.
  - A conservative same-template execution boundary drops an activated default-source fact when
    that contract has no rendered consumer outside the source subtree. This prevents a mutation in
    one Helm template from becoming a fictitious default for a separately executed template;
    helper-expanded and ordinary same-template consumers retain the fact.
  - The focused chart matrix accepts the active source-supplied token and disabled invalid values,
    rejects an active integer root-overlay name and a null-deleted source token, and pins the
    carrier predicates beside the generated behavior.
  - The immutable final2 archive contains 88 binaries and 126 files. Its clean schema and IR dumps
    write 84 and 18 artifacts with zero fixture-byte changes.
  - The full-depth comparison against `e99d74b6` checks 121,055 probes across 60 charts and reports
    zero flips, with 112,260/112,260 base and 7,465/7,465 third-level probes.

- Deviations:
  - The first lint preflight exited 201 because guarded document preparation pushed
    `LoweredEmissionPlan::build` over the line cap. The preparation became one named phase helper;
    no suppression was added.
  - The next lint preflight exited 201 on one needless raw-string hash and two oversized focused
    tests. The hash was removed, chart construction and signal inspection became named test
    helpers, and nested-carrier coverage moved into its compact IR operation test.
  - Rejected final1 initially retained every guarded default source at chart scope. A direct Helm
    4.2.3 preflight split the mutation and consumer across separate template files: the candidate
    accepted the source-supplied document while Helm aborted because `.Values` mutation does not
    cross template executions. No final1 artifact was adopted. The corrected design retains a
    guarded source only with a same-contract rendered consumer; the explicit mutation-only test
    pins the abstention.
  - The first missing-source Helm input used `defaults: {}`, which Helm coalesced with the declared
    mapping rather than deleting its `token`. The adjudication was corrected to the actual
    null-deletion input `defaults.token: null`; Helm then aborted and the candidate rejected it.
  - Production Rust grows by 548 lines. The measured shape contradicts no registered LOC promise:
    preserving two semantically distinct guarded channels required a total carrier, scoped
    overlay identity, prepared-document variants, and exhaustive conversion tests.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` on the exact chart under
    `target/arch-v4-a7-helm-probe`: declared active defaults render (exit 0); active
    `profile.name=3` aborts at `b64enc` (exit 1); active `defaults.token=null` aborts at
    `.Values.token.value` (exit 1); disabled with both invalid values renders no child manifests
    (exit 0). The candidate verdicts are accept, reject, reject, accept respectively.
  - The rejected cross-template preflight aborted in Helm at `.Values.token.value` while the first
    candidate accepted it. That candidate was discarded before any fixture adoption.
  - The final full-depth corpus reports zero flips and zero candidate-accepts/Helm-aborts cells, so
    no corpus fixture required individual replay or adoption.

### Producer and route coverage

| Route | Expected owner | Verification |
|---|---|---|
| Unconditional root overlay | Scoped target/source pair with empty guards | Byte-exact corpus and existing Istiod regressions. |
| Activated root overlay | Scoped pair plus activation predicate | Focused enabled/disabled implication test and Helm replay. |
| Unconditional default source | Existing eager composition set | Byte-exact regression. |
| Activated default source | Guarded source handled after exact activation selection | Focused active/inactive deletion and source-absence matrix. |
| Nested dependency activation | Cross-product guard conjunction | IR two-level carrier test plus existing chart activation test. |
| Multiple activation alternatives | One normalized disjunction per payload | Focused condition fallback and predicate-implication assertion. |
| Separate template execution | Conservative carrier abstention | Mutation-only IR test and rejected Helm preflight. |

### Review dossier

- Carrier audit: the old `clear` calls are deleted. Unguarded sources remain the only input to
  eager composition; guarded sources have one accessor and are consumed only by guarded document
  preparation and guarded terminal absence lowering.
- Focused proof: `cargo nextest run -p helm-schema-ir -E
  'test(activation_guards_scope_values_default_sources) |
  test(activation_drops_a_default_source_without_same_template_consumers) |
  test(nested_activation_conjoins_every_default_source_guard) |
  test(activation_guards_scope_dependency_root_overlay_twins)'`; exit 0, four tests pass.
  `cargo nextest run -p helm-schema -E
  'test(dependency_activation_scopes_root_overlay_and_default_source_facts)'`; exit 0.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-a7-final2.tar.zst`; exit 0, 88
  binaries and 126 files. Final1 belongs to the rejected cross-template design.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a7-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a7-final2.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=e99d74b6
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-a7-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a7-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only --no-capture`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells.
- Public/wire decision: additive public API. `GuardedValuesDefaultSource`, its signal accessor, the
  guarded/scoped root-overlay builder, and exact `ConditionalGuard` predicate reconstruction expose
  the new phase boundary because `ContractSchemaSignals` is public. No serde type changes and no
  inspection wire-format changes.

### Self-adversarial pass

- One generic guarded wrapper would invite eager composition of default sources. The implementation
  keeps overlay implications and prepared values as distinct types and consumption paths.
- Exact predicate implication selects prepared documents. Syntactic subset tests would miss the
  normalized condition/tag disjunction and recreate the active-source false rejection.
- Multiple guarded sources sharing one activation are composed together before absence lowering;
  multiple sources for one target still abstain through the existing unique-source rule.
- A payload grouped across activation alternatives keeps a Boolean disjunction, not an
  unconditional fact. Nested conjunctions remain inside each disjunct.
- Template execution is a semantic boundary: a mutation-only contract cannot lend defaults to a
  sibling template. The conservative consumer-presence check prefers an open result if execution
  scope cannot be proved.
- Root-overlay rebasing uses target-relative suffixes. Carrying only the mapped source prefix would
  duplicate a dependency root and project requirements to the wrong path.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the two rejected 201 preflights.
- `task lint:fc`; exit 0.
- `cargo nextest run --workspace`; exit 0, 1,285 tests pass.
- `task test:integration`; exit 0, 564 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,853 tests pass and 24 are skipped.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,138 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +548 (62,590 to 63,138).

## A8 — bounded exactness and fanout abstention

- Status: landed in `90c037b6`.
- Contract: behavior-bearing, three corrected operations. `TruthCondition` may become exact only
  when its proven polarities are both disjoint and exhaustive. All three scalar fanout sites must
  replace over-cap alternatives with one unconditional taint carrying only the union of influencing
  paths. A changed truthy reduction that cannot be stamped within `MAX_STAMPED_GUARDS` must be
  removed before branch joining.
- Acceptance baseline: `c9967847` (A7).
- Baseline production LOC: 63,138 Rust lines from `task tokei:core` on `c9967847`.
- Pre-registered acceptance expectations:
  - WIDEN only predicates previously narrowed through a `complete=true` claim whose true/false
    subsets are not both disjoint and exhaustive. Preserve every genuinely total partition.
  - WIDEN only over-cap scalar fragments whose old unconditional concatenation invented text or
    quote-context claims. The fallback retains influencing paths but no rendered-value semantics.
  - WIDEN only branch-local truthy reductions that exceed the stamping cap; the reduction must not
    escape through the later union join. Preserve every reduction at or below the cap.
  - These are bounded stress families and are expected to produce zero corpus fixture or acceptance
    changes. Any other delta stops the round for individual Helm 4.2.3 adjudication.
    Candidate-accepts/Helm-aborts allowance remains zero; mandatory base and third-level probe
    categories permit zero drops.

- Measured results:
  - All three operations are implemented. `TruthCondition::from_subsets` proves both polarity
    obligations through the bounded BDD; the rendered-identity producer now supplies the exact
    complement it already knows instead of an empty false subset.
  - The three fanout sites share one collapse operation. It deletes all text and splice semantics,
    unions only influencing paths, and publishes a provenance-only taint that cannot emit a
    scalar/value-kind claim.
  - Truthy stamping separates the monotone accumulator's newly added alternatives from its entry
    alternatives. A changed contribution that already implies the branch condition needs no
    stamp; a stamp that still exceeds six guards removes the reduction and records a truthiness
    abstention before the later union join.
  - The authoritative corpus has zero acceptance flips across 60 charts and 121,055 emitted
    probes. Mandatory base coverage is 112,260/112,260 and third-level coverage is
    7,465/7,465, both with zero drops.
  - Six schema fixtures change byte-for-byte: Argo CD, Datadog, Falco, Harbor, oauth2-proxy, and
    the lean temporal-wrapper lane. The full-depth battery finds no acceptance delta in any of
    them; the symbolic-IR corpus remains byte-exact.
- Deviations:
  - The pre-registration expected zero fixture changes. The six byte changes above were not
    adopted until the final immutable dump and complete old-vs-new battery established zero
    acceptance flips and zero accepted-abort cells. They are disclosed rather than described as
    normalization.
  - Final1 retained an unstamped fragment-value fallback after deleting the reduction; Falco's
    defaults became a candidate rejection. Rejected, and no artifact was adopted.
  - Final2 added an abstention marker but exposed an under-specified rendered-identity equality:
    its `complete=true` producer supplied `False` rather than the known complement. Airflow,
    Cilium, and Datadog then admitted candidate-accepts/Helm-aborts cells. Rejected; the producer
    was corrected before a new archive.
  - Final3 still made Prometheus reject Helm-renderable non-string values and made Velero accept
    Helm-aborting legacy object forms. Isolation proved the former came from mutating a stored
    reduction merely to measure its compact size, and the latter from replacing the accumulator's
    non-truth value carrier. Both designs were rejected.
  - Final6/7 normalized a large reduction before applying the bound. Kyverno consumed one CPU core
    for 776 seconds before the run was interrupted; this was an algorithmic regression, not
    macOS execution-policy latency. The preflight was rejected and replaced with bounded,
    non-expanding implication checks.
  - Final8 through final14 were successive rejected cap preflights. Prometheus and Velero exposed
    that the entry accumulator must be separated from only the newly added alternative, nested
    presence implies parent-key/range liveness, and representation-mutating DNF minimization is not
    acceptable merely to prove a stamp redundant. No dump from those code states was adopted.
  - Final15 passed the battery but was superseded by a smaller equivalent implementation before
    adoption. Final16/17 then failed four merge-shadowing unit teeth: truthy child paths jointly
    implied matching parent-presence disjunctions, but the bounded implication checker only handled
    one consequent at a time. The corrected pairwise disjunction proof restores all four teeth.
    Final18 established the final production behavior; final19 rebuilds the same code with every
    adjudicated fixture embedded and is the sole authoritative artifact.
  - The clean final6 dump hit the repository's former ten-minute nextest termination threshold
    while Kyverno was still running. The separately committed infrastructure round `78fb04a7`
    raises integration and CI termination to the already-established three-hour job ceiling; it is
    not folded into A8.
- Adjudication evidence:
  - Helm version: v4.2.3, pinned by `82f0ea11` and used by the authoritative prober.
  - Final16 reports zero acceptance flips, so there are no changed cells requiring individual
    fixture adoption. The zero candidate-accepts/Helm-aborts allowance is satisfied exactly.
  - Rejected preflights were individually replayed before further engineering: Falco defaults,
    Prometheus `server.useExistingClusterRoleName`, Velero legacy backup/snapshot location objects,
    and the Airflow/Cilium/Datadog cells named above. No artifact from a failing replay was copied
    into `testdata/`.

### Producer and route coverage

| Route | Corrected operation | Verification |
|---|---|---|
| Truth subset construction | Bounded disjointness and exhaustiveness proof | Direct exact/overlap/gap matrix. |
| Cross-segment scalar fanout | Unconditional path-only taint | Focused over-cap fragment test. |
| Alternative scalar fanout | Same path-only taint helper | Focused choice/first-truthy over-cap test. |
| Inline branch scalar fanout | Same path-only taint helper | Focused inline-control over-cap test. |
| Truthy reduction stamping cap | Remove changed reduction | Direct state test plus branch-join regression. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-ir -E
  'test(over_cap_branch_stamp_removes_the_changed_truthy_reduction) |
  test(over_cap_scalar_arms_keep_only_influencing_paths) |
  test(complete_subset_claim_requires_a_total_disjoint_partition)'`; exit 0. Focused final-tree
  corpus generation for Falco, Kyverno, Prometheus, and Velero also exits 0; Kyverno returns to
  18.5 seconds after the rejected unbounded-normalization preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-a8-final19.tar.zst`; exit 0, 88
  binaries and 126 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a8-final19.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and 84 artifacts are written.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-a8-final19.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written. The same archive without dump variables passes against every existing IR fixture.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=c9967847
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-a8-final19-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-a8-final19.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: no public API or wire-format change. The additional `TaintPart` state is
  crate-private, `Default` preserves the prior ordinary-taint behavior, and the abstaining
  constructor is crate-private.

### Self-adversarial pass

- A `complete` Boolean is not a proof of both polarities. The direct gap and overlap cases remain
  partial even when the caller passes `true`; the rendered-identity producer must publish its
  actual false subset.
- An ordinary scalar taint still implies partial rendered text. Reusing it for fanout abstention
  recreated a Prometheus string false rejection, so provenance-only taint is an explicit state and
  every value-kind publisher checks it.
- Removing a truthy reduction is insufficient if condition lowering immediately reconstructs the
  same claim from the local's fragment value. The abstention marker crosses snapshots, joins, and
  condition contexts without deleting non-truth shape facts.
- A monotone accumulator's entry alternatives were already scoped when they were recorded. Only
  its newly added alternative is eligible for the current branch stamp; stamping the whole union
  creates exponential predicates and false decisions.
- BDD normalization is bounded for the exactness proof, but invoking it as a pre-cap simplifier on
  a large accumulator defeats the bound. The final stamp proof performs no distributive expansion.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,288 tests pass.
- `task test:integration`; exit 0, 564 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,856 tests pass and 24 are skipped, including all four live-network
  tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,457 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +319 (63,138 to 63,457).

## S-A1 — total sibling-junctor expansion

- Status: landed in `3bfc6942`.
- Contract: behavior-bearing deletion of the first-junctor early return in provider-schema
  expansion. The existing generic keyword walk must expand every `allOf`, `anyOf`, and `oneOf`
  sibling instead of resolving only the first keyword present.
- Acceptance baseline: `90c037b6` (A8).
- Baseline production LOC: 63,457 Rust lines from `task tokei:core` on `90c037b6`.
- Pre-registered acceptance expectations:
  - Resolve `$ref`s under every sibling junctor while preserving their keyword, item order, and
    surrounding node exactly.
  - The expansion is semantically equivalent for valid provider documents and is expected to
    produce zero corpus acceptance flips and zero fixture changes. A changed cell stops the round
    for individual Helm 4.2.3 adjudication before any fixture is adopted.
  - Candidate-accepts/Helm-aborts allowance remains zero; mandatory base and third-level probe
    categories permit zero drops.

- Measured results:
  - Deleted the eight-line first-junctor loop and early return. The existing exhaustive keyword
    walker now expands all schema-array keywords, so sibling `allOf`, `anyOf`, and `oneOf` arrays
    are processed in one pass.
  - The authoritative corpus remains byte-exact. The full-depth battery covers 60 charts and
    121,055 probes with zero acceptance flips; mandatory base and third-level categories have zero
    drops.
- Deviations: the first immutable archive's focused test returned `eyre::Result<()>` despite having
  no fallible setup, so `task lint` rejected it under `clippy::unnecessary_wraps`. Removing the
  unused result channel was test-only and produced final2; production code, the clean dump, and all
  acceptance results are unchanged.
- Adjudication evidence: Helm v4.2.3; zero changed acceptance cells and zero
  candidate-accepts/Helm-aborts cells, so no fixture required individual adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| `allOf` plus sibling `anyOf` | Both arrays expanded | Direct provider-document equality test. |
| `anyOf` plus sibling `oneOf` | Both arrays expanded | Same exhaustive sibling matrix. |
| Nested `$ref` inside each item | Resolved through ordinary recursion | Cross-file provider fixture. |
| Node without junctors | Existing generic keyword behavior | Byte-exact corpus. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-k8s -E
  'test(full_expansion_resolves_every_sibling_junctor)'`; exit 0. One cross-file document carries
  all three sibling junctors and proves each referenced item expands while the sibling title and
  item order remain exact.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-sa1-final2.tar.zst`; exit 0, 88
  binaries and 126 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa1-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass, 84 artifacts are written, and every fixture is byte-exact.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sa1-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=90c037b6
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sa1-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sa1-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: none. The deleted branch and new crate-private test do not alter public API
  or serialized formats.

### Self-adversarial pass

- The generic walker classifies junctors as schema arrays and retains the parent node while replacing
  each array value, so deleting the special case does not change sibling-key ownership.
- `$ref` cycle detection and the depth cap still execute inside each array item's ordinary recursive
  expansion; total sibling traversal does not weaken either bound.
- Boolean schema children remain skipped exactly as before, and non-schema data keywords are not
  traversed.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected unnecessary-result preflight.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,289 tests pass.
- `task test:integration`; exit 0, 564 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,857 tests pass and 24 are skipped, including all four live-network
  tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,449 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -8 (63,457 to 63,449).

## S-A2 — delete the fused lookup trace

- Status: landed in `390bd303`.
- Contract: behavior-bearing deletion of both production-dead `LookupTrace` halves, their duplicate
  tri-state, and both recorders. Resource lookup must carry local-override-unreadable directly on
  the chain outcome at the decision point; API-presence lookup keeps only its answer. Offline tests
  are re-pinned to answers and recorded fetcher calls.
- Acceptance baseline: `3bfc6942` (S-A1).
- Baseline production LOC: 63,449 Rust lines from `task tokei:core` on `3bfc6942`.
- Pre-registered acceptance expectations:
  - Schema resolution, capability answers, provider ordering, and fetch/cache behavior remain
    unchanged; zero schema fixture or acceptance flips are expected.
  - The local-override-unreadable diagnostic begins firing when a local provider owns the resource
    but cannot read its document and the chain cannot resolve elsewhere. This diagnostic-only
    family is the sole expected behavioral delta.
  - Any other diagnostic or acceptance delta stops the round before fixture adoption. The
    candidate-accepts/Helm-aborts allowance remains zero; mandatory base and third-level probe
    categories permit zero drops.

- Measured results:
  - Deleted `LookupTrace`, both traced result wrappers, the source-probe trace tri-state, and the
    provider/chain recorders. Resource and capability lookup now return their existing semantic
    outcomes directly.
  - Added `ChainLookupOutcome::LocalOverrideUnreadable` at the provider decision point. Candidate
    planning retains the first such diagnostic and publishes it only after every candidate misses;
    an ordinary missing resource still follows the existing missing-schema route.
  - Capability probes now use `SourceDocOutcome` directly. The offline matrix proves the same
    positive, authoritative-negative, and uncertain answers together with the same fetcher-call
    counts, so the cache-safety contract is preserved without a second tri-state.
  - The authoritative schema and symbolic-IR corpora remain byte-exact. The full-depth battery
    covers 60 charts and 121,055 probes with zero acceptance flips; mandatory base coverage is
    112,260/112,260 and third-level coverage is 7,465/7,465, both with zero drops.
- Deviations:
  - The first lint preflight rejected two `let ... else` expressions in the simplified capability
    path under `clippy::question_mark`. They were replaced with the direct `?` form before the
    immutable archive was built; no rejected-state artifact was adopted.
  - Removing the direct traced-chain test initially left the unreadable-override tooth below the
    production diagnostic projection. The final test drives `schema_fragment_for_use` through the
    real resource oracle and asserts the emitted diagnostic, so the pre-registered behavior is
    pinned at its actual consumer boundary.
- Adjudication evidence: Helm v4.2.3. The sole registered delta is diagnostic-only and is pinned by
  the local-override regression; the schema battery reports zero changed cells and zero
  candidate-accepts/Helm-aborts cells, so no fixture required adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Resource found after earlier misses | Same fragment, no trace allocation | Chain provider-order test. |
| Owned local override unreadable, terminal miss | Direct outcome diagnostic | Focused diagnostic regression. |
| Ordinary terminal miss | Existing missing-schema diagnostic only | Focused negative control. |
| API-presence positive/negative/uncertain | Same tri-state answer | Offline oracle matrix plus fetcher calls. |
| Resource-qualified capability probe | Same direct provider sequence | Existing capability tests. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-k8s --profile integration -E
  'binary(lookup_chain) | binary(capability_oracle_offline)'`; exit 0, 23 tests pass. The ordinary
  crate suite also passes 70 tests.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-sa2-final2.tar.zst`; exit 0, 88
  binaries and 126 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa2-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass and every fixture is byte-exact.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa2-final2.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes, 18 artifacts are
  written, and every fixture is byte-exact.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=3bfc6942
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sa2-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sa2-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: this round deliberately narrows the public Rust API by deleting the dead
  trace types and traced methods. `ChainLookupOutcome` gains the explicit unreadable-override
  variant needed by the surviving semantic path. No serialized wire format changes.

### Self-adversarial pass

- An unreadable local document is not the same as an absent resource. Carrying it as a typed chain
  outcome prevents the diagnostic from depending on a discarded execution log while still
  allowing a later candidate to resolve successfully.
- An uncertain capability probe remains potentially live. Folding the trace tri-state into
  `SourceDocOutcome` preserves `None` whenever any source is uncertain and no source succeeds;
  only an all-authoritative miss returns `Some(false)`.
- Provider lookup caching remains keyed and ordered exactly as before. The deletion changes what
  is retained after each lookup, not which provider is called or which semantic result wins.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected question-mark preflight.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,289 tests pass.
- `task test:integration`; exit 0, 562 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,855 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,253 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -196 (63,449 to 63,253).

## S-A3 — required-in-parent single descent

- Status: landed in `12b2f79a`.
- Contract: behavior-bearing replacement of the independent parent re-descent with one path descent
  that returns the actual parent selected alongside the leaf. Requiredness must be read from that
  parent, eliminating the repeated provider walk and the must-agree junctor invariant.
- Acceptance baseline: `390bd303` (S-A2).
- Baseline production LOC: 63,253 Rust lines from `task tokei:core` on `390bd303`.
- Pre-registered acceptance expectations:
  - A leaf reached through a junctor reads `required` from the exact branch that supplied that leaf,
    including when the containing node has no top-level `required` list. This is the sole expected
    behavior family.
  - Ordinary property paths, array-item paths, dynamic mapping values, cross-file references, and
    unresolved paths retain their current schema and requiredness outcomes.
  - The authoritative corpus is expected to remain byte-exact with zero acceptance flips. Any
    changed fixture or acceptance cell stops the round for individual Helm v4.2.3 adjudication
    before adoption. Candidate-accepts/Helm-aborts allowance remains zero; mandatory base and
    third-level categories permit zero drops.

- Measured results:
  - `descend_schema_path_node` now returns the parent selected for the final segment together with
    the leaf. `required_in_parent` reads that exact node instead of rebuilding a root and walking
    the prefix a second time.
  - The direct `anyOf` regression proves a leaf whose `required` membership exists only inside the
    selected branch is marked provider-required. Terminal array-item and dynamic-mapping segments
    remain non-required; both cross-file location/expansion controls remain exact.
  - Schema and symbolic-IR dumps are byte-for-byte identical to S-A2. The full-depth battery covers
    60 charts and 121,055 probes with zero acceptance flips; mandatory base coverage is
    112,260/112,260 and third-level coverage is 7,465/7,465, both with zero drops.
- Deviations:
  - The first immutable-archive preflight named a step-local `TMPDIR` before creating the directory;
    clang failed immediately with `unable to make temporary file`. No archive or dump was produced.
    A distinct pre-created final2 directory was used for the sole authoritative archive and every
    downstream artifact.
- Adjudication evidence: Helm v4.2.3. The selected-junctor-parent correction is pinned directly,
  while the authoritative corpus has zero changed fixture bytes, zero acceptance flips, and zero
  candidate-accepts/Helm-aborts cells; no fixture required adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Plain property leaf | Same parent `required` membership | Existing provider-required tests. |
| Leaf selected inside a junctor | Requiredness from selected branch | New direct branch regression. |
| Cross-file parent or leaf `$ref` | Same location and expanded schema | Existing cross-file descent matrix. |
| Array item or dynamic mapping | Never provider-required | Focused negative controls. |
| Missing path | Same abstention | Existing lookup tests and corpus. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-k8s -E
  'test(path_descent_reads_required_from_the_selected_junctor_parent) |
  test(terminal_collection_segments_are_not_required_leaves) |
  test(lazy_path_descent_matches_full_expansion_for_cross_file_array_ref) |
  test(lazy_path_descent_reports_leaf_source_location_after_cross_file_refs)'`; exit 0, four tests
  pass.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-sa3-final2.tar.zst`; exit 0, 88
  binaries and 126 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa3-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the S-A2 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa3-final2.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written. A recursive byte comparison against the S-A2 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=390bd303
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sa3-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sa3-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: none. The changed descent functions and parent carrier are crate-private;
  public schema fragments and serialized formats are unchanged.

### Self-adversarial pass

- Capturing the node before the final descent would still be wrong when the final segment is found
  inside a junctor. The segment function therefore returns the resolved branch node that actually
  supplied the child, not merely the caller's containing node.
- Cross-file `$ref` resolution still happens at the same per-segment boundary. The returned parent
  is post-reference and post-junctor selection, while the leaf retains its source location before
  expansion.
- Empty paths have no parent and remain non-required. Terminal `[*]` and dynamic-map markers are
  rejected before consulting the selected parent's `required` array.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,291 tests pass.
- `task test:integration`; exit 0, 562 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,857 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,245 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -8 (63,253 to 63,245).

## S-A4 — derive inference candidate ordering

- Status: landed in `0c74988d`.
- Contract: behavior-bearing deletion of the two hand-written rank tables and the diagnostic
  sink's `Debug`-string ordering. Both aggregation and diagnostic canonicalisation must use the
  enums' derived declaration order, with one test pinning declaration-order-as-priority.
- Acceptance baseline: `12b2f79a` (S-A3).
- Baseline production LOC: 63,245 Rust lines from `task tokei:core` on `12b2f79a`.
- Pre-registered acceptance expectations:
  - Inference resolution remains identical because both rank tables already reproduce the enums'
    declaration order exactly.
  - Ambiguous diagnostic candidates with the same `api_version` change from alphabetical
    `Debug`-name order to declaration-priority order: sources become `ChartLocalCrd`, `Shortlist`,
    `LocalCacheScan`, `OnlineProbe`; origins become `LocalOverride`, `ChartLocalCrd`,
    `DefaultCatalog`, `KubernetesOpenApi`. This one diagnostic-order family is expected and must
    not change the candidate set.
  - Schema and symbolic-IR fixtures must remain byte-exact with zero acceptance flips. Any other
    diagnostic or acceptance delta stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Aggregation and diagnostic canonicalisation now compare `InferenceSource` and `ProviderOrigin`
    directly. The two exhaustive hand-written rank functions and the allocating `Debug`-string
    comparisons are deleted.
  - One direct test pins both enum declaration orders as the priority contract. The existing
    same-version aggregation tooth still selects `Shortlist` over `LocalCacheScan`.
  - The registered diagnostic-order family changes exactly as expected: the candidate set is
    unchanged, while equal-api-version candidates follow source priority and then origin priority.
  - Schema and symbolic-IR dumps are byte-for-byte identical to S-A3. The full-depth battery covers
    60 charts and 121,055 probes with zero acceptance flips; mandatory base coverage is
    112,260/112,260 and third-level coverage is 7,465/7,465, both with zero drops.
- Deviations:
  - The first focused compilation warned that `InferenceSource` remained imported by production
    after the rank-table deletion. The type is now imported only by the private test module; the
    warning-producing preflight preceded lint and no immutable artifact was built from it.
- Adjudication evidence: Helm v4.2.3. The sole changed family is diagnostic candidate ordering and
  is adjudicated by exact candidate-vector equality; candidate membership and inference winners
  remain unchanged. The Helm-backed schema battery has zero changed cells and zero
  candidate-accepts/Helm-aborts cells, so no fixture required adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Inference-source tie break | Declaration order is priority | Direct exhaustive enum-order test. |
| Provider-origin tie break | Declaration order is priority | Same exhaustive test. |
| Resolved same-version candidates | Same winner | Aggregator priority regression. |
| Ambiguous diagnostic candidates | Priority order, unchanged set | Canonicalisation regression. |
| Schema consumers | No emitted-schema change | Byte-exact corpus and full-depth battery. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-k8s -E
  'test(declaration_order_is_inference_priority)'` and `cargo nextest run -p helm-schema-k8s
  --profile integration -E 'test(ambiguous_candidates_use_inference_priority_order) |
  test(api_version_guess_aggregates_across_providers)'`; both exit 0, three tests pass.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-sa4-final1.tar.zst`; exit 0, 88
  binaries and 126 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa4-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the S-A3 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sa4-final1.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written. A recursive byte comparison against the S-A3 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=12b2f79a
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sa4-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sa4-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: the public enum variants and their derived ordering were already part of
  the Rust API. This round deliberately makes ambiguous diagnostic JSON candidate arrays follow
  that documented priority instead of alphabetical debug names; the candidate set and field
  representation are unchanged.

### Self-adversarial pass

- Declaration order is now semantic priority, so the exhaustive order test is load-bearing: future
  variant insertion or reordering must update the priority contract deliberately.
- Sorting still begins with `api_version`, preserving deterministic grouping and the aggregator's
  first/last single-version shortcut. Only equal-version tie breakers changed.
- `Debug` formatting is no longer a hidden wire-order dependency. Diagnostic canonicalisation and
  inference selection now share the same typed comparisons without a helper facade or duplicate
  table.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected unused-import preflight.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,292 tests pass.
- `task test:integration`; exit 0, 563 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,859 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,229 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -16 (63,245 to 63,229).

## S-D1 — narrow the IR fragment domain

- Status: landed in `8429e2bd`.
- Contract: representation-only move of the two fragment golden-test modules into `src/tests/`,
  followed by crate-private fragment-domain visibility, deletion of the one-field
  `SymbolicIrContextInner` wrapper, removal of its redundant `Rc` layer, and deletion of
  constructors that become provably dead. This API narrowing precedes B2/B4 by design.
- Acceptance baseline: `0c74988d` (S-A4).
- Baseline production LOC: 63,229 Rust lines from `task tokei:core` on `0c74988d`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, fragment-golden, diagnostic, or acceptance changes. The two moved
    test modules must execute the same test bodies with byte-exact expected strings.
  - `SymbolicIrContext` remains public with the same construction and contract-generation
    behavior; only the test-only fragment evaluation surface and fragment-domain types are removed
    from the public Rust API.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Moved `fragment_golden.rs` and `fragment_dict_config_guards.rs` from crate integration targets
    into `src/tests/`. All 12 test bodies and their reviewed expected strings are unchanged and pass
    from the private test module tree.
  - Made the `fragment_eval` module, its domain types, evaluated document/read carriers, dump, and
    `eval_document_fragment` crate-private or test-only. Broad domain re-exports disappeared; only
    the two test-used types retain a test-only re-export.
  - Deleted `Guarded::conditional` and `Splice::scalar`, the two constructors exposed only because
    the domain was public and unused by production or private tests.
  - Deleted one-field `SymbolicIrContextInner`; `SymbolicIrContext` now shares `Rc<IrAnalysisDb>`
    directly, preserving clone/cache semantics with one fewer allocation and dereference layer.
  - Schema and symbolic-IR dumps are byte-for-byte identical to S-A4. The full-depth battery covers
    60 charts and 121,055 probes with zero acceptance flips; mandatory base coverage is
    112,260/112,260 and third-level coverage is 7,465/7,465, both with zero drops.
- Deviations:
  - The first visibility preflight left the former broad domain re-export in place and compiled the
    dump/evaluation hook in non-test builds. `cargo check` reported unused imports and dead dump
    functions. That state was rejected; the final tree removes the re-export and gates the golden
    hook under `cfg(test)`. No immutable artifact was produced from the warning state.
  - `task test:integration` decreases from 563 to 551 because the 12 moved golden tests no longer
    form integration binaries. This is an intentional test-layout transfer, not a coverage drop:
    `cargo nextest run --workspace` increases from 1,292 to 1,304 and the combined suite remains
    exactly 1,859 tests.
- Adjudication evidence: Helm v4.2.3. This representation-only round has zero changed fixture
  bytes, zero acceptance flips, and zero candidate-accepts/Helm-aborts cells; no fixture required
  adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Fragment golden dumps | Same byte strings after module move | Moved golden module suite. |
| Dict-context guard goldens | Same guards and projections | Moved dict-config module suite. |
| Public contract generation | Same `ContractIr` | Full unit/integration corpus. |
| Context reuse | Same shared analysis cache | Existing multi-template session tests. |
| Fragment projection | Same schema and IR output | Immutable dumps and full-depth battery. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-ir -E
  'test(/fragment_(golden|dict_config_guards)/)'`; exit 0, all 12 moved tests pass. `cargo check -p
  helm-schema-ir --tests`; exit 0 with no warnings after the rejected visibility preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-sd1-final1.tar.zst`; exit 0, 86
  binaries and 124 files. The two-binary reduction is exactly the moved integration-test targets.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sd1-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the S-A4 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sd1-final1.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written. A recursive byte comparison against the S-A4 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=0c74988d
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sd1-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-sd1-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: deliberate Rust API narrowing. `helm_schema_ir::fragment_eval`, its
  fragment-domain and evaluated-document types, `dump_document`, and
  `SymbolicIrContext::eval_document_fragment` are no longer public. The audit found no production
  workspace consumer; the only external consumers were the two moved golden targets. No wire
  format changes.

### Self-adversarial pass

- `SymbolicIrContext` still derives `Clone`, and cloning still shares the same `IrAnalysisDb` cache;
  moving the `Rc` inward changes neither ownership nor cache identity.
- The dump module is test-only, but the fragment interpreter and projection remain production code.
  Only the observation hook used by the moved goldens is cfg-gated.
- Moving tests out of `tests/` could silently reduce the integration profile. The explicit 12-test
  focused run, +12 unit-suite count, unchanged combined-suite count, and byte-exact dumps account
  for every moved tooth.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,304 tests pass.
- `task test:integration`; exit 0, 551 tests pass and 24 are skipped; the 12-test decrease is the
  disclosed move into the unit suite.
- `task test:all`; exit 0, 1,859 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,212 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -17 (63,229 to 63,212).

## B1.1 — exhaustive capture-kind dispatcher

- Status: landed in `86ff2213`.
- Contract: representation-only conversion of `record_fail_conjunction` from an ordered
  `if let` ladder into one exhaustive early-return `CaptureKind` match with no wildcard. Only
  `CaptureKind::Fail` may reach the generic fail-negation tail. `StringRequirement` gets its own
  exhaustive nested `StringRequirementRoute` match because its four routes are behavior-sensitive.
- Acceptance baseline: `8429e2bd` (S-D1).
- Baseline production LOC: 63,212 Rust lines from `task tokei:core` on `8429e2bd`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, or acceptance changes. Every existing capture kind and
    string-requirement route must execute the same lowering body and return at the same boundary.
  - `Selected` string requirements retain their member-scope rewrite when available and their
    existing execution-scoped fallback otherwise; `Serialized` still abstains; `Direct` alone may
    publish unconditional facts; `Scoped` stays conditional.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Replaced the 18-arm ordered `if let` ladder with one exhaustive 19-variant `CaptureKind`
    match. Every specialized kind records through its existing helper and returns; the explicit
    `Fail` arm is the only path into generic predicate negation.
  - `StringRequirement` contains an exhaustive nested match over `Direct`, `Scoped`, `Selected`,
    and `Serialized`. `Selected` retains its member rewrite and scoped fallback; the common tail is
    reached only by the three non-serialized routes.
  - The 57-test focused contract/signal suite passes without expected-value edits. Schema and
    symbolic-IR dumps are byte-for-byte identical to S-D1.
  - The full-depth battery covers 60 charts and 121,055 probes with zero acceptance flips;
    mandatory base coverage is 112,260/112,260 and third-level coverage is 7,465/7,465, both with
    zero drops.
- Deviations: none. The first compiled dispatcher passed the focused suite, lint, and immutable
  battery; no rejected code-state artifact was produced.
- Adjudication evidence: Helm v4.2.3. This representation-only round has zero changed fixture
  bytes, zero acceptance flips, and zero candidate-accepts/Helm-aborts cells; no fixture required
  adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Every `CaptureKind` variant | Same dedicated lowering and early return | Compiler-exhaustive match plus existing suite. |
| `StringRequirement::Serialized` | Abstain | Focused string-route controls. |
| `StringRequirement::Selected` | Member rewrite or scoped fallback | Existing selected-member tests and corpus. |
| `StringRequirement::Direct` | Unconditional only with empty scope | Existing direct string controls. |
| `StringRequirement::Scoped` | Conditional capture only | Existing scoped controls. |
| `CaptureKind::Fail` | Sole generic-negation fallthrough | Focused fail-lowering suite. |

### Review dossier

- Focused proof: `cargo nextest run -p helm-schema-ir -E
  'test(/tests::contract::/) | test(/tests::contract_signals::/)'`; exit 0, all 57 contract and
  signal tests pass.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b11-final1.tar.zst`; exit 0, 86
  binaries and 124 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b11-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the S-D1 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b11-final1.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are
  written. A recursive byte comparison against the S-D1 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=8429e2bd
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b11-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b11-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: none. Both enums and the dispatcher are crate-private; serialized contracts
  and public APIs are unchanged.

### Self-adversarial pass

- `Selected` cannot simply return after its nested match: when no member-range rewrite applies, its
  exact selection predicates must still join the ordinary scoped capture. The common tail remains
  reachable for that case.
- `Serialized` returns from the nested match before any selection predicate or string type fact is
  published, preserving the certified-serializer abstention.
- The final `Fail` arm is syntactically explicit and empty. Adding a new capture kind now fails
  compilation at this semantic dispatch point instead of silently falling into fail negation.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,304 tests pass.
- `task test:integration`; exit 0, 551 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,859 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,217 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +5 (63,212 to 63,217).

## B1.2 — exhaustive semantic destructures

- Status: landed in `0e042d29`.
- Contract: representation-only compiler-enforcement sweep across both `ContractValuePathFacts`
  merges, all struct-level values-path mappers, `BoundHelperCallCacheKey::from_resolution`, the
  predicate contract-guard twins, and both identity predicates. Universally quantified render-use
  bits gain an `AllUses` identity wrapper whose default is true.
- Acceptance baseline: `86ff2213` (B1.1).
- Baseline production LOC: 63,217 Rust lines from `task tokei:core` on `86ff2213`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, or acceptance changes. `AllUses` must preserve every
    existing computed Boolean while making empty/default aggregation use the mathematical identity
    `true` instead of the derived-Boolean default `false`.
  - Path rebasing must rewrite exactly the fields it rewrites today and name every deliberately
    unmapped field in exhaustive destructures. Helper cache keys must retain the same complete key.
  - Predicate guard projection must return the same guard vectors and exactness verdicts from one
    owner. Identity predicates must retain their current truth tables.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Added `AllUses`, whose `Default` is the universal empty-set identity `true`, and replaced both
    universal render-use Booleans. `ContractValuePathFacts` now has an explicit exhaustive default;
    both render-fact merges exhaustively destructure the carrier and choose every field's merge or
    deliberate non-merge behavior.
  - `ContractUse`, `ObservedFacts`, and `ContractIr` path mapping now begin with exhaustive
    destructures. Rendered YAML paths, helper names, and other deliberately unmapped fields are
    named explicitly; existing rebasing behavior is unchanged.
  - `BoundHelperCallCacheKey::from_resolution` destructures all four semantic resolution fields
    before cloning them into the key, keeping future inputs from silently escaping cache identity.
  - Collapsed `contract_guards`, `contract_guards_are_exact`, the separate negation recursion, and
    the `Or` helper into one polarity-aware `Option<Vec<Guard>>` flatten. `None` is the sole inexact
    result, so callers cannot accidentally consume a partial vector after ignoring exactness.
  - Both helper-output identity predicates and the fragment splice identity predicate exhaustively
    destructure their carriers, preserving the current truth tables while making new fields fail
    compilation at the classification sites.
  - Schema and symbolic-IR dumps are byte-for-byte identical to B1.1. The full-depth battery covers
    60 charts and 121,055 probes with zero acceptance flips; mandatory base coverage is
    112,260/112,260 and third-level coverage is 7,465/7,465, both with zero drops.
- Deviations:
  - The first compiler preflight intentionally exposed every Boolean call site and every old
    guard-vector consumer. Those compile failures guided explicit `.holds()` and `Option` handling;
    no immutable artifact was produced from an uncompilable state.
  - The first lint preflight rejected the mandated explicit `ContractValuePathFacts::default` as
    derivable. A narrow `#[expect(clippy::derivable_impls)]` records why the exhaustive field list is
    semantic enforcement and remains self-validating.
  - Final1's immutable dumps and prober were byte-exact, but the full unit gate found one failing
    synthetic resolve-policy tooth: it set `has_render_use: true` with `..default()` and implicitly
    relied on the old false universal defaults. Final1 was rejected. Four synthetic literals now
    state their intended universal facts explicitly; their focused suite passes 27/27. A fresh
    final2 archive is the sole adopted artifact.
- Adjudication evidence: Helm v4.2.3. Final2 has zero changed fixture bytes, zero acceptance flips,
  and zero candidate-accepts/Helm-aborts cells; no fixture required adoption.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Path-fact branch merges | Same union and universal quantification | Direct identity/merge matrix. |
| `ContractUse`, `ObservedFacts`, `ContractIr` path maps | Same mapped and unmapped fields | Existing rebasing tests plus exhaustive destructures. |
| Bound helper cache key | Same bindings/dot/root facts/seen set | Existing cache identity suite. |
| Positive/negative guard flattening | Same vector and exactness | Predicate truth-table suite. |
| Helper/splice identity | Same classifications | Existing identity tests. |

### Review dossier

- Focused proof: 39 core predicate tests, four core public-surface tests, 101 IR
  condition/path/identity tests, and 27 resolve-policy/shape tests all pass. The focused tests pin
  universal identity, false contribution merging, whole-formula guard abstention, path rebasing,
  and identity classifications.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b12-final2.tar.zst`; exit 0, 86
  binaries and 124 files. Final1 is rejected as described above.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b12-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the B1.1 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b12-final2.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written. A
  recursive byte comparison against the B1.1 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=86ff2213
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b12-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b12-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells.
- Public/wire decision: deliberate Rust API enforcement. The two public universal fields now use
  `AllUses` instead of `bool`; callers read them through `holds()`. `Predicate::contract_guards`
  now returns `Option<Vec<Guard>>`, and `contract_guards_are_exact` is deleted because exactness is
  represented by `Some`. No serialized wire format changes.

### Self-adversarial pass

- `AllUses::default() == true` is safe only as an aggregation identity. Synthetic tests that assert
  a render exists must state whether all uses satisfy a property; the rejected final1 proved that
  implicit false assumptions can no longer hide in `..default()`.
- An inexact `And` must not keep exact sibling guards while dropping its opaque conjunct. The new
  direct regression proves the whole flatten returns `None`, and control-flow callers retain the
  original raw predicate in that case.
- The path mappers explicitly leave rendered document paths and helper include names unchanged.
  Their destructures make those omissions reviewable rather than relying on fields not mentioned
  by the old imperative bodies.
- The helper cache key still owns clones of bindings, dot, root truth predicates, root dispatches,
  and the seen set; the destructure changes compile-time completeness, not key equality.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the rejected derivable-default preflight.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings.
- `cargo nextest run --workspace`; exit 0, 1,305 tests pass.
- `task test:integration`; exit 0, 553 tests pass and 24 are skipped.
- `task test:all`; exit 0, 1,862 tests pass and 24 are skipped, including all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,417 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +200 (63,217 to 63,417).

## B1.3 — derived serde and real generator modules

- Status: landed in `a79c0c47`.
- Contract: representation-only deletion of the `WireContractUse` mirror in favor of direct
  `Deserialize`, plus conversion of five generator `include!` fragments into real Rust modules
  whose explicit visibility lists are compiler-checked.
- Acceptance baseline: `0e042d29` (B1.2).
- Baseline production LOC: 63,417 Rust lines from `task tokei:core` on `0e042d29`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, JSON wire, diagnostic, or acceptance changes. `ContractUse` must
    deserialize every existing field/default exactly as before; `GuardDnf` remains deliberately
    lossy across its public guard projection and is not represented as round-trip serialization.
  - The five module moves preserve every function body and call edge. Interface visibility may
    narrow to the actual parent/crate consumers but no production caller may disappear.
  - Any fixture, wire, or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance remains zero; mandatory base and third-level categories permit zero
    drops.

- Measured results:
  - `ContractUse` now derives `Deserialize` directly. The private `WireContractUse` field mirror
    and conversion are deleted, while serde field defaults and the flattened `GuardDnf` projection
    remain byte-compatible.
  - The five textual `include!` fragments are ordinary child modules with explicit imports and
    compiler-checked `pub(super)`/`pub(crate)` interfaces. Function bodies and call edges are
    unchanged.
  - Schema and symbolic-IR dumps are byte-for-byte identical to B1.2. The full-depth battery
    covers 60 charts and 121,055 probes with zero acceptance flips, zero mandatory base or
    third-level drops, and zero candidate-accepts/Helm-aborts cells.
- Deviations:
  - Protocol deviation: the ledger section was appended after the initial compiler preflight
    rather than before the first edit. The zero-change acceptance contract was stated in the active
    handoff and reiterated in commentary before implementation, and no immutable archive, dump, or
    fixture adoption occurred before this entry. This timing error is recorded rather than hidden.
  - The first compiler preflight rejected the mechanically moved fragments because their former
    textual parent scope had hidden the child interfaces. The functions used by parents were made
    `pub(super)` and the sibling-facing APIs remained `pub(crate)`; no artifact from the rejected
    state was adopted.
  - The first lint preflight rejected the fragments' temporary `use super::*` imports. Each module
    now names only the types and functions it consumes. No artifact from the wildcard-import state
    was adopted.
- Adjudication evidence: zero acceptance flips require no Helm cell adjudication. The prober ran
  with Helm adjudication enabled and reports zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| `ContractUse` current wire | Exact field/default decode | Round-trip and legacy-default tests. |
| `GuardDnf` wire projection | Same deliberate lossy guard form | Existing serde fixtures. |
| Overlay member/conditional modules | Same lowerings | Full gen unit and corpus suites. |
| Path fail-requirement module | Same requirement schemas | Requirement-domain suite. |
| Resolve-policy scalar/default modules | Same schema decisions | Resolve-policy and shape suites. |

### Review dossier

- Focused proof: all five core public-surface tests pass, including a new legacy-default decode
  test, and all 616 generator unit tests pass after the module moves.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b13-final1.tar.zst`; exit 0, 86
  binaries and 124 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b13-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the B1.2 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b13-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written. A
  recursive byte comparison against the B1.2 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=0e042d29
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b13-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b13-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed bounded
  reductions remain unchanged.
- Public/wire decision: the public `ContractUse` JSON wire is deliberately unchanged. Replacing its
  private deserialization mirror with a derive changes no public Rust signature. Generator module
  boundaries are crate-internal visibility narrowing only. `GuardDnf` documentation now states
  that its deserializer reconstructs the public guard projection rather than opaque predicates.

### Self-adversarial pass

- The legacy-default test omits every optional `ContractUse` wire field, proving the derive retains
  the former mirror's defaults rather than merely round-tripping current full documents.
- Explicit child imports expose the five real module dependencies in source. Parent-only entry
  points are `pub(super)`; APIs used by other generator modules retain only crate visibility.
- `GuardDnf` is intentionally not advertised as round-trip serde: serialization has always emitted
  the public guard projection and cannot recreate opaque internal predicates. The new comment
  prevents the direct `ContractUse` derive from implying a stronger invariant.
- Recursive schema and IR comparisons exclude only nextest's extracted archive directories; all
  authored dump artifacts participate in the byte comparison.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0 after the two rejected preflights above.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings; 1,648.33 seconds.
- `cargo nextest run --workspace`; exit 0, 1,305 tests pass.
- `task test:integration`; exit 0, 554 tests pass and 24 are skipped; 924.023 seconds.
- `task test:all`; exit 0, 1,863 tests pass and 24 are skipped, including all live-network tests;
  978.196 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,428 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +11 (63,417 to 63,428).

## G2 stage 1 — generated transform-by-position behavior suite

- Status: landed in `07429db6`.
- Contract: test-infrastructure-only addition of generated synthetic microcharts that pin current
  transform behavior at identity projection, member identity, range subject, `hasKey` decoding,
  and splice lowering positions. Each case must assert its produced semantic fact before asserting
  the complete generated schema. Exhaustive `Transform::ALL`, mixed-branch, removal/clear, scalar
  dispatch, quoted/plain capture, and payload-bearing-state coverage remains registered for G2
  stage 2 during B2.
- Acceptance baseline: `a79c0c47` (B1.3).
- Baseline production LOC: 63,428 Rust lines from `task tokei:core` on `a79c0c47`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, wire, or corpus acceptance changes. This round adds only
    generated tests and their test-local case vocabulary.
  - Every generated microchart must prove the intended IR/schema-signal fact before comparing the
    entire schema value; a schema-only selective assertion is not sufficient.
  - The suite must exercise all five consuming-position families with at least one
    identity-preserving and one identity-breaking transform route. It must not invent a production
    `Transform` enum early or make test-only production hooks.
  - Any existing fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance remains zero; mandatory base and third-level categories permit zero
    drops.

- Measured results:
  - One generated matrix owns ten synthetic microchart cells: a raw and transformed route for each
    of identity projection, member identity, range subject, `hasKey` host decoding, and splice
    lowering.
  - Every cell asserts the produced `ContractUse` or `ContractValuePathFacts`/requirement fact
    before comparing the complete emitted schema. The suite pins stringified serialized uses,
    JSON-decoded range integer exclusion, parsed-map `hasKey` abstention, YAML-serialized splices,
    and the current identity-preserving JSON member projection.
  - Schema and symbolic-IR corpus dumps remain byte-for-byte identical to B1.3. The full-depth
    battery covers 60 charts and 121,055 probes with zero acceptance flips, zero mandatory base or
    third-level drops, and zero candidate-accepts/Helm-aborts cells.
- Deviations:
  - The first inspection preflight printed the ten cells' uses, schema signals, and schemas instead
    of asserting them. It established the measured current matrix, then was deleted; no archive,
    dump, or fixture from the inspection scaffold was adopted.
  - The first assertion preflight used a nonexistent `serde_json::Map::from` array conversion and
    failed to compile. The test now collects the single property explicitly; no artifact from the
    rejected state was produced.
- Adjudication evidence: zero acceptance flips require no Helm cell adjudication. The prober ran
  with Helm adjudication enabled and reports zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Identity projection | Preserve only exact raw-value identities | Produced contract fact, then full schema equality. |
| Member identity | Preserve member projection only for structural identity | Produced member fact, then full schema equality. |
| Range subject | Retain the transform-specific iterable domain | Produced range fact, then full schema equality. |
| `hasKey` decoding | Bind the host kind without reviving transformed input identity | Produced requirement fact, then full schema equality. |
| Splice lowering | Preserve transform-specific scalar/fragment placement | Produced render fact, then full schema equality. |

### Review dossier

- Focused proof: `transforms_keep_their_position_specific_facts_and_schemas` passes all ten matrix
  cells. The test returns `eyre::Result`, reports missing facts as ordinary errors, and uses
  `sim_assert_eq!` for every equality.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-g2s1-final1.tar.zst`; exit 0, 86
  binaries and 124 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-g2s1-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the B1.3 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-g2s1-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written. A
  recursive byte comparison against the B1.3 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=a79c0c47
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-g2s1-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-g2s1-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed bounded
  reductions remain unchanged.
- Public/wire decision: none. The matrix is crate-private test code and changes neither public Rust
  API nor serialized formats.

### Self-adversarial pass

- The matrix does not infer correctness from final schema alone. Each branch first proves the
  specific producer fact B2 will migrate, so a later transform carrier cannot accidentally get the
  right schema through a different path.
- Raw comparison cells prevent the transformed route from redefining a position's baseline. The
  JSON member cell deliberately pins that roundtripping preserves member projection, while the JSON
  range cell independently pins that decoding removes Helm's integer-count lane.
- Parsed-map `hasKey` is not asserted as a raw object requirement: the transform supplies the map
  host. Its cell instead pins pathless fragment/serialization evidence and the absence of a raw
  `SchemaType("object")` implication.
- G2 stage 1 is intentionally not exhaustive over the future vocabulary. `Transform::ALL`,
  mixed-branch merge/clear semantics, capture positions, and payload-bearing identity breakers stay
  explicit stage-2 obligations rather than being falsely claimed here.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings; 451.48 seconds.
- `cargo nextest run --workspace`; exit 0, 1,306 tests pass.
- `task test:integration`; exit 0, 554 tests pass and 24 are skipped; 915.430 seconds.
- `task test:all`; exit 0, 1,864 tests pass and 24 are skipped, including all live-network tests;
  970.298 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,428 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: 0 (63,428 to 63,428); 260 test Rust lines added.

## B4a.1 — introduce the segmented `ValuesPath` carrier

- Status: landed in `e5aaf250`.
- Contract: representation-only introduction of the core `ValuesPath` value object with a private
  segmented representation, explicit `parse`/`from_segments`/`encode` boundaries, structural
  navigation, custom string serde, and manual ordering identical to the legacy encoded-string
  order. No semantic carrier migrates in this first crate-local round.
- Acceptance baseline: `07429db6` (G2 stage 1).
- Baseline production LOC: 63,428 Rust lines from `task tokei:core` on `07429db6`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, wire, ordering, or corpus acceptance changes. The new
    type is additive until the following crate-by-crate carrier migrations.
  - `parse(path).encode()` must preserve the legacy canonical spelling, including escaped dots and
    backslashes; serde must use that same string wire. `Ord` must compare exactly as the legacy
    encoded `String`, not by segment vectors.
  - The carrier exposes no `Deref<Target = str>`, `AsRef<str>`, or `Display`. Callers must choose a
    structural operation or an explicit encoded-string boundary.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Added public `ValuesPath` with a private `Vec<String>` representation, explicit parsing and
    encoding, structural navigation, custom string serde, and manual ordering over the legacy
    encoded character stream. It implements no string-coercion traits.
  - The existing `join_value_path`, `split_value_path`, and `append_value_path` APIs delegate to the
    carrier without changing their signatures or canonical output. Four public integration tests
    pin escaped-dot/backslash parsing, navigation, serde bytes, and B-tree order.
  - Schema and symbolic-IR corpus dumps remain byte-for-byte identical to G2 stage 1. The
    full-depth battery covers 60 charts and 121,055 probes with zero acceptance flips, zero
    mandatory base or third-level drops, and zero candidate-accepts/Helm-aborts cells.
- Deviations:
  - The first workspace preflight ran the focused test and `cargo check` concurrently, so the two
    Cargo processes serialized on build locks. Both completed successfully, but this saved no
    latency and is not repeated for shared-target Cargo work.
  - The first focused preflight reported a missing crate-level doc on the new integration test.
    The test gained a factual module doc and the final lint is warning-free; no archive or dump was
    produced from the warning-bearing state.
  - Self-review caught that the first helper delegation changed `join_value_path`'s generic bound
    from `AsRef<str>` to `Into<String>`. The original public signature was restored before the
    authoritative matrix and archive; no artifact from the narrowed-API state was adopted.
- Adjudication evidence: zero acceptance flips require no Helm cell adjudication. The prober ran
  with Helm adjudication enabled and reports zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Legacy string parse/encode | Canonical bytes preserved | Escaped-segment and empty-segment matrix. |
| Segment construction/navigation | Structural operations avoid string parsing | `segments`, `parent`, `push`, descendant, and item-parent tests. |
| JSON wire | Same encoded JSON string | Custom serde full-value equality. |
| B-tree order | Same as encoded `String` order | Adversarial sorted-set comparison. |
| Existing callers | No behavior change | Immutable corpus and full-depth battery. |

### Review dossier

- Focused proof: four `helm-schema-core/tests/value_path.rs` tests pass. They cover canonicalization
  of empty separators and non-special backslashes, literal dot/backslash segments, empty-segment
  push compatibility, strict descendant semantics, trailing item parents, JSON wire equality, and
  adversarial ordering against encoded `String` values.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a1-final1.tar.zst`; exit 0, 87
  binaries and 125 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a1-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the G2 stage-1 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a1-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written. A
  recursive byte comparison against the G2 stage-1 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=07429db6
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a1-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a1-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed bounded
  reductions remain unchanged.
- Public/wire decision: deliberate additive public API. `ValuesPath` is exported from core so the
  following crate migrations share one semantic carrier. Its JSON wire is the existing encoded
  string, and the pre-existing helper signatures remain unchanged. No existing public item narrows
  or changes format in this round.

### Self-adversarial pass

- `Ord` compares an iterator of encoded Unicode scalar values rather than the segment vector. UTF-8
  preserves scalar-value order, so this is exactly the legacy `String` byte order without allocating
  a temporary encoding at every B-tree comparison.
- Equality and hashing use canonical segments, matching ordering because parsing canonicalizes
  redundant separators and escaping. The tests include spellings whose segment-vector order would
  differ from encoded order.
- `item_parent` recognizes the legacy trailing literal `*` because B4b owns the later
  `EachMember`/literal-star distinction. B4a does not smuggle that behavior change into the carrier.
- The compatibility free functions now have one parser/encoder implementation while retaining the
  old argument and return types. No downstream carrier changes are hidden in this introductory
  round.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across Linux, Windows, and
  macOS, with zero errors and warnings; 1,715.53 seconds.
- `cargo nextest run --workspace`; exit 0, 1,306 tests pass.
- `task test:integration`; exit 0, 558 tests pass and 24 are skipped; 939.252 seconds.
- `task test:all`; exit 0, 1,868 tests pass and 24 are skipped, including all live-network tests;
  993.208 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- `PATH=/private/tmp/helm-schema-xargs-shim:$PATH
  HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema task -t
  /Volumes/T7/branches/luup2/deployment/charts/taskfile.yaml check:local`; exit 0, 32/32 charts
  pass.
- `task tokei:core`; exit 0, 63,553 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +125 (63,428 to 63,553).

## B4a.2a — migrate IR range-mode keys

- Status: landed in `fed82111`.
- Contract: representation-only migration of `RangeModes` map keys to `ValuesPath`, the first of
  the three IR-internal carriers named by B4a. Raw strings may be parsed at existing producer/query
  boundaries, but the map retains no parallel encoded path key.
- Acceptance baseline: `e5aaf250` (B4a.1).
- Baseline production LOC: 63,553 Rust lines from `task tokei:core` on `e5aaf250`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, wire, ordering, or corpus acceptance changes. All encoded
    strings leaving IR must preserve exact bytes and order.
  - Structural prefix and relative-path operations over range keys use `ValuesPath`; explicit
    `encode()` is confined to legacy capture/builder boundaries.
  - The migration must not add `Deref`, `AsRef<str>`, `Display`, string-comparison shims, or a cached
    encoded field to make old string operations compile.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results: `RangeModes` now stores `BTreeMap<ValuesPath, RangeMode>`; structural global
  projection consumes typed segments and legacy capture/builder boundaries encode explicitly. Two
  focused tests pin escaped identity, legacy order, and union after remapping. Schema and IR dumps
  are byte-identical to B4a.1; 60 charts and 121,055 probes report zero flips, zero mandatory drops,
  and zero candidate-accepts/Helm-aborts cells.
- Deviations:
  - The initial preflight attempted all three IR-private carriers together. The compiler exposed
    121 call sites spanning independent abstract-value and fragment-splice domains after the first
    structural fixes. That state was rejected and fully removed. The IR migration is split into
    `RangeModes`, `AbstractValue`, and `Splice` subrounds so each interface remains reviewable and
    byte-exact; no archive, dump, fixture, or test artifact from the rejected combined state is
    adopted.
- Adjudication evidence: zero flips require no Helm cell adjudication; adjudication ran enabled with
  zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Range modes | Same per-path union/remap/query facts | Range and fail-capture suites. |
| Builder/capture boundaries | Same encoded strings | Symbolic-IR and schema byte comparisons. |

### Review dossier

- Focused proof: two range-mode tests pass. Immutable archive
  `/private/tmp/arch-v4-b4a2a-final1.tar.zst` contains 87 binaries and 125 files. The clean 62-test
  schema dump and one-test/18-artifact IR dump are recursively byte-identical to B4a.1. The
  full-depth prober passes with 121,055 probes and 28,868 unchanged disclosed bounded reductions.
- Public/wire decision: none; `RangeModes` is crate-private and all boundary strings retain exact
  bytes and order.

### Self-adversarial pass

- Typed key ordering inherits B4a.1's legacy encoded-string `Ord`; remapping parses only after the
  existing string callback and unions collisions exactly once. The rejected combined-carrier
  preflight was fully reverted before the authoritative archive.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 combinations, 13 packages, three targets; 1,064.17 seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass.
- `task test:integration`; exit 0, 558 pass, 24 skipped; 930.365 seconds.
- `task test:all`; exit 0, 1,870 pass, 24 skipped including live-network tests; 984.682 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,564 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +11 (63,553 to 63,564).

## B4a.2b — migrate abstract-value path identities

- Status: landed in `054f7ed8`.
- Contract: representation-only migration of the frozen plan's named
  `AbstractValue::ValuesPath(String)` carrier to segmented `ValuesPath`. Decoded/output/range-key
  transform variants remain in their current representation until B2 migrates their transform
  provenance and payloads together.
- Acceptance baseline: `fed82111` (B4a.2a).
- Baseline production LOC: 63,564 Rust lines from `task tokei:core` on `fed82111`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, wire, ordering, or corpus acceptance changes.
  - Selector application, range-item projection, root recognition, ancestry, and removal operate
    structurally; explicit encoding occurs only at unmigrated carrier boundaries.
  - No string coercion trait, comparison shim, cached encoding, or parallel identity field may be
    added.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results: `AbstractValue::ValuesPath` now stores segmented `ValuesPath`; root/member/range
  navigation is structural and all still-string effects, predicates, output metadata, and contract
  boundaries encode explicitly. All 393 focused IR tests pass. Schema and IR dumps are byte-exact;
  60 charts and 121,055 probes report zero flips, zero mandatory drops, and zero
  candidate-accepts/Helm-aborts cells.
- Deviations:
  - A preflight migrated all five path-shaped variants together and exposed 166 compiler sites
    spanning decoded/output/range-key transform domains. The state was rejected and reduced to the
    exact `AbstractValue::ValuesPath(String)` item named by B4a; no archive, dump, or fixture from
    the rejected state is adopted. The remaining variants stay coupled to B2's transform-carrier
    migration rather than being split from their payload semantics here.
  - The first test migration retained 129 redundant `.to_string()` calls behind its test-local
    constructor macro; lint rejected that state. A bounded mechanical rewrite removed only those
    macro-argument allocations. No artifact from the rejected state was adopted.
- Adjudication evidence: zero flips require no Helm cell adjudication; adjudication ran enabled with
  zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Raw/decoded/output identities | Same identity and metadata selection | Abstract-value and expression suites. |
| Range keys/items | Same collection/member projection | Range and fail-capture suites. |
| Root/member selection | Same escaped structural paths | Selector and helper suites. |
| Effects/contract boundaries | Same encoded bytes and order | Schema/IR dumps and full-depth battery. |

### Review dossier

- Focused proof: 393/393 IR tests pass. Immutable archive
  `/private/tmp/arch-v4-b4a2b-final1.tar.zst` contains 87 binaries and 125 files. The clean 62-test
  schema dump and one-test/18-artifact IR dump are recursively byte-identical to B4a.2a. The
  full-depth prober passes 121,055 probes with 28,868 unchanged disclosed bounded reductions.
- Public/wire decision: none; `AbstractValue` is crate-private and every external path remains the
  exact legacy encoded string.

### Self-adversarial pass

- The raw identity is encoded only when entering still-string Effects, predicates, output metadata,
  scalar dispatch, or contract facts. Selector append, range-item append, root recognition,
  descendant checks, and item-parent checks are structural.
- Two function-level `too_many_lines` expectations document exhaustive identity/source dispatches
  whose explicit typed arm raised them just over the lint threshold; splitting would scatter the
  invariant. No broad lint allowance was added.
- Test-local `values_path!` macros eliminate fixture boilerplate without adding a production
  compatibility constructor or string coercion trait.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 combinations, 13 packages, three targets; 1,076.38 seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass.
- `task test:integration`; exit 0, 558 pass, 24 skipped; 922.300 seconds.
- `task test:all`; exit 0, 1,870 pass, 24 skipped including live-network tests; 975.868 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,682 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +118 (63,564 to 63,682).

## B4a.2c — migrate fragment splice paths

- Status: landed in `19f74410`.
- Contract: representation-only migration of fragment `Splice.values_path` to segmented
  `ValuesPath`. Fragment construction, placement, metadata lookup, rendered-row projection, and
  capture boundaries must preserve exact encoded output.
- Acceptance baseline: `054f7ed8` (B4a.2b).
- Baseline production LOC: 63,682 Rust lines from `task tokei:core` on `054f7ed8`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, wire, ordering, or corpus acceptance changes.
  - Item-parent and root checks use structural methods; explicit encoding occurs only at still-string
    metadata/effects/rendered-row boundaries.
  - No coercion trait, comparison shim, cached encoding, or parallel splice path field is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results: `Splice.values_path` now stores segmented `ValuesPath`; construction and
  structural item/root tests stay typed, while metadata, effects, rendered rows, captures, and dump
  text encode explicitly. All 393 focused IR tests pass. Schema and IR dumps are byte-exact; 60
  charts and 121,055 probes report zero flips, zero mandatory drops, and zero
  candidate-accepts/Helm-aborts cells.
- Deviations: none.
- Adjudication evidence: zero flips require no Helm cell adjudication; adjudication ran enabled with
  zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Splice construction | Same path and metadata | Fragment unit/golden suites. |
| Placement/rendered rows | Same encoded contract sources | Schema/IR byte dumps. |
| Capture/effect lookups | Same range/string/provider behavior | Full IR/gen and corpus suites. |

### Review dossier

- Focused proof: 393/393 IR tests pass. Immutable archive
  `/private/tmp/arch-v4-b4a2c-final1.tar.zst` contains 87 binaries and 125 files. The clean 62-test
  schema dump and one-test/18-artifact IR dump are recursively byte-identical to B4a.2b. The
  full-depth prober passes 121,055 probes with 28,868 unchanged disclosed bounded reductions.
- Public/wire decision: none; `Splice` is crate-private and its golden dump and every external path
  preserve exact legacy bytes.

### Self-adversarial pass

- Item-parent and root decisions use `item_parent()` and structural segment count. Every string map,
  capture, rendered row, contract use, and diagnostic/dump boundary calls `encode()` explicitly.
- The dump compiler failure proved no accidental `Display` escape hatch exists; the golden formatter
  was migrated at its legitimate wire boundary.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- `task lint:fc`; exit 0, 48 combinations, 13 packages, three targets; 1,040.44 seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass.
- `task test:integration`; exit 0, 558 pass, 24 skipped; 917.204 seconds.
- `task test:all`; exit 0, 1,870 pass, 24 skipped including live-network tests; 975.923 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,684 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +2 (63,682 to 63,684).

## B4a.3a — migrate predicate approximation paths

- Status: landed in `c72e7d1e` (`refactor(core): type approximation paths`).
- Contract: representation-only migration of `Predicate::Approximate.paths` to a segmented
  `BTreeSet<ValuesPath>`, the first compiler-bounded core guard subround. Atomic `Guard` and
  `ConditionalGuard` payloads follow separately. Approximation markers remain strings.
- Acceptance baseline: `19f74410` (B4a.2c).
- Baseline production LOC: 63,684 Rust lines from `task tokei:core` on `19f74410`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, JSON wire, ordering, or corpus acceptance changes.
  - Predicate ordering and diagnostic path bytes stay exact. Public approximation constructors
    accept explicit `ValuesPath` sets; downstream callers parse only at genuine AST/text boundaries.
    No coercion traits or string comparison shims are added.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results: `Predicate::Approximate.paths` now stores segmented `ValuesPath` values.
  Approximation construction parses transient source strings exactly once, collection encodes only
  at the existing public string boundary, remapping performs the existing string callback before
  rebuilding typed identities, and the fragment dump encodes explicitly. All 30 focused core
  predicate/guard tests pass. Schema and symbolic-IR dumps are recursively byte-exact; 60 charts
  and 121,055 probes report zero flips, zero mandatory drops, and zero candidate-accepts/Helm-aborts
  cells.
- Deviations:
  - The pre-registration named atomic `Guard`, `ConditionalGuard`, and approximation carriers as
    one core guard round. The compile surface proved each domain independently large enough to
    audit, so the round was split at compiler-enforced carrier boundaries: B4a.3a types only
    approximation storage; atomic and conditional guards follow in separate byte-exact rounds. No
    archive, dump, or fixture from the combined preflight is adopted.
  - The pre-registration expected the four public approximation constructors to accept
    `BTreeSet<ValuesPath>`. Their callers are parser/text boundaries that produce transient paths,
    and narrowing them would export conversion work without removing another persistent string
    carrier. The constructors therefore retain their existing `BTreeSet<String>` API and parse
    once into typed enum storage. Direct enum construction now uses the typed field. This preserves
    the public insertion boundary while eliminating the stored JSON-shape protocol.
- Adjudication evidence: zero flips require no Helm cell adjudication; adjudication ran enabled with
  zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Approximation constructors | Same markers, roles, and subsets | Predicate truth-table suites. |
| Path mapping/collection | Same rewritten encoded paths and order | Exhaustive mapping tests. |
| IR/gen consumers | Same behavior and output | Full workspace, dumps, and prober. |

### Review dossier

- Focused proof: 30/30 core predicate and `GuardDnf` tests pass. Workspace all-target compilation
  and the 393-test focused IR suite pass during the compiler-guided migration.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a3a-final1.tar.zst`; exit 0, 87
  binaries and 125 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a3a-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass. A recursive byte comparison against the B4a.2c dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3a-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes and 18 artifacts are written. A
  recursive byte comparison against the B4a.2c dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=19f74410
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3a-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3a-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0, 60 charts, 121,055 probes, zero flips, and zero unallowed accepted-abort
  cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed bounded
  reductions remain unchanged.
- Public/wire decision: the public `Predicate::Approximate` variant's stored `paths` field narrows
  from `BTreeSet<String>` to `BTreeSet<ValuesPath>` for direct enum construction. The four public
  constructors deliberately retain their string insertion API, and all diagnostic/dump bytes and
  ordering remain exact. `Predicate` has no serialized wire format.

### Self-adversarial pass

- Approximation markers remain strings because they describe source-expression shapes, not values
  identities. Persistent path storage is typed; only public source insertion, collection, remap,
  and dump boundaries encode or parse.
- `ValuesPath` still has no `Deref`, `AsRef<str>`, or `Display`; the dump must name the explicit
  encoding boundary. Manual ordering therefore continues to prove legacy encoded-string order
  rather than relying on a string compatibility escape hatch.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free.
- `task lint:fc`; exit 0, 48 combinations, 13 packages, three targets; 1,678.38 seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass; 111.710 seconds.
- `task test:integration`; exit 0, 558 pass, 24 skipped; 926.950 seconds.
- `task test:all`; exit 0, 1,870 pass, 24 skipped including live-network tests; 981.896 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,703 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +19 (63,684 to 63,703).

## B4a.3b — migrate atomic guard paths

- Status: landed in `c7a5eb3a` (`refactor(core): type guard paths`).
- Contract: representation-only migration of all value-path payloads in the public `Guard` enum to
  segmented `ValuesPath`, including the `Or.paths` collection and recursively nested `AnyOf`
  alternatives. Literal patterns, keys, members, schema types, and comparison values remain in
  their distinct string/scalar domains. JSON guard bytes and legacy ordering stay exact.
- Acceptance baseline: `c72e7d1e` (B4a.3a).
- Baseline production LOC: 63,703 Rust lines from `task tokei:core` on `c72e7d1e`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, JSON wire, ordering, or corpus acceptance changes.
  - Guard serde emits and accepts the exact legacy escaped strings; `Guard::value_paths` remains an
    explicit encoded-string boundary for unmigrated consumers, while `map_value_paths` applies its
    existing string callback before reconstructing typed paths.
  - Every atomic guard constructor and pattern match migrates compiler-exhaustively. No coercion
    trait, parallel path field, comparison shim, cached encoding, or unrelated string newtype is
    allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results: all 25 single-path `Guard` variants and `Guard::Or.paths` now store segmented
  `ValuesPath`; `AnyOf` remains recursively exhaustive through `Guard`. Custom `ValuesPath` serde
  preserves the exact guard JSON strings, manual `Ord` preserves canonical guard and DNF order,
  and `value_paths`/`map_value_paths` encode only at their existing public string-callback
  boundaries. String ancestry, wildcard, relative-member, and key-concretization readers exposed by
  the compiler now operate on segments. The 432-test focused core+IR suite passes. Schema and
  symbolic-IR dumps are recursively byte-exact; 60 charts and 121,055 probes report zero flips,
  zero mandatory drops, and zero candidate-accepts/Helm-aborts cells.
- Deviations:
  - The first compiler preflight exposed 47 core production sites and then 286 IR production sites,
    including raw comparison, prefix/suffix, wildcard, and ancestry protocols rather than only
    constructors. The full atomic-guard scope was retained: every reader was migrated structurally
    or given an explicit encode at a genuinely still-string carrier. No compatibility comparison
    trait was added.
  - A test-only mechanical preflight rewrote every literal field ending in `path`, which included
    unrelated string-backed DTOs such as `ConditionalGuard` and provider uses. Workspace
    all-target compilation rejected that state; compiler-selected rows were restored and Guard-only
    test construction was migrated explicitly. No archive, dump, fixture, or result from the
    rejected state is adopted.
  - Exhaustive `Guard` path remapping and `Guard`→`ConditionalGuard` conversion crossed the
    repository's 100-line lint threshold after explicit conversion made every variant visible.
    Narrow self-validating `too_many_lines` expectations keep each exhaustive match auditable; the
    underlying needless ownership warning in the remap helper was fixed by borrowing.
  - The final1 archive, dumps, prober, and complete gates were clean, but the final self-adversarial
    search found four typed→encode→parse cycles at already-typed `AbstractValue` and `Splice`
    boundaries. That state was rejected despite byte identity. Direct typed clones replaced the
    cycles; only final2 artifacts and gates are authoritative. Final2 schema and IR dumps are also
    recursively byte-identical to final1, so no artifact or fixture from the rejected state is
    adopted.
- Adjudication evidence: zero flips require no Helm cell adjudication; adjudication ran enabled with
  zero unallowed accepted-abort cells.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Scalar guard variants | Same truth, comparison, pattern, type, and collection semantics | Core guard/predicate suites. |
| `Or` and nested `AnyOf` | Same canonical order, deduplication, and recursive paths | Guard-DNF and serde tests. |
| IR/gen consumers | Same encoded diagnostics, contracts, and schema output | Full workspace, dumps, and prober. |

### Review dossier

- Focused proof: 432/432 core and IR tests pass, including Guard DNF canonicalization, JSON serde,
  predicate normalization, typed condition extraction, wildcard/member scoping, and guard lowering.
  Workspace all-target compilation and all-target/all-feature Clippy pass warning-free.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a3b-final2.tar.zst`; exit 0, 87
  binaries and 125 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a3b-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 182.472 seconds. A recursive byte comparison against the B4a.3a dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3b-final2.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.104 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.3a dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=c72e7d1e
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3b-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3b-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 68.726 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed
  bounded reductions remain unchanged.
- Public/wire decision: the public `Guard` variant fields narrow from `String`/`Vec<String>` to
  `ValuesPath`/`Vec<ValuesPath>`, and `Guard::value_paths` now returns owned encoded strings instead
  of borrowed strings because typed paths have no cached encoding. `map_value_paths` deliberately
  retains its existing string callback as a public rewrite boundary. JSON bytes, variant tags,
  field names, and ordering remain exact. No `Deref`, `AsRef<str>`, `Display`, `From<&str>`, or
  cross-type comparison implementation was added.

### Self-adversarial pass

- Literal regexes, mapping keys, member names, schema-type names, markers, and comparison values
  remain in their distinct domains. Only values identities are newtyped.
- Range ancestry, member-relative paths, range-key concretization, and wildcard detection use
  segments. Compatibility with the legacy literal `*` spelling is intentionally retained until
  B4b introduces `EachMember`; this round makes no literal-star behavior claim.
- `ConditionalGuard` remains string-backed for its separately pre-registered migration. Every
  Guard↔ConditionalGuard seam parses or encodes explicitly, and the byte-exact symbolic dump proves
  the temporary boundary preserves the existing contract.
- Manual encoded-string ordering remains the only `ValuesPath::Ord`; `Guard::Or` canonicalization,
  nested `AnyOf`, DNF sets, serde, and all 18 symbolic fixtures therefore retain exact legacy order.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free; 4 minutes 44 seconds.
- `task lint:fc`; exit 0, 48 combinations, 13 packages, three targets; 1,376.32 seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass; 174.936 seconds execution time.
- `task test:integration`; exit 0, 558 pass, 24 skipped; 1,453.192 seconds.
- `task test:all`; exit 0, 1,870 pass, 24 skipped including live-network tests; 1,535.351 seconds.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,929 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +226 (63,703 to 63,929).

## B4a.3c — migrate conditional guard paths

- Status: landed in `8d75632f` (`refactor(core): type conditional guard paths`).
- Contract: representation-only migration of every value-path payload in `ConditionalGuard` to
  segmented `ValuesPath`, including recursive `Not`, `AllOf`, and `AnyOf` trees. Literal patterns,
  mapping keys, member names, schema types, and scalar comparisons remain in their own domains.
- Acceptance baseline: `c7a5eb3a` (B4a.3b).
- Baseline production LOC: 63,929 Rust lines from `task tokei:core` on `c7a5eb3a`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, or corpus acceptance changes.
  - Guard→conditional conversion stays typed; conversion back to predicates and still-string
    overlay/schema boundaries encode explicitly. Recursive path collection and remapping remain
    exhaustive.
  - No coercion trait, parallel path field, comparison shim, cached encoding, or unrelated string
    newtype is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Every one of the 15 atomic `ConditionalGuard` path fields now carries `ValuesPath`; recursive
    `Not`, `AllOf`, and `AnyOf` nodes retain the same typed leaves and structural ordering.
  - Guard conversion, predicate reconstruction, layered-merge guard collapse, default-source
    projection, presence reasoning, and schema condition encoding now pass typed paths directly.
    Encoding remains explicit only at still-string map, callback, diagnostic, and fixture-prober
    boundaries.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `c7a5eb3a`.
    The full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed bounded
    reductions.
- Deviations:
  - The first lint preflight was rejected: deletion shortened `TryFrom<&Guard>` below its
    `too_many_lines` threshold, making the existing self-validating expectation unfulfilled, while
    typed layered-suffix handling moved `collapse_layered_truthy_gates` five lines above the
    threshold. The stale expectation was deleted and the structural suffix calculation moved to a
    small domain helper; the clean lint rerun passes warning-free.
  - The first immutable-archive command was rejected before producing an archive because its new
    step-local `TMPDIR` did not exist and clang could not create a temporary file. The directory was
    created under `target/` and the unchanged final code state produced the sole authoritative
    archive. No artifact from the failed invocation was used.
  - The existing public `map_value_paths` callback remains string-shaped so callers may perform
    legacy encoded rewrites; the exhaustive walk now performs its encode/parse exactly at that API
    boundary. Changing the callback is deferred until its callers' carriers are typed.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Guard↔conditional conversion | Same exact lowerable guard tree | Core conversion and gen lowering suites. |
| Nested Boolean guards | Same canonical order and recursive paths | Conditional overlay and requirement suites. |
| Schema consumers | Same conditional schemas and diagnostics | Full workspace, dumps, and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,157/1,157 tests across the four
  affected crates pass; the Airflow stress case passes in 174.344 seconds. Whole-workspace Clippy
  then passes warning-free after the rejected lint preflight was repaired.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a3c-final1.tar.zst`; exit 0,
  87 binaries and 125 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a3c-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 181.431 seconds. A recursive byte comparison against the B4a.3b dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3c-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.165 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.3b dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=c7a5eb3a
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a3c-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a3c-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 67.300 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed
  bounded reductions remain unchanged.
- Public/wire decision: the public `ConditionalGuard` variant fields narrow from `String` to
  `ValuesPath`. No serialization implementation exists for this carrier, so there is no wire-format
  change; schema, IR, diagnostics, canonical ordering, and fixture bytes remain exact. No `Deref`,
  `AsRef<str>`, `Display`, `From<&str>`, or cross-type comparison implementation was added.

### Self-adversarial pass

- All 15 path-bearing variants were enumerated directly; literal patterns, mapping keys, member
  names, schema-type names, and comparison values remain string/scalar domains.
- Guard→conditional and conditional→predicate conversion now clone typed paths directly. A
  whole-tree search finds no constructor that encodes a `ValuesPath` merely to populate a
  `ConditionalGuard`, and no typed→encode→parse cycle outside the deliberately string-callback
  `map_value_paths` API.
- Default lookup, ancestor stripping, presence inference, member-key append, merge-layer suffix,
  and schema lowering inspect segments structurally. Encoding is used only where a still-string
  carrier or stable textual key requires it.
- Manual encoded-string `ValuesPath::Ord` remains the only ordering rule. Conditional DNF,
  complementary-guard normalization, nested Boolean sets, and kind partitions therefore retain
  byte-exact legacy order. Literal `*` remains the B4a compatibility spelling until B4b.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free.
- `task lint:fc`; exit 0, all configured feature combinations warning-free.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0, including the complete corpus fixture lane.
- `task test:all`; exit 0, including live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 63,975 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +46 (63,929 to 63,975).

## B4a.4 — migrate capture-kind paths

- Status: landed in `1c118d2e` (`refactor(ir): type capture paths`).
- Contract: representation-only migration of every values-path payload in IR's internal
  `CaptureKind` vocabulary to segmented `ValuesPath`, including path sets, ordered range-selection
  chains, and every singular payload. Schema-type names, patterns, separators, member-kind sets,
  routing, indexes, and quoting styles remain in their own domains.
- Acceptance baseline: `8d75632f` (B4a.3c).
- Baseline production LOC: 63,975 Rust lines from `task tokei:core` on `8d75632f`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, or corpus acceptance changes.
  - Capture producers, `sole_value_path`, dependency rebasing, requirement lowering, and selection
    chains retain exact identities and order; encoded strings appear only at still-string map,
    callback, and diagnostic boundaries.
  - No coercion trait, comparison shim, parallel field, cached encoding, or unrelated string
    newtype is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - Every path-bearing `CaptureKind` payload now carries `ValuesPath`: four path-set variants, 13
    singular-path variants, and both the selected path and ordered candidate chain in
    `RangeSelection`.
  - Capture producers publish typed identities directly when they already own `ValuesPath`, or
    parse once where their still-string carrier has not yet migrated. `sole_value_path` returns the
    typed identity; dependency projection and exact selection compare segments structurally.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `8d75632f`.
    The full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first lint preflight was rejected because typing the payload made two helper parameters'
    owned `String` values unnecessary. Both parameters and their callers were narrowed to `&str`;
    the clean lint rerun passes warning-free.
  - Requirement accumulators, several expression-effect channels, and diagnostic/public maps are
    still string-keyed carriers scheduled later in B4a. `CaptureKind` encodes explicitly when
    crossing those boundaries; it does not retain a parallel encoded field.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Singular captures | Same target requirement and guard scope | IR capture and gen requirement suites. |
| Path-set captures | Same stable deduplication and iteration | Collection/range/string consumer suites. |
| Range-selection chains | Same candidate order and exact selection | Range and fallback-selection suites. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  strict consumers, capture projection, dependency globals, range selection, requirement lowering,
  string routes, and fragment positions. Whole-workspace Clippy passes warning-free after the
  rejected ownership preflight was repaired.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a4-final1.tar.zst`; exit 0,
  87 binaries and 125 files.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a4-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 179.940 seconds. A recursive byte comparison against the B4a.3c dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a4-final1.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.100 seconds and 18
  artifacts are written. A recursive byte comparison against the B4a.3c dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=8d75632f
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a4-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a4-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 66.150 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868 disclosed
  bounded reductions remain unchanged.
- Public/wire decision: `CaptureKind` is crate-private IR state, so the field narrowing creates no
  public API or wire-format obligation. No coercion or cross-type comparison implementation was
  added.

### Self-adversarial pass

- Every path-bearing variant is included in the exhaustive `sole_value_path` and
  `map_value_paths` matches. The latter retains its string callback only at the dependency
  namespacing boundary and performs explicit encode/parse there.
- Schema types, patterns, separators, handled-kind sets, indexes, route tags, quote styles, and
  Boolean flags remain in their distinct domains. Only values identities are newtyped.
- Whole-tree construction searches find no `CaptureKind` path populated by encoding an already
  typed path. Existing string producers parse at their boundary; already-typed abstract and splice
  identities clone directly.
- Manual encoded-string `ValuesPath::Ord` preserves path-set and range-selection chain order.
  Literal `*` remains the B4a compatibility spelling until B4b.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free.
- `task lint:fc`; exit 0, all configured feature combinations warning-free.
- `cargo nextest run --workspace`; exit 0.
- `task test:integration`; exit 0, including the complete corpus fixture lane.
- `task test:all`; exit 0, including live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,029 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +54 (63,975 to 64,029).

## B4a.5a — migrate expression-effect path channels

- Status: landed in `c285745d` (`refactor(ir): type expression effect paths`).
- Contract: representation-only migration of every value-path set and path-keyed map in IR's
  internal `Effects` carrier to segmented `ValuesPath`, plus the path on `MemberHostConversion`.
  Local/root variable names, mutation member keys, schema types, helper identifiers, rendered rows,
  predicates, captures, and other distinct string domains remain unchanged.
- Acceptance baseline: `1c118d2e` (B4a.4).
- Baseline production LOC: 64,029 Rust lines from `task tokei:core` on `1c118d2e`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, or corpus acceptance changes.
  - Every exhaustive `Effects::merge`, `execution_only`, construction, and projection boundary
    retains the same channels and stable encoded order.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string newtype
    is allowed. Existing string consumers encode explicitly until their carrier round.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/Helm-aborts
    allowance remains zero; mandatory base and third-level categories permit zero drops.

- Measured results:
  - All 22 values-path sets and the one path-keyed map on `Effects` now store `ValuesPath`;
    `MemberHostConversion.path` is typed as well. The exhaustive `merge` and `execution_only`
    destructures still name every channel, preserving their previous union/discard decisions.
  - Producers parse only at still-string expression/summary boundaries. Direct consumers compare
    typed identities structurally; `LowerScope` borrows typed effect sets and encodes only when it
    crosses into legacy fragment metadata, rendered rows, diagnostics, or public string results.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `1c118d2e`.
    The full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first lint preflight was rejected by three mechanical consequences of the explicit
    boundaries: `eval_printf` and `absorb_hole_effects` exceeded the 100-line lint, and
    `record_total_conversion_effects` cloned rather than consumed its owned path set. A small
    structural membership helper, one shared typed-path encoder, and consuming the owned set
    removed the duplication without a lint suppression.
  - The second lint preflight was rejected because `eval_printf` remained two lines over the
    limit. Recording typed formatter paths in its existing metadata loop removed the parallel
    traversal; the clean rerun is warning-free.
  - The `final1` archive command was rejected before producing an archive because the new
    step-local `TMPDIR` did not exist, so clang could not create a temporary object. The isolated
    build/dump/prober directories were created explicitly; only the successful `final2` archive
    and artifacts are authoritative.
  - The first final-tree `task test:integration` process was externally terminated at 122/558 when
    its tool session was interrupted. It had reported no failure, but an incomplete gate is a
    failed gate; the exact command was rerun from scratch, and only the completed 558/558 run is
    authoritative.
  - `ObservedFacts`, rendered/helper summary rows, output metadata, fragment interpreter state,
    and contract-signal carriers remain string-keyed until their own B4a rounds. Every crossing is
    explicit; no parallel encoded field or comparison shim was introduced.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Expression transforms | Same per-path facts and selection | Expr-eval and transform suites. |
| Helper transfer/merge | Same union, removal, and execution-only behavior | Helper and effects suites. |
| Fragment consumers | Same slot, range, and capture facts | Fragment/IR corpus and schema dumps. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  transforms, defaults, helper transfer, selection, serialization, fragment positions, range
  paths, and capture production. Whole-workspace Clippy passes warning-free after both rejected
  lint preflights were repaired.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a5a-final2.tar.zst`; exit 0,
  87 binaries and 125 files in 330 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a5a-final2.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 189.788 seconds. A recursive byte comparison against the B4a.4 dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a5a-final2.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.134 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.4 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=1c118d2e
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5a-final2-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a5a-final2.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 66.750 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: `Effects`, `MemberHostConversion`, and `LowerScope` are crate-private IR
  state, so the field narrowing creates no public API or wire-format obligation. Explicit encoding
  preserves every existing public and symbolic-IR byte.

### Self-adversarial pass

- The exhaustive `Effects::merge` and `execution_only` destructures still enumerate all 34 fields;
  compiler-driven construction sweeps cover every non-default `Effects` literal.
- Local/root variable names, mutation keys, schema types, helper names, rendered row paths,
  diagnostics, patterns, and ordinary scalar strings remain in their distinct domains.
- Searches find no typed effect path encoded and immediately reparsed. String callbacks in
  `CaptureKind::map_value_paths` and `RangeModes::map_value_paths` remain deliberate dependency
  rebasing boundaries scheduled outside this carrier.
- `ValuesPath` still supplies no `Deref`, `AsRef<str>`, `Display`, cross-type comparison, or cached
  encoding. Manual legacy-order `Ord` therefore remains the sole ordering rule for every migrated
  set and map.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,265.15 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 2,231.490 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 2,400.176 seconds, including the live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; the release binary is installed at
  `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,285 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +256 (64,029 to 64,285). The increase is explicit parse/encode
  boundary code while adjacent carriers remain string-keyed; no LOC promise governs B4a.

## B4a.5b — migrate observed-fact path carriers

- Status: landed in `4e6c495f` (`refactor(ir): type observed fact paths`).
- Contract: representation-only migration of IR-internal `ObservedFacts` path keys and path sets to
  segmented `ValuesPath`: type-hint map keys, shape-erased paths, and both target/source identities
  on guarded and unguarded values-root overlays. Helper names, schema-type strings, `HintGrade`,
  captures, `RangeModes`, and the public `ValuesDefaultSource` wire carrier remain unchanged.
- Acceptance baseline: `c285745d` (B4a.5a).
- Baseline production LOC: 64,285 Rust lines from `task tokei:core` on `c285745d`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, or corpus acceptance changes.
  - `ObservedFacts::absorb`, `execution_only`, `promote_tested_type_hints`, and `map_value_paths`
    retain every field and the same encoded ordering at string consumers.
  - The public `ValuesDefaultSource` target/source strings are not silently narrowed in this IR
    round; they will migrate with the phase-crossing contract-signal carrier and its Part-F record.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string newtype
    is allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ObservedFacts::type_hints` now maps segmented `ValuesPath` keys, and
    `shape_erased_paths` is a typed set. Both guarded and unguarded values-root overlay facts carry
    typed target/source identities through absorption, activation, remapping, and finalization.
  - Producers publish typed identities at the observed-fact boundary; fragment and contract-signal
    consumers encode only where their still-string carrier or public API requires it. The last
    callers of the legacy `path_is_encoded` and string-keyed `insert_type_hint` helpers disappeared,
    so both helpers were deleted.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `c285745d`.
    The full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The public `ValuesDefaultSource` and its activated wrapper remain string-backed exactly as
    pre-registered. They cross crate and wire boundaries and are scheduled with the remaining
    contract-signal carrier round; no duplicate typed mirror was introduced here.
  - Helper identifiers in `values_root_helper_includes`, schema-type values, grades, capture
    payloads, and range modes remain in their already-correct domains. No rejected code preflight
    or rejected artifact occurred in this round.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Hint producers and promotions | Same grades, types, and path keys | Expr-eval and observed-facts suites. |
| Shape-erasure consumers | Same transform and lowering abstention | Transform, fragment, and schema suites. |
| Values-root overlays | Same guarded target/source projection | Dependency/global and contract suites. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  hint grades and promotion, total conversions, helper transfer, values-root overlays, dependency
  activation, fragment lowering, and contract-signal derivation. Whole-workspace Clippy passes
  warning-free on the first lint preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a5b-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 374 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a5b-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 261.585 seconds. A recursive byte comparison against the B4a.5a dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a5b-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 4.586 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.5a dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=c285745d
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a5b-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a5b-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 106.604 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: all narrowed fields are crate-private IR state. The public
  `ValuesDefaultSource` representation is deliberately unchanged, so this round creates no public
  API or wire-format obligation.

### Self-adversarial pass

- The exhaustive nine-field `ObservedFacts::absorb` and `map_value_paths` destructures retain every
  channel. `execution_only` still clears only type hints, exactly as before.
- Type-hint values remain schema-type strings, and values-root helper includes remain helper names;
  neither domain was accidentally newtyped as a values path.
- Root-overlay activation moves typed identities without an encode/parse cycle. Encoding occurs
  only at final public contract-signal projection; dependency rebasing retains the explicit string
  callback required by the existing phase boundary.
- `ValuesPath` still supplies no coercion or cross-type comparison. Its manual legacy-order `Ord`
  governs every migrated observed-fact set/map, and the byte-exact dumps prove stable ordering.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,003.70 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 192.921 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,649.437 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,654.107 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 24.47
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,298 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +13 (64,285 to 64,298). The two obsolete string helpers were
  deleted; the net increase is explicit encoding at consumers whose carrier round is still due.

## B4a.6 — migrate contract-use path carriers

- Status: landed in `52166b67` (`refactor(core): type contract use paths`).
- Contract: representation-only migration of the public phase-crossing `ContractUse.source_expr`
  identity and every `MergeLayersUse.layers` identity to segmented `ValuesPath`. YAML paths,
  resource references, literal member keys, split separators, source provenance, transform tags,
  and Boolean row facts remain unchanged.
- Acceptance baseline: `4e6c495f` (B4a.5b).
- Baseline production LOC: 64,298 Rust lines from `task tokei:core` on `4e6c495f`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or serialized-contract byte
    changes. `ValuesPath` custom serde must preserve the existing JSON string representation.
  - Construction, canonicalization, normalization, global projection, dependency rebasing,
    merge-layer shadowing, and every public/session consumer retain exact identities and order.
  - Part F decision: this deliberately narrows two public Rust field types and
    `MergeLayersUse::shadowed_by`; it preserves wire bytes and is accepted as the scheduled B4a
    compiler-enforced path-carrier migration, not an accidental API side effect.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string newtype
    is allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ContractUse.source_expr` and `MergeLayersUse.layers` now carry segmented `ValuesPath` through
    fragment projection, normalization, dependency/global remapping, string-requirement routing,
    ordered merge shadowing, session explanations, and generator overlay lowering.
  - All three public `ContractUse` constructors take `ValuesPath`; `MergeLayersUse::shadowed_by`
    returns typed paths. The migration deleted every production `source_expr.as_str`, raw path
    split, and cross-type comparison instead of retaining an owned-string compatibility facade.
  - `ValuesPath` custom serde preserves the legacy string wire shape. The authoritative schema and
    symbolic-IR dumps are recursively byte-identical to `4e6c495f`; the full-depth battery checks
    121,055 probes across 60 charts with zero acceptance flips, zero mandatory base drops, zero
    third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - Five lint preflights were rejected in sequence as the public constructor seam was made honest:
    owned `String` survived unnecessarily first in `with_condition_and_provenances`, then
    `with_provenances`, then `new`, and finally in IR's `placed_row`. Each signature was narrowed
    to `ValuesPath`, and producers now parse or clone at their real boundary; no lint suppression
    or string adapter was added.
  - The next lint preflight was rejected because explicit path construction pushed one integration
    test two lines over the configured function limit. A small `has_source` test helper replaces
    five repeated comparisons; the clean lint rerun passes warning-free.
  - The compiler-enforced public test migration touched direct contract constructors across core,
    IR, generator, engine, and integration tests. No archive/dump artifact was produced before that
    migration and the clean lint state, so only `final1` artifacts are authoritative.
  - The first final-tree `cargo nextest run --workspace` attempt was externally interrupted at
    approximately 544/1,308 tests. Its process exited and no result from that incomplete run was
    adopted; the gate was restarted from scratch and the complete rerun below is authoritative.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Direct and helper contract rows | Same path identity and serialized bytes | IR/public-surface suites and dumps. |
| Dependency/global rebasing | Same mapped prefixes and guards | Contract projection suites. |
| Ordered merge layers | Same precedence, transforms, and shadowing | Merge-shadowing suites and corpus. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,049/1,049 core/IR/gen tests pass,
  covering public serde, contract normalization, helper/direct projection, dependency globals,
  merge-layer precedence, requirement routing, and generator lowering. Whole-workspace Clippy
  passes warning-free after the five rejected ownership preflights and one test-size preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a6-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 535 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a6-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 185.830 seconds. A recursive byte comparison against the B4a.5b dump
  exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-ir SYMBOLIC_DUMP=1
  IR_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a6-final1.tar.zst --profile
  integration -E 'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.233 seconds and 18
  artifacts are written. A recursive byte comparison against the B4a.5b dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=4e6c495f
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a6-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a6-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 72.397 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: `ContractUse.source_expr`, `MergeLayersUse.layers`, all three public
  `ContractUse` constructors, and `MergeLayersUse::shadowed_by` deliberately narrow from strings to
  `ValuesPath`. This is the frozen B4a public API migration; serde remains byte-compatible and no
  wire-format version is required.

### Self-adversarial pass

- Exhaustive `ContractUse::map_value_paths` still names every field and structurally remaps the
  source identity, every merge layer, condition guards, and omitted-member retain guards.
- Whole-tree searches find no production `source_expr.as_str`, raw source-expression split/prefix
  operation, `source_expr: String`, or string-backed `MergeLayersUse.layers`; every remaining
  encoding is at a string map, public signal, diagnostic, or callback boundary.
- Split separators, YAML paths, resource names, helper provenance, member keys, and schema types
  remain in their own domains. Literal `*` keeps its legacy segment spelling until B4b.
- No `From<String>`, `From<&str>`, deref, display, cross-type equality, cached encoding, or parallel
  field was added to make old call sites compile.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 4 minutes 57 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,476.10 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 324.675 seconds on the complete
  authoritative rerun.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 2,231.599 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 2,082.921 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 42.90
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,334 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +36 (64,298 to 64,334), entirely explicit typed construction and
  still-string phase-boundary encoding; no LOC promise governs B4a.

## B4a.7 — migrate contract-schema signal path indexes

- Status: landed in `54a261a9` (`refactor(core): type contract signal paths`).
- Contract: representation-only migration of `ContractSchemaSignals` path-indexed maps and sets to
  segmented `ValuesPath`, deleting the duplicate `ContractPathSchemaEvidence.value_path` identity.
  Schema evidence, path order, provider overlays, requiredness, omission, range, diagnostic, and
  wire behavior remain unchanged.
- Acceptance baseline: `52166b67` (B4a.6).
- Baseline production LOC: 64,334 Rust lines from `task tokei:core` on `52166b67`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
    The map key becomes the sole path identity; every consumer receives that key explicitly where
    it previously read the duplicated evidence field.
  - Evidence construction, referenced/pruned/omitted/direct-range derivation, root-overlay twin
    projection, path resolution, emission planning, provider synthesis, and session explanations
    retain exact identities and legacy encoded-string order.
  - Part F decision: public `ContractSchemaSignals` accessors and constructor deliberately narrow
    from string-keyed collections and string lookup to `ValuesPath`. This is the scheduled B4a API
    migration; no wire-format field changes.
  - No coercion trait, cross-type comparison, parallel encoded key, or cached string identity is
    allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ContractSchemaSignals` now owns one `ValuesPath` identity for every evidence entry and every
    referenced, pruned-parent, unconditionally-omitted, direct-range, and wrapper-exclusion set.
    `ContractPathSchemaEvidence.value_path` is deleted; consumers receive the enclosing key.
  - Direct generator consumers retain the typed identity through `ValuesYamlPathInfo`,
    `ResolvedPathSchema`, synthesized provider implications, conditional conjunct carriers, and
    conditional-target indexes. Encoding remains explicit only at diagnostics, test presentation,
    or still-string neighboring carriers scheduled for later B4a rounds.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `52166b67`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The compiler showed that the pre-rewrite program-wrapper exclusion snapshot was another path
    set feeding `ContractSchemaSignals`; leaving it string-backed would have reintroduced a
    parallel identity at finalization. It moved into this round together with its interpreter and
    helper-summary carriers.
  - The generator could not consume the typed signal key honestly while `ResolvedPathSchema`,
    synthesized implication maps, and conditional conjunct/target indexes copied it back to
    strings. Those directly dependent internal carriers moved in the same compiler-driven round;
    wrapper scopes/default sources and `ProviderSchemaUse.value_path` remain separate scheduled
    carrier seams rather than being pulled in opportunistically.
  - The first whole-workspace lint preflight was rejected because
    `ContractPathAccumulator::into_schema_evidence` still accepted an owned encoded `String` that
    it only compared. The seam now borrows the canonical `ValuesPath`; the clean lint rerun passes
    warning-free. No archive or dump was produced from the rejected state.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Builder evidence aggregation | One typed key identity, same merged evidence | Core/IR suites and IR dump. |
| Overlay twin and omission derivation | Same structural descendants and member paths | Contract suites and corpus. |
| Resolver/emission consumers | Same ordered resolutions and schema bytes | Generator suites and schema dump. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,049/1,049 core/IR/gen tests pass,
  covering evidence aggregation, root-overlay projection, values-default pruning, required-source
  synthesis, conditional lowering, wrapper exclusion, provider resolution, and public queries.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a7-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 362 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a7-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 186.019 seconds and 84 artifacts are written. A recursive byte
  comparison against the B4a.6 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a7-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.219 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.6 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=52166b67
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a7-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a7-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 69.826 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: `ContractSchemaSignals::new`, its path-set/map accessors, and
  `evidence_for` deliberately narrow to `ValuesPath`; the duplicate evidence field is deleted.
  No serialized contract field changes, so no wire-format version is required.

### Self-adversarial pass

- Whole-tree searches find no string-keyed `ContractSchemaSignals` evidence/set carrier, no
  `ContractPathSchemaEvidence.value_path`, and no string-backed pre-rewrite wrapper-exclusion set.
- Manual legacy-order `ValuesPath::Ord` governs every migrated map/set and schema/IR byte equality
  proves stable iteration. Dotted/backslash literal-key identity remains structural; literal `*`
  keeps its legacy segment spelling until B4b.
- Provider use paths, wrapper scope strings, default-source paths, YAML paths, resource names,
  schema type names, and helper identifiers remain in their separate domains. No coercion trait,
  cross-type comparison, cached encoding, or parallel signal key was introduced.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 5 minutes 11 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,545.48 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 190.086 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,521.942 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,623.850 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 26.93
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,371 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +37 (64,334 to 64,371), from explicit typed lookups and direct
  structural segment iteration after deleting the duplicate evidence identity; no LOC promise
  governs B4a.

## B4a.8 — migrate provider-use source paths

- Status: landed in `6509de14` (`refactor(core): type provider use paths`).
- Contract: representation-only migration of the public phase-crossing
  `ProviderSchemaUse.value_path` carrier to segmented `ValuesPath`. Provider resource identity,
  YAML slot path, transforms, omission guards, merge layering, lookup policy, and diagnostics
  remain unchanged.
- Acceptance baseline: `54a261a9` (B4a.7).
- Baseline production LOC: 64,371 Rust lines from `task tokei:core` on `54a261a9`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
    The existing provider-use source identity and ordering survive through builder aggregation,
    provider lookup cache keys, conditional overlays, and generator synthesis.
  - Custom `ValuesPath` serde preserves any serialized string field exactly; no provider-schema
    query, path descent, or use-dedup behavior changes.
  - Part F decision: the public Rust field deliberately narrows from `String` to `ValuesPath` as
    the scheduled B4a carrier migration. Wire bytes remain unchanged.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string newtype
    is allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ProviderSchemaUse.value_path` now carries `ValuesPath` from the contract-use producer through
    conditional overlays, provider lookup/dedup, requirement synthesis, and K8s provider calls.
    The old producer-side encode is deleted.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `54a261a9`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations: none. Whole-workspace compilation and lint passed on the first completed preflight;
  only direct struct fixtures required explicit typed construction, and no rejected artifact was
  produced.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Direct/helper provider rows | Same source and slot identity | IR suites and dump. |
| Conditional/provider overlays | Same guarded provider uses | Contract/generator suites. |
| Lookup and requirement synthesis | Same cache keys and schema bytes | Provider suites and schema dump. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,122/1,122 core/IR/gen/K8s tests
  pass, covering the producer, conditional overlays, provider lookup plans/cache keys, API-version
  inference, required-source synthesis, and K8s chain behavior. Whole-workspace Clippy passes
  warning-free on the first completed lint preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a8-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 426 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a8-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 184.115 seconds and 84 artifacts are written. A recursive byte
  comparison against the B4a.7 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a8-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.178 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.7 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=54a261a9
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a8-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a8-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 68.755 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: `ProviderSchemaUse.value_path` deliberately narrows to `ValuesPath`.
  Custom serde preserves the string wire, so no wire-format version is required.

### Self-adversarial pass

- Whole-tree construction search finds one production owner: the conditional-overlay builder now
  clones the typed `ContractUse.source_expr`; no production encode/parse cycle remains for this
  field.
- Provider slot `YamlPath`, resource identity, schema types, literal member keys, split separators,
  and transforms remain in their separate domains. No coercion trait, cross-type comparison,
  cached encoding, or parallel provider-use source field was added.
- Manual `ValuesPath::Ord` and serde govern the field's ordering/wire representation; byte-exact
  schema and IR dumps prove both remain stable.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 5 minutes 14 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,522.02 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 185.713 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,519.951 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,606.047 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 25.83
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,371 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: 0 (64,371 to 64,371); the typed field and producer clone replace
  the string field and encode one-for-one.

## B4a.9 — migrate values-default source paths

- Status: landed in `3538aa36`.
- Contract: representation-only migration of both public `ValuesDefaultSource` path fields to
  segmented `ValuesPath`, including guarded sources and their IR-to-generator route. Default merge
  direction, activation guards, null deletion, source grouping, and schema composition remain
  unchanged.
- Acceptance baseline: `6509de14` (B4a.8).
- Baseline production LOC: 64,371 Rust lines from `task tokei:core` on `6509de14`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
    Target/source identities retain legacy ordering through observed facts, finalization, values
    composition, conditional lowering, and terminal-clause evaluation.
  - Custom `ValuesPath` serde preserves serialized source objects exactly. Empty target paths keep
    denoting the values root; no merge precedence or activation behavior changes.
  - Part F decision: the two public Rust fields deliberately narrow from `String` to `ValuesPath`
    as the scheduled B4a carrier migration. Wire bytes remain unchanged.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string newtype
    is allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - Both `ValuesDefaultSource` identities now remain typed from root-mutation discovery through
    observed-fact remapping, activation finalization, generator composition, and conditional
    source-path projection. The last string-only YAML values lookup helper is deleted.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `6509de14`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations: none. Compilation and lint passed on the first completed preflight; the
  compiler-driven test migration touched only direct source fixtures and no rejected artifact was
  produced.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Root and nested default sources | Same target/source direction | IR and values-yaml suites. |
| Activation-guarded sources | Same guard grouping and branch scope | Contract/conditional suites. |
| Generator composition | Same merged defaults and schema bytes | Generator/corpus suites and dump. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,049/1,049 core/IR/gen tests pass,
  covering root mutation, path remapping, guarded source finalization, YAML composition, and
  conditional source projection. Whole-workspace Clippy passes warning-free on the first
  completed lint preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a9-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 302 seconds.
- Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-schema
  SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-b4a9-final1.tar.zst --profile
  integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
  test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
  exit 0, 62 tests pass in 186.005 seconds and 84 artifacts are written. A recursive byte
  comparison against the B4a.8 dump exits 0.
- Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-ir
  SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a9-final1.tar.zst --profile integration -E
  'test(ir_corpus_fixtures_match)'`; exit 0, one test passes in 3.227 seconds and 18 artifacts are
  written. A recursive byte comparison against the B4a.8 dump exits 0.
- Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-prober
  SCHEMA_ACCEPTANCE_BASELINE_REF=6509de14
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-schema
  SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a9-final1-coverage.json
  ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
  /private/tmp/arch-v4-b4a9-final1.tar.zst --profile integration -E
  'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
  ignored-only`; exit 0 in 68.604 seconds, 60 charts, 121,055 probes, zero flips, and zero unallowed
  accepted-abort cells. Mandatory base and third-level categories have zero drops; 28,868
  disclosed bounded reductions remain unchanged.
- Public/wire decision: `ValuesDefaultSource.target_path` and `source_path` deliberately narrow to
  `ValuesPath`; custom serde preserves their string wire fields, so no wire version is required.

### Self-adversarial pass

- Whole-tree construction and access searches find no string-backed default-source path field or
  raw split/parse at generator consumers. Empty `ValuesPath` retains the values-root identity.
- Merge direction, activation guards, YAML member keys, and source grouping remain distinct and
  unchanged. No coercion trait, cached encoding, or parallel source-path field was added.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 6 minutes 25 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,524.39 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 186.191 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,534.620 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,634.630 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 25.74
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,378 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +7 (64,371 to 64,378), from explicit structural segment
  iteration after deleting the last string-only YAML lookup helper.

## B4a.10 — migrate values-program wrapper scopes

- Status: landed in `70cf0fed`.
- Contract: representation-only migration of public `ValuesProgramWrapper.scope_path` to
  segmented `ValuesPath`, preserving sentinel keys, spread/replace policy, scope ordering, wrapper
  exclusions, and schema alternatives exactly.
- Acceptance baseline: `3538aa36` (B4a.9).
- Baseline production LOC: 64,378 Rust lines from `task tokei:core` on `3538aa36`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Empty scope remains the values root; nested scope descent and wrapper exclusion identity stay
    structural and preserve legacy ordering/wire strings.
  - Part F decision: the public Rust field deliberately narrows to `ValuesPath`; wire bytes remain
    unchanged. No coercion, cross-type comparison, or parallel encoded field is allowed.
  - Any fixture or acceptance flip stops the round before adoption; candidate-accepts/Helm-aborts
    allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ValuesProgramWrapper.scope_path` now remains typed through helper discovery, contract
    remapping, finalization, generator scope grouping, and structural property descent.
  - Schema and IR dumps are recursively byte-identical to `3538aa36`; 121,055 probes across 60
    charts report zero flips, zero mandatory drops, and 28,868 unchanged disclosed reductions.
- Deviations: none. Compilation and lint passed on the first completed preflight; no rejected
  artifact was produced.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 reports zero
  candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Helper-discovered root wrappers | Same sentinel and spread policy | IR suites and dump. |
| Scoped wrapper remapping | Same structural scope | Contract suites. |
| Generator alternatives/exclusions | Same schema bytes | Generator/corpus suites and dump. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 1,049/1,049 core/IR/gen tests pass.
  Whole-workspace Clippy passes warning-free on the first completed lint preflight.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a10-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a10-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 404 seconds.
- Clean schema dump: the B4a.10 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 183.912 seconds and 84 artifacts are byte-identical to B4a.9.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.174 seconds and 18 artifacts are byte-identical to B4a.9.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `3538aa36`,
  Helm adjudication enabled; exit 0 in 68.417 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: `ValuesProgramWrapper.scope_path` deliberately narrows to `ValuesPath`;
  custom serde preserves its string wire field, so no wire version is required.

### Self-adversarial pass

- Root scope is the empty typed path; nested scope descent iterates literal segments directly.
  Sentinel keys and spread/replace policy remain separate domains.
- No string-backed wrapper scope, raw split, coercion trait, cached encoding, or parallel scope
  field remains.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 5 minutes 31 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,550.18 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 185.827 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,519.600 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,608.415 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 26.52
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,378 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: 0 (64,378 to 64,378).

## B4a.11 — migrate pathless-read identities

- Status: landed in `7953733d`.
- Contract: representation-only migration of `ValueRead.values_path` to segmented `ValuesPath`,
  including direct reads, helper-demoted reads, sibling-condition pruning, nested-read absorption,
  graph remapping, and contract-row lowering. Read kind, condition, resource scope, dependency
  lane, provenance, ordering, and emitted wire strings remain unchanged.
- Acceptance baseline: `70cf0fed` (B4a.10).
- Baseline production LOC: 64,378 Rust lines from `task tokei:core` on `70cf0fed`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Direct and helper reads retain identical path identity and guards through deduplication,
    pruning, remapping, and final contract-row absorption.
  - `ValueRead` is crate-private, so this round changes no public API or wire format. Explicit
    encoding is permitted only where an existing diagnostic or serialized row requires text.
  - No coercion trait, cross-type comparison, cached encoded twin, or unrelated string carrier is
    allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - `ValueRead.values_path` now remains typed from direct and widened-read construction through
    helper absorption, deduplication, sibling-condition pruning, document projection, graph
    remapping, and contract-row lowering.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `70cf0fed`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations: none. Compiler-driven construction and comparison updates compiled on the first
  completed implementation preflight; no rejected code state produced an artifact. Existing
  string sibling-claim sets remain scheduled B4a carriers, so this round encodes a typed read only
  at those existing boundaries instead of widening its scope into their producer graph.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Direct/control pathless reads | Same path, kind, site, and guards | IR focused suite and dumps. |
| Helper-demoted/nested reads | Same dependency lane and provenance | Helper/fragment suites. |
| Pruning and contract projection | Same sibling scope and final rows | IR/schema dumps and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  direct reads, helper summaries, sibling pruning, fragment projection, graph remapping, and
  contract lowering. Whole-workspace Clippy passes warning-free on the first completed lint
  preflight in 5 minutes 18 seconds.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a11-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a11-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 362 seconds.
- Clean schema dump: the B4a.11 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 183.524 seconds and 84 artifacts are byte-identical to B4a.10.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.186 seconds and 18 artifacts are byte-identical to B4a.10.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `70cf0fed`,
  Helm adjudication enabled; exit 0 in 69.114 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. `ValueRead` is crate-private and its dump boundary explicitly emits
  the unchanged escaped-dot spelling.

### Self-adversarial pass

- Whole-tree construction and use searches find no string-backed `ValueRead` identity or
  parse-on-contract-projection cycle. Typed equality now handles self-guard pruning directly, and
  strict-descendant suppression uses segmented path comparison.
- Read kind, resource scope, dependency lane, provenance, guard DNF, rendered-row paths, and
  sibling-claim sets remain separate domains. No coercion trait or parallel encoded read field was
  added.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 5 minutes 2 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,137.86 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 185.932 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,523.473 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,622.656 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 22.46
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,382 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +4 (64,378 to 64,382), from explicit typed-path construction and
  structural descendant checks replacing encoded-string comparisons.

## B4a.12 — migrate range-subject paths

- Status: landed in `826b7334`.
- Contract: representation-only migration of `RangeSubjectIdentity.path`,
  `RangeSubject.influence_paths`, and the already-typed `RangeModes` map's string-only query/update
  interface to segmented `ValuesPath`, preserving range input/member identity, JSON-decoded
  provenance, truth reachability, member-value projection, and every published `RangeMode` exactly.
- Acceptance baseline: `7953733d` (B4a.11).
- Baseline production LOC: 64,382 Rust lines from `task tokei:core` on `7953733d`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Direct, JSON-decoded, helper-output, merged-layer, and collection-member range identities
    retain identical paths and mode flags; callers no longer parse keys already stored in the
    typed `RangeModes` map.
  - Both types are crate-private, so this round changes no public API or wire format. Encoded text
    remains confined to existing string-backed producers or diagnostics.
  - No coercion trait, cross-type comparison, parallel encoded field, or unrelated string carrier
    is allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - Range subject influence and input/member identities now remain segmented through expression
    evaluation, document and inline range lowering, active capture state, `RangeModes` publication,
    global dependency projection, and contract requirement queries. The typed `RangeModes` map no
    longer reparses keys in its query/update interface.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `7953733d`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first compiler-only preflight typed the two range-subject fields alone and was rejected
    after 35 expected mismatches showed that the adjacent `RangeModes` interface and active-range
    state still forced encoded strings. No archive or dump was produced from that state.
  - The second compiler-only preflight typed that interface and was rejected with 26 production
    and seven test callsites still passing encoded paths. The third preflight was production-clean
    but rejected until the seven private range-mode tests used typed constructors. Neither state
    produced an artifact. These failures refined the pre-registered round boundary; they did not
    expose behavioral drift.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Direct/decoded/helper-output range subjects | Same identity and reachability | IR focused suite. |
| Document and inline range lowering | Same reads, guards, member/key bindings | Fragment suites/dump. |
| Range-mode capture and global projection | Same per-path flags and remapping | Range/contract suites. |
| Requirement synthesis queries | Same member/key obligations | Schema dump and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  range subject evaluation, direct and derived iteration, decoded members, active captures,
  global projection, and range-conditioned requirement synthesis. Whole-workspace Clippy passes
  warning-free on the first completed lint preflight in 6 minutes 14 seconds.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a12-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a12-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 447 seconds.
- Clean schema dump: the B4a.12 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 184.284 seconds and 84 artifacts are byte-identical to B4a.11.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.184 seconds and 18 artifacts are byte-identical to B4a.11.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `7953733d`,
  Helm adjudication enabled; exit 0 in 68.928 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. All migrated fields and the `RangeModes` interface are crate-private;
  existing dump and contract boundaries retain the unchanged escaped-dot spelling.

### Self-adversarial pass

- Whole-tree API searches find `RangeModes` accepts only `ValuesPath`; its map keys, active range
  state, range-subject identities, and influence set now share one path currency. Exact encoded
  ordering remains governed by `ValuesPath::Ord`.
- `AbstractValue::RangeKey` and helper/contract-builder path sets remain explicitly scheduled B4a
  carriers, so their current encodes are visible boundaries rather than a hidden parallel field.
  JSON-decoded identity, member-vs-input mode, truth reachability, and wildcard semantics remain
  distinct domains. No coercion trait was added.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, whole workspace warning-free in 4 minutes 59 seconds.
- `task lint:fc`; exit 0, 48 feature combinations for 13 packages across three targets in
  1,132.24 seconds, with zero warnings and zero errors.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 194.085 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,544.638 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,650.956 seconds, including live
  network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; release build completes in 23.65
  seconds and installs `/Users/roman/.cargo/bin/helm-schema`.
- Downstream luup2 gate with the recorded shim and binary override; exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,403 production Rust LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +21 (64,382 to 64,403), from explicit conversions at remaining
  string-backed producer/contract-builder boundaries and direct typed map access elsewhere.

## B4a.13 — migrate abstract-value path identities

- Status: landed in `4c9e321c` (`refactor(ir): type abstract value paths`).
- Contract: representation-only migration of the identity-bearing `AbstractValue` variants
  `JsonDecodedPath`, `RangeKey`, `KeysList`, and `OutputPath` to segmented `ValuesPath`, including
  selection, descent, join, helper-output metadata, range/member recovery, serialization preimage,
  and fragment projection routes. Literal string sets and dictionary keys remain unrelated domains.
- Acceptance baseline: `826b7334` (B4a.12).
- Baseline production LOC: 64,403 Rust lines from `task tokei:core` on `826b7334`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Raw, JSON-decoded, ranged-key, keys-list, and helper-output identities retain identical
    selection and projection behavior, including escaped dot/backslash ordering.
  - All variants are crate-private, so this round changes no public API or wire format. Explicit
    encoding remains confined to still-string-backed metadata, diagnostics, or string producers.
  - No coercion trait, cross-type comparison, cached encoded twin, or unrelated string newtype is
    allowed. Any fixture or acceptance flip stops the round before adoption; candidate-accepts/
    Helm-aborts allowance and mandatory coverage drops remain zero.

- Measured results:
  - JSON-decoded, range-key, keys-list, and helper-output identities now remain segmented through
    abstract-value selection/descent, helper metadata, range projection, serialization preimages,
    fragment lowering, predicate decoding, and exact input-identity recovery.
  - Identical raw/decoded/output path operations collapse onto shared match arms, removing 66
    production LOC while keeping transform metadata and decoded-vs-raw semantics distinct.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `826b7334`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first compiler-only preflight changed the four enum payloads and was rejected with 134
    production and 151 all-target mismatches. The second was rejected with 93 production and 110
    all-target mismatches after the enum-owned operations were made structural. No archive or dump
    was produced from either state.
  - The first whole-workspace lint preflight was rejected on 14 mechanical findings: 13 newly
    identical match arms and one `too_many_lines` threshold crossed by explicit path construction.
    The arms were merged and a direct `output_path` constructor extracted; no suppression or
    artifact from the rejected state was retained.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Raw/decoded identity and member descent | Same path selection and joins | Abstract-value/IR suites. |
| Range keys and `keys` lists | Same key-domain/member projection | Range/collection suites. |
| Helper output metadata | Same predicates, transforms, provenance | Helper/fragment suites. |
| Serialization and parser preimages | Same decoded identity | Eval/serialization suites. |
| Fragment/predicate lowering | Same reads, guards, and rows | IR/schema dumps and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds and 393/393 IR tests pass, covering
  abstract-value algebra, JSON/YAML roundtrips, range keys, helper output metadata, fragment
  projection, condition decoding, and strict operands. Whole-workspace Clippy passes warning-free
  on the first completed lint preflight in 4 minutes 10 seconds.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a13-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a13-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 455 seconds.
- Clean schema dump: the B4a.13 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 184.994 seconds and 84 artifacts are byte-identical to B4a.12.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.268 seconds and 18 artifacts are byte-identical to B4a.12.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `826b7334`,
  Helm adjudication enabled; exit 0 in 68.513 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. All migrated enum variants are crate-private; encoded strings remain
  explicit at existing metadata, diagnostic, and dump boundaries.

### Self-adversarial pass

- Whole-tree variant searches find all four identity-bearing payloads use `ValuesPath`; structural
  descendant/member operations no longer split their strings. `StringSet`, dictionary/member keys,
  separators, and literal values remain untyped strings in their separate domains.
- Raw versus JSON-decoded identity still uses distinct variants, and helper output transforms stay
  in `HelperOutputMeta`; the shared carrier removes only path representation duplication. No
  coercion trait, display implementation, or encoded twin was added.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, complete workspace Clippy pass in 5 minutes 43 seconds.
- `task lint:fc`; exit 0, 48 combinations across 13 packages and 3 targets in 1,415.17
  seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 196.272 seconds.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,820.833 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,805.769 seconds, including
  all live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; the final release binary replaces
  `/Users/roman/.cargo/bin/helm-schema` after a 23.63-second build.
- Downstream luup2 gate with the documented macOS `xargs`/`flock` shims and installed binary;
  exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,337 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -66 (64,403 to 64,337), from merged structural path operations
  and deletion of repeated encode/parse branches.

## B4a.14 — migrate abstract-value influence paths

- Status: landed in `a65493bb` (`refactor(ir): type abstract influence paths`).
- Contract: representation-only migration of `AbstractValue` influence-path sets
  (`DerivedBoolean`, `SplitList`, `SplitSegment`, and `Widened`) and its identity/influence path
  accessors to segmented `ValuesPath`. Literal string sets, dictionary keys, separators, helper
  identifiers, and still-string phase boundaries remain separate domains.
- Acceptance baseline: `4c9e321c` (B4a.13).
- Baseline production LOC: 64,337 Rust lines from `task tokei:core` on `4c9e321c`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Derived Boolean, split source, widened, direct identity, ranged-key, and collected influence
    paths retain the same membership and legacy encoded ordering, including dot/backslash cases.
  - All affected carriers are crate-private, so this round changes no public API or wire format.
    Existing string consumers must encode explicitly at their boundary; no coercion trait,
    cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - `DerivedBoolean`, `SplitList`, `SplitSegment`, and `Widened` now retain segmented influence
    paths. `TaintPart` and `Opaque`, the fragment carriers fed directly by those variants, use the
    same representation instead of immediately restoring string-shaped twins.
  - Direct/input identity, unique, collected, ranged-key, shallow fragment-source, and full
    fragment-rendered accessors return `ValuesPath`. Descendant, item-parent, root, membership,
    and stable-order operations stay structural; encoding is explicit only at remaining metadata,
    diagnostic, predicate-constructor, or dump boundaries.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `4c9e321c`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The initial compiler-only carrier change was rejected with 101 production and 107 all-target
    mismatches. After the production migration compiled, the first all-target preflight exposed 14
    test/dump construction mismatches. Both inventories were resolved explicitly; no archive or
    dump was produced from either state.
  - The first whole-workspace lint preflight was rejected on two mechanical findings: a root-path
    emptiness guard eligible for `?`, and `eval_printf` crossing the 100-line limit because two
    identical string-to-path boundary decodes were expanded inline. The guard now uses `?`, and a
    narrowly named consuming decoder removes the duplication without a suppression.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Derived Boolean influences | Same guard and comparison attribution | IR/unit suite. |
| Split list/segment sources | Same cardinality and segment constraints | Serialization/split suites. |
| Widened call influences | Same output projection and fallback behavior | Helper/fragment suites. |
| Direct/range/collected identities | Same descent, range, and selection behavior | Abstract-value/IR suites. |
| String-facing phase boundaries | Same encoded paths and stable order | Fixture dumps and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds in 5 minutes 2 seconds, and 393/393 IR
  tests pass, covering influence algebra, split provenance, widened helpers, fragment fanout,
  projection, summaries, predicates, traversal, and stable dump rendering. The corrected
  whole-workspace lint preflight passes warning-free in 5 minutes 7 seconds.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a14-final1-build cargo
  nextest archive --workspace --archive-file /private/tmp/arch-v4-b4a14-final1.tar.zst`; exit 0,
  87 binaries and 125 files in 7 minutes 32 seconds.
- Clean schema dump: the B4a.14 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 184.594 seconds and 84 artifacts are byte-identical to B4a.13.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.960 seconds and 18 artifacts are byte-identical to B4a.13.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `4c9e321c`,
  Helm adjudication enabled; exit 0 in 68.890 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. All migrated variants, accessors, and fragment carriers are
  crate-private; encoded dump and contract bytes remain identical.

### Self-adversarial pass

- Whole-tree carrier searches find no string-backed influence set on the four variants, and no
  string-backed `TaintPart`/`Opaque` path set. All named `AbstractValue` identity/influence
  accessors return `ValuesPath`; literal string sets, dictionary keys, separators, helper names,
  and file paths remain strings in their distinct domains.
- Dot/backslash path identity never routes through a new textual split or concatenation. The only
  new decoder consumes a still-string boundary set, and there is no `Deref`, `AsRef<str>`,
  `Display`, cross-type comparison, or parallel encoded field.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, complete workspace Clippy pass in 6 minutes 17 seconds.
- `task lint:fc`; exit 0, 48 combinations across 13 packages and 3 targets in 1,278.62
  seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 184.922 seconds after the
  final build.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,523.107 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,607.205 seconds, including all
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; the final release binary replaces
  `/Users/roman/.cargo/bin/helm-schema` after a 22.22-second build.
- Downstream luup2 gate with the documented macOS `xargs`/`flock` shims and installed binary;
  exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,301 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: -36 (64,337 to 64,301), from collapsing repeated typed
  set conversions and descendant/root checks onto the carrier.

## B4a.15 — migrate helper-output metadata path indexes

- Status: landed in `debd89e8` (`refactor(ir): type helper metadata paths`).
- Contract: representation-only migration of every IR per-value-path `HelperOutputMeta` map key and
  `suppress_predicate_paths` set to segmented `ValuesPath`, including effects, evaluation
  environments, abstract-value metadata projections, symbolic local state, fragment summaries,
  helper contexts, and condition decoding. Helper/local names and literal member keys remain
  separate string domains.
- Acceptance baseline: `a65493bb` (B4a.14).
- Baseline production LOC: 64,301 Rust lines from `task tokei:core` on `a65493bb`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Helper metadata keeps identical branch predicates, transforms, suppressions, defaults,
    serialization flags, merge layers, and stable legacy encoded ordering at every join.
  - All affected carriers are crate-private, so this round changes no public API or wire format.
    Encoding remains explicit only at string-facing diagnostics/dumps or unrelated map domains;
    no coercion trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - Every IR per-value-path `HelperOutputMeta` index now uses `ValuesPath`: expression effects,
    evaluation environments, abstract-value projections, symbolic local state and branch joins,
    helper contexts, type-descriptor sources, fragment summaries, rendered rows, and lowering.
    Predicate-suppression paths use the same carrier.
  - `RenderedRow.path`, selection-chain identities, merge-layer identities, and lowering's splice
    lookup accept structural paths directly, eliminating the encode/decode joins that existed only
    to address string-keyed metadata maps. Helper names and literal member keys remain strings.
  - The authoritative schema and symbolic-IR dumps are recursively byte-identical to `a65493bb`;
    the full-depth battery checks 121,055 probes across 60 charts with zero acceptance flips, zero
    mandatory base drops, zero third-level drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The initial compiler-only carrier change was rejected with 99 production mismatches. Once
    production compiled, the first all-target pass exposed 29 test construction mismatches and the
    second exposed four. Every boundary was migrated explicitly; no archive or dump was produced
    from those states.
  - The first whole-workspace lint preflight was rejected on one `?`-eligible empty-path guard in
    parsed-map merge lowering. It now uses direct `?` propagation; no lint suppression was added.
  - The first immutable-archive write was rejected after its 7-minute-30-second build because
    `/private/tmp` had only 270 MiB free and returned `ENOSPC`. No partial archive survived. The
    completed B4a.13 archive was moved, not deleted, to the T7-backed
    `target/campaign-archives/`; the unchanged final code state then archived successfully.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Expression effects and abstract values | Same metadata attachment/merge | IR/unit suite. |
| Symbolic local state and branch joins | Same per-local path facts | Symbolic-state suite. |
| Helper contexts and condition decoding | Same selected identities and predicates | Condition/helper suites. |
| Fragment summaries and lowering | Same splices, taint, suppression, and rows | IR/schema dumps. |
| String-facing boundaries | Same encoded bytes and stable order | Fixture dumps and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds in 5 minutes 24 seconds, and 393/393 IR
  tests pass, covering metadata attachment, selection chains, merge layers, branch joins, helper
  contexts, fragment rows, suppression, and type-descriptor decoding. The corrected whole-workspace
  lint preflight passes warning-free in 6 minutes 20 seconds.
- Immutable build: after the recorded storage-only rejection, the unchanged final tree produces
  `/private/tmp/arch-v4-b4a15-final1.tar.zst`; exit 0, 87 binaries and 125 files. The successful
  retry reused the completed build and archived in 1.19 seconds.
- Clean schema dump: the B4a.15 `final1` archive under the step-local schema `TMPDIR`; exit 0, 62
  tests pass in 184.452 seconds and 84 artifacts are byte-identical to B4a.14.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.840 seconds and 18 artifacts are byte-identical to B4a.14.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `a65493bb`,
  Helm adjudication enabled; exit 0 in 69.159 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. Every migrated map, set, accessor, and rendered row is crate-private;
  encoded IR and schema bytes remain identical.

### Self-adversarial pass

- Whole-tree searches find no string-keyed `HelperOutputMeta` map and no string-backed
  `suppress_predicate_paths` set in production IR. Map keys are structural through construction,
  merge, selection, lowering, and branch joins; encoding occurs only where an existing string
  predicate, diagnostic, or dump API requires it.
- Literal helper/local names, dictionary keys, omitted member names, schema types, and file paths
  remain strings in their separate domains. No `Deref`, `AsRef<str>`, `Display`, cross-type
  equality, cached encoded twin, or public/wire change was introduced.

### Gates

- `cargo fmt --check`; exit 0.
- `task lint`; exit 0, complete workspace Clippy pass in 6 minutes 7 seconds.
- `task lint:fc`; exit 0, 48 combinations across 13 packages and 3 targets in 1,216.29
  seconds.
- `cargo nextest run --workspace`; exit 0, 1,308 tests pass in 232.724 seconds after the
  final build.
- `task test:integration`; exit 0, 558 tests pass and 24 skip in 1,804.955 seconds.
- `task test:all`; exit 0, 1,870 tests pass and 24 skip in 1,893.657 seconds, including all
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`; exit 0; the final release binary replaces
  `/Users/roman/.cargo/bin/helm-schema` after a 30.61-second build.
- Downstream luup2 gate with the documented macOS `xargs`/`flock` shims and installed binary;
  exit 0, 32/32 charts pass.
- `task tokei:core`; exit 0, 64,328 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`; exit 0.
- `git diff --check`; exit 0.

- Measured production LOC delta: +27 (64,301 to 64,328), from explicit typed test/boundary
  construction and direct carrier-aware lowering signatures.

## B4a.16 — migrate default-source path state

- Status: landed in `378efab8` (`refactor(ir): type default source paths`).
- Contract: representation-only migration of IR local and chart-default path sets to segmented
  `ValuesPath`, including evaluation environments, symbolic local state and branch joins,
  `ValuePathContext`, fragment summaries, and lowering. Local/helper names, literal default values,
  and static template programs remain separate string domains.
- Acceptance baseline: `debd89e8` (B4a.15).
- Baseline production LOC: 64,328 Rust lines from `task tokei:core` on `debd89e8`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Defaultedness keeps identical local scope, branch-join intersection, helper propagation,
    selection, and lowering behavior with legacy encoded ordering preserved.
  - All affected carriers are crate-private, so this round changes no public API or wire format.
    No coercion trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - Local default-path maps and chart-default sets now use `ValuesPath` across evaluation
    environments, symbolic state/snapshots/branch joins, helper contexts, fragment summaries, and
    lowering. Default-path collection now returns the typed set directly.
  - Defaultedness no longer round-trips through encoded strings between expression effects,
    assignment binding, helper-summary propagation, or splice construction. Static template
    program text and literal default values remain strings in their distinct domains.
  - Schema and symbolic-IR dumps are byte-identical to `debd89e8`; the full-depth battery checks
    121,055 probes across 60 charts with zero flips, zero mandatory base/third-level drops, and
    28,868 unchanged disclosed reductions.
- Deviations:
  - The initial compiler-only carrier change was rejected with 15 production mismatches. After
    production compiled, the first all-target pass exposed 13 test construction mismatches. Every
    boundary was migrated explicitly; no archive or dump was produced from either state.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Expression/evaluation defaults | Same fallback and selected-path facts | IR/unit suite. |
| Symbolic scopes and joins | Same per-local union and chart intersection | Symbolic-state suite. |
| Helper contexts and summaries | Same propagated defaultedness | Helper/fragment suites. |
| Lowering and emission | Same splice flags and schema bytes | Dumps and prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds in 4 minutes 26 seconds and 393/393 IR
  tests pass. Whole-workspace lint passes warning-free in 5 minutes 40 seconds.
- Immutable build: B4a.16 `final1`; exit 0, 87 binaries and 125 files after an 8-minute-1-second
  build and 1.58-second archive write.
- Clean schema dump: exit 0, 62 tests pass in 188.237 seconds and 84 artifacts are byte-identical
  to B4a.15.
- Clean IR dump: exit 0, one test passes in 4.068 seconds and 18 artifacts are byte-identical to
  B4a.15.
- Full-depth proof: baseline `debd89e8`, Helm adjudication enabled; exit 0 in 73.926 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none; every migrated carrier is crate-private and serialized bytes remain
  unchanged.

### Self-adversarial pass

- Whole-tree searches find local and chart-default state typed across the named owners. Helper and
  local names, literal values, and static program paths remain strings in distinct domains.
- No coercion trait, cross-type equality, display implementation, cached encoded twin, or public/
  wire change was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 6 minutes 24 seconds.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,312.24
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 188.712 seconds after the
  macOS final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,534.263 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,615.370 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 23.25 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,322 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: -6 (64,328 to 64,322), from eliminating repeated default-path
  encoding and parsing.

## B4a.17 — migrate scalar-dispatch identity paths

- Status: landed in `84caca86` (`refactor(ir): type scalar identity paths`).
- Contract: representation-only migration of exact scalar dispatch identities and rendered scalar
  identity parts from encoded `String` paths to segmented `ValuesPath`. Literal scalar text,
  formatter tokens, and lexical escapes remain their existing domains.
- Acceptance baseline: `378efab8` (B4a.16).
- Baseline production LOC: 64,322 Rust lines from `task tokei:core` on `378efab8`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Equality dispatch, formatter selection, serialized-route qualification, strict operands, and
    helper-render lowering retain the same identities and conditions in legacy encoded order.
  - The scalar domain is crate-private, so this round changes no public API or wire format. No
    string coercion trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - `ScalarValue::{Identity, PrintfStringIdentity}` and `ScalarRenderPart::Identity.path` now carry
    `ValuesPath`; construction from abstract values, helper-render summaries, and test fixtures no
    longer encode the path before storing it.
  - Equality, truthiness, pattern, lexical-escape, strict-comparison, formatter-selection, and
    fragment-lowering consumers use the segmented path directly. Encoding remains only at two
    still-string strict/serialization helper boundaries scheduled for later B4a rounds.
  - Schema and symbolic-IR dumps are recursively byte-identical to `378efab8`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first immutable-archive command was rejected before compilation because its absolute
    step-local `TMPDIR` had not been created; clang reported `unable to make temporary file: No
    such file or directory`. No archive or dump was produced. After creating that directory, the
    same final tree built the authoritative `final1` archive.
  - A pre-archive `cargo fmt --check` identified two formatting-only line wraps after the semantic
    preflight. `cargo fmt` applied them before the immutable archive and every final gate.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Abstract-value scalar dispatch | Same raw identity and branch arms | Eval/IR suites. |
| Helper-render summaries | Same rendered identity parts and escapes | Fragment/IR dump. |
| Equality, truthiness, and patterns | Same typed guards and preimages | Scalar/condition suites. |
| Formatter and strict consumers | Same selection conditions and captures | Chart re-audits/prober. |
| Fragment lowering | Same splice metadata and schema bytes | Schema dump/full corpus. |

### Review dossier

- Focused proof: all-target IR compilation succeeds in 42.07 seconds; 393/393 IR tests pass after
  the host's 78-second build. The first whole-workspace lint preflight also passes warning-free.
- Immutable build: B4a.17 `final1`; exit 0, 87 binaries and 125 files after a 9-minute-24-second
  build and 1.49-second archive write.
- Clean schema dump: exit 0, 62/62 pass in 186.666 seconds; all 84 artifacts are recursively
  byte-identical to B4a.16.
- Clean IR dump: exit 0, one test passes in 3.266 seconds; all 18 artifacts are recursively
  byte-identical to B4a.16.
- Full-depth proof: baseline `378efab8`, Helm adjudication enabled; exit 0 in 69.841 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none. The scalar domain is crate-private and every serialized artifact is
  byte-identical.

### Self-adversarial pass

- Whole-tree variant searches find every scalar identity payload typed as `ValuesPath`; the
  formatter, condition, strict-operand, summary, and lowering consumers no longer parse those
  payloads. Literal text, format strings, and lexical tokens remain separate string domains.
- No coercion trait, display implementation, cross-type comparison, cached encoding, public API,
  or wire-format change was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 6 minutes 07 seconds.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,364.40
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 185.991 seconds after the native
  final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,545.567 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,633.825 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 27.77 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,321 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: -1 (64,322 to 64,321); the typed payload removes repeated parsing
  while explicit encoding at two still-string consumer boundaries keeps this round attributable.

## B4a.18 — migrate fragment serialization path state

- Status: landed in `61f5d577` (`refactor(ir): type fragment serialization paths`).
- Contract: representation-only migration of fragment-interpreter YAML parse/serialization state,
  current-run templated-text identities, and scalar-arm position claims from encoded strings to
  segmented `ValuesPath`. Text alternatives, quote state, and helper names remain strings.
- Acceptance baseline: `84caca86` (B4a.17).
- Baseline production LOC: 64,321 Rust lines from `task tokei:core` on `84caca86`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Helper-summary propagation, YAML round-trip recognition, templated quote/plain-slot claims,
    and fragment lowering retain identical path membership and legacy encoded ordering.
  - All affected carriers are crate-private, so this round changes no public API or wire format.
    No coercion trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - Fragment summaries and interpreter state now keep parsed-YAML inputs and YAML-serialized paths
    as `ValuesPath`; the helper-call boundary clones those sets directly instead of encoding and
    reparsing every member.
  - Current-run `tpl` identities and the per-arm token/quote/plain-slot claim sets use the same
    typed carrier. Capture construction now consumes those paths directly.
  - Schema and symbolic-IR dumps are recursively byte-identical to `84caca86`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - A pre-archive `cargo fmt --check` identified one formatting-only line wrap. `cargo fmt` applied
    it before the immutable archive and every final gate.
  - The first downstream invocation exited 201 before executing a chart because overnight cleanup
    left the documented shim directory empty and macOS `xargs` rejected `-a`. The `xargs` and
    atomic-mkdir `flock` shims were recreated under `/private/tmp` with exit-code propagation; the
    unchanged repository tree then passed 32/32 charts. Neither repository was edited for host
    compatibility.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Expression YAML serialization | Same effect-path membership | Eval/serialization suite. |
| Helper summary propagation | Same parsed/serialized identities | Fragment/IR dump. |
| Current-run `tpl` text | Same raw-versus-templated classification | Templated-slot re-audits. |
| Quote/plain-slot claims | Same capture paths and styles | IR/schema suites and prober. |
| Fragment lowering | Same splice metadata and output | Schema dump/full corpus. |

### Review dossier

- Focused proof: all-target IR compilation succeeds in 44.70 seconds; 393/393 IR tests pass after
  the host's 80-second build. Whole-workspace lint passes warning-free in 7 minutes 07 seconds.
- Immutable build: B4a.18 `final1`; exit 0, 87 binaries and 125 files after an 8-minute-19-second
  build and 1.36-second archive write.
- Clean schema dump: exit 0, 62/62 pass in 188.343 seconds; all 84 artifacts are recursively
  byte-identical to B4a.17.
- Clean IR dump: exit 0, one test passes in 3.271 seconds; all 18 artifacts are recursively
  byte-identical to B4a.17.
- Full-depth proof: baseline `84caca86`, Helm adjudication enabled; exit 0 in 68.951 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none. All migrated state is crate-private and serialized bytes remain
  unchanged.

### Self-adversarial pass

- Whole-tree searches find the fragment interpreter and summary parsed/serialized path sets, the
  current-run templated set, and all six arm-claim sets use `ValuesPath`; their producers and
  consumers no longer encode or parse at those boundaries.
- Text alternatives, quote state, helper identifiers, and format tokens remain strings in their
  distinct domains. No coercion trait, display implementation, cross-type comparison, cached
  encoding, public API, or wire-format change was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 7 minutes 07 seconds.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,534.84
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 188.413 seconds after the native
  final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,526.546 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,614.761 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 23.00 seconds.
- downstream luup2 `check:local` with the recreated documented macOS shims and explicit installed
  binary: exit 0; 32/32 charts pass. The rejected missing-shim preflight is recorded above.
- `task tokei:core`: exit 0; 64,309 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: -12 (64,321 to 64,309), from removing encoded-path set maps and
  repeated encode/parse conversions across fragment interpretation and summary propagation.

## B4a.19 — migrate integer-cast source paths

- Status: landed in `2aa21c84` (`refactor(ir): type integer cast source paths`).
- Contract: representation-only migration of `IntCastSource.path` from encoded `String` to
  segmented `ValuesPath`, including branch/local propagation and every comparison-predicate
  consumer. The optional integer fallback remains unchanged.
- Acceptance baseline: `61f5d577` (B4a.18).
- Baseline production LOC: 64,309 Rust lines from `task tokei:core` on `61f5d577`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Inline and local-bound `int`/`int64` comparisons retain the same raw-path predicates, fallback
    subsets, branch joins, and integer domains in legacy encoded order.
  - The carrier is crate-private, so this round changes no public API or wire format. No coercion
    trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - `IntCastSource.path` now carries `ValuesPath` through local bindings, scope snapshots, and
    branch joins. Inline cast recognition parses once at the expression-resolution boundary.
  - Every equality, inequality, and bounded-integer predicate consumer clones the typed path
    directly instead of reparsing the same encoded string.
  - Schema and symbolic-IR dumps are recursively byte-identical to `61f5d577`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - A pre-archive `cargo fmt --check` identified one formatting-only line wrap. `cargo fmt` applied
    it before the immutable archive and every final gate.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Inline `int`/`int64` cast | Same raw path and integer subset | Condition-predicate suite. |
| Local cast binding | Same source through assignment/scope | Symbolic-local suite. |
| Branch join | Same equal-source retention | Branch-join/IR suite. |
| Comparisons and fallbacks | Same equality and bounded domains | Cilium/Jenkins re-audits. |

### Review dossier

- Focused proof: all-target IR compilation succeeds in 43.42 seconds; 393/393 IR tests pass after
  the host's 78-second build. Whole-workspace lint passes warning-free in 7 minutes 25 seconds.
- Immutable build: B4a.19 `final1`; exit 0, 87 binaries and 125 files after a 5-minute-42-second
  build and 1.36-second archive write.
- Clean schema dump: exit 0, 62/62 pass in 187.454 seconds; all 84 artifacts are recursively
  byte-identical to B4a.18.
- Clean IR dump: exit 0, one test passes in 3.242 seconds; all 18 artifacts are recursively
  byte-identical to B4a.18.
- Full-depth proof: baseline `61f5d577`, Helm adjudication enabled; exit 0 in 68.617 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none. The cast-source carrier is crate-private and serialized bytes remain
  unchanged.

### Self-adversarial pass

- Whole-tree searches find `IntCastSource.path` typed and no consumer reparses it. The optional
  integer fallback and local-variable map keys remain in their distinct domains.
- No coercion trait, display implementation, cross-type comparison, cached encoding, public API,
  or wire-format change was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 7 minutes 25 seconds.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,576.30
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 186.247 seconds after the native
  final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,525.794 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,704.758 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 24.62 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,310 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +1 (64,309 to 64,310); the typed source removes repeated parsing
  at thirteen consumers while the once-only expression-boundary construction remains explicit.

## B4a.20 — migrate bound `get` paths

- Status: landed in `13b74300` (`refactor(ir): type bound get paths`).
- Contract: representation-only migration of bound Sprig `get` base paths and resolved selector
  path sets from encoded strings to segmented `ValuesPath`, including symbolic-local propagation,
  condition decoding, and expression-effect publication. Variable names and literal key domains
  remain strings.
- Acceptance baseline: `2aa21c84` (B4a.19).
- Baseline production LOC: 64,310 Rust lines from `task tokei:core` on `2aa21c84`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Literal-key range filtering, bound selector expansion, grouped member reads, and truthiness
    predicates retain identical membership and legacy encoded ordering.
  - The carriers are crate-private, so this round changes no public API or wire format. No
    coercion trait, cross-type comparison, or cached encoded twin is allowed.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - `GetBinding.base` now carries `ValuesPath` through parser recognition, symbolic-local state,
    scope snapshots, and branch joins. Bound selector expansion clones the base and pushes literal
    key and suffix segments structurally.
  - `BoundValueContext::selector_paths` returns typed paths; expression effects consume the set
    directly, and bound-key truthiness builds a typed guard without append/parse round-trips.
  - Schema and symbolic-IR dumps are recursively byte-identical to `2aa21c84`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first all-target compiler preflight rejected six test constructions after production
    compiled. Tests were migrated explicitly; no archive or dump was produced from that state.
  - A later `cargo fmt --check` identified two formatting-only wraps. `cargo fmt` applied them
    before the immutable archive and final gates.
  - The first whole-workspace lint preflight rejected two redundant `.into_iter()` calls exposed by
    the typed set. The calls were deleted; no suppression or artifact from that state was adopted.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| `get` binding recognition | Same base and key variable | Bound-value suite. |
| Literal range domain | Same filtered key alternatives | Bound-value/condition suites. |
| Selector expansion | Same base/key/suffix paths | Expr/IR suites. |
| Symbolic state and joins | Same binding lifetime and equality | Symbolic-local suite. |
| Truthiness/effects | Same typed guards and bound outputs | Re-audits/dumps/prober. |

### Review dossier

- Focused proof: the second all-target IR compilation succeeds in 21.21 seconds; 393/393 IR tests
  pass after the host's 78-second build. Corrected whole-workspace lint passes warning-free in 6
  minutes 04 seconds.
- Immutable build: B4a.20 `final1`; exit 0, 87 binaries and 125 files after a 5-minute-32-second
  build and 1.39-second archive write.
- Clean schema dump: exit 0, 62/62 pass in 195.159 seconds; all 84 artifacts are recursively
  byte-identical to B4a.19.
- Clean IR dump: exit 0, one test passes in 3.350 seconds; all 18 artifacts are recursively
  byte-identical to B4a.19.
- Full-depth proof: baseline `2aa21c84`, Helm adjudication enabled; exit 0 in 77.326 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none. The bound-value carriers are crate-private and serialized bytes
  remain unchanged.

### Self-adversarial pass

- Whole-tree searches find `GetBinding.base` and bound selector result sets typed. Expansion uses
  structural pushes; expression effects no longer parse bound outputs. Variable names, literal
  range values, and key variables remain strings in their distinct domains.
- No coercion trait, display implementation, cross-type comparison, cached encoding, public API,
  or wire-format change was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 6 minutes 04 seconds after the rejected preflight above.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,451.84
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 194.796 seconds after the native
  final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,621.499 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,701.503 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 28.95 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,312 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +2 (64,310 to 64,312); structural expansion removes path
  re-encoding at consumers while retaining an explicit once-only parser boundary.

## B4a.21 — type core path accessors and rewrites

- Status: landed in `c8d58a10` (`refactor(core): type contract path accessors`).
- Contract: representation-only migration of `Guard`, `Predicate`, and `ConditionalGuard` path
  accessors to `ValuesPath`, plus total typed `map_value_paths` callbacks across the core contract
  carriers. Encoded paths remain only at diagnostics and wire boundaries.
- Acceptance baseline: `13b74300` (B4a.20).
- Baseline production LOC: 64,312 Rust lines from `task tokei:core` on `13b74300`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Guard/predicate path collection and every scope/reroot rewrite retain identical path membership
    and legacy encoded ordering, including dotted and backslash-containing literal segments.
  - Public API decision: accessor and rewrite callback types narrow from encoded `String`/`&str`
    to `ValuesPath`; this is the scheduled B4a carrier migration. Serde/wire spellings remain
    byte-identical, and no compatibility overload or string coercion is retained.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - `Guard`, `Predicate`, `GuardDnf`, `ConditionalGuard`, `ContractUse`, capture kinds, observed
    facts, range modes, and the contract graph now exchange `ValuesPath` directly through their
    path accessors and rewrite callbacks. Scope/reroot operations manipulate structural segments;
    encoded strings remain explicit at diagnostics, legacy builder maps, and wire boundaries.
  - Range-key concretization now rewrites typed member/concrete paths structurally, so a literal
    segment cannot be reinterpreted as selector syntax during a predicate rewrite.
  - Schema and symbolic-IR dumps are recursively byte-identical to `13b74300`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first core-only compiler preflight rejected three callback conversions. The next workspace
    preflight exposed seven generator and 95 IR/test mismatches, and the narrowed IR library pass
    exposed 48 production consumers. Each boundary was migrated explicitly; no archive or dump was
    produced from those states.
  - The first all-target pass after production compiled rejected one generator delimiter mistake
    and 24 IR test constructions. The next pass exposed remaining generator and engine boundary
    conversions, and the first focused nextest preflight exposed the example CLI's string
    comparison. All were corrected before the immutable archive.
  - The first lint preflight rejected three redundant encode closures and one function that grew
    past the 100-line limit. The first shortening attempt used a method pointer with the wrong
    owned/reference signature; the second remained one line over the limit. The final version
    extracts the range-body selection construction into one direct helper, with no suppression.
    No artifact from any rejected lint state was adopted.
  - A pre-archive `cargo fmt --check` identified formatting-only wraps. `cargo fmt` applied them
    before the immutable archive and every final gate.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Guard/predicate accessors | Same path membership and encoded order | Core/IR suites and IR dump. |
| Contract graph scope/reroot | Same dependency/global projection | Contract and engine suites. |
| Capture and observed-fact rewrites | Same capture paths and selection predicates | IR suite and schema dump. |
| Range-key concretization | Same member substitutions without syntax loss | Fragment/condition suites. |
| Generator guard consumers | Same conditional schemas and requirements | Schema dump and full-depth prober. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds; 1,157/1,157 focused core/IR/gen/engine
  tests pass in 189.149 seconds after the native build. Corrected whole-workspace lint passes
  warning-free in 3 minutes 40 seconds.
- Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-b4a21-final1-build cargo
  nextest archive --workspace --archive-file
  /Volumes/T7/dev/helm-schema/target/campaign-archives/arch-v4-b4a21-final1.tar.zst`; exit 0, 87
  binaries and 125 files after a 9-minute-35-second build and 1.95-second archive write.
- Clean schema dump: the B4a.21 `final1` archive under the step-local schema `TMPDIR`; exit 0,
  62/62 pass in 190.795 seconds; all 84 artifacts are recursively byte-identical to B4a.20.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one test passes in
  3.334 seconds; all 18 artifacts are recursively byte-identical to B4a.20.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `13b74300`,
  Helm adjudication enabled; exit 0 in 72.014 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: the public core accessor and rewrite callback types intentionally narrow
  from encoded strings to `ValuesPath`, as scheduled by B4a and recorded before implementation.
  Serde output and every fixture byte remain unchanged; no compatibility overload was retained.

### Self-adversarial pass

- Whole-tree compilation proves every accessor and rewrite caller consumes the typed carrier.
  Structural reroots use segment iteration/pushes; no `Deref`, `AsRef<str>`, `Display`, cross-type
  comparison, cached encoded twin, or compatibility callback was introduced.
- Encoded conversions remain visible only where the adjacent representation is still genuinely
  string-keyed or serialized. Helper names, local variables, literal keys, and diagnostic text stay
  strings in their distinct domains.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 3 minutes 40 seconds after the rejected preflights above.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 2,504.60
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 230.681 seconds after the
  7-minute-34-second native final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,699.223 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 2,040.556 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 27.91 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,355 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +43 (64,312 to 64,355). The public carrier seam removes implicit
  string currency but necessarily makes the remaining encoded boundaries explicit; no live
  semantic code or test coverage was deleted to force a negative delta.

## B4a.22 — migrate contract-builder path indexes

- Status: landed in `41036f3f` (`refactor(ir): type contract builder path indexes`).
- Contract: representation-only migration of the contract-signal builder's path accumulator map,
  descendant indexes, and path-accumulator API from encoded `String` to segmented `ValuesPath`.
  The builder must hand its typed map directly to `ContractSchemaSignals`, deleting the final
  map-key parse and the remaining dual encoded-key/typed-value identity.
- Acceptance baseline: `c8d58a10` (B4a.21).
- Baseline production LOC: 64,355 Rust lines from `task tokei:core` on `c8d58a10`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Accumulator insertion, ancestor/descendant discovery, item-parent promotion, requirement
    targeting, and final signal construction retain identical membership and legacy encoded order.
  - Every path entering from a still-string expression boundary parses once; every typed producer
    remains typed. No compatibility map, string coercion, or cached encoded key is allowed.
  - The carrier and builder are crate-private, so this round changes no public API or wire format.
  - Any fixture or acceptance flip stops the round before adoption. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level coverage drops remain zero.

- Measured results:
  - The contract-signal builder now indexes every `ContractPathAccumulator` by `ValuesPath` from
    first insertion through final signal construction. Descendant, item-descendant, and structured-
    item-descendant sets are derived from structural segments and preserve legacy encoded order.
  - `finish_schema_signals` hands the typed map directly to `ContractSchemaSignals`; the final
    map-key parse and encoded item-parent reconstruction are deleted.
  - Schema and symbolic-IR dumps are recursively byte-identical to `c8d58a10`. The full-depth
    battery checks 121,055 probes across 60 charts with zero flips, zero mandatory base/third-level
    drops, and 28,868 unchanged disclosed reductions.
- Deviations:
  - The first compiler preflight after changing the accumulator type rejected 43 builder
    consumers. Each typed producer was wired directly and each still-string capture boundary was
    parsed explicitly; no archive or dump was produced from that state.
  - A pre-archive `cargo fmt --check` identified formatting-only wraps. `cargo fmt` applied them
    before the immutable archive and every final gate.
- Adjudication evidence: zero flips require no per-cell Helm verdict. Helm 4.2.3 adjudication was
  enabled and reports zero candidate-accepts/Helm-aborts cells against the zero allowance.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Contract rows and captures | Same accumulator ownership | IR/contract suites. |
| Ancestor/item discovery | Same parent and ranged-member facts | Builder/schema suites. |
| Requirement implications | Same targets and outer guards | Fail/member-access suites. |
| Final signal construction | Same typed keys and evidence | IR/schema dumps. |
| Generator consumption | Same schema and acceptance | Full-depth prober and luup2. |

### Review dossier

- Focused proof: workspace all-target compilation succeeds; 1,157/1,157 focused core/IR/gen/engine
  tests pass in 183.747 seconds after a 3-minute-02-second native build. Whole-workspace lint
  passes warning-free in 7 minutes 58 seconds.
- Immutable build: B4a.22 `final1`; exit 0, 87 binaries and 125 files after a 6-minute-34-second
  build and 2.01-second archive write.
- Clean schema dump: exit 0, 62/62 pass in 188.431 seconds; all 84 artifacts are recursively
  byte-identical to B4a.21.
- Clean IR dump: exit 0, one test passes in 3.290 seconds; all 18 artifacts are recursively
  byte-identical to B4a.21.
- Full-depth proof: baseline `c8d58a10`, Helm adjudication enabled; exit 0 in 69.636 seconds, 60
  charts, 121,055 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
  and 28,868 disclosed reductions.
- Public/wire decision: none. The builder and accumulator carrier are crate-private, and serialized
  signal order and bytes remain unchanged.

### Self-adversarial pass

- Whole-tree searches find no `BTreeMap<String, ContractPathAccumulator>` and no parse between the
  builder's final map and `ContractSchemaSignals`. Ancestors and item parents are structural.
- Remaining strings in the builder are literal keys, type/kind names, diagnostics, relative field
  paths, or explicit capture-boundary spellings; no compatibility map, string coercion, display
  implementation, cross-type comparison, or cached encoded key was introduced.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0 in 7 minutes 58 seconds.
- `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 1,637.48
  seconds, followed by the ast-grep policy checks.
- `cargo nextest run --workspace`: exit 0; 1,308/1,308 pass in 187.729 seconds after the
  6-minute-53-second native final-tree build.
- `task test:integration`: exit 0; 558/558 pass, 24 skipped, in 1,534.377 seconds.
- `task test:all`: exit 0; 1,870/1,870 pass, 24 skipped, in 1,627.162 seconds, including the
  live-network tests.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 23.24 seconds.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,352 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: -3 (64,355 to 64,352), from deleting the final map-key parse and
  replacing encoded ancestor/item-parent reconstruction with the shared structural carrier.

## B2/E1 — `Transform` vocabulary spike

- Status: recorded and abandoned after two failed E-gated attempts; no production change landed.
- Contract: representation-only first spike on `HelperOutputMeta`: replace its parallel transform
  Booleans with the exhaustive `Transform` vocabulary and one `TransformSet`, including
  `Encoded`. Preserve payload-bearing facts separately and define identity as set emptiness plus
  absence of those payloads. No other carrier migrates until this spike proves the design.
- Acceptance baseline: `41036f3f` (B4a.22).
- Baseline production LOC: 64,352 Rust lines from `task tokei:core` on `41036f3f`.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Every helper-output transform producer, merge, clear, identity predicate, and consumer retains
    exactly its prior truth table, including mixed branches and payload-bearing states.
  - G2 stage 2 must enumerate `Transform::ALL`, assert the produced fact before full schema
    equality, and cover mixed-branch, clear/removal, scalar-dispatch, quoted/plain-token, and
    payload-bearing cases before adoption.
  - E-gates: delete the helper-output Boolean representation; non-positive whole-tree production
    LOC; byte-exact schema/IR fixtures; flat-or-better corpus wall-clock. A miss records and
    abandons the spike rather than forcing it.
  - Any fixture or acceptance flip fails the representation spike. Candidate-accepts/Helm-aborts
    allowance and mandatory base/third-level coverage drops remain zero.

- Deviations:
  - The initial compiler-only carrier replacement rejected 127 helper-output Boolean consumers.
    This is a preflight inventory, not an adopted state; no archive, dump, fixture, or acceptance
    artifact has been produced from it.
  - Attempt 1 completed that compiler migration but failed E1's first hard adoption gate:
    `task tokei:core` measured 64,486 production Rust lines, +134 from the 64,352 baseline. The
    verbose ordered-set membership API moved the representation without simplifying the tree, so
    the entire code state was rejected before fixtures or corpus timing. Attempt 2 restarts from
    the clean baseline with a compact bit-set carrier and shorter `has`/`set` operations.
  - Attempt 2 completed the same compiler migration with a compact `u16` set and concise
    operations. `cargo check -p helm-schema-ir --lib` passed, but `task tokei:core` measured
    64,419 production Rust lines: +67 from baseline. It therefore failed the same non-positive
    whole-tree LOC gate. The code state was fully reverted before any archive, fixture dump, or
    corpus timing run. Per the two-attempt rule, B2/E1 is abandoned for this wave rather than
    forced; G2 stage 2 remains coupled to a future viable B2 design.
- Measured results:
  - Both attempts deleted all 12 helper-output transform Booleans and compiled their complete
    producer/consumer surface into one transform set, proving the representation is technically
    viable but not simpler under the frozen whole-tree metric.
  - The authoritative production tree is byte-identical to baseline after reverting both spikes.
    No fixture or acceptance artifact was generated from either rejected state.
- Adjudication evidence: no candidate behavior or fixture state was produced, so there are zero
  flips and no Helm cells to adjudicate.

### Review dossier

- Attempt 1: compiler migration complete; `task tokei:core` exit 0, 64,486 LOC (+134); rejected.
- Attempt 2: `cargo check -p helm-schema-ir --lib` exit 0; `task tokei:core` exit 0, 64,419 LOC
  (+67); rejected. The remaining E-gates were intentionally not run because the first hard gate
  already failed.
- Public/wire decision: none. Neither spike was adopted; the production API and wire formats remain
  those of `41036f3f`.

### Self-adversarial pass

- The second design removed ordered-set allocation and shortened membership/update operations, yet
  remained +67 LOC. Further compression would select for clever APIs or hidden macros rather than
  architectural deletion, exactly what the E-gate forbids.
- Whole-tree status after reversion contains no `Transform` spike code. The recorded failure is an
  explicit campaign result, not residual compatibility debt.

### Gates

- Attempt 1 `task tokei:core`: exit 0, failed adoption at +134 LOC.
- Attempt 2 `cargo check -p helm-schema-ir --lib`: exit 0.
- Attempt 2 `task tokei:core`: exit 0, failed adoption at +67 LOC.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0 on the reverted tree.
- `git diff --check`: exit 0 on the reverted tree.

- Measured production LOC delta: 0 adopted; attempt 1 measured +134 and attempt 2 +67 before full
  reversion.

## B4b — distinguish literal `*` keys from ranged members

- Status: landed in `45606b5b`.
- Contract: behavior-bearing typed `Segment::{Literal(String), EachMember}` inside `ValuesPath`.
  Literal `*` keys encode distinctly and remain object members; only `EachMember` selects array/map
  member semantics. Existing wildcard paths and their serialized order remain stable.
- Acceptance baseline: `3de22377` (recorded B2/E1 abandonment; production equals `41036f3f`).
- Baseline production LOC: 64,352 Rust lines.
- Pre-registered acceptance expectations:
  - One expected behavior family: a chart that structurally reads a literal values key named `*`
    now emits a literal `"*"` object property instead of routing that path through member/items
    lowering. The microchart uses quoted YAML and `index .Values "*"`; Helm 4.2.3 must render it.
  - Existing ranged-member paths retain the legacy `*` wire spelling, ordering, requirements, and
    schema bytes. Quoted-key and dotted/backslash literal behavior remain unchanged.
  - The audited corpus contains no chart-authored literal-`*` values read, so tracked corpus and
    luup2 fixture flips are expected to be zero. Any unrelated cell stops the round for individual
    Helm adjudication before fixture adoption.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero; bounded disclosed reductions are reported.
  - Public/wire decision: `Segment` becomes part of the public core path API and `segments()` yields
    typed segments. This is the scheduled B4b narrowing. Previously serialized wildcard paths keep
    decoding; the new escaped literal-star spelling is additive.

- Measured results:
  - `ValuesPath` now stores `Segment::{Literal, EachMember}`. Parsing the legacy unescaped `*`
    spelling yields `EachMember`; a chart-authored literal star serializes as `\*`; dots,
    backslashes, existing wildcard spellings, serde, and `ValuesPath` ordering retain their prior
    bytes.
  - Range, integer-index, dependency-global, capture, guard, and schema-tree producers append or
    preserve `EachMember` explicitly. Literal selectors (`index .Values "*"`) use `Literal("*")`.
  - The synthetic literal-star chart changes from a root object/array collection union to the
    exact closed root object with a named `"*": {}` property. Both direct and guarded full-schema
    equality controls pass.
  - The authoritative full and lean schema dumps are byte-identical to `3de22377`: 56 chart-corpus
    schemas plus four lean schemas. All 18 symbolic-IR dumps are byte-identical too.
  - The full-depth battery checks 121,055 probes across 60 charts with zero flips, zero mandatory
    base/third-level drops, 28,868 disclosed reductions, and zero candidate-accepts/Helm-aborts.
- Deviations:
  - The first compiler sweep exposed 60 IR and nine test consumers of the formerly stringly
    segment iterator. Each consumer was classified as a literal-key, encoded-boundary, or
    ranged-member route before compilation was allowed to proceed.
  - A seven-test generator preflight exposed wildcard prefixes reassembled through the new literal
    constructor. The rejected schemas widened range-member contracts. Typed capture construction
    and typed consecutive-wildcard reconstruction restored all seven tests before any final
    artifact was accepted.
  - The `final1` immutable archive is rejected. Its clean dump exposed `$defs` ordering drift and
    spurious `"\\*"` properties. No dump or acceptance result from that archive is adopted.
  - Focused Airflow, Prometheus/KPS, External DNS, and Falco preflights isolated three remaining
    compatibility teeth: encoded dependency-global joins, the measured ranged-presence
    `HasMemberEvenDefaulted("*")` lowering, and integer indexing that had called the now-literal
    `apply_to_path("*")`. The final design gives integer indexing its own `indexed_item` producer,
    preserves the measured presence tooth explicitly, and keeps generator-local ordering strings
    while the semantic carrier remains typed.
  - The first final2 IR-dump command used a non-matching nextest binary filter and exited 94 before
    running a test. The corrected package-and-binary filter ran the one corpus test and produced
    the adopted 18-file dump.
- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` renders the quoted literal-star microchart with `selected: "value"` and
    also renders `--set-string '*=override'` with `selected: "override"`.
  - The candidate schema names the literal `"*"` property and accepts the chart default. The
    baseline's fabricated array lane is removed; a Helm values document is a mapping, so this is
    not a false rejection.
  - The tracked 60-chart battery has zero flips, so no tracked fixture cell needs individual Helm
    adjudication; live adjudication was enabled and the zero candidate-accepts/Helm-aborts allowance
    is satisfied.

### Producer and route coverage

| Route | Expected result | Verification |
|---|---|---|
| Literal selector/index key | `Literal("*")`, named object property | Core codec and two full-schema generator tests; Helm microchart. |
| `range`/integer-index member | `EachMember`, legacy `*` bytes | Range/member suites and Falco byte preflight. |
| Nested/consecutive wildcards | Typed prefixes survive capture lowering | Nested `hasKey`, fail, and range-domain suites. |
| Dependency/global projection | Encoded joins preserve member identity | Contract/global suites and Prometheus/KPS dumps. |
| Conditional/schema-tree lowering | Existing wildcard bytes; literal star stays named | Full and lean dump identity plus literal-star schema equality. |
| Public serde and ordering | Legacy wildcard wire/order; additive `\*` literal wire | Core serde, ordering, and component-codec tests. |

### Review dossier

- Focused proof: 619/619 generator tests pass; literal-star core and generator controls pass; the
  complete 56-chart and four-chart lean preflights are byte-identical before the final archive.
- Immutable build: B4b `final2`; exit 0, 87 binaries and 125 files after a 9-minute-24-second build
  and 2.84-second archive write.
- Clean schema dump: the final2 archive under the step-local schema `TMPDIR`; 56/56 chart tests
  pass in 236.490 seconds and the lean fixture test passes in 75.321 seconds; all 60 artifacts are
  byte-identical to the committed fixtures.
- Clean IR dump: the final2 archive under the step-local IR `TMPDIR`; one corpus test passes in
  3.660 seconds and the 18 dump hashes are byte-identical to the committed IR fixtures.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `3de22377`,
  Helm adjudication enabled; exit 0 in 73.388 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: `Segment` is a public core enum and `segments()` now yields typed segments.
  Existing wildcard serde bytes remain stable; `\*` is the additive literal-star spelling. No
  `Deref`, `AsRef<str>`, `Display`, or cross-type equality compatibility surface was added.

### Self-adversarial pass

- Whole-tree searches find no wildcard-producing `push("*")` or `append_value_path(..., "*")`.
  Every semantic member producer uses `push_each_member`, `append_each_member_value_path`, typed
  prefix reconstruction, or `indexed_item`.
- `from_segments` and `push` remain literal-only by construction. Encoded component reassembly is
  named explicitly, and the component codec round-trips a literal star separately from both
  `EachMember` and a literal backslash-star key.
- The rejected final1 dump and three subsequent byte preflights demonstrate that ordering,
  ranged-presence compatibility, nested member captures, and generator-local wildcard behavior
  were checked against real corpus output rather than inferred from unit green alone.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0.
- `task lint:fc`: exit 0.
- `cargo nextest run --workspace`: exit 0.
- `task test:integration`: exit 0.
- `task test:all`: exit 0.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0.
- downstream luup2 `check:local` with the documented macOS shims and explicit installed binary:
  exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,721 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +369 (64,352 to 64,721). The typed producer/consumer migration,
  explicit integer-index projection, reversible component codec, and schema-tree distinction add
  code; no live semantics or audit coverage was deleted to force a negative number.

## E2 — split fail conditions from execution context

- Status: recorded and abandoned after two failed E-gated attempts; no production change landed.
- Contract: representation-only. Replace `FailCapture`'s mixed predicate conjunction with one
  Boolean `condition: Predicate` and an exhaustive `context: Vec<ContextMark>` for `with`, `range`,
  and `default` execution scope. Delete marker stripping and marker classification from requirement
  lowering while preserving `fail_outer_guard`. As an E-step, adoption additionally requires at
  least one representation deleted, non-positive whole-tree production LOC, byte-exact fixtures,
  and flat-or-better corpus wall-clock.
- Acceptance baseline: `45606b5b` (B4b).
- Baseline production LOC: 64,721 Rust lines.
- Pre-registered acceptance expectations:
  - Zero schema or symbolic-IR fixture byte changes and zero acceptance flips.
  - Multi-path `with` keeps its exact selection predicate; `with`, `range`, and `default` execution
    marks never become negatable Boolean conditions.
  - Direct, derived, nested, member-key, and key-equality range lanes preserve their existing
    implication sets. `fail_outer_guard` and its polarity rules remain unchanged.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero.
  - Public/wire decision: none; all candidate carriers are crate-private.

- Measured results:
  - Attempt 1 implemented the split and deleted the two marker-strip dances, but grew production
    Rust to 64,932 lines (+211). Its apparent dump success was invalid: dump mode returns before
    fixture comparison. The first real integration comparison found 24 failures, and a manual
    comparison found 22/56 changed chart-corpus artifacts. The attempt was rejected.
  - Attempt 2 restored exact typed semantics for the audited failures: selection predicates remain
    Boolean, context marks retain `With` versus `Truthy` flavor, key-equality subsumes its paired
    range context, and the contract-row and requirement lanes share one range-context helper.
    Argo CD, NATS, the generator corpus, and the Temporal lean fixture were byte-exact in the final
    focused preflight.
  - Attempt 2 required deterministic compatibility ordering, exhaustive producer bookkeeping,
    typed member-context classification, and checked scope transfer. Production Rust measured
    65,083 lines (+362), failing the non-positive E-step adoption gate more heavily than attempt 1.
  - Per the frozen spike rule, the candidate was abandoned instead of deleting live semantics or
    tests to manufacture a negative result. All E2 production and test changes were removed with an
    apply-patch restoration; `git diff --exit-code 45606b5b -- crates/helm-schema-ir` exits 0 and
    production LOC is again 64,721.

- Deviations:
  - The initial compiler-driven migration stopped with 121 all-target errors while all 47 capture
    construction sites were classified. That preflight was expected and rejected.
  - Generator preflights successively exposed selection-chain, ranged-member, with-scoped absence,
    numeric-suffix, nil-strict, and guarded member/key regressions. No artifact from those states was
    adopted.
  - The first lint preflight exited 201 after context bookkeeping pushed `activate_with` over the
    line limit; extraction fixed the lint without a suppression, but did not change the E-gate
    result.
  - Two full-depth commands were rejected before authoritative use: one omitted `--run-ignored all`
    and ran zero tests (exit 4), and one omitted `SCHEMA_PROBE_COVERAGE_REPORT` (exit 100 after
    70.254 seconds).
  - Final1 was rejected for conflating multi-arm selection with singleton fallback truthiness.
    Final2 was rejected after a final-tree `if_not_else` lint changed the compiled artifact.
  - Final3 was rejected when the integration fixture comparison disproved the earlier dump-only
    claim. The immutable final1/final2/final3 archives and all their artifacts remain rejected.
  - A speculative candidate-age tracker produced no fixture change and was removed. Three temporary
    trace insertions initially matched an earlier similarly shaped helper or failed a missing-`Debug`
    compile; each was corrected only for diagnosis and removed before restoration.
  - A separate baseline source probe under `/private/tmp/helm-schema-e2-baseline-45606b5b` proved the
    current legacy capture order was faithful and isolated the real twin-helper/key-equality rules.
    It did not modify either repository and is not adoption evidence.

- Adjudication evidence:
  - The rejected final3 full-depth prober used Helm `v4.2.3+g43e8b7f` and measured 60 charts,
    121,055 probes, zero flips, 112,260/112,260 mandatory base probes, 7,465/7,465 mandatory
    third-level probes, 28,868 disclosed reductions, and zero candidate-accepts/Helm-aborts.
  - Those semantic results show the spike's drift was representation/grouping drift, but they do
    not override byte identity or the E-step LOC gate. No fixture or acceptance result was adopted.

### Producer and route coverage

| Route | Attempt-2 typed finding | Disposition |
|---|---|---|
| Direct/guarded fail | Boolean condition is separable from execution context. | Reverted with spike. |
| Single/multi-path `with` | Flavor and selected-candidate identity are load-bearing. | Reverted with spike. |
| Direct/derived range | One shared context helper is required across row and requirement lanes. | Reverted with spike. |
| Nested/member range | Member-local `with` marks require typed selector classification. | Reverted with spike. |
| Range-key equality | Key equality subsumes the paired outer range context. | Reverted with spike. |
| `default` fallback | Default is context and remains an abstention/null-safety boundary. | Reverted with spike. |
| Helper/scoped transfer | Condition composition and context union require stable ordering. | Reverted with spike. |

### Review dossier

- Rejected immutable archives: E2 final1, final2, and final3; none is authoritative.
- Focused attempt-2 proof before abandonment: exact Argo CD, NATS, generator-corpus, and Temporal
  lean fixtures. The broader fixture battery was deliberately not promoted after the LOC gate failed.
- Representation deletion: achieved inside the spike (mixed marker interpretation and duplicate
  range-context ownership removed), but only by adding a larger producer/context compatibility
  representation; therefore it did not satisfy the deletion gate in the architecture-level sense.
- Corpus wall-clock: not accepted or compared because the non-positive LOC gate failed first.
- Public/wire decision: none. No E2 carrier or serialization change landed.

### Self-adversarial pass

- The typed split can be made semantically and byte exact on every audited sensitive route, but the
  compatibility ordering and producer-side ownership required to do so are a net-new representation.
- Removing that compatibility state reintroduced deterministic `$defs` ordering/grouping drift;
  retaining it violated the non-positive whole-tree LOC gate. There is no honest adoption state in
  this wave.
- The rejected spike therefore supplies a measured design result: E2 is not a deletion at the
  current boundary. A future attempt must first remove or redesign the ordering dependency instead
  of layering context beside it.

### Gates

- Spike adoption gate (attempt 1): failed; +211 production Rust LOC.
- Spike adoption gate (attempt 2): failed; +362 production Rust LOC.
- Post-abandon restoration: `git diff --exit-code 45606b5b -- crates/helm-schema-ir`; exit 0.
- `task tokei:core`: exit 0; 64,721 production Rust lines.
- Final ledger/frozen-plan gates: the wave close-out run below passes every gate.

- Measured production LOC delta: 0 landed (64,721 to 64,721). Rejected spike deltas were +211 and
  +362.

## Wave 1 close-out

- Status: implementation complete through the eligible extended scope. B2/E1 and E2 are measured
  abandoned spikes; B4b is the last landed semantic round. This ledger-only close-out does not
  reopen either spike.
- Range: `0a31f95e` (62,082 production Rust LOC) through `45606b5b` (64,721 LOC), plus this
  close-out ledger commit. The wave landed 49 commits before close-out: one Helm pin, one standalone
  timeout-infrastructure commit, the scheduled correctness/enforcement/path-carrier rounds, and the
  B2/E1 abandonment record.
- Final production Rust LOC: 64,721; net +2,639 from the wave start. LOC is evidence rather than a
  target for ordinary rounds. Both E-spikes correctly used the stricter non-positive adoption gate
  and landed zero production LOC.

### Success-metric reconciliation

| Frozen success metric | Wave-1 result |
|---|---|
| Battery coverage guarantee | Achieved: capped probes are deterministic round-robin buckets over every top-level root × replacement kind before any repeat; a zero-base battery is a hard failure. |
| Capability-probe table | Achieved: every pinned `(api_version, kind)` row is corpus-validated without changing the table's authoritative upstream-first contract. |
| Raw-string operations on the values-path currency | The entire scheduled B4a carrier surface is segmented and byte-compatible. No `Deref`, `AsRef<str>`, `Display`, cross-type equality, or string-key compatibility map was added; encoding remains at wire/diagnostic boundaries. |
| Hand-synced producer/consumer pairs | Materially reduced but not honestly countable as zero: exhaustive `CaptureKind`, carrier destructures, cache-key construction, and serde/module ownership are compiler-enforced. B2/E1 and E2 proved remaining transform/context synchronization cannot yet be deleted under their E-gates. |
| Rules with two owners | Reduced by the B1 guard-flatten collapse and the landed A-round ownership fixes, but not zero. The rejected E2 spike found a concrete duplicate range-context rule; it remains because the whole spike was reverted. |
| Semantic vocabularies (6+ → 3) | Not achieved in wave 1. B2/E1 failed at +134/+67 LOC; E2 failed at +211/+362 LOC. No vocabulary was forced through a failed simplification gate. |
| Airflow generation wall-clock | Not re-profiled: B5/C2 are out of this wave. The 112.7-second frozen baseline remains the next relevant comparison point. |

- G2 stage 1 suite size: ten generated transform × consuming-position cells. Every cell first
  asserts the produced semantic fact and then the complete schema. G2 stage 2 did not land because
  it is coupled to a viable B2 `Transform::ALL` design, and B2 was abandoned after two LOC-gate
  failures.
- Behavior-bearing adjudication: every landed Part-A/S-A flip family is individually recorded in
  its dossier with Helm 4.2.3 evidence; representation rounds preserve exact schema and symbolic-IR
  bytes. Neither rejected E-spike adopted a fixture or acceptance change.
- Public/wire decisions: the S-D fragment API narrowing and every scheduled B4a public typed-path
  narrowing are recorded in their round dossiers. All serialized contract documents and schema
  fixtures retain their legacy bytes.
- Helm reproducibility: Helm 4.2.3 is pinned in `mise.toml`/`mise.lock` by `82f0ea11`; the live
  battery version guard prevented a transient 4.2.4 upgrade from contaminating adjudication.

### Final close-out gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy tests pass after an
  11-minute-43-second macOS build/scan.
- `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across Linux, Windows GNU,
  and macOS pass with zero errors or warnings in 2,041.44 seconds.
- `cargo nextest run --workspace`: exit 0; 1,310/1,310 tests pass, one slow, in 279.331 seconds
  after compilation.
- `task test:integration`: exit 0; 559/559 tests pass, 24 skipped, 22 slow, in 2,188.437 seconds.
- `task test:all`: exit 0; 1,873/1,873 tests pass, 24 skipped, 26 slow, including live-network
  fetch tests, in 2,292.569 seconds.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; release build finishes in 44.23
  seconds and replaces `/Users/roman/.cargo/bin/helm-schema`.
- downstream luup2 `check:local` with `/private/tmp/helm-schema-xargs-shim`, prefixed `PATH`, and
  `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,721 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

### Wave-2 handoff

- Resume at B3, the next item in the frozen suggested order and the first explicitly out-of-scope
  round for this handoff.
- Keep B2/E1 and E2 recorded as abandoned. G2 stage 2 remains coupled to a future B2 design that
  passes the non-positive whole-tree LOC gate; do not revive either spike by layering compatibility
  state onto the current carriers.
- After B3, follow the frozen order: `MergeLayersUse`, S-B normalization-once with B5 profiling,
  B5a/B5b/B5c, E3 spike, E4 study, then C1--C4 and the remaining scoped simplifications.

## Wave 2 decision register

- Frozen plan: `plan/architecture-review-v4.md` at `bb61a78f`; it remains immutable.
- Starting tree: clean `main` at `5d8c6443`.
- Starting production Rust LOC: 64,721.
- Wave scope: B3 remainder; `MergeLayersUse`; S-B normalization-once; B5a--B5c; C2;
  remaining S-D; the requested quick S-B/S-C rounds; then the eligible extended C3, C1, C4,
  remaining S-B, E4 study, activation-DNF spike, and S-E measurement.
- B2/E1 and E2 remain abandoned. No round may restore either representation by layering
  compatibility state onto the current carriers. G2 stage 2 remains coupled to a future viable
  B2 design.
- E3 is blocked by E2's abandonment unless a future ledger entry first supplies a design that
  genuinely avoids the context split and retains the generated requirement-lane suite gate.
- Performance law: the frozen Airflow release-binary baseline is 112.7 seconds CPU / 2:10 wall.
  A fresh same-host measurement is recorded before S-B normalization-once and after every
  performance-bearing round.
- Helm adjudicator: pinned Helm 4.2.3. Candidate-accepts/Helm-aborts allowance remains zero.
- Public API and wire-format changes are decisions recorded in their owning dossiers, never
  incidental fallout.

## B3.1 — make faithfulness a decoder result

- Status: landed in `ef71d425` (`refactor(ir): unify condition fidelity decoding`).
- Contract: representation-only. Replace the separate faithfulness table with one condition
  decoder returning `Decoded::{Exact, Approximate}` together with the predicate it produced.
  `condition_lowering_is_faithful`, control-flow usability, and ordinary predicate lowering become
  projections of that same exhaustive decoder, so a call shape cannot gain a predicate without the
  same arm deciding its fidelity. Preserve every exact/approximate boundary and serialized order.
- Acceptance baseline: `5d8c6443`.
- Baseline production Rust LOC: 64,721.
- Current-tree audit:
  - A4 fully closed B3's requirement-kind bullet. The deleted
    `requirements_allow_runtime_kind` and `requirement_admits_runtime_type` have no callers;
    `admitted_json_value_kinds` owns JSON-kind domain admission and
    `integer_range_constraint` separately owns value-sensitive integer-count bounds.
  - The remaining faithfulness split is live in
    `value_path_context/condition_predicate.rs`: `condition_lowering_fidelity` classifies the same
    expression forms that `condition_predicate` lowers, including separately recursive `and`,
    `or`, and `not` paths.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - `MergedLayers` and `FirstTruthy` remain exact only when their composite decoder proves the
    result; control-flow lowering retains its existing positive-polarity tolerance where the exact
    fail-negation lane abstains.
  - Every `not` path negates only an exact child. Approximate predicates and positive-only sound
    subsets remain non-negatable.
  - Helper literal dispatch, pipeline membership, `Files.Get` formatting, stringification, merge,
    default, coalesce, and root-field paths retain their present truth tables.
  - If the consolidation exposes a disagreement between the twin tables, stop before fixture
    adoption and classify it as a separately pre-registered behavior-bearing repair. No fixture
    change is accepted in this round.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero.
- Public/wire decision: none expected. The decoder and its fidelity carrier are crate-private;
  serialized predicates and schemas must remain byte-identical.

- Measured results:
  - `Decoded::{Exact, Approximate}` now carries the produced predicate and, for an approximation,
    whether the positive control-flow lane may still consume it. Exact fail negation, control-flow
    usability, and ordinary condition lowering are projections of this one decoder.
  - The former `ConditionFidelityUse`, `condition_lowering_fidelity`,
    `field_condition_lowering_fidelity`, and the separate top-level `condition_predicate` dispatch
    table are deleted. Expression-family helpers split the exhaustive decoder without duplicating
    its classification rules.
  - The decoder deliberately preserves two output positions as dataflow, not as twin semantic
    tables: a top-level condition prefers an exactly evaluated truth predicate, while an operand
    nested inside `and`, `or`, or `not` retains the existing structural operand projection. Both
    positions share the same fidelity classification.
  - A4's requirement-kind consolidation is confirmed complete and unchanged. No B3 requirement-kind
    code was added.
  - The final3 schema dump writes 84 artifacts. All 60 corpus/lean artifacts shared with the B4b
    baseline are byte-identical, and the additional 24 focused resource/final-output artifacts pass
    their committed full-equality fixtures. All 18 symbolic-IR artifacts are byte-identical.
  - The final3 full-depth comparison checks 121,055 probes across 60 charts with zero acceptance
    flips. Mandatory base coverage is 112,260/112,260 and third-level coverage is 7,465/7,465, both
    with zero drops. It records 427 guard pairs, 238 composite pairs, 35,428 bounded guard-witness
    reductions, 2,277 bounded composite reductions, and 28,868 disclosed total drops.

- Deviations:
  - The first compiler preflight failed the 100-line function lint: the unified decoder was 199
    lines. It was split by expression family with no lint suppression and no second classification
    table; the focused Clippy rerun passed.
  - The rejected final1 archive changed Bitnami Redis and Cilium schema ordering and admitted an
    extra Cilium resource-quota branch. The cause was a real pre-existing positional distinction:
    `condition_predicate_expr` prefers an exact evaluated truth predicate at a top-level condition,
    while nested `condition_predicate` operands historically use the structural call decoder. The
    initial refactor applied the top-level preference recursively and therefore was not
    representation-only.
  - No final1 fixture, dump, or prober result was adopted. The corrected decoder carries the
    position explicitly, shares fidelity classification across both positions, and a focused
    Bitnami Redis/Cilium dump became byte-exact before the final2 archive was built.
  - The final self-adversarial review rejected final2 as the authoritative tree because the generic
    `Decoded<T>` carrier had only one concrete use. The final3 tree makes `Decoded` concrete over
    `Option<Predicate>`, deleting an unnecessary type parameter without changing behavior. No
    final2 artifact or gate is used as final evidence.
  - The final3 dump filter executes focused resource/final-output tests that the retained B4b dump
    directory did not contain, so the candidate directory has 84 files against 60 baseline files.
    Equality was checked over every shared artifact; the 24 additional artifacts were independently
    checked by their full-fixture tests.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` is selected by the committed mise pin and accepted by the battery version
    guard.
  - The final3 battery reports zero flips and zero candidate-accepts/Helm-aborts cells. No fixture
    or acceptance change required adoption.

### Producer and route coverage

| Route | Single-owner result | Verification |
|---|---|---|
| Literal, field, selector, and variable truth | Decoder returns predicate plus exact/control fidelity | Focused 41-test condition suite and full IR suite. |
| `and` / `or` | One junctor decoder combines child fidelity while preserving operand projection | Nested junctor controls, Cilium and Bitnami Redis byte dumps. |
| `not` | Exact only when its sole child and negated projection are exact | Negated include, membership, equality, type, and simple-path controls. |
| Atomic calls | One exhaustive call dispatcher selects the exact decoder or explicit truthy fallback | Default, merge, coalesce, `Files.Get`, stringification, pattern, type, and membership suites. |
| Pipeline calls | One exact-candidate chain owns default, stringification, and membership | Pipeline and helper behavior suites plus corpus identity. |
| Composite values | `MergedLayers` / `FirstTruthy` exactness comes from their composite decoder | Exact and undecodable composite controls. |

### Review dossier

- Focused proof: `cargo clippy -p helm-schema-ir --all-targets --all-features -- -D warnings`;
  exit 0 after the rejected long-function preflight. `cargo nextest run -p helm-schema-ir`; exit 0,
  393/393 tests pass.
- Immutable build: final3 archive under the absolute step-local build `TMPDIR`; exit 0 after 11m14s,
  87 binaries and 125 files archived to `/private/tmp/arch-v4-b31-final3.tar.zst`.
- Clean schema dump: final3 archive under the step-local schema `TMPDIR`; exit 0, 62 tests pass in
  239.543 seconds and 84 artifacts are written. Every final2 and shared B4b artifact is
  byte-identical.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one corpus test passes
  in 4.105 seconds and all 18 artifacts are byte-identical to final2 and B4b.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `5d8c6443`, Helm
  adjudication enabled; exit 0 in 69.296 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: none. `Decoded` and its position are crate-private. Contract documents,
  predicate serialization, schema bytes, and public construction remain unchanged.

### Self-adversarial pass

- A decoder that returned only `Predicate` would erase the exactness boundary, while a decoder that
  returned only fidelity would preserve the twin-table defect. The concrete `Decoded` carrier keeps
  its `Option<Predicate>` abstention state and fidelity in the same exhaustive expression arm.
- The rejected final1 archive proves nested call operands are not interchangeable with top-level
  evaluated truth. The final design names that position in the decoder entry point instead of
  restoring a second function-name classification table.
- Approximate values can be usable in positive control flow without becoming negatable. The
  `usable_for_control` bit exists only on the `Approximate` variant, so exact values cannot disagree
  with control usability and approximate values cannot silently masquerade as exact.
- Whole-tree searches find no `ConditionFidelityUse`, `condition_lowering_fidelity`, or second
  function-name fidelity match. A4's two distinct requirement operations remain the correct
  quantifier split, not unfinished B3 duplication.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy tests pass in 12m45s.
- `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across Linux, Windows GNU,
  and macOS pass with zero errors and warnings in 2,314.06 seconds.
- `cargo nextest run --workspace`: exit 0; 1,310/1,310 tests pass, one slow.
- `task test:integration`: exit 0; 559/559 tests pass, 24 skipped, 22 slow, in 2,182.045 seconds.
- `task test:all`: exit 0; 1,873/1,873 tests pass, 24 skipped, 24 slow, including live-network
  tests, in 2,154.308 seconds.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; release build and replacement complete
  in 22.75 seconds.
- Downstream luup2 `check:local` with `/private/tmp/helm-schema-xargs-shim`, prefixed `PATH`, and
  `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,797 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +76 (64,721 to 64,797). The delta is the typed fidelity carrier
  and position-preserving exhaustive decoder; no LOC promise applies to this ordinary round.

## B3.2 — centralize self-guard classification

- Status: landed in `60cefe48` (`refactor(core): centralize self-guard classification`).
- Contract: representation-only. Make `ConditionalGuard` the single owner of whether a guard is
  self-truthy or self-presence for a target path. Replace the builder, requirement, and generator
  copies with projections of the core methods while preserving each consumer's operation-specific
  restrictions, especially the sole `Not(Absent)` overlay fold. If the formerly parallel tables
  disagree, stop and split the divergence into a behavior-bearing repair.
- Acceptance baseline: `ef71d425` (B3.1).
- Baseline production Rust LOC: 64,797.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Self truthiness remains exactly `Truthy(target)` or `With(target)`.
  - Self presence remains exactly `Not(Absent(target))` or `HasKey(parent, literal_member)` whose
    structurally appended member equals the target. Literal dots, backslashes, and stars in the
    member remain one typed segment.
  - The builder's unconditional overlay fold remains restricted to a sole `Not(Absent(target))`;
    a sole `HasKey` guard does not silently acquire that operation-specific rewrite.
  - Recursive `AllOf` / `AnyOf` presence implication keeps its current quantifiers: any conjunct
    may establish presence, while every disjunct must establish it.
  - Any divergence between old and centralized classification stops the representation round
    before fixture adoption. Candidate-accepts/Helm-aborts allowance and mandatory coverage drops
    remain zero.
- Public/wire decision: additive public Rust API. `ConditionalGuard` is already public, and the two
  read-only classification methods expose semantics previously duplicated by downstream crates.
  No variant, constructor, serde shape, ordering, or wire bytes change.

- Measured results:
  - `ConditionalGuard::is_self_truthy_for` now owns the exact `Truthy` / `With` classification,
    and `ConditionalGuard::is_self_presence_for` owns `Not(Absent)` plus structural `HasKey`
    membership. The IR builder, recursive requirement implication, and generator overlay lowering
    project those methods instead of maintaining three local shape tables.
  - Operation-specific restrictions remain at their operations: the builder and generator still
    require the sole guard to be `Not(_)` before folding a guarded source into an unconditional
    source. Central classification therefore does not promote a sole `HasKey` guard.
  - A public-surface test exercises truthy, with, absent, and opaque `HasKey` members containing a
    dot, backslash, and literal star. All members remain one `ValuesPath` segment.
  - The final1 clean schema dump writes 84 artifacts; every artifact is byte-identical to B3.1.
    The symbolic-IR dump writes 18 artifacts; every artifact is byte-identical to B3.1.
  - The full-depth comparison checks 121,055 probes across 60 charts with zero acceptance flips.
    Mandatory base coverage is 112,260/112,260 and third-level coverage is 7,465/7,465, both with
    zero drops. It records 427 guard pairs, 238 composite pairs, 35,428 bounded guard-witness
    reductions, 2,277 bounded composite reductions, and 28,868 disclosed total drops.

- Deviations:
  - The first focused test command used nextest's default profile. That profile excludes the
    integration-test binary, so it ran zero tests and exited 4. No code or artifact changed. The
    corrected command selected the `integration` profile explicitly and the intended public-
    surface test passed 1/1.
  - No semantic preflight was rejected and no fixture was adopted. The old classifiers agreed on
    every route once each consumer's separate fold restriction was retained.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` is selected by the committed mise pin and accepted by the battery version
    guard.
  - The final1 battery reports zero flips and zero candidate-accepts/Helm-aborts cells. No fixture,
    diagnostic, or acceptance change required adoption.

### Producer and route coverage

| Route | Single-owner result | Verification |
|---|---|---|
| `Truthy(path)` / `With(path)` | Core method owns self-truthiness | Public-surface controls, full IR and schema identity. |
| `Not(Absent(path))` | Core method owns direct self-presence | Public-surface control and requiredness corpus suite. |
| `HasKey(parent, member)` | Core appends one opaque member segment before comparison | Dot, backslash, and literal-star public controls. |
| `AllOf` / `AnyOf` implication | Requirement recursion retains any/all quantifiers around the core leaf classifier | Full IR identity and requiredness re-audit suite. |
| Guarded-source folding | Builder and generator retain their sole-`Not` operation restriction | Full schema identity and downstream 32-chart sweep. |

### Review dossier

- Focused proof: Clippy for `helm-schema-core`, `helm-schema-ir`, and `helm-schema-gen` with all
  targets and features; exit 0. The corrected integration-profile public-surface test passes 1/1.
- Immutable build: final1 archive under the absolute step-local build `TMPDIR`; exit 0 after
  14m33s, 87 binaries and 125 files archived to
  `/private/tmp/arch-v4-b32-final1.tar.zst`.
- Clean schema dump: final1 archive under the step-local schema `TMPDIR`; exit 0, 62 tests pass in
  240.048 seconds and all 84 artifacts are byte-identical to B3.1.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one corpus test passes
  in 4.101 seconds and all 18 artifacts are byte-identical to B3.1.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `ef71d425`,
  Helm adjudication enabled; exit 0 in 70.038 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: the two read-only methods are an additive public Rust API on the already
  public guard type. Serde, ordering, variants, constructors, and wire bytes are unchanged.

### Self-adversarial pass

- Moving only the leaf match while broadening the overlay fold would have made a `HasKey` guard
  unconditional. Both fold sites retain their explicit `Not` shape check, and byte identity pins
  the behavior.
- Comparing encoded path strings would have reintroduced the raw-path protocol removed in B4a.
  `HasKey` classification clones the typed parent and pushes the member as one structural segment.
- The core method does not recurse through `AllOf` / `AnyOf`; those quantifiers answer a different
  implication question and remain in the requirement consumer. This keeps one owner per semantic
  question instead of moving unrelated policy into a convenience method.
- Whole-tree searches leave no local `Truthy` / `With` / `HasKey` / `Not(Absent)` self-scope table
  in the builder or generator. Remaining shape matches implement operation-specific restrictions,
  not a parallel identity classifier.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy tests pass in 12m32s.
- `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across Linux, Windows GNU,
  and macOS pass with zero errors and warnings in 2,955.44 seconds.
- `cargo nextest run --workspace`: exit 0; 1,310/1,310 tests pass, one slow, in 241.391 seconds
  after the final-tree build.
- `task test:integration`: exit 0; 560/560 tests pass, 24 skipped, 21 slow, in 1,981.664 seconds.
- `task test:all`: exit 0; 1,874/1,874 tests pass, 24 skipped, 22 slow, including live-network
  tests, in 2,094.453 seconds.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; release build and replacement complete
  in 25.46 seconds.
- Downstream luup2 `check:local` with `/private/tmp/helm-schema-xargs-shim`, prefixed `PATH`, and
  `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,793 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: -4 (64,797 to 64,793). The delta deletes parallel consumer
  classifiers after adding the core owner and its public-surface proof; no LOC promise applies.

## MergeLayersUse — validate layered-use identity

- Status: landed in `312da1dc` (`refactor(core): validate merge-layer uses`).
- Contract: representation-only. Replace the parallel `layers` / `transforms` vectors with ordered
  `MergeLayer { path, transform }` entries, validate the own-layer position at construction and
  deserialization, and make the carrier's fields private. Delete the shadow-prefix clamp and the
  missing-transform `Identity` fallback. Preserve legacy serialization and every merge selection,
  shadow, binding, and path-remapping behavior byte-for-byte.
- Acceptance baseline: `60cefe48` (B3.2).
- Baseline production Rust LOC: 64,793.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Every production construction site already derives `position` from enumeration over the same
    flattened layer list; expected invalid-state count is zero.
  - `layers.len() == transforms.len()` and `position < layers.len()` become construction and serde
    invariants. Invalid public or wire inputs are rejected instead of clamped, defaulted, or partly
    interpreted.
  - Ordered precedence remains unchanged: `shadowed_by` yields exactly the entries before the own
    position; `own_transform` is the transform paired with the own path; later layers remain lower
    precedence.
  - Binding-carried identity-only merges retain ordinary routing. Any structurally transformed
    layer retains layered routing, and helper-summary propagation still marks the carrier as
    binding-owned.
  - Values-path remapping visits every paired layer path once without changing its transform or
    position.
  - Any observed invalid production state makes this round behavior-bearing and stops it for
    individual adjudication. Candidate-accepts/Helm-aborts allowance and mandatory coverage drops
    remain zero.
- Public/wire decision: intentional public Rust API narrowing. Direct `MergeLayersUse` field
  construction and mutation are replaced by a validating constructor and read-only accessors;
  `MergeLayer` is the public paired entry type. The existing serialized object keys, field order,
  values, and accepted valid documents remain byte-identical; invalid parallel-vector documents
  become explicit deserialization errors.

- Measured results:
  - `MergeLayer` now pairs each values path with its transform. `MergeLayersUse` owns a private
    ordered vector, checked own-layer position, cached validated own transform, and binding origin.
    Its constructor and custom deserializer reject empty/out-of-range positions and unequal legacy
    path/transform vector lengths.
  - `shadowed_by` iterates the exact prefix before the checked own position. `own_transform` no
    longer defaults to `Identity`, and generator lowering consumes each earlier layer's paired
    transform directly instead of looking it up in a second vector.
  - Every production constructor derives the position while enumerating the same flattened layer
    list. No invalid construction occurred in focused tests, corpus generation, the full-depth
    battery, or the downstream sweep.
  - Custom serialization preserves the legacy `layers`, `position`, `transforms`, `via_binding`
    object keys and field order. A public-surface test pins exact JSON bytes, valid round-trip,
    unequal-vector rejection, and out-of-bounds rejection.
  - The final1 clean schema dump writes 84 artifacts; every artifact is byte-identical to B3.2.
    The symbolic-IR dump writes 18 artifacts; every artifact is byte-identical to B3.2.
  - The full-depth comparison checks 121,055 probes across 60 charts with zero acceptance flips.
    Mandatory base coverage is 112,260/112,260 and third-level coverage is 7,465/7,465, both with
    zero drops. It records 427 guard pairs, 238 composite pairs, 35,428 bounded guard-witness
    reductions, 2,277 bounded composite reductions, and 28,868 disclosed total drops.

- Deviations:
  - The first schema-dump invocation used the required absolute `TMPDIR` before creating that
    directory. Archive extraction exited 96 without running a test or writing an artifact. After
    creating the step-local directory, the same immutable archive produced the sole adopted dump.
  - The round was committed at the user's requested pause after its exact-tree immutable evidence,
    `cargo fmt`, whole-workspace lint, feature-combination lint, LOC, frozen-plan, and diff gates had
    passed. The exact-tree workspace suite was interrupted during compilation and deliberately
    terminated; no result from that run is counted.
  - Before the deferred gates resumed, the independent unspaced-pipe grammar hotfix and v0.0.7 bump
    landed in `abb753c2` through `7fa6dd99`. The remaining unit, integration, live-network, install,
    and downstream gates therefore ran on cumulative HEAD `7fa6dd99`, not on the exact
    `312da1dc` tree. This is an explicit user-directed exception to the normal final-tree gate
    discipline. The exact round's immutable schema/IR/prober evidence remains isolated at
    `312da1dc`; the cumulative gates exercise that code plus the separately reviewed hotfix.
  - No invalid merge-layer state, semantic preflight rejection, fixture adoption, or behavior
    repair was needed.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` is selected by the committed mise pin and accepted by the battery version
    guard.
  - The final1 battery reports zero flips and zero candidate-accepts/Helm-aborts cells. No fixture,
    diagnostic, or acceptance change required adoption.

### Producer and route coverage

| Route | Validated owner | Verification |
|---|---|---|
| Abstract-value output metadata | Paired layers built from the same flattened identity enumeration | Focused IR tests and 18-file IR identity. |
| Fragment splice lowering | Paired transform list shared by every enumerated position | Fragment and merge-shadowing suites plus schema identity. |
| Helper-summary propagation | `into_via_binding` changes only the origin bit | Binding-carried identity and transformed merge corpus controls. |
| Contract path remapping | Core mutates each paired layer path while retaining transform and position | Public contract path-remapping regression. |
| IR row routing | Accessors own own-path, transformed-layer, and binding decisions | Contract synthesis tests and full IR identity. |
| Generator shadow lowering | Earlier layer path and transform come from one `MergeLayer` | Merge-shadowing fixtures and 121,055-probe equality. |
| Legacy wire input/output | Custom serde validates on input and preserves exact valid bytes | Public-surface byte and rejection controls. |

### Review dossier

- Focused proof: Clippy for `helm-schema-core`, `helm-schema-ir`, and `helm-schema-gen` with all
  targets and features; exit 0 in 3m09s. Seven core public-surface tests, four focused IR contract
  tests, and three provider-requirement tests pass.
- Immutable build: final1 archive under the absolute step-local build `TMPDIR`; exit 0 after
  13m59s, 87 binaries and 125 files archived to
  `/private/tmp/arch-v4-merge-final1.tar.zst`.
- Clean schema dump: final1 archive under the step-local schema `TMPDIR`; exit 0, 62 tests pass in
  238.619 seconds and all 84 artifacts are byte-identical to B3.2.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one corpus test passes
  in 4.224 seconds and all 18 artifacts are byte-identical to B3.2.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `60cefe48`,
  Helm adjudication enabled; exit 0 in 97.842 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,868 disclosed reductions.
- Public/wire decision: direct public field construction is intentionally removed so invalid
  parallel state cannot cross the API. Valid legacy JSON retains exact bytes and round-trips;
  invalid legacy JSON now returns a serde error.

### Self-adversarial pass

- Keeping public vectors with a validating helper would preserve the illegal state after
  construction. Private fields make the constructor and deserializer the only creation boundaries.
- Returning `Option` from `own_transform` would force every consumer to invent an invalid-state
  policy. Validation caches the selected transform, so downstream reads are total without a panic,
  clamp, or semantic fallback.
- Pairing only at generator consumption would leave the IR wire carrier and builder vulnerable to
  drift. The paired entry is the core representation and both producer families construct it.
- Custom serde is compatibility work, not a second semantic representation: the parallel vectors
  exist only within the serialization edge and are immediately zipped or rejected.
- Whole-tree searches leave no production `transforms` field, position clamp, or missing-transform
  `Identity` fallback. Remaining `MergeLayersUse` construction flows through `new`.

### Gates

- Exact `312da1dc` tree, `cargo fmt --check`: exit 0.
- Exact `312da1dc` tree, `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy
  tests pass in 9m21s.
- Exact `312da1dc` tree, `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across
  Linux, Windows GNU, and macOS pass with zero errors and warnings in 2,837.12 seconds.
- Cumulative `7fa6dd99` tree, `cargo nextest run --workspace`: exit 0; 1,322/1,322 tests pass, one
  slow, in 251.928 seconds.
- Cumulative `7fa6dd99` tree, `task test:integration`: exit 0; 565/565 tests pass, 24 skipped and 21
  slow, in 2,380.329 seconds.
- Cumulative `7fa6dd99` tree, `task test:all`: exit 0; 1,891/1,891 tests pass, 24 skipped and 27
  slow, including live-network tests, in 2,338.903 seconds.
- Cumulative `d77a6372` tree, `cargo install --path ./crates/helm-schema-cli/`: exit 0; v0.0.7
  release build and replacement complete in 29.93 seconds.
- Cumulative `d77a6372` tree, downstream luup2 `check:local` with the macOS shims, prefixed `PATH`,
  and `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- Exact `312da1dc` tree, `task tokei:core`: exit 0; 64,881 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0 on both the exact and
  cumulative trees.
- `git diff --check`: exit 0 on the exact tree; rerun after this ledger-only closure below.

- Measured production LOC delta: +88 (64,793 to 64,881). The delta is the paired public carrier,
  validating/accessor surface, and byte-compatible serde edge; no LOC promise applies.

## S-B — normalize the contract once

- Status: landed in `6dee3291` (`refactor(ir): normalize contracts once`).
- Contract: representation-only and performance-bearing. Give contract normalization one owner
  with explicit form transitions: raw DNF rows expand once into single-conjunction rows; primary and
  dependency subsumption, append, pathless-resource merging, and merge-source rebasing operate on
  that expanded form; path mapping is re-canonicalized; only the final boundary compacts rows back
  into DNF. Store the resulting `ContractDocument` in `FinalizedContract` instead of rebuilding and
  re-normalizing it for each request. Preserve exact IR, schema, diagnostic, and serialized order.
- Acceptance baseline: `ad2b3c68` (MergeLayersUse dossier closure, after the independent v0.0.7
  grammar hotfix and its tooling cleanup).
- Baseline production Rust LOC: 64,941.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Every row entering subsumption, append, pathless-resource merging, or merge-source rebasing has
    exactly one conjunction. No operation may compact early and force a later re-expansion.
  - Primary uses retain the existing default/self-truthy/pathless-resource sequence. Dependency
    uses retain their separate self-truthy pass before append. Cross-source default and self-truthy
    subsumption still run after append.
  - `merge_pathless_resource_variants` remains before the second primary self-truthy pass; its
    post-merge deduplication effect is load-bearing.
  - Merge-source rebasing may make formerly distinct rows equal. Re-canonicalization immediately
    after the path map remains mandatory before final compaction.
  - Final compaction preserves the current `GuardDnf` order, complement absorption, provenance
    union, and render-site grouping exactly.
  - `FinalizedContract::document` returns a clone of the once-built versioned document. It must not
    expose mutable state or change the public wire format.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero. Any acceptance or fixture change stops the representation round before adoption.
- Public/wire decision: no public API or wire change. `FinalizedContract`'s private storage changes
  from normalized uses to a `ContractDocument`; `uses()` and `document()` retain their signatures
  and exact bytes.
- Performance baseline:
  - Frozen campaign baseline: 112.7 seconds CPU / 2m10s wall for the installed release binary.
  - Fresh pre-round run on `ad2b3c68`-equivalent v0.0.7 production code:
    `/usr/bin/time -p helm-schema --k8s-version 1.31.0 testdata/charts/airflow`, with schema stdout
    written to `target/arch-v4-sb-baseline/airflow.schema.json`; exit 0, 221.37 seconds wall,
    219.87 seconds user, 0.91 seconds system (220.78 seconds CPU).
  - Host caveat: load average was 3.57 / 3.77 / 4.69 after the only stale analyzer worker was gone;
    WindowServer and browser processes remained active. Per user direction, load is recorded but
    does not pause or invalidate subsequent same-host measurements.
  - The first timing preflight used `/usr/bin/time -lp`; the generator completed in 222.83 seconds
    wall / 221.63 seconds CPU, but the wrapper exited 1 after a sandbox-blocked
    `sysctl kern.clockrate`. That run is not the authoritative baseline.

- Measured results:
  - `normalize_contract_uses` now owns the complete production transition from raw primary and
    dependency DNF rows to expanded rows, primary normalization, dependency normalization, append,
    cross-source subsumption, merge-source rebasing, post-map re-canonicalization, and one final DNF
    compaction.
  - `drop_default_guard_subsumed_duplicates` and `drop_self_truthy_subsumed_duplicates` no longer
    defensively re-expand the vector. `ContractIr::finalize` delegates the whole sequence once
    instead of invoking four canonicalization/compaction cycles around append and rebasing.
  - `canonicalize_expanded_contract_uses` performs one rank-assisted sort and preserves the legacy
    full-row winner as its final tie-break. `compact_contract_uses` consumes that order without a
    second sort and unions only at the final boundary.
  - Merge-source rebasing and its requirement-index helpers moved beside the normalization owner.
    The graph module now supplies the two row sets and fail captures rather than coordinating phase
    transitions itself.
  - `FinalizedContract` stores one `ContractDocument`; `uses()` borrows its rows and `document()`
    clones the already-versioned artifact without another canonicalization pass.
  - Whole-tree production search finds `expand_condition_disjuncts` only in the normalization
    owner (once per initial row set) and in the raw expert `ContractDocument::from_contract_uses`
    compatibility path. The finalized hot path never re-enters that raw constructor.
  - The final2 clean schema dump writes 84 artifacts; every artifact is byte-identical to the
    pre-round baseline. The symbolic-IR dump writes 18 artifacts; every artifact is byte-identical.
  - The final2 full-depth comparison checks 121,055 probes across 60 charts with zero acceptance
    flips. Mandatory base coverage is 112,260/112,260 and third-level coverage is 7,465/7,465, both
    with zero drops. It records 427 guard pairs, 238 composite pairs, 35,428 bounded guard-witness
    reductions, 2,277 bounded composite reductions, and 28,856 disclosed total drops.
  - Clean corpus dump wall time improves from 238.274 seconds to 234.805 seconds (-3.469 seconds,
    -1.46%). Final2 Airflow measures 229.70 seconds wall / 229.27 seconds CPU at load average
    6.14 / 6.54 / 6.64, versus 221.37 / 220.78 at baseline load 3.57 / 3.77 / 4.69. The whole-chart
    signal is noisy and +3.85% CPU; no speedup is claimed.

- Deviations:
  - Final1 passed focused IR tests, 84-file schema identity, 18-file IR identity, and the full-depth
    battery, but the complete unit gate exposed an order-sensitive provider-cache regression:
    `provider_schema_cache_distinguishes_stringified_scalar_uses` failed while the other 1,321 tests
    passed. Removing the preliminary full-row sort had left equal render-site/condition rows in
    caller order before their row-scoped flags collapsed.
  - No final1 artifact or performance result is adopted. The repair adds the old full `ContractUse`
    ordering as the last tie-break of the single rank-assisted sort, preserving the deterministic
    winner without restoring a preliminary sort or extra normalization pass. The focused cache test
    passes, and final2 rebuilt every authoritative artifact from that corrected tree.
  - `task lint` reports two pre-existing AST-grep warnings in the independent grammar hotfix tests
    for explicit LF/CRLF escape spellings. The command's own exit is 0; S-B does not alter those
    tests or lint policy.
  - The fresh Airflow baseline is roughly twice the frozen 112.7-second CPU observation. Host load,
    current cache/network state, and the independent grammar release differ from the frozen run, so
    the ledger preserves both measurements rather than treating the old number as reproducible.
  - The normalization round meets its representation and work-reduction contract but does not
    produce a measurable whole-chart Airflow improvement under the recorded host load. Per the wave
    contract, wall-clock is evidence rather than an adoption threshold for this ordinary round.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` is selected by the committed mise pin and accepted by the battery version
    guard.
  - The final2 battery reports zero flips and zero candidate-accepts/Helm-aborts cells. No fixture,
    diagnostic, ordering, or acceptance change required adoption.

### Producer and route coverage

| Phase | Final form and owner | Verification |
|---|---|---|
| Raw primary rows | Expand once to one conjunction per row | Normalization controls and 18-file IR identity. |
| Raw dependency rows | Expand once, then dependency-local self-truthy subsumption | Dependency/global projection suites. |
| Primary normalization | Default → self-truthy → pathless merge → self-truthy | Full contract normalization and corpus identity. |
| Append | Both inputs remain expanded | Cross-source default/self-truthy controls. |
| Merge-source rebase | Expanded paths map, then re-canonicalize immediately | Recursive merge-source tests and provider-cache order control. |
| Final compaction | One render-site union into stable `GuardDnf` | Contract document equality and all serialized IR artifacts. |
| Inspection document | Constructed once inside `FinalizedContract` | Public session/document deterministic tests. |

### Review dossier

- Focused proof: `cargo clippy -p helm-schema-ir --all-targets --all-features -- -D warnings`;
  exit 0. `cargo nextest run -p helm-schema-ir`; exit 0, 393/393 tests pass. The isolated
  provider-cache order regression passes after the final2 tie-break repair.
- Baseline immutable archive: 90 binaries and 128 files at
  `/private/tmp/arch-v4-sb-baseline.tar.zst`. Its clean schema dump passes 62 tests in 238.274
  seconds; its IR dump passes in 4.138 seconds.
- Final immutable build: final2 archive under the absolute step-local build `TMPDIR`; exit 0 after
  27.56 seconds of compilation and 1.83 seconds of archiving, with 90 binaries and 128 files at
  `/private/tmp/arch-v4-sb-final2.tar.zst`.
- Clean schema dump: final2 archive under the step-local schema `TMPDIR`; exit 0, 62 tests pass in
  234.805 seconds and all 84 artifacts are byte-identical to baseline.
- Clean IR dump: the same archive under the step-local IR `TMPDIR`; exit 0, one corpus test passes
  in 4.152 seconds and all 18 artifacts are byte-identical to baseline.
- Full-depth proof: the same archive under the step-local prober `TMPDIR`, baseline `ad2b3c68`,
  Helm adjudication enabled; exit 0 in 71.959 seconds, 60 charts, 121,055 probes, zero flips, zero
  unallowed accepted-abort cells, zero mandatory drops, and 28,856 disclosed reductions.
- Public/wire decision: none. The private `FinalizedContract` storage changes; public methods,
  contract version 3, serialized field order, row order, and schema signals remain exact.

### Self-adversarial pass

- Compacting primary or dependency rows before append recreates the repeated-expansion profile and
  can change subsumption quantifiers. The owner keeps both sets expanded until every path-changing
  operation is complete.
- Omitting the post-rebase canonicalization leaves duplicate projected paths and changes provenance
  grouping. The map is immediately followed by the expanded-form canonicalizer before compaction.
- Removing the full-row tie-break makes row-scoped flags caller-order dependent even when corpus
  fixtures happen to be stable. The rejected final1 unit gate exposed this exact hidden invariant;
  final2 preserves it inside the one required sort.
- The public raw `ContractDocument::from_contract_uses` constructor still canonicalizes arbitrary
  caller rows. Finalized production code uses the private normalized constructor, so compatibility
  does not reintroduce hot-path work.
- Caching a document without deriving schema signals from the same stored rows would create two
  owners. `FinalizedContract::new` builds the document first and passes `document.uses` directly to
  signal derivation.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy tests pass in 12.35
  seconds. Two disclosed pre-existing AST-grep scan warnings remain outside this round.
- `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across Linux, Windows GNU,
  and macOS pass with zero compiler errors or warnings in 74.95 seconds; the same two independent
  AST-grep scan warnings follow the matrix.
- `cargo nextest run --workspace`: exit 0; 1,322/1,322 tests pass, one slow, in 232.438 seconds.
- `task test:integration`: exit 0; 565/565 tests pass, 24 skipped and 21 slow, in 1,929.932 seconds.
- `task test:all`: exit 0; 1,891/1,891 tests pass, 24 skipped and 22 slow, including live-network
  tests, in 2,037.176 seconds.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; release replacement completes in 3.31
  seconds on the final tree.
- Downstream luup2 `check:local` with the macOS shims, prefixed `PATH`, and
  `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 64,957 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +16 (64,941 to 64,957). The graph loses its normalization
  sequencing and merge-source helper block; the normalization owner gains explicit phase code,
  comments, and the cached document constructor. No LOC promise applies.

## B5a — canonical conjunctions and shared predicate nodes

- Status: recorded and abandoned after the representation-only byte gate failed repeatedly; no
  production or test change landed.
- Contract: representation-only. Introduce one canonical `Conjunction` owner that sorts,
  deduplicates, flattens nested `And` predicates, and removes `True` at construction. Replace the
  raw predicate vectors that represent fail and implication conjunctions, deleting their manual
  canonicalization. Store immutable predicate nodes behind private `Arc`s; use a cached structural
  hash only to reject unequal nodes quickly, while manual ordering exactly preserves the former
  derived-enum order used by `GuardDnf` and serialized fixtures.
- Acceptance baseline: `6dee3291`.
- Baseline production Rust LOC: 64,957.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
  - Predicate structural order remains exactly `True`, `False`, `Approximate`, `Guard`, `Not`,
    `And`, `Or`, with each variant retaining its former derived field and recursive ordering.
    Cached hashes never participate in `Ord` or serialization.
  - Canonical conjunction construction may remove only nested `And` wrappers, `True`, and exact
    duplicate predicates. It preserves `False`, `Or`, approximation markers and sound subsets,
    context-marker guards, and every nontrivial predicate in former structural order.
  - Ordered predicate stacks whose order represents evaluation, branch priority, or provenance
    remain ordinary vectors. Only set-like logical conjunction carriers migrate.
  - `FailCapture.conjunction`, member-host implication predicates, selected-string implication
    predicates, and their parser/normalization handoffs use the new carrier. Any audited raw vector
    that is not provably a conjunction stays unchanged.
  - Public API decision: `Predicate` becomes opaque at ownership while keeping an exhaustive
    borrowed view. This is an intentional Part-F narrowing; `GuardDnf` wire bytes remain exact.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero. Any acceptance flip or fixture-byte difference rejects the representation round before
    fixture adoption.

- Measured results:
  - The compiler-driven candidate made `Predicate` a private `Arc`-backed node with cached
    structural hash, manual `Eq`/`Ord`/`Hash`, and an exhaustive borrowed kind. Focused tests proved
    the former seven-variant order and canonical conjunction construction.
  - `Conjunction` replaced `FailCapture.conjunction`, member-host predicates, selected-string
    predicates, parser selection conjunctions, and route handoffs. The former sort-plus-dedup pairs
    disappeared from contract normalization, input-channel routing, two requirement paths, two
    member-host paths, and parser selection.
  - Focused final-candidate Clippy for core and IR exits 0; 434/434 focused core/IR tests pass.
    Symbolic-IR corpus output remains byte-identical across all 18 artifacts.
  - The schema byte gate is irreconcilable at the present boundary. A canonical flattened
    presentation makes Jenkins byte-identical but changes Bitnami PostgreSQL's repeated-definition
    grouping. Retaining the raw producer presentation restores Bitnami PostgreSQL but changes
    Jenkins definition allocation. Hybrid candidates reduced the full-corpus difference from 12
    artifacts to four, but never to zero.
  - Every differing schema retained the same tested behavior in focused generation; the observed
    drift is repeated-subtree grouping and `$defs` allocation. Representation-only acceptance is
    nevertheless byte identity, so none of those states is adoptable and no fixture was touched.
  - All candidate production/test changes were restored from the saved reverse patch. Production
    LOC is again 64,957 and the source tree is byte-identical to `6dee3291` outside this ledger.

- Deviations:
  - The first opaque-node compile preflight produced 316 expected pattern-match errors across the
    exhaustive predicate reader surface. The migration was completed mechanically, then audited by
    the compiler; no public pattern-matching compatibility facade was retained.
  - Rejected full archive `final1`: 62/62 dump tests pass in 189.375 seconds and 84 artifacts are
    written, but 12 schema artifacts differ from the acceptance baseline.
  - Rejected full archive `final2`: 62/62 pass in 189.838 seconds; explicit legacy capture ordering
    does not change the same 12-artifact failure.
  - Rejected full archive `final3`: 62/62 pass in 188.834 seconds; separating canonical identity
    from producer presentation restores Jenkins and Traefik, but ten artifacts still differ.
  - Rejected full archive `final8`: 62/62 pass in 189.814 seconds; retaining raw logical grouping
    restores eight more charts, but Cilium, Jenkins, Kyverno, and Traefik still differ.
  - Targeted discriminator preflights `final4` through `final14` tested raw order, top-level
    sort/dedup, flattened order, `True` elision, exact-only flattening, and canonical-vs-presentation
    capture identity. Bitnami PostgreSQL and Jenkins require opposite presentation choices. No
    artifact from any targeted run was adopted.
  - The attempted byte-compatibility lane required a second producer-order vector beside the
    canonical conjunction. It reproduced the E2 finding: `$defs` grouping is coupled to legacy
    capture shape. Keeping that vector would violate the wave's no-compatibility-state lesson even
    if another chart-specific ordering heuristic were added.

- Adjudication evidence:
  - No acceptance cell was eligible for adoption because fixture identity failed first. The
    full-depth prober was therefore not run on a rejected code state, and no generated fixture was
    copied or modified.
  - Helm remains pinned at 4.2.3. Candidate-accepts/Helm-aborts allowance remains zero for the next
    viable design.

### Producer and route coverage

| Route | Candidate result | Disposition |
|---|---|---|
| Direct and guarded fail captures | Canonical membership works; raw grouping affects `$defs`. | Reverted. |
| Selected string requirements | One conjunction carrier removes the route-vector sort pairs. | Reverted. |
| Member-host implications | Canonical construction removes both local sort/dedup sites. | Reverted. |
| Parser selection branches | Canonical construction removes branch-local sorting. | Reverted. |
| Contract normalization/routes | Canonical handoffs remove two more manual pairs. | Reverted. |
| Predicate readers | Exhaustive borrowed kind compiles across every production and test reader. | Reverted. |
| Guard DNF ordering | Manual predicate order matches the former derived enum order. | Reverted. |

### Review dossier

- Rejected immutable archives: `/private/tmp/arch-v4-b5a-final1.tar.zst`, `final2`, `final3`, and
  `final8`; none is authoritative or adoption evidence.
- Focused proof before abandonment: affected-crate Clippy exits 0; 434/434 core/IR tests pass; the
  18-file symbolic-IR dump is byte-identical.
- Schema proof: each full archive writes 84 artifacts from 62 passing tests. The best candidate
  still changes four artifacts, so the byte-exact gate fails.
- Public/wire decision: the proposed opaque `Predicate` API narrowing was reviewed and mechanically
  viable, but did not land. The restored public enum API and every wire format remain unchanged.
- Resume prerequisite: first remove or redesign the deterministic `$defs` ordering/grouping
  dependency as its own representation-only, byte-exact round. Then re-run B5a from `6dee3291`.
  B5b/B5c remain blocked because they explicitly profile and optimize the B5a tree.

### Self-adversarial pass

- Treating `$defs` renumbering as harmless would violate the round's explicit byte contract and
  repeat the rejected E2 reasoning. No fixture normalization or regeneration is allowed here.
- Keeping both canonical and producer-order conjunction vectors is a compatibility representation,
  not the deletion promised by B5a. Adding chart-specific ordering rules would make that debt worse.
- The structural predicate order itself is not the fault: focused tests and manual comparison prove
  the exact former variant/field order. The conflict appears only when canonical conjunction shape
  reaches repeated-schema grouping.
- The honest result is therefore a blocker, not a partial B5a commit. C2 and later rounds are not
  started because the wave's stop rule says an unfixable in-scope gate stops the campaign.

### Gates

- Candidate `cargo fmt --check`: exit 0 before every immutable archive.
- Candidate affected-crate Clippy (`helm-schema-core`, `helm-schema-ir`, all targets/features,
  warnings denied): exit 0.
- Candidate focused nextest: exit 0; 434/434 tests pass.
- Candidate symbolic-IR identity: exit 0; all 18 artifacts byte-identical.
- Candidate schema fixture identity: **failed**; best measured state differs in 4/84 artifacts.
- Restored-tree `git diff --exit-code 6dee3291 -- crates`: exit 0.
- Restored-tree `task tokei:core`: exit 0; 64,957 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: 0. The rejected candidate peaked at 65,423 lines (+466), but all
  production and test changes were removed. The next session resumes at the `$defs` ordering
  prerequisite, before B5a; B5b, B5c, C2, and the remaining wave-2 scope are untouched.

## B5 prerequisite — canonical logical-schema fingerprints

- Status: landed in `e6660809` (`fix(minify): canonicalize logical schema definitions`).
- Contract: behavior-bearing output canonicalization. Give schema minimization one logical
  fingerprint for validation-equivalent `allOf` and `anyOf` grouping, ordering, and duplication.
  Definition planning, replacement, and emitted definition bodies consume that same normalized
  shape so `$defs` sharing and names no longer depend on incidental capture/conjunction
  presentation. This is the prerequisite required by the B5a and E2 ordering findings.
- Acceptance baseline: `f2bdee35` (production-equivalent to `6dee3291`).
- Baseline production Rust LOC: 64,957.
- Pre-registered acceptance expectations:
  - Zero corpus acceptance flips and zero candidate-accepts/Helm-aborts cells.
  - Fixture changes are limited to deterministic `$defs` selection, normalized logical-combinator
    grouping/order, rewritten internal references, and the policy fingerprint derived from those
    output bytes. Root properties, defaults, descriptions, runtime kinds, requirements, and
    unreferenced inline schemas remain unchanged.
  - `allOf` and `anyOf` arms are recursively normalized, flattened only through an object whose sole
    validation keyword is the same junctor, sorted by canonical JSON bytes, and deduplicated.
    `oneOf` is not deduplicated because duplicate arms change its validation semantics.
  - Objects carrying annotations, `$id`/anchor scope, unevaluated keywords, or any sibling keyword
    are never flattened through their junctor wrapper. Unsafe reference scopes remain ineligible for
    generated definitions exactly as before.
  - Candidate counting, savings calculation, definition planning, replacement lookup, and emitted
    definition values use one normalized schema; no raw-vs-normalized compatibility map is allowed.
  - Repeated runs on the same input are byte-identical. Reordering or regrouping equivalent
    `allOf`/`anyOf` arms before minimization produces the same final schema bytes.
  - Public/wire decision: the minimized schema's `$defs` names and grouping are intentionally
    canonicalized. This changes generated output bytes but not the public Rust API or accepted
    values language; the exact changed fixture family is adjudicated as this round's output-format
    decision under Part F.
  - Mandatory base and third-level probe drops remain zero. Every changed acceptance cell requires
    Helm 4.2.3 adjudication, and the accepted-abort allowance remains zero.

- Measured results:
  - The minifier now removes the existing root `$defs`, recursively normalizes schema positions,
    and restores the caller-owned definitions unchanged before candidate collection. Generated
    candidates, replacement lookup, and emitted generated definitions consequently share one
    normalized schema tree; there is no raw-versus-normalized compatibility map.
  - `allOf` and `anyOf` normalize bottom-up. A wrapper is flattened only when it is an object whose
    sole key is the same junctor; annotation, reference-scope, evaluation, and other sibling
    boundaries remain intact. `oneOf` arms retain their source multiplicity.
  - Logical arms use a stable structural 128-bit digest for the common ordering path. Equal-digest
    collision buckets compare canonical JSON strings, so a collision can affect neither
    determinism nor deduplication correctness. The digest is an ordering accelerator, not an
    equality or semantic identity oracle.
  - Full equality tests prove that regrouped and reordered equivalent conjunctions minimize to one
    definition and identical complete schema bytes. Separate controls prove annotated wrappers are
    preserved and duplicate `oneOf` arms remain semantically significant.
  - The authoritative clean dump writes 84 artifacts from 62 passing tests. Exactly 60 tracked
    fixtures change: 54 chart-corpus schemas, three lean-profile schemas, and three final-output
    schemas. Generator-only fixtures and the remaining profile/final-output controls stay exact.
    All changes are confined to logical-arm order/grouping, generated-definition selection or
    numbering and dependent internal references, plus the final-output policy fingerprint derived
    from those bytes.
  - The full-depth battery reports zero acceptance flips. Canonical output therefore changes no
    tested accepted-values language and introduces no candidate-accepts/Helm-aborts cell.

- Deviations:
  - Rejected preflight `final1` normalized each candidate independently while fingerprinting. Its
    62/62 dump passed but took 255.183 seconds and repeated normalization work at every occurrence;
    no artifact was adopted.
  - Rejected preflight `final2` moved normalization to the whole generated schema and sorted every
    arm by a cached canonical JSON string. Its 62/62 dump passed in 234.912 seconds, but the
    temporary strings retained avoidable allocation proportional to every logical arm.
  - Rejected preflight `final3` used a cryptographic SHA digest as the sort key. Its 62/62 dump
    passed in 245.678 seconds, but a cryptographic dependency and cost are unnecessary for a stable
    in-process ordering key; no artifact was adopted.
  - Adopted preflight `final4` replaces SHA with an in-module deterministic FNV-1a structural
    digest and canonical-string collision fallback. Its 62/62 clean dump passed in 234.562 seconds,
    effectively flat against S-B's 234.805-second clean dump on the non-idle host. The later
    `final5` archive rebuilds the exact final test tree and is the sole authoritative artifact.
  - The pre-registration described arm ordering by canonical JSON bytes. The adopted implementation
    produces a deterministic digest order instead, with canonical bytes only inside collision
    buckets. This is an intentional implementation-level deviation: byte ordering itself is newly
    owned by this round, while equivalence, stability, and collision correctness are preserved and
    exhaustively tested. It avoids turning serialization allocation into the normal hot path.
  - No fixture from a rejected preflight was adopted. The 60 changed tracked fixtures were copied
    once from the final4 production-equivalent clean dump; final5 independently reproduces every
    adopted byte from the exact final source and test tree.

- Adjudication evidence:
  - Helm `v4.2.3+g43e8b7f` remains selected by the committed mise pin and accepted by the battery
    version guard.
  - The final5 full-depth battery checks 60 charts and 121,061 probes: 112,260/112,260 mandatory
    base probes and 7,465/7,465 mandatory third-level probes are emitted, with zero drops in either
    category. It emits 430 guard witness pairs and 238 composite pairs, while disclosing 52,554
    bounded reductions in capped categories.
  - The battery reports zero flips, so no individual Helm rendering cell required fixture
    adjudication. Helm adjudication was enabled; candidate-accepts/Helm-aborts is 0 against an
    allowance of 0.

### Producer and route coverage

| Route | Canonicalization boundary | Verification |
|---|---|---|
| Root generated schema | Normalize once before candidate collection | Full corpus and repeated-run equality. |
| Existing caller `$defs` | Remove before normalization; restore unchanged | Existing-definition controls and full output fixtures. |
| Candidate identity | Exact canonical JSON of the normalized subtree | Regrouped/reordered full-schema equality. |
| Definition planning | One normalized candidate count and savings input | 60 changed fixtures and minifier unit controls. |
| Replacement lookup | Same normalized schema traversed for fingerprints | Both equivalent properties reference one definition. |
| Emitted generated definitions | Clone from the normalized tree | Definition body has one flat, deduplicated junctor. |
| Annotation and scope wrappers | Never flattened through sibling keywords | Annotated-wrapper and unsafe-reference tests. |
| `oneOf` | Recursed into, never flattened or deduplicated | Duplicate-arm semantic control. |

### Review dossier

- Focused proof: minifier Clippy exits 0; 13/13 minifier tests pass, including the new complete
  schema equality and semantic-boundary controls.
- Final immutable build: absolute step-local build `TMPDIR`; exit 0, 90 binaries and 128 files
  archived to `/private/tmp/arch-v4-def-order-final5.tar.zst` in 2.11 seconds.
- Clean schema dump: the final5 archive under its absolute step-local schema `TMPDIR`; exit 0,
  62/62 tests pass in 230.575 seconds and 84 artifacts are written in one batch. Direct comparison
  of all 56 chart, four lean-profile, and four final-output artifacts against the adopted fixtures
  checks 64/64 exact with zero mismatches.
- Full-depth proof: the same archive under its absolute step-local prober `TMPDIR`, acceptance
  baseline `f2bdee35`, and Helm adjudication enabled; exit 0 in 77.380 seconds, 60 charts, 121,061
  probes, zero flips, zero unallowed accepted-abort cells, and zero mandatory drops.
- Public/wire decision: no Rust public API changes. Generated schema bytes intentionally acquire a
  canonical logical-junctor order and definition allocation; the 60-file fixture update is the
  reviewed wire-format decision. JSON Schema acceptance remains unchanged across the measured
  battery.

### Self-adversarial pass

- Flattening a junctor wrapper with annotations or reference-scope siblings could alter annotation
  collection or base URI semantics. The owner requires the wrapper object to have exactly one key,
  a stricter boundary than trying to enumerate every non-validation keyword.
- Deduplicating `oneOf` arms changes exclusive-choice truth counts. Only `allOf` and `anyOf`, where
  duplicate arms are idempotent, enter the deduplication path.
- A digest alone is not a collision-safe identity. Exact `Value` equality controls deduplication,
  and canonical-string ordering resolves distinct values in the same digest bucket.
- Normalizing caller-owned `$defs` would silently rewrite external identities and bodies. They are
  removed before traversal and restored byte-for-byte; only generated definition allocation is
  newly canonicalized.
- Sorting only during candidate fingerprinting would let planning and emitted bodies observe
  different representations. The schema tree is normalized once before every minifier phase.
- Treating the 60-file rewrite as representation-only would hide a real wire-format decision. This
  prerequisite is explicitly behavior-bearing in output bytes, and its zero-flip battery proves
  acceptance equivalence rather than assuming it.

### Gates

- `cargo fmt --check`: exit 0.
- `task lint`: exit 0; whole-workspace Clippy and all three AST-grep policy tests pass in 21.08
  seconds. Two pre-existing AST-grep scan warnings in the independent grammar tests remain
  disclosed.
- `task lint:fc`: exit 0; 48/48 feature combinations for 13 packages across Linux, Windows GNU,
  and macOS pass in 125.80 seconds; the same two independent scan warnings follow the matrix.
- `cargo nextest run --workspace`: exit 0; 1,325/1,325 tests pass, one slow, in 231.549 seconds.
- `task test:integration`: exit 0; 565/565 tests pass, 24 skipped and 21 slow, in 1,944.505
  seconds.
- `task test:all`: exit 0; 1,894/1,894 tests pass, 24 skipped and 22 slow, including live-network
  tests, in 2,020.632 seconds.
- `cargo install --path ./crates/helm-schema-cli/`: exit 0; the release binary is installed in
  24.26 seconds.
- Downstream luup2 `check:local` with the macOS shims, prefixed `PATH`, and
  `HELM_SCHEMA_BIN=/Users/roman/.cargo/bin/helm-schema`: exit 0; 32/32 charts pass.
- `task tokei:core`: exit 0; 65,055 production Rust lines.
- `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
- `git diff --check`: exit 0.

- Measured production LOC delta: +98 (64,957 to 65,055). The new lines are the single logical-form
  owner and collision-safe stable ordering implementation; no compatibility carrier, alternate
  representation, or call-site adapter remains. No LOC promise applies.

## B5a retry — canonical conjunctions and shared predicate nodes

- Status: first retry rejected before fixture adoption; redirected through the behavior-bearing
  canonical-capture prerequisite below.
- Contract: representation-only. Retry the frozen B5a carrier migration after the canonical
  logical-schema prerequisite removed `$defs` allocation's dependency on raw conjunction grouping.
  Introduce one canonical `Conjunction` owner and private immutable predicate nodes behind `Arc`;
  delete the scattered sort/dedup pairs without retaining producer-presentation compatibility
  state.
- Acceptance baseline: `e6660809`.
- Baseline production Rust LOC: 65,055.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
    The prerequisite's canonical output bytes are the baseline for this round.
  - Predicate structural order remains exactly `True`, `False`, `Approximate`, `Guard`, `Not`,
    `And`, `Or`, with each variant retaining its former derived field and recursive ordering.
    Cached hashes are equality fast-rejects only and never participate in `Ord` or serialization.
  - `Conjunction` owns one canonical vector. Construction recursively flattens nested `And`, drops
    `True`, sorts in legacy structural predicate order, and deduplicates exact predicates. It
    preserves `False`, `Or`, approximation markers and sound subsets, context-marker guards, and
    every nontrivial predicate.
  - No second raw, presentation, producer-order, or legacy-order vector may land. Logical schema
    canonicalization is the sole owner of emitted junctor order and repeated-definition identity.
  - Ordered predicate stacks whose order represents evaluation, branch priority, or provenance
    remain ordinary vectors. Only set-like logical conjunction carriers migrate.
  - `FailCapture.conjunction`, member-host implication predicates, selected-string implication
    predicates, parser selection conjunctions, and their normalization/route handoffs use the new
    carrier. Every remaining raw predicate vector is audited as non-conjunctive or migrated.
  - Public API decision: `Predicate` becomes opaque at ownership while retaining one exhaustive
    borrowed `PredicateKind` view. This is an intentional Part-F narrowing; `GuardDnf` wire bytes
    and all generated schema bytes remain exact.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero. Any acceptance flip or fixture-byte difference rejects the representation round before
    fixture adoption.

- Measured preflight result:
  - The compatibility-free compiler migration passes affected-crate Clippy and 434/434 focused
    core/IR tests. Its symbolic-IR corpus remains byte-exact, and its 62-test schema dump completes
    in 188.620 seconds versus the prerequisite's 230.575-second final dump under non-idle host
    load.
  - Direct schema comparison rejects the round: Airflow, Cilium, Jenkins, Kyverno, and Traefik
    differ from `e6660809`. No fixture was copied or modified.
  - Diagnostic full-depth adjudication fails with 12 candidate-accepts/Helm-aborts cells, all the
    same Kyverno family: `false` and empty-string values at each of six component/global
    `imagePullSecrets` default-selection inputs render successfully in Helm 4.2.3 but the candidate
    rejects them.

- Rejected-preflight diagnosis:
  - Canonical conjunction dedup removes duplicate positive markers that formerly distinguished a
    `RangeSelection` chain's synthetic all-candidates stamp from the selected candidate's real
    truthy tail. The reducer removes the stamp after recognizing it; without duplicate
    multiplicity it also removes the real tail and promotes iterable requirements outside the
    selected value's truthy scope.
  - Reconstructing the real tail from `RangeSelection.path` is structural and needs no legacy
    vector: `path` is the typed selected identity. A targeted Kyverno generation restores the one
    correctly scoped image-pull-secret conditional and eliminates the over-broad second
    conditional.
  - Rejected diagnostic variants that flattened only exact `And` nodes, ordered captures by kind,
    or reversed canonical conjunction order all retained the five-file mismatch. They prove the
    issue is marker multiplicity at lowering, not approximation grouping or set iteration order.
  - Because the B5a contract is byte-exact, the representation round remains rejected even after
    the false-rejection repair. The canonical capture behavior must land first as its own
    adjudicated output round; B5a then retries against that new baseline.

## B5 prerequisite 2 — canonical capture boundary and selected-tail reconstruction

- Status: rejected at the complete integration gate; no production, test, or fixture change
  retained.
- Contract: behavior-bearing output canonicalization. Canonicalize fail-capture and selected-string
  conjunction vectors once at the `ContractIr` finalization boundary using the exact B5a logical
  form (flatten `And`, remove `True`, sort, deduplicate). Reconstruct a `RangeSelection` capture's
  selected truthy tail from its typed `path` after removing the synthetic chain stamp. This makes
  lowering independent of duplicate-marker multiplicity before the carrier type changes.
- Acceptance baseline: `e6660809`.
- Baseline production Rust LOC: 65,055.
- Pre-registered acceptance expectations:
  - Zero corpus acceptance flips and zero candidate-accepts/Helm-aborts cells. In particular,
    Kyverno's `false` and empty-string component/global `imagePullSecrets` inputs remain accepted
    because Helm's `default` bypasses those falsy values.
  - Expected fixture-byte changes are confined to the five preflight charts: Airflow, Cilium,
    Jenkins, Kyverno, and Traefik. They may change logical-arm factoring, repeated-definition
    selection/naming, and dependent internal references only. No other schema, symbolic-IR,
    diagnostic, default, description, provider shape, or policy surface may change.
  - The canonicalizer is one temporary boundary owner, not per-producer hand synchronization. It
    processes `FailCapture.conjunction` and the selected-string conjunction embedded in its
    `CaptureKind`; the immediately following B5a round deletes it when the carrier enforces the
    invariant at construction.
  - The selected truthy tail is reconstructed only after the complete typed chain stamp and its
    disjunction are recognized. Incomplete stamps, genuine enclosing markers, fallback negations,
    and unrelated predicates remain untouched.
  - Public/wire decision: no public Rust API change. The five generated schema files intentionally
    adopt canonical capture factoring; acceptance must remain Helm-equivalent under the full-depth
    battery.
  - Mandatory base and third-level probe drops remain zero. Every unexpected flip or sixth fixture
    file stops the round before adoption; accepted-abort allowance remains zero.

- Measured rejected result:
  - The final1 immutable archive contains 90 binaries and 128 files. Its clean 62-test schema dump
    passes in 230.525 seconds. Direct comparison confirms the five registered chart-corpus changes
    and no sixth chart/profile/final-output change.
  - The full-depth battery checks 60 charts and 120,833 probes with zero acceptance flips, zero
    candidate-accepts/Helm-aborts cells, and zero mandatory base/third-level drops. The repaired
    Kyverno family keeps every falsy `default` input Helm accepts.
  - Focused IR Clippy and the typed selected-tail regression pass. Format, workspace lint, 48/48
    feature-matrix lint, and 1,326/1,326 unit tests pass on the final1 tree.
  - The complete integration gate correctly rejects the round after 6,315.452 seconds: 564/565
    tests pass, but `helm-schema-gen::corpus schema_fixtures_match` exposes one unregistered sixth
    artifact, `signoz-zookeeper-statefulset.schema.json`. The change is the same canonical
    conjunction factoring/order family, but the pre-registration explicitly required every
    generator-only fixture to remain exact.
  - The five provisionally copied chart fixtures were restored immediately. All final1 production,
    test, and fixture changes were then removed; the tree is again byte-identical to `e6660809`
    outside this ledger. No rejected artifact is adopted.

- Deviations:
  - The clean-dump comparison initially covered all 56 chart-corpus, four lean-profile, and four
    final-output artifacts but omitted the 20 generator-owned artifacts written by the same dump.
    That incomplete comparison let the five chart files reach provisional adoption before the full
    integration gate caught the Signoz mismatch. The next round compares all 84 dump artifacts
    before copying any fixture.
  - Energy-saver mode and unrelated background work made this gate unsuitable for performance
    conclusions: Airflow re-audit cases took 723–749 seconds and the integration suite took about
    105 minutes. These numbers are recorded as loaded-host evidence only.

## B5 prerequisite 2b — complete canonical capture output family

- Status: landed in `3164e8c7` (`fix(ir): canonicalize capture conjunctions`).
- Contract: behavior-bearing output canonicalization, retried from the clean `e6660809` tree with
  the complete measured artifact family. Canonicalize fail-capture and selected-string conjunction
  vectors once at `ContractIr` finalization, and reconstruct `RangeSelection`'s selected truthy tail
  from its typed `path` after removing the synthetic chain stamp.
- Acceptance baseline: `6cf6e4ff` (test-infra-only successor to production baseline `e6660809`).
- Baseline production Rust LOC: 65,055.
- Pre-registered acceptance expectations:
  - Zero corpus acceptance flips and zero candidate-accepts/Helm-aborts cells. Kyverno `false` and
    empty-string values in all six measured image-pull-secret selection roots remain accepted.
  - Exactly six fixture artifacts may change: the chart-corpus schemas for Airflow, Cilium,
    Jenkins, Kyverno, and Traefik, plus the generator fixture
    `signoz-zookeeper-statefulset.schema.json`. Changes are limited to logical-arm factoring/order,
    repeated-definition selection/naming, and dependent internal references.
  - The other 78 artifacts from the one clean 84-artifact dump remain byte-exact, including all
    symbolic-IR fixtures, the other generator schemas, lean profiles, and final-output controls.
  - The canonicalizer remains one temporary finalization-boundary owner and is deleted by the
    immediately following B5a carrier round. No producer-local copies or compatibility vectors may
    land.
  - The selected truthy tail is reconstructed only after recognizing the complete typed chain stamp
    and disjunction. Incomplete stamps, genuine enclosing markers, fallback negations, and unrelated
    predicates remain unchanged.
  - Public/wire decision: no public Rust API change. The six generated schema artifacts
    intentionally adopt canonical capture factoring; Helm-equivalent acceptance is mandatory.
  - Mandatory base and third-level probe drops remain zero. Any seventh artifact or acceptance flip
    rejects the retry before fixture adoption; accepted-abort allowance remains zero.
  - Infra note: standalone commit `6cf6e4ff` raises the integration and CI nextest profiles from
    four to eight workers at the user's request. It contains no semantic or ledger change; this
    round records the first eight-worker wall-clock, with energy-saver/background load disclosed.

- Measured results:
  - The final2 immutable nextest archive contains 90 binaries and 128 files. The one clean
    step-local dump produces all 84 expected artifacts and completes 62/62 schema-generation tests
    in 702.632 seconds under disclosed energy-saver/background load.
  - Direct comparison checks all 84 dump artifacts before adoption. Exactly the six pre-registered
    artifacts differ; the other 78 are byte-identical. The changed bytes are confined to canonical
    conjunction factoring/order, repeated-definition selection/naming, and dependent internal
    references.
  - The final2 full-depth battery checks 60 charts and 120,833 probes in 481.799 seconds. It records
    zero acceptance flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base
    probes, and 7,465/7,465 mandatory third-level probes.
  - Disclosed bounded categories emit 427 guard witness pairs and 127 composite pairs. The caps
    skip 13,981 guard rows and 704 composite rows; total disclosed drops are 29,718. Mandatory base
    and third-level drops remain zero.
  - The complete eight-worker integration gate passes 565/565 tests in 3,990.563 seconds. On the
    same loaded host and semantic code, this is 36.8% faster than the rejected four-worker run at
    6,315.452 seconds, though neither run is a clean performance benchmark.

- Deviations:
  - The first preflight exposed five chart fixtures and 12 false-rejection cells because canonical
    dedup erased duplicate range-selection marker multiplicity. That state was rejected before any
    fixture adoption. Reconstructing the selected truthy tail from the typed `RangeSelection.path`
    removes the accidental multiplicity dependency and restores every Helm-accepted falsy value.
  - The first prerequisite attempt registered only five chart artifacts because its dump
    comparison omitted generator-owned outputs. The complete integration gate exposed the sixth
    Signoz generator fixture; that attempt was fully restored and recorded above. This retry
    compares the complete 84-artifact family before adoption.
  - The canonicalizer is intentionally temporary at the finalization boundary. The immediately
    following B5a carrier retry must delete it rather than retain a second normalization owner.
  - Energy-saver mode and unrelated background work make the 702.632-second dump,
    481.799-second prober, 3,990.563-second integration gate, and 2,030.843-second complete battery
    unsuitable for the frozen quiet-host Airflow curve. They are reported as loaded-host iteration
    evidence only.

- Adjudication evidence:
  - Helm 4.2.3 adjudication is enabled in the authoritative full-depth battery. It finds zero
    acceptance flips and therefore no changed cell requiring an individual fixture decision.
  - The 12 Kyverno falsy-input false rejections from the rejected B5a preflight are absent. `false`
    and empty-string values at all six component/global image-pull-secret roots remain accepted by
    both the candidate and Helm.
  - Accepted-abort allowance remains zero and observed candidate-accepts/Helm-aborts remains zero.

- Producer/route coverage:

  | Producer or route | Final evidence |
  | --- | --- |
  | `FailCapture` finalization | Every finalized capture conjunction is recursively flattened, stripped of `True`, sorted, and deduplicated once. |
  | Selected-string implication | The nested selection conjunction receives the same canonical form at the same boundary. |
  | Complete range-selection chain | The synthetic stamp and disjunction are removed, then the typed selected path republishes its real truthy tail. |
  | Incomplete or unrelated markers | The recognizer does not fire; enclosing, fallback, and unrelated predicates remain unchanged. |
  | Chart-corpus output | Five registered chart schemas adopt canonical factoring; 51 other chart schemas remain byte-identical. |
  | Generator-only output | One registered Signoz schema adopts canonical factoring; the other 19 generator artifacts remain byte-identical. |
  | Symbolic IR and profiles | All symbolic-IR, lean-profile, and final-output controls remain byte-identical. |
  | Full-depth acceptance | 120,833 probes, zero flips, zero mandatory drops, and zero accepted-abort cells. |

- Review dossier:
  - Local-vs-global verdict: this is a narrow prerequisite, not the destination. The duplicate
    marker multiplicity was an accidental protocol between producer and reducer; deriving the real
    tail from typed selection identity is the structurally correct repair. B5a remains responsible
    for moving canonicality into the carrier and deleting this boundary helper.
  - Ownership: `ContractIr::finalize` is the sole temporary owner because every capture crosses it
    exactly once. No producer-local sort/dedup copy, presentation vector, legacy-order field, or
    compatibility adapter is added.
  - Invariants: canonical capture conjunctions contain no nested `And`, no `True`, and no duplicate
    predicate. Range-selection lowering preserves a real truthy tail independently of stamp
    multiplicity. Both are pinned by focused private-IR coverage and the full fixture battery.
  - Part-F decision: no public API or wire-format contract changes. Six generated schema artifacts
    intentionally change internal canonical factoring while preserving Helm-equivalent acceptance.

- Self-adversarial pass:
  - Rechecked the canonicalizer's recursive `And` flattening, `True` removal, structural sort, and
    exact dedup against the forthcoming B5a constructor semantics.
  - Rechecked that tail reconstruction occurs only after a complete typed stamp and its disjunction
    are recognized, and that it uses the selected `path` rather than a string or positional guess.
  - Audited the clean dump as one immutable-code-state batch and compared all 84 outputs before
    adopting the six fixtures. No rejected-state artifact remains.
  - Inspected the eight-worker run under load: all eight long re-audit processes remained
    CPU-saturated at about 99-100% with modest memory use. There is no evidence of a deadlock or a
    memory-pressure regression.
  - Rechecked the frozen plan, working-tree whitespace, and generated fixture set after the final
    edit. The frozen plan remains byte-identical to `bb61a78f`.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 57.10 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 333.67 seconds; the same two warnings remain.
  - `cargo nextest run --workspace`: exit 0, 1,326/1,326 tests in 692.169 seconds.
  - `task test:integration`: exit 0, 565/565 tests in 3,990.563 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,895/1,895 tests in 2,030.843 seconds; 24 tests skipped by profile.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 34.13 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts, using the documented macOS shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,095.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +40 (65,055 to 65,095). The delta is the temporary boundary
  canonicalizer plus typed-tail reconstruction; the next B5a carrier round is required to delete
  the boundary canonicalizer. No LOC promise applies.

## B5a retry 2 — canonical conjunction carrier and shared predicate nodes

- Status: landed in `ac9b54ab` (`refactor(core): canonicalize predicate conjunctions`).
- Contract: representation-only. Retry B5a after both measured output-order prerequisites. Replace
  set-like predicate vectors with one canonical `Conjunction` carrier, move predicate nodes behind
  private immutable `Arc`s, preserve former structural ordering manually, and delete the temporary
  capture-boundary canonicalizer plus scattered sort/dedup owners.
- Acceptance baseline: `deb4ef28` (prose-only successor to semantic baseline `3164e8c7`).
- Baseline production Rust LOC: 65,095.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, ordering, corpus acceptance, or fixture byte changes.
    The six canonical-output artifacts landed by `3164e8c7` are now part of the byte baseline.
  - `Predicate` structural order remains exactly `True`, `False`, `Approximate`, `Guard`, `Not`,
    `And`, `Or`, with former field and recursive ordering. Cached structural hashes are equality
    fast-rejects only and never participate in `Ord`, serialization, or definition naming.
  - `Conjunction` owns one canonical vector. Construction recursively flattens nested `And`, drops
    `True`, sorts in legacy structural order, and deduplicates exact predicates while preserving
    `False`, `Or`, approximations, context markers, and every nontrivial predicate.
  - The temporary `ContractIr::finalize` capture canonicalizer is deleted. Canonicality is enforced
    by the carrier at construction; no raw presentation, producer-order, legacy-order, or
    compatibility vector may remain.
  - The typed `RangeSelection.path` truthy-tail reconstruction survives the migration. Its local
    post-reconstruction sort/dedup disappears into `Conjunction` construction without changing
    scope or acceptance.
  - Ordered predicate stacks representing evaluation, priority, or provenance remain ordinary
    vectors. `FailCapture`, member-host implications, selected-string implications, parser
    selection conjunctions, and their route handoffs migrate because their semantics are set-like.
  - Public API decision: `Predicate` becomes opaque at ownership while retaining an exhaustive
    borrowed `PredicateKind` view. This is the planned Part-F narrowing; all wire bytes remain exact.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level coverage drops remain
    zero. Any fixture-byte difference or acceptance flip rejects the representation retry before
    artifact adoption.

- Measured results:
  - `Predicate` is now an opaque cloneable owner around private immutable nodes. Nontrivial nodes
    share an `Arc`; each node caches its structural hash for equality fast-rejection, while manual
    `Ord` and `Hash` walk the exact former enum variant/field order.
  - `Conjunction` owns one canonical predicate vector. Construction recursively flattens nested
    `And`, removes `True`, sorts structurally, and deduplicates. `extend`, `push`, `prepend`,
    `retain`, iteration, conversion, equality, ordering, and hashing preserve that one invariant;
    no presentation or producer-order vector remains.
  - `FailCapture`, selected-string implications, member-host implications, parser selection
    handoffs, and string-requirement merge-source routing use the carrier. The temporary
    `ContractIr::finalize` canonicalizer and the migrated local sort/dedup calls are deleted.
  - The final2 immutable archive contains 90 binaries and 128 files. Its sole clean schema dump
    passes 62/62 tests in 180.753 seconds and writes 84 artifacts; direct comparison finds all
    84 byte-identical to the `deb4ef28` semantic baseline.
  - The final2 symbolic-IR dump passes in 3.328 seconds and writes 18 artifacts. A separate exact
    fixture-comparison invocation from the same archive passes in 3.293 seconds.
  - The final2 full-depth battery passes in 89.283 seconds across 60 charts and 120,833 probes with
    zero acceptance flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base
    probes, and 7,465/7,465 mandatory third-level probes.
  - Canonical dedup reduces disclosed duplicate guard candidates: discovered guards fall from
    14,542 to 12,538 and total capped reductions from 29,718 to 25,710. The emitted guard-pair count
    remains 427, composite pairs remain 127, and mandatory coverage remains complete.

- Deviations:
  - The first compiler pass found one private regression test still constructing
    `FailCapture.conjunction` as a raw vector. Converting that test setup through the carrier was the
    only compiler-driven correction; no archive or dump was produced from the failing state.
  - Final1 passed focused tests, 84-file schema identity, 18-file IR identity, and the full-depth
    prober, but a self-adversarial audit found one string-requirement merge-source handoff converting
    a canonical conjunction back to `Vec<Predicate>` for map/set storage. That handoff migrated to
    `Conjunction`; every final1 artifact is therefore non-authoritative, and final2 alone supplies
    adoption evidence.
  - Final2's schema dump marked `final_outputs_match_policy_annotation_fixtures` as `LEAK` because a
    child process outlived nextest's grace interval. The assertion passed, the command exited 0, all
    four final-output artifacts are byte-identical, and no later gate reproduced a failure.
  - The host was not guaranteed idle. Dump/prober/gate wall times are iteration evidence, not the
    quiet-host Airflow curve required by B5b.

- Adjudication evidence:
  - Helm 4.2.3 adjudication is enabled in the final2 full-depth run. Zero fixture bytes and zero
    acceptance cells change, so no individual Helm fixture decision is required.
  - The Kyverno selected-range regression passes in focused, integration, and complete batteries;
    the typed selected truthy tail remains present after carrier canonicalization.
  - Accepted-abort allowance remains zero and observed candidate-accepts/Helm-aborts remains zero.

- Producer/route coverage:

  | Producer or route | Final representation and proof |
  | --- | --- |
  | Direct and guarded fail captures | `FailCapture.conjunction: Conjunction`; focused IR, corpus, and re-audit batteries are exact. |
  | Selected string requirements | `CaptureKind::StringRequirement.selection: Conjunction`; NATS/OAuth2/Kyverno routes and schema bytes are exact. |
  | Member-host implications | `MemberHostConversion.outer_predicates: Conjunction`; provider/member-host re-audits pass. |
  | Parser and strict-operand selection | Set-like branch conjunctions construct the carrier; ordered evaluation and provenance stacks remain vectors. |
  | Contract normalization handoff | String-requirement ancestor and merge-source maps retain `Conjunction` rather than reopening raw vectors. |
  | Range-selection lowering | The typed selected-path truthy tail survives stamp removal; the temporary boundary canonicalizer is deleted. |
  | Predicate readers | Every reader exhaustively matches borrowed `PredicateKind`; focused Clippy and all feature/target combinations pass. |
  | Guard DNF ordering | Manual predicate order matches the former derived order; all 84 schema and 18 IR artifacts are byte-exact. |

- Review dossier:
  - Local-vs-global verdict: the two prerequisites removed the accidental `$defs` grouping and
    duplicate-marker dependencies, so this retry reaches the intended global shape rather than
    preserving legacy presentation. One canonical carrier now owns conjunction invariants and one
    opaque predicate node owns sharing/hash/order semantics.
  - Ownership: private `PredicateNode` owns nontrivial formula storage and cached equality hashes;
    `Conjunction::new` owns set-like canonicalization. Callers can inspect formulas only through the
    exhaustive borrowed `PredicateKind`, so adding a variant still forces compiler-visible readers.
  - Remaining raw predicate vectors were audited. Active interpreter predicates, branch-priority
    lists, render alternatives, BDD paths, and provenance/shadow stacks retain order semantics and
    are not conjunction carriers. Local filtered vectors do not cross a phase boundary or own an
    invariant.
  - Part-F decision: the public `Predicate` enum becomes an opaque public struct and public
    `PredicateKind` borrowed view. External exhaustive value-pattern matching is intentionally
    narrowed; constructor methods and semantic operations remain available. No serialized IR,
    schema, diagnostic, or other wire-format byte changes.

- Self-adversarial pass:
  - Verified the cached hash is consulted only in node equality and never in `Ord`, serialization,
    definition naming, or public `Hash` ordering. Exact node-kind equality remains the collision
    backstop.
  - Compared manual ordering against the former derived variant sequence and recursive field order:
    `True`, `False`, `Approximate`, `Guard`, `Not`, `And`, `Or`. The focused ordering test and all
    BTree-backed fixture bytes remain exact.
  - Rejected the final1 audit gap rather than treating a carrier-to-vector conversion as harmless.
    Final2 repeats the archive, dump, IR equality, prober, and every required gate after that edit.
  - Confirmed no `presentation`, producer-order, legacy-order, or temporary capture canonicalizer
    remains in production core/IR source.
  - Confirmed canonical `retain` preserves sorted/deduplicated order, while mutation operations that
    add predicates reconstruct through `Conjunction::new`.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 29.81 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 171.05 seconds; the same two warnings remain.
  - `cargo nextest run --workspace`: exit 0, 1,328/1,328 tests in 158.898 seconds.
  - `task test:integration`: exit 0, 565/565 tests in 970.962 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,897/1,897 tests in 1,210.982 seconds; 24 tests skipped by profile.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 28.39 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts, using the documented macOS shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,634.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +539 (65,095 to 65,634). Opaque node ownership requires explicit
  constructors, borrowed variants, manual structural traits, and compiler-driven reader matches;
  it deletes the temporary boundary canonicalizer and scattered conjunction normalization without
  meeting an E-style LOC gate. B5a is an ordinary representation round, so no LOC promise applies.

## B5b — re-profile predicate work after canonical sharing

- Status: recorded in `55e322fa` (`chore(plan): record b5 predicate profile`).
- Contract: measurement-only. Re-run the frozen release Airflow command on the B5a tree and capture
  symbolicated samples from the debug binary. Record wall/CPU time, host-load caveats, and the new
  dominant stacks so B5c optimizes measured work rather than the pre-B5a profile.
- Acceptance baseline: `4ec344bf` (prose-only successor to B5a semantic commit `ac9b54ab`).
- Baseline production Rust LOC: 65,634.
- Pre-registered acceptance expectations:
  - Zero production, test, fixture, schema, diagnostic, acceptance, public-API, or wire changes.
    This round may add only its ledger dossier and step-local profile artifacts under `target/`.
  - The release measurement uses the pinned binary and frozen command shape:
    `helm-schema --k8s-version 1.31.0 testdata/charts/airflow`. Host load and power state are
    disclosed; a loaded measurement is not relabeled as the frozen idle-host baseline.
  - The debug measurement samples the actual `target/debug/helm-schema` process so function names
    resolve. Hot-stack conclusions require repeated stack presence, not one leaf sample.
  - B5c proceeds only from the measured profile. If predicate equality/ordering and
    `minimize_disjunction_by` no longer dominate, the optional boundary-only half is skipped and
    recorded rather than forced.

- Measured results:
  - The authoritative portable release run exits 0 at 127.14 seconds wall, 126.10 seconds user,
    and 0.66 seconds system: 126.76 seconds CPU. Its 3,744,672-byte schema is deterministic across
    both release attempts.
  - Against the frozen idle-host baseline (130 seconds wall, 112.7 seconds CPU), measured wall time
    is 2.86 seconds lower (-2.2%) while CPU is 14.06 seconds higher (+12.5%). The host was on AC
    power, but load averages were 2.92/6.19/11.64 and caches were warmed between attempts; this is
    not evidence of a B5a regression or speedup.
  - The successful debug profile samples every 10 milliseconds for 30 seconds and records 2,621
    main-thread samples with a 71.4 MiB physical footprint at the sample window. The debug workload
    exits 0 and its schema is byte-identical to the authoritative release output.
  - Top-of-stack counts remain predicate-heavy: `PredicateNode::eq` 394 samples (15.0%),
    `ValuesPath::encode` 365 (13.9%), SipHash writes 172 (6.6%), platform `memcmp` 113 (4.3%),
    `minimize_disjunction_by`'s retain closure 56 (2.1%), predicate `Ord` 49 (1.9%), and direct
    minimizer leaves 16 (0.6%). Recursive stacks are additionally dominated by
    `PredicateBdd::collect_paths` and repeated BDD normalization.
  - `Arc` clone/drop glue is no longer a leading leaf; the shared-node representation removed that
    portion of the frozen profile. Equality, hashing, path encoding, BDD reconstruction, and the
    minimizer's pair scan remain the measured work.

- Deviations:
  - The first release wrapper used `/usr/bin/time -lp`. The workload completed with 125.82 seconds
    wall and 125.59 seconds CPU, but macOS `time -l` then failed `sysctl kern.clockrate` under the
    sandbox and returned exit 1. Its timing is diagnostic only; the portable `time -p` rerun is the
    authoritative measurement.
  - The first debug `sample` call was denied process-inspection access while the owned workload kept
    running and exited 0. Two escalated retries missed their short-lived target while permission was
    pending. After the user permitted `/usr/bin/sample`, the final retry attached immediately and
    produced the sole adopted profile.
  - The frozen baseline was captured on an explicitly idle host; the current run was not. B5b
    reports the raw curve point and host state without smoothing or substituting loaded CI timings.

- Adjudication evidence:
  - This round changes no code or fixture and generates byte-identical release/debug schemas.
    Therefore it has zero acceptance flips and no Helm cell requiring adjudication.
  - Helm remains pinned at 4.2.3; the B5a final battery's zero accepted-abort allowance and result
    remain the semantic baseline.

- Review dossier:
  - B5c decision: predicate operations still dominate, and `minimize_disjunction_by` remains on the
    hottest repeated normalization stacks. Proceed with the scheduled in-place minimizer
    optimization; do not move call sites in that round.
  - Optional boundary-only decision: defer. Re-profile after the in-place algorithm before paying
    the inventory/peak-DNF/RSS/ordering cost of moving minimization boundaries. This keeps the
    optional half conditional on evidence rather than treating predicate dominance alone as proof
    that delayed normalization is safe.
  - Secondary finding: `ValuesPath::encode` and BDD path reconstruction are now as material as
    predicate equality. They belong to later measured S-B/S-C work, not the narrowly scoped B5c
    algorithm round.
  - Public/wire decision: none. Only ignored step-local profile artifacts and this ledger dossier
    are added; production, tests, fixtures, public APIs, and serialized bytes are unchanged.

- Self-adversarial pass:
  - Did not compare debug wall time to the release baseline; debug exists only for symbolication.
  - Counted hot leaves against all 2,621 samples and separated top-of-stack counts from recursive
    stack membership. The profile supports dominance and prioritization, not precise per-function
    CPU attribution.
  - Verified release attempt outputs against each other and the sampled debug output byte-for-byte.
  - Rejected both the forbidden-sysctl timing wrapper and missed sampler targets rather than
    adopting partial tooling results.

- Immutable battery:
  - Because B5b changes no production or test byte, it reuses B5a's sealed final2 archive (90
    binaries, 128 files) rather than rebuilding identical executables.
  - One B5b-local clean dump passes 62/62 tests in 197.136 seconds and writes 84 artifacts; all 84
    are byte-identical to the B5a final2 dump.
  - The B5b-local full-depth prober passes in 118.864 seconds against `4ec344bf`, with 60 charts,
    120,833 probes, zero flips, zero accepted-abort cells, and zero mandatory base/third-level
    drops.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 31.02 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 71.85 seconds; the same two warnings remain.
  - `cargo nextest run --workspace`: exit 0, 1,328/1,328 tests in 302.527 seconds.
  - `task test:integration`: exit 0, 565/565 tests in 1,457.691 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,897/1,897 tests in 1,476.489 seconds; 24 tests skipped by profile.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 2.91 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts, using the documented macOS shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC remains 65,634.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: 0. B5b changes only the progress ledger; profile outputs remain
  ignored under `target/`.

## B5c — optimize disjunction minimization in place

- Status: landed in `b052fb06` (`perf(core): optimize disjunction minimization`).
- Contract: representation/performance-only. Optimize `minimize_disjunction_by` at its existing
  boundary without moving any call site. Preserve exact disjunct membership and serialized order,
  then remeasure the frozen Airflow command before considering the optional boundary-only half.
- Acceptance baseline: `1fb87efc` (B5b hash-record successor to semantic B5a tree `ac9b54ab`).
- Baseline production Rust LOC: 65,634.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, public-API, wire, or fixture byte changes.
    Any changed byte or acceptance cell rejects the round before artifact adoption.
  - Keep both `GuardDnf` call sites in place. No delayed/boundary-only minimization, observer move,
    or intermediate DNF representation change enters this round.
  - Preserve the existing deterministic algorithm: lexicographically sorted unique disjuncts;
    first resolvable pair by that order; sorted insertion of the common conjunction; fixed-point
    complementary resolution; then strict-superset absorption without reordering survivors.
  - Replace repeated linear membership scans within sorted conjunctions by a two-pointer walk.
    Replace `contains` plus whole-vector resort after resolution by binary search/insertion, and
    replace the cloned absorption snapshot by index-based keep accounting.
  - Both current callers already produce sorted, deduplicated conjunctions. The optimized owner
    checks that invariant in debug builds rather than silently sorting inner keys and changing the
    existing function contract.
  - The final release Airflow command and clean corpus dump are measured on the final tree with host
    load disclosed. Byte identity is mandatory regardless of timing direction.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.
    Optional boundary-only minimization remains deferred unless the final profile justifies its
    separate observer/max-DNF/RSS/order study.

- Measured results:
  - The adopted minimizer keeps cached-hash `PartialEq` membership checks but removes the temporary
    `left_only`/`right_only` vectors. Successful resolution clones only the final common key.
  - Resolved keys use equality membership plus `partition_point` insertion, preserving the former
    sorted fixed-point order without re-sorting the whole disjunction after every pair. Final
    absorption computes an index-aligned keep vector over borrowed keys instead of deep-cloning the
    complete disjunction.
  - A 512-input exhaustive small-space test compares the optimized function with the exact former
    implementation across two complementary atom pairs, duplicates, empty conjunctions,
    fixed-point resolution, and absorption. Every result and survivor order is identical.
  - The final release Airflow run exits 0 at 92.57 seconds wall, 90.00 seconds user, and 0.46 seconds
    system: 90.46 seconds CPU. Against B5b's loaded 127.14/126.76 point this is 27.2% less wall and
    28.6% less CPU; against the frozen idle 130/112.7 baseline it is 28.8% less wall and 19.7% less
    CPU. The run began under load averages 17.25/13.03/18.01 and ended at 9.40/11.42/16.82, so the
    direction is strong but not an idle-host benchmark.
  - The final immutable archive contains 90 binaries and 128 files. Its one clean schema dump passes
    62/62 tests in 221.207 seconds and writes 84 artifacts, all byte-identical to B5b. Its IR dump
    and exact comparison pass in 3.973 and 3.297 seconds with all 18 artifacts unchanged.
  - The full-depth battery passes in 150.785 seconds across 60 charts and 120,833 probes with zero
    flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and
    7,465/7,465 mandatory third-level probes.

- Deviations:
  - The pre-registered two-pointer design replaced B5a's cached-hash equality with recursive `Ord`
    comparisons. It remained byte-exact and passed the equivalence test, but its release preflight
    took 249.54 seconds wall and 242.49 seconds CPU under load 8.39/11.50/18.92. That implementation
    was rejected before archive construction; none of its timing or output artifacts is adopted.
  - The corrected design retains equality scans and removes allocation/sorting/cloning work around
    them. Its 92.57/90.46 result demonstrates that the first preflight's regression was the deep
    ordering substitution, not the round's in-place optimization premise.
  - B5c's debug sample records a 157.3 MiB physical footprint during its window versus B5b's
    71.4 MiB window. The samplers attached at different execution phases (about 26 versus 17 seconds
    after launch) and neither reports process-lifetime peak RSS, so the values are disclosed but not
    treated as comparable memory evidence. Full batteries show no memory-pressure failure.

- Adjudication evidence:
  - Helm 4.2.3 adjudication is enabled in the final full-depth run. Zero schema bytes and zero
    acceptance cells change, so no individual fixture decision is required.
  - Accepted-abort allowance remains zero and observed candidate-accepts/Helm-aborts remains zero.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Predicate DNF construction | Same call site, fixed-point resolution, survivor membership, and order; 512-case oracle plus exact dumps. |
  | Conditional-guard normalization | Same generic owner and complement callback; conditional-guard tests and serialized contract bytes are exact. |
  | Complement candidate search | Equality-only scan with no temporary difference vectors; cached predicate hashes remain effective. |
  | Resolved common insertion | Equality dedup plus lexicographic `partition_point`; no full-vector re-sort. |
  | Strict-superset absorption | Borrowed pair scan plus aligned keep vector; survivor order is unchanged. |
  | Intermediate consumers | No call-site move; all existing `GuardDnf` observers retain immediate normalized form. |

- Review dossier:
  - The optimization stays on the correct hill: one normalization owner and the same semantic
    boundary, with fewer allocations and repeated sorts. It does not add caches, alternate DNF
    forms, lazy state, or call-site policy.
  - Post-B5c debug sampling records 2,494 samples. Leading leaves are `ValuesPath::encode` 370
    (14.8%), hashing 283 (11.3%), minimizer internals 174 (7.0%), predicate equality 77 (3.1%), and
    predicate ordering 66 (2.6%). B5b's predicate-equality leaf was 15.0%; the final profile and
    release timing agree that repeated deep equality work materially fell.
  - Optional boundary-only minimization is skipped. Intermediate observers include
    `is_unconditional`, `is_never`, `disjuncts`, guard projection/serialization,
    `single_guard_conjunction`, `conjoined`, equality/ordering/hashing, union, and path mapping.
    Delaying normalization would require a raw-versus-normalized representation and its own
    max-disjunct/RSS/order study, while path encoding and hashing now exceed the minimizer leaf.
  - Because no boundary spike is attempted, its adoption-only max-disjunct and peak-RSS gates are
    not claimed. The precise future starting point would be a separately pre-registered lazy-DNF
    representation study, not an extension of this in-place commit.
  - Public/wire decision: none. `minimize_disjunction_by` and the equivalence oracle are private;
    serialized guards, IR, schemas, diagnostics, and public construction remain byte-exact.

- Self-adversarial pass:
  - Rejected the superficially faster-asymptotic two-pointer walk when measurement showed it
    defeated the cached equality invariant. The final code keeps the cheapest comparator for the
    actual `Predicate` domain.
  - Verified that `partition_point` sees the same sorted outer vector produced by the former
    post-push full sort, and that equality is checked first so an existing common key is not inserted
    twice.
  - Verified that keep flags are computed before mutation and consumed once in original index order,
    making absorption membership and survivor ordering identical to the cloned-snapshot algorithm.
  - Compared release and sampled-debug schemas byte-for-byte, then repeated schema/IR identity from
    the immutable archive. No rejected-preflight artifact enters final evidence.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 30.63 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 177.57 seconds; the same two warnings remain.
  - `cargo nextest run --workspace`: exit 0, 1,329/1,329 tests in 120.547 seconds.
  - `task test:integration`: exit 0, 565/565 tests in 935.117 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,898/1,898 tests in 1,116.664 seconds; 24 tests skipped by profile.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 1.63 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts, using the documented macOS shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,659.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +25 (65,634 to 65,659). The production delta is the allocation-
  and sort-free fixed-point bookkeeping; the exhaustive reference oracle lives under `src/tests/`
  and is excluded from production LOC. No E-style LOC gate applies.

## C2 — one schema materialization and one description traversal

- Status: landed in `2a20036f` (`perf(gen): eliminate repeated schema materialization`).
- Contract: representation/performance-only. Keep the schema as `SchemaNode` while materializing
  declared ranged-map members, editing the typed tree directly instead of serializing and reparsing
  the whole root for each ranged path. Replace the per-description root walk with one trie-guided
  schema traversal. Preserve every emitted schema and symbolic-IR byte.
- Acceptance baseline: `e4bc9fcc` (B5c closure commit).
- Baseline production Rust LOC: 65,659.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, public-API, wire, or fixture byte changes.
    Any changed byte or acceptance cell rejects the round before artifact adoption.
  - `SchemaDocument::materialize_declared_member_schema` traverses its existing `SchemaNode` root
    directly. It may clone the member schema into declared keys, but it must not call root
    `into_value()` or reconstruct the root through `SchemaNode::from_value`/`foreign`.
  - Typed traversal must preserve the legacy branch domain exactly: `anyOf`, `allOf`, and `oneOf`
    arms are visited at the same remaining path; `then` and `else` are likewise traversed; a `*`
    segment descends through `items`; a literal segment descends through `properties`; non-schema
    values and absent keywords abstain.
  - Declared-member materialization retains the legacy object-lane test and overwrite semantics:
    object-typed schemas, schemas with `properties`, and schemas with `additionalProperties` gain
    or replace every declared property with the exact ranged-member schema; other lanes remain
    untouched.
  - Non-blank values descriptions are parsed once into a prefix trie. One root traversal follows
    only trie edges while preserving the legacy branch fanout and exact-path description overwrite;
    blank descriptions and paths absent from the emitted schema remain no-ops.
  - The B5c sealed dump's 221.207-second 62/62 clean corpus run is the before point. A fresh immutable
    archive supplies one clean final-tree dump; host load is disclosed and byte identity is
    mandatory regardless of timing direction.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.
    No fixture is regenerated or adopted in this representation round.

- Measured results:
  - `SchemaDocument::materialize_declared_member_schema` now edits the resident `SchemaNode` tree.
    The per-ranged-path root `into_value()`/raw-JSON walk/`SchemaNode::foreign` cycle is deleted;
    member schemas stay typed while being cloned into the declared property slots.
  - The former JSON round-trip also carried a load-bearing representation transition: after the
    first declared-member materialization, every existing generated node was represented exactly as
    a parsed schema. The final implementation performs that transition once, directly in memory,
    through `SchemaNode::into_parsed_representation`; it never serializes or reparses JSON. A
    document bit prevents repeating the whole-tree conversion for later ranged paths.
  - `apply_values_descriptions` builds one borrowed-description prefix trie and walks the schema
    once. Combinator and conditional branches retain the same semantic path without reapplying an
    exact-path description; property and `items` edges consume one trie segment. Blank and missing
    paths remain no-ops.
  - Four focused tests pin typed branch materialization, the typeless-empty-lane abstention, branch/
    exact-path/wildcard description behavior, and byte identity of the direct parsed-representation
    transition across generated objects, arrays, empty slots, Boolean schemas, combinators, and
    unknown keywords.
  - The final2 immutable archive contains 90 binaries and 128 files. Its sole clean schema dump
    passes 62/62 tests in 155.923 seconds (158.78 seconds process wall), writes 84 artifacts, and is
    recursively byte-identical to B5c. Against B5c's 221.207-second clean dump, the comparable
    nextest wall time falls by 65.284 seconds, or 29.5%.
  - The final release Airflow run exits 0 at 85.49 seconds wall, 85.06 seconds user, and 0.33 seconds
    system: 85.39 seconds CPU. Against B5c's 92.57/90.46 point this is 7.7% less wall and 5.6% less
    CPU; against the frozen 130/112.7 baseline it is 34.2% less wall and 24.2% less CPU. The run
    began on AC power under load averages 9.24/9.62/9.17, so it is a loaded-host curve point rather
    than an idle benchmark.
  - The final IR dump passes in 3.196 seconds with all 18 artifacts unchanged. The full-depth battery
    passes in 84.874 seconds across 60 charts and 120,833 probes with zero flips, zero
    candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and 7,465/7,465
    mandatory third-level probes.

- Deviations:
  - Final1 deleted both raw-JSON paths but failed the byte gate in one Airflow artifact. The schemas
    remained acceptance-equivalent, but repeated-payload `$defs` names were permuted because the
    old first ranged-map round-trip had converted existing generator-owned `Object` nodes into the
    parsed keyword representation; later insertions deliberately dispatch differently at that
    boundary. Final1's archive and 62/62 dump were rejected, no fixture was touched, and no IR or
    prober evidence was run from that state.
  - A corrected preflight made the representation transition once in memory and produced an Airflow
    schema byte-identical to B5c. Its broad `test(airflow)` filter also selected twelve chart-reaudit
    tests, so the 13/13, 239.999-second run is diagnostic only. Final2 rebuilt every authoritative
    artifact after adding the exhaustive transition test.
  - One focused nextest command used a literal substring expression that selected zero tests and
    exited nonzero. The corrected regex selected and passed both intended declared-member tests;
    neither command produced an adopted dump.
  - The first final-tree `task lint:fc` attempt completed all 48 rows but the 32 Linux/Windows rows
    failed before Clippy because the sandbox denied Zig's cache under `/Users/roman/.cache/zig`.
    The exact gate was rerun with that host-cache permission and passed 48/48; no repository or
    tool configuration changed.

- Adjudication evidence:
  - Helm 4.2.3 remains pinned and was used by the final full-depth run. All 84 schema bytes and all
    18 symbolic-IR bytes match the acceptance baseline, and the prober reports zero acceptance
    flips. No fixture or individual Helm cell required adoption.
  - Candidate-accepts/Helm-aborts remains zero against a zero allowance. Mandatory base and
    third-level coverage have zero drops; the disclosed bounded reductions total 25,710 and do not
    enter the mandatory categories.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Declared ranged-map members | Typed property insertion across direct, `anyOf`, `allOf`, `oneOf`, `then`, and `else` paths; focused exact-schema test plus 84-artifact corpus identity. |
  | Parsed-representation boundary | One direct in-memory transition after the first materialization, byte-equivalent for every generated node family; exhaustive focused test and Airflow `$defs` identity. |
  | Values descriptions | One trie-guided traversal through properties, wildcard `items`, combinators, and conditionals; focused full-schema equality and corpus identity. |
  | Missing/blank metadata | Same abstention without path creation; focused test and values-description corpus tests. |
  | Repeated provider payloads | Exact core membership and `$defs` naming after the preserved transition; Airflow and all 84 final artifacts are byte-identical. |

- Review dossier:
  - Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-build cargo
    nextest archive --workspace --archive-file /private/tmp/arch-v4-c2-final2.tar.zst`; exit 0, 90
    binaries and 128 files. Final1 and the Airflow-only preflight are rejected artifacts.
  - Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-schema
    SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-c2-final2.tar.zst --profile
    integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
    exit 0, 62/62 in 155.923 seconds and 84 exact artifacts.
  - Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-ir
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-c2-final2.tar.zst --profile integration -E
    'test(ir_corpus_fixtures_match)'`; exit 0, one test in 3.196 seconds and 18 exact artifacts.
  - Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-prober
    SCHEMA_ACCEPTANCE_BASELINE_REF=e4bc9fcc
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-schema
    SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-c2-final2-coverage.json
    ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-c2-final2.tar.zst --profile integration -E
    'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
    ignored-only`; exit 0 in 84.874 seconds with zero flips and zero mandatory drops.
  - Public/wire decision: none. `SchemaDocument`, `SchemaNode`, its transition, and the description
    trie are crate-private; final JSON, IR, diagnostics, and public construction remain byte-exact.

- Self-adversarial pass:
  - Rejected semantic equivalence as insufficient when final1 permuted only definition identities.
    The Airflow delta proved the legacy representation transition was behaviorally load-bearing for
    later grouping even though the pre/post-transition node serializations are equal.
  - Preserved that transition without reviving the raw JSON protocol: generated fields move between
    the two internal variants directly, unknown JSON keywords stay lossless, and a test compares
    every converted node's complete JSON value.
  - Corrected the initial direct materializer's assumption that every `SchemaNode::Object` is an
    emitted object lane. A relaxed, typeless, empty object serializes as `{}` and must abstain; the
    object-lane predicate now mirrors the exact emitted keywords and has a dedicated regression.
  - Verified a trie node's exact-path description is not copied into same-path combinator branches;
    only descendant trie edges fan through branches, matching the old early return at an exhausted
    path. Overlapping parent/child descriptions and wildcard items are pinned together.
  - Confirmed production contains no per-ranged root conversion/reparse and no per-description root
    loop. No fixture, public API, wire format, infra configuration, or compatibility carrier changed.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0 in 1.01 seconds.
  - `task lint`: exit 0 in 30.49 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 100.60 seconds after the disclosed sandbox-only
    Zig-cache failure.
  - `cargo nextest run --workspace`: exit 0, 1,333/1,333 tests in 104.138 seconds.
  - `task test:integration`: exit 0, 565/565 tests in 741.464 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,902/1,902 tests in 788.509 seconds; 24 tests skipped by profile and
    all live network tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 16.45 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts in 25.85 seconds on the retained-log rerun,
    using the documented macOS shims and `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,848.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +189 (65,659 to 65,848). The direct typed transition and trie
  replace two superlinear raw-JSON protocols and yield byte-exact output plus a 29.5% clean-corpus
  wall-time reduction. C2 is an ordinary performance/representation round, so no E-style LOC gate
  applies.

## S-D — delete the remaining mechanism-backed dead surface

- Status: landed in `8dd6a115` (`refactor: delete dead provider and emission surfaces`).
- Contract: behavior-bearing diagnostic ownership plus representation/API-surface deletion.
  Complete the frozen S-D work that remains after
  wave 1's already-landed IR privacy round: remove dead provider ownership/source knobs, centralize
  shortlist evidence, replace the completion enum with named phases, collapse fact accounting to
  its sole live dimension, and reuse the existing root-definition collector.
- Acceptance baseline: `ddbf3933` (C2 closure commit).
- Baseline production Rust LOC: 65,848.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, acceptance, wire, or fixture byte changes. Diagnostic changes are
    limited to the registered shortlist-origin family below; any other diagnostic or acceptance
    cell rejects the round before artifact adoption.
  - Remove public `K8sSchemaProvider::has_resource` and all four production implementations. Tests
    that used the dead ownership probe must assert through typed `lookup` outcomes or cache files;
    no replacement ownership Boolean enters production.
  - Remove only `KubernetesJsonSchemaProvider.record_source` and its public builder: production
    never wires them. Preserve the separately wired CRD catalog `--crd-cache-record-source`
    behavior and test. Replace fixed cache-request Boolean call-site literals with named request
    policy constructors; the K8s cache bypass and persistent not-found semantics stay exact.
  - Consult the canonical kind shortlist once in the inference owner. Assign
    `ProviderOrigin::KubernetesOpenApi` for core/built-in API groups and
    `ProviderOrigin::DefaultCatalog` for CRD groups, and add evidence only when that provider family
    is configured. Provider scans contribute cache/online/chart-local facts only; local overrides
    retain their authoritative origin without restamping cache evidence as `Shortlist`.
  - Registered diagnostic flip: a shortlist-inferred core/built-in kind previously received two
    duplicate shortlist candidates and reported the earlier-sorting, false
    `ProviderOrigin::DefaultCatalog`. It now reports `ProviderOrigin::KubernetesOpenApi`.
    External/CRD shortlist kinds remain `DefaultCatalog`; explicit local overrides remain
    authoritative `LocalOverride`, with their evidence source changing from the provider-local
    synthetic `Shortlist` stamp to the truthful cache-scan source. No schema or Helm acceptance can
    change because the selected apiVersion is identical.
  - Delete `CompletionPass`, its seven early returns, `FactAccounting`, and
    `generate_values_schema_through`. `LoweredEmissionPlan::complete` composes named typed-tree and
    materialized-tree steps; the profile monotonicity test composes those same functions directly
    to inspect every boundary.
  - Collapse `EmissionReport`'s private `(class, origin)` map to a class-keyed map and delete the
    unused public `counts_for_class_and_origin` accessor. Total and per-class counts remain exact.
  - Implement `OwnedDefinitions::capture` through the file's existing `root_definitions` owner with
    no change to reachability, caller-ownership, or pruning order.
  - Public API removals are deliberate Part-F decisions: `K8sSchemaProvider::has_resource`,
    `KubernetesJsonSchemaProvider.record_source`,
    `KubernetesJsonSchemaProvider::with_record_source`, and
    `EmissionReport::counts_for_class_and_origin`. No wire-format version is required because none
    of these APIs serialize.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - `K8sSchemaProvider::has_resource` and all four production implementations are deleted. Typed
    `lookup` outcomes remain the sole ownership/path/miss protocol; test fakes and cache-layout
    tests now exercise that same path. Two integration tests whose only subject was the removed
    Boolean disappear, accounting exactly for the 565→563 and 1,902→1,900 gate-count changes.
  - Kubernetes OpenAPI's unwired `record_source` field/builder are deleted. A typed
    `SchemaCachePolicy` makes the remaining cache differences explicit: K8s alone can bypass cache
    and persists authoritative-not-found markers; the CRD catalog always caches and alone carries
    its wired source-sidecar option. The CRD sidecar integration test remains green.
  - The canonical kind shortlist is consulted once in `infer_api_version`. Core and built-in API
    groups are attributed to `KubernetesOpenApi`; CRD groups to `DefaultCatalog`; the row is omitted
    if the modeled provider family is absent. Provider methods now contribute only their actual
    chart-local, cache-scan, or online facts, and local overrides no longer rewrite scan evidence.
  - `CompletionPass`, seven early-return branches, `FactAccounting`, and the private generation
    forwarder are deleted. `complete` now composes seven named phase functions over
    `ProjectedTree` and `MaterializedTree`; the monotonicity test directly composes and validates
    all eight observable boundaries using those same functions.
  - `EmissionReport` stores `BTreeMap<EmissionClassKind, FactCounts>` directly. The unused
    class-and-origin accessor and producer dimension are gone; public total/per-class accounting
    stays exact. `OwnedDefinitions::capture` delegates to the existing `root_definitions` owner.
  - The final immutable archive contains 90 binaries and 128 files. Its one clean schema dump passes
    62/62 tests in 155.369 seconds (158.13 process seconds), writes 84 artifacts, and is recursively
    byte-identical to C2. The IR dump passes in 3.190 seconds with all 18 artifacts unchanged.
  - The full-depth battery passes in 84.705 seconds across 60 charts and 120,833 probes with zero
    schema/acceptance flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory
    base probes, and 7,465/7,465 mandatory third-level probes.
- Deviations:
  - The initial audit classified the shortlist hoist as representation-only. A rejected preflight
    exposed that both remote providers had been emitting the same shortlist row, so built-in kinds
    falsely attributed the chosen diagnostic to `DefaultCatalog`; local overrides also restamped
    every scan as `Shortlist`. The centralization code was fully reverted before this diagnostic
    family was pre-registered, then the behavior-bearing round resumed. No archive, dump, fixture,
    or test artifact was produced from the unregistered state.
  - The first completion lint preflight rejected three named phase methods whose receiver was not
    used. They became associated functions instead of gaining suppressions; no archive or dump had
    been built.
  - The first K8s integration preflight retained a test that gathered shortlist rows directly from
    providers. It failed as intended after ownership moved. The test now drives the chain and pins
    the final `ServiceMonitor` diagnostic tuple; the complete 96-test K8s integration suite then
    passed before the immutable archive was built.
  - The first final-tree `task test:all` terminal session closed after reporting 1,897/1,900 passes
    but before its command exit reached the harness. That is not gate evidence. The exact task was
    rerun with a retained log and explicit exit propagation; the recorded 1,900/1,900 result below
    is solely that verified rerun.
- Adjudication evidence:
  - Helm 4.2.3 remains pinned and was used by the final full-depth battery. All 84 schema and all 18
    symbolic-IR bytes match `ddbf3933`; zero acceptance cells change, so no fixture or individual
    Helm cell requires adoption.
  - Focused diagnostic proof pins external `ServiceMonitor` to
    `(monitoring.coreos.com/v1, Shortlist, DefaultCatalog)` and constructs both CRD and K8s
    providers for a built-in `ConfigMap`, proving the corrected K8s ownership suppresses the former
    false CRD inference notice. Local-override aggregation remains authoritative while its source
    now reflects the scan rather than a provider-local restamp.
  - Candidate-accepts/Helm-aborts remains zero against a zero allowance. Mandatory base and
    third-level coverage have zero drops; 25,710 disclosed bounded reductions remain outside the
    mandatory categories.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Concrete provider lookup | One typed `ProviderLookupResult` protocol; chain precedence/path/miss tests and 32-chart downstream gate. |
  | K8s cache policy | Cache bypass plus persistent negative markers; offline capability and multi-version integration suites. |
  | CRD cache policy | Persistent cache plus optional source sidecar, no not-found marker; mirror and sidecar integration tests. |
  | API-version shortlist | One central consult with explicit built-in/CRD origin; focused external and dual-provider built-in diagnostics. |
  | Completion phases | Named typed/materialized transitions shared by production and boundary monotonicity test; 84 exact schemas. |
  | Fact accounting | One class-keyed owner with conserved totals/per-class counts; emission-profile tests and exact reports. |
  | Owned definition pruning | Shared `root_definitions` capture; reachability unit/integration tests and exact output. |

- Review dossier:
  - Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-build cargo
    nextest archive --workspace --archive-file /private/tmp/arch-v4-sd2-final1.tar.zst`; exit 0, 90
    binaries and 128 files.
  - Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-schema
    SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sd2-final1.tar.zst --profile
    integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
    exit 0, 62/62 in 155.369 seconds and 84 exact artifacts.
  - Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-ir
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sd2-final1.tar.zst --profile integration -E
    'test(ir_corpus_fixtures_match)'`; exit 0, one test in 3.190 seconds and 18 exact artifacts.
  - Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-prober
    SCHEMA_ACCEPTANCE_BASELINE_REF=ddbf3933
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-schema
    SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sd2-final1-coverage.json
    ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sd2-final1.tar.zst --profile integration -E
    'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
    ignored-only`; exit 0 in 84.705 seconds with zero flips and zero mandatory drops.
  - Part-F decisions: remove the public `K8sSchemaProvider::has_resource`,
    `KubernetesJsonSchemaProvider.record_source`,
    `KubernetesJsonSchemaProvider::with_record_source`, and
    `EmissionReport::counts_for_class_and_origin` surfaces. Each had zero production consumer and
    represented a deleted parallel protocol/dimension; no deprecation facade is retained. No wire
    format changes because none of these APIs serialize.

- Self-adversarial pass:
  - Distinguished the unwired Kubernetes source knob from the live CRD CLI option. The latter and
    its meta-sidecar test survive; deleting both would have exceeded frozen scope and broken a real
    feature.
  - Rejected moving three independent cache Booleans behind constructor defaults. The typed policy
    enum instead makes the two valid provider policies exhaustive and prevents invalid combinations
    without preserving literal flag drift.
  - Verified shortlist centralization does not invent candidates when the appropriate provider
    family is absent, and uses the existing built-in-group classifier rather than a suffix heuristic.
  - Kept local-override authority separate from evidence tier: aggregation still selects the first
    authoritative partition, so deleting the false `Shortlist` stamp cannot change its apiVersion.
  - Confirmed every completion boundary is exercised from the production functions, with no
    test-only helper in production and no enum/Boolean replacement for the deleted pass knob.
  - Confirmed no `has_resource`, `CompletionPass`, `FactAccounting`, class-and-origin report map,
    duplicated definition capture, or Kubernetes record-source symbol remains in production.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0 in 1.00 seconds.
  - `task lint`: exit 0 in 31.57 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 143.20 seconds using the previously approved Zig
    cache permission.
  - `cargo nextest run --workspace`: exit 0, 1,333/1,333 tests in 104.869 seconds.
  - `task test:integration`: exit 0, 563/563 tests in 741.074 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,900/1,900 tests in 788.398 seconds; 24 tests skipped by profile and
    all live network tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 16.77 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts in 41.87 seconds, using the documented
    macOS shims and `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,789.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: -59 (65,848 to 65,789). The round deletes four public dead APIs,
  three Boolean cache knobs, one pass enum, one wrapper, one forwarder, one report dimension, and
  one duplicate collector while adding typed cache and materialized-phase carriers. No E-style LOC
  gate applies.

## S-B quick paths — remove repeated resolution, composition, and version work

- Status: landed in `c8d4bdce` (`perf: remove repeated path and chart preparation`).
- Contract: representation/performance-only. Complete the independent short S-B items that remain
  before C3: resolve path evidence without deep clones, derive the dependency document from the
  already-composed refill document, thread parsed dependency metadata through discovery recursion,
  and materialize the Kubernetes version chain once.
- Acceptance baseline: `a408db2a` (verified S-D full-suite evidence commit).
- Baseline production Rust LOC: 65,789.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, wire, or fixture byte changes. Any changed
    byte or acceptance cell rejects the round before artifact adoption.
  - `PathSchemaResolver::resolve_all` field-splits the owned resolver and resolves directly from the
    borrowed evidence map. No `ValuesPath`, `ContractPathSchemaEvidence`, or staging path vector is
    cloned solely to satisfy the mutable provider-cache borrow.
  - Compose dependency refill defaults once, then derive the dependency document as
    `refill − root-declared paths`. Null-deletion, subchart-global propagation, the
    `include_subchart_values = false` lane, and document ordering remain exact.
  - Parse every discovered subchart's `Chart.yaml`/`Chart.template.yaml` once and carry that parsed
    value into recursive discovery, including multiple aliases of one installed chart. Duplicate
    installed-name rejection and alias order remain unchanged.
  - `K8sVersionChain::new` materializes the ordered explicit-plus-auto list once. `ordered` and
    `inference_scan_versions` borrow stable slices; lookup, diagnostics, capability probes, and
    explicit-only inference keep their exact order and membership.
  - Part-F decision: `K8sVersionChain`'s `explicit` and `auto_fallback_window` fields become private,
    and its two list accessors return slices rather than newly allocated vectors. Construction
    remains the public policy boundary; no wire format is involved.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - `PathSchemaResolver::resolve_all` destructures its owned fields and resolves each borrowed map
    entry directly with one mutable provider cache. The cloned evidence object, cloned path staging
    vector, second map lookup, and private `resolve_path` adapter are deleted.
  - Session preparation now composes dependency refill defaults once and derives the dependency
    document by subtracting the root chart's declared paths. `compose_subchart_values` calls fall
    from three to two; the no-subchart lane still supplies two `Null` documents.
  - Discovery reads each root/installed chart metadata document before recursion and passes the
    parsed `ChartYaml` by reference into the recursive call. Multiple aliases share the same parsed
    value during their sibling loop; installed-name validation consumes the same tuple without a
    cloned staging projection.
  - `K8sVersionChain` stores explicit and fully ordered lists. Auto-fallback formatting occurs once
    in `new`; lookup and inference scan borrowed slices. Public tests pin the borrowed policy view,
    and provider diagnostics still clone only at the trait boundary that owns a `Vec<String>`.
  - The final immutable archive contains 90 binaries and 128 files. Its clean schema dump passes
    62/62 tests in 155.317 seconds, writes 84 artifacts, and is recursively byte-identical to S-D.
    The IR dump passes in 3.186 seconds with all 18 artifacts unchanged.
  - The full-depth battery passes in 87.671 seconds across 60 charts and 120,833 probes with zero
    flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and
    7,465/7,465 mandatory third-level probes.

- Deviations:
  - The first discovery lint preflight carried parsed `ChartYaml` by value. Clippy correctly
    rejected the unnecessary ownership transfer; the final design borrows it, which also removes
    the initially proposed per-alias clone. No archive or dump had been built.

- Adjudication evidence:
  - Helm 4.2.3 remains pinned and was used by the full-depth battery. All 84 schema and 18 IR bytes
    match `a408db2a`, and the prober reports zero acceptance flips. No fixture or Helm cell requires
    adoption.
  - Candidate-accepts/Helm-aborts remains zero against a zero allowance. Mandatory base and
    third-level coverage have zero drops; 25,710 disclosed bounded reductions remain outside the
    mandatory categories.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Path evidence resolution | Same ordered evidence map, one shared provider cache, no cloned evidence; focused gen suite and exact corpus. |
  | Composed root values | Same root plus scoped subchart defaults; chart values tests and 84 exact schemas. |
  | Dependency/refill values | Refill composed once, dependency derived by root declaration subtraction; dependency provider tests and corpus identity. |
  | Chart discovery | One metadata parse per visited chart/alias sibling set; duplicate-name, alias, activation, and public chart tests. |
  | K8s lookup order | Borrowed precomputed explicit-plus-auto slice; version-chain, fallback, cache, capability, and 32-chart downstream tests. |

- Review dossier:
  - Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-build cargo
    nextest archive --workspace --archive-file /private/tmp/arch-v4-sbquick-final1.tar.zst`; exit 0,
    90 binaries and 128 files.
  - Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-schema
    SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sbquick-final1.tar.zst
    --profile integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
    exit 0, 62/62 in 155.317 seconds and 84 exact artifacts.
  - Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-ir
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sbquick-final1.tar.zst --profile integration -E
    'test(ir_corpus_fixtures_match)'`; exit 0, one test in 3.186 seconds and 18 exact artifacts.
  - Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-prober
    SCHEMA_ACCEPTANCE_BASELINE_REF=a408db2a
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-schema
    SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sbquick-final1-coverage.json
    ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sbquick-final1.tar.zst --profile integration -E
    'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
    ignored-only`; exit 0 in 87.671 seconds with zero flips and zero mandatory drops.
  - Part-F decision: `K8sVersionChain`'s construction fields are private and its ordered/explicit
    views borrow slices. Callers cannot mutate a list out of sync with the precomputed chain; no
    serialized format changes.

- Self-adversarial pass:
  - Verified dependency derivation subtracts from the refill document, not the composed root-plus-
    dependencies document; the latter includes parent overrides and would change refill semantics.
  - Kept root `values.yaml` parsing and the remaining two compositions in place because the C3
    snapshot owns the parse-once close. This round does not introduce a partial file cache.
  - Passed parsed chart metadata by reference through alias recursion and kept the parsed value in
    the installed-chart tuple, avoiding both the old second read and a replacement clone protocol.
  - Kept explicit versions separately from the materialized ordered list so inference scans cannot
    accidentally include auto-fallback escape versions.
  - Confirmed no resolve-path staging vector/evidence clone, third subchart composition, recursive
    chart metadata reread, or per-lookup version formatting remains.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0 in 1.00 seconds.
  - `task lint`: exit 0 in 33.27 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 134.30 seconds.
  - `cargo nextest run --workspace`: exit 0, 1,333/1,333 tests in 104.885 seconds.
  - `task test:integration`: exit 0, 563/563 tests in 740.960 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,900/1,900 tests in 786.106 seconds; 24 tests skipped by profile and
    all live network tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 15.66 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts in 40.47 seconds, using the documented
    macOS shims and `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,793.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +4 (65,789 to 65,793). The stored ordered version list and parsed
  discovery tuple make the phase artifacts explicit while deleting repeated cloning/composition/
  parsing work. This is an ordinary performance/representation round, so no E-style LOC gate
  applies.

## S-C scope discipline — one interpreter mark and context-owned dispatch depth

- Status: landed in `67b4ac13` (`refactor(ir): centralize interpreter scope state`).
- Contract: representation-only. Replace manual save/truncate/decrement clusters for interpreter
  scope state with one typed mark/rewind owner, and move helper-dispatch recursion depth from thread
  global state onto `ValuePathContext`.
- Acceptance baseline: `9ac3d3ee` (S-B quick-path closure commit).
- Baseline production Rust LOC: 65,793.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, public-API, wire, or fixture byte changes. Any
    changed byte or acceptance cell rejects the round before artifact adoption.
  - `ScopeMark` captures lengths for `active_predicates`, `dot_stack`, `active_range_modes`, and
    `alternative_capture_approximates`, plus `loop_depth`. `mark_scope()`/`rewind(mark)` replace the
    manual control-arm, range-item, inline-if/with/range, and node-list restoration clusters;
    `locals` remains explicitly cloned/joined because its exits are semantic dataflow.
  - Every rewind restores exactly the caller's entry depth and truncates only post-mark stack
    entries. Range bodies still observe incremented depth during evaluation, and loop-control
    extraction occurs before rewind where the old code required it.
  - `ValuePathContext` owns a `Cell<u8>` initialized for each context. All three helper-dispatch
    recursion lanes consult/increment/decrement that cell around nested decoding; the cap remains 2
    and no process/thread state can leak between independent contexts.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - One `ScopeMark` captures predicate, dot, range-mode, and capture-approximation stack lengths plus
    loop depth. `Interpreter::rewind` is the sole restoration owner for control arms, individual
    range alternatives, inline if/with/range bodies, and node-list entries.
  - Manual entry-length variables, paired truncations, range-body decrement clusters, and the
    per-item dot pop disappear. `SymbolicLocalState` remains explicit at each branch because its
    outcome is joined semantic data, not stack hygiene.
  - `ValuePathContext` now owns `Cell<u8>` dispatch depth. Literal equality, include truthiness, and
    literal-membership helper dispatches save/increment/restore the same context cell around nested
    decoding; the cap remains 2 and no thread-global state remains.
  - All 394 focused IR tests pass. The final immutable archive contains 90 binaries and 128 files;
    its clean schema dump passes 62/62 in 155.498 seconds with all 84 artifacts byte-identical to
    S-B, and its IR dump passes in 3.178 seconds with all 18 artifacts unchanged.
  - The full-depth battery passes in 85.886 seconds across 60 charts and 120,833 probes with zero
    flips, zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and
    7,465/7,465 mandatory third-level probes.

- Deviations: none. The first implementation compiled, passed 394/394 focused tests, and passed
  lint before the immutable archive was built.

- Adjudication evidence:
  - Helm 4.2.3 remains pinned and was used by the full-depth battery. All schema and symbolic-IR
    bytes match `9ac3d3ee`; zero acceptance cells change, so no fixture or Helm cell requires
    adoption.
  - Candidate-accepts/Helm-aborts remains zero against a zero allowance. Mandatory base and
    third-level coverage have zero drops; 25,710 disclosed bounded reductions remain outside the
    mandatory categories.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Structural control arms | Entry stacks and loop depth restored from one mark; control-flow IR and corpus identity. |
  | Range alternatives/items | Per-item predicates/dot/approximation restored together; range-mode and capture suites. |
  | Inline if/with/range | Same branch conditions and local joins with complete stack rewind; inline and scalar suites. |
  | Node-list loop control | Remaining predicate scoped per node while loop exit is read before rewind; terminal/fail tests. |
  | Helper literal equality | Context depth cap 2, same exact dispatch predicate; condition-predicate tests. |
  | Include truthiness/membership | Same cap and restored depth across nested dispatch; redis/cilium corpus and IR identity. |

- Review dossier:
  - Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-build
    cargo nextest archive --workspace --archive-file /private/tmp/arch-v4-sc-scope-final1.tar.zst`;
    exit 0, 90 binaries and 128 files.
  - Clean schema dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-schema
    SCHEMA_DUMP=1 cargo nextest run --archive-file /private/tmp/arch-v4-sc-scope-final1.tar.zst
    --profile integration --no-fail-fast -E 'test(schema_fixtures_match) | binary(/chart_corpus/) |
    test(lean_profile_schemas_match_their_separate_fixture_lane) | binary(/final_output_policy/)'`;
    exit 0, 62/62 in 155.498 seconds and 84 exact artifacts.
  - Clean IR dump: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-ir
    SYMBOLIC_DUMP=1 IR_DUMP=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sc-scope-final1.tar.zst --profile integration -E
    'test(ir_corpus_fixtures_match)'`; exit 0, one test in 3.178 seconds and 18 exact artifacts.
  - Full-depth proof: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-prober
    SCHEMA_ACCEPTANCE_BASELINE_REF=9ac3d3ee
    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-schema
    SCHEMA_PROBE_COVERAGE_REPORT=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-scope-final1-coverage.json
    ADJUDICATE_WITH_HELM=1 cargo nextest run --archive-file
    /private/tmp/arch-v4-sc-scope-final1.tar.zst --profile integration -E
    'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)' --run-ignored
    ignored-only`; exit 0 in 85.886 seconds with zero flips and zero mandatory drops.
  - Public/wire decision: none. Both carriers and all affected functions remain crate-private;
    serialized IR/schema formats are unchanged.

- Self-adversarial pass:
  - Kept the range-arm decrement until after loop-control extraction and arm-local evaluation; the
    outer rewind happens only at the same next-arm/final boundary where old stacks were truncated.
  - Rewound individual range items before reading their loop-control result, matching the former
    pop/truncate order while also restoring approximation and loop state as one invariant.
  - Did not fold locals into `ScopeMark`: declarations/assignments deliberately escape through
    branch and range joins, so treating them as stack state would erase semantics.
  - Verified each context constructor initializes a fresh dispatch cell and recursive decoder calls
    reuse `&self`; independent analyses cannot inherit another context's depth.
  - Confirmed no thread-local helper depth, entry-stack length variable, or manual scope truncation
    remains in the interpreter production tree.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0 in 1.00 seconds.
  - `task lint`: exit 0 in 34.76 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0, 48/48 combinations in 146.38 seconds.
  - `cargo nextest run --workspace`: exit 0, 1,333/1,333 tests in 104.153 seconds.
  - `task test:integration`: exit 0, 563/563 tests in 743.934 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0, 1,900/1,900 tests in 789.521 seconds; 24 tests skipped by profile and
    all live network tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 20.07 seconds.
  - downstream luup2 `check:local`: exit 0, 32/32 charts in 37.28 seconds, using the documented
    macOS shims and `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,802.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +9 (65,793 to 65,802). The typed scope mark and context cell add
  one explicit invariant each while deleting the parallel restoration/global-state protocols. This
  is an ordinary representation round, so no E-style LOC gate applies.

## S-C canonical forms — constructor-owned conjunctions and duplicate deletion

- Status: landed in `b18a7da7` (`refactor(ir): centralize conditional canonicalization`).
- Contract: representation-only. Make `GuardScopes`, `ContractRequirementImplication`, and
  `ConditionalPathOverlay` the canonicalization owners for their conditional-guard conjunctions;
  recursively flatten `AbstractValue::MergedLayers` at construction; and retain one owner for
  guard-value truthiness, Boolean predicates, and render-site identity/order.
- Acceptance baseline: `9a4d77e2` (S-C scope-discipline closure commit).
- Baseline production Rust LOC: 65,802.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, wire, or fixture byte changes. Every existing
    producer already intends these vectors as sets/conjunctions, so constructor canonicalization is
    expected to reproduce the current order exactly.
  - If a formerly unsorted producer changes only serialized guard ordering, stop before fixture
    adoption, prove semantic equivalence and Helm 4.2.3 behavior, and record it as the frozen plan's
    anticipated ordering-bug correction. Any other changed cell or byte rejects the round.
  - The three constructors sort and deduplicate every guard conjunction they accept; production
    call sites no longer hand-maintain the same invariant. Public read access may narrow to slices
    or accessors only where enforcement requires it, and any such Part-F API decision is recorded.
  - `AbstractValue::merged_layers` recursively preserves precedence order while flattening nested
    merges. Both read-time flatten helpers disappear, and every construction route uses the
    canonical constructor.
  - The shared guard-value truthiness and Boolean-predicate owners preserve Helm scalar semantics;
    the render-site key becomes the sole equality/ordering source without changing row grouping.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - `ConditionalGuard::canonicalize_conjunction` exhaustively owns recursive guard-tree sorting and
    deduplication. `GuardScopes::new`, `ContractRequirementImplication::new`, and
    `ConditionalPathOverlay::new` apply it at their construction boundaries; the overlay insertion
    method preserves the same invariant. Production has no direct implication/overlay struct
    construction and no hand-written conditional-guard `sort`/`dedup` pair in the migrated lanes.
  - `ContractRequirementImplication::new` also canonicalizes its requirement conjunction. Public
    construction tests pin top-level and nested guard deduplication plus requirement deduplication.
  - `AbstractValue::merged_layers` recursively flattens nested layers in precedence order. All
    production merge creation routes and shape-changing transformations use it; the two recursive
    read-time flatten helpers and nested-consumer compatibility arms are deleted.
  - Guard-value truthiness now has one shared owner, Boolean-to-predicate conversion has one owner,
    and contract normalization calls its base comparator directly instead of retaining a pure alias.
  - The final archive contains 90 binaries and 128 files. Its clean schema dump passes 62/62 in
    156.407 seconds: 80 artifacts remain byte-identical and four (`airflow`, `ingress-nginx`,
    `tempo`, `traefik`) contain only the registered canonical guard-order/derived `$defs` ordering
    rewrite. All 18 symbolic-IR artifacts remain byte-identical.
  - The final full-depth battery checks 120,833 probes across 60 charts with zero acceptance flips,
    zero candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and
    7,465/7,465 mandatory third-level probes. The schema-order-prefix disclosed category records
    25,990 bounded reductions after canonical schema ordering.

- Deviations:
  - The first lint preflight rejected two redundant method-call closures exposed by deleting the
    flatten adapters. Both became direct method references; no suppression or archive from that
    state was adopted.
  - The first archive command (`final1`) failed before producing an archive because its absolute
    `TMPDIR` had not been created, so clang could not create a temporary file. The harness was
    corrected by explicitly creating fresh step-local directories; no code or repository setting
    changed.
  - The `final2` clean dump exposed the four pre-registered ordering-only fixture changes. Its
    independent full-depth battery reported zero acceptance flips before those fixtures were
    adopted. A self-adversarial API cleanup then made inserted nested guards canonical and routed a
    remaining scalar Boolean spelling through the shared owner; protocol therefore discarded
    `final2` as authoritative and rebuilt `final3`. The final3 schema dump is byte-identical to
    final2, and its own clean full-depth battery independently repeats the zero-flip result.
  - The current tree contains standalone conditional conjunction carriers added after the frozen
    review's line inventory. They call the same exhaustive core canonicalizer rather than retaining
    hand-written sort/dedup pairs; the three scheduled phase constructors remain the canonical
    production boundaries.

- Adjudication evidence:
  - The four changed schemas move equivalent `allOf`/`anyOf` guard branches into structural order;
    the resulting definition-number shifts follow from deterministic traversal, not from a changed
    constraint. The generated schema fixtures were copied only from the final clean dump.
  - Helm 4.2.3 remains pinned and enabled in the full-depth battery. It reports zero acceptance
    flips, so there is no candidate-accepts/Helm-aborts cell to adopt or waive. Mandatory base and
    third-level coverage have zero drops.

- Producer/route coverage:

  | Route | Final behavior and proof |
  | --- | --- |
  | Guard scopes | Outer and nested conjunctions recursively canonicalized by `GuardScopes::new`; emission suite/full schemas. |
  | Requirement implications | Every production construction uses `new`; core public-surface test and contract suites. |
  | Conditional overlays | Initial and inserted kind guards preserve canonical order; four adjudicated schemas and overlay suites. |
  | Merged helper layers | Creation and shape-changing transforms flatten once in precedence order; focused constructor test and IR identity. |
  | Guard-value/Boolean helpers | One owner each across scalar, symbolic-local, comparison, and condition lanes; 1,060 focused tests. |
  | Render-site grouping | Comparator alias deleted with unchanged base comparator calls; normalization tests and IR identity. |

- Review dossier:
  - Focused proof: 1,060/1,060 core/IR/gen tests pass; corrected whole-workspace lint passes with
    only the two pre-existing ast-grep multiline-string warnings.
  - Immutable build: `TMPDIR=/Volumes/T7/dev/helm-schema/target/arch-v4-sc-canonical-final3-build
    cargo nextest archive --workspace --archive-file
    /private/tmp/arch-v4-sc-canonical-final3.tar.zst`; exit 0, 90 binaries and 128 files.
  - Clean schema dump: final3 archive and step-local `TMPDIR`; exit 0, 62/62 in 156.407 seconds.
    Final3 is byte-identical to the adjudicated final2 dump and to every adopted fixture; comparison
    against `9a4d77e2` identifies only the four registered ordering rewrites.
  - Clean IR dump: the same archive and step-local `TMPDIR`; exit 0, one test in 4.715 seconds; all
    18 artifacts are byte-identical to `9a4d77e2`.
  - Full-depth proof: same final3 archive, baseline `9a4d77e2`, Helm adjudication enabled; exit 0 in
    85.440 seconds, 60 charts, 120,833 probes, zero flips, zero unallowed accepted-abort cells, zero
    mandatory drops, and 25,990 disclosed bounded reductions.
  - Public/wire decision: additive public constructors plus a canonical conjunction operation and
    overlay insertion method. Existing public fields remain source-compatible; production writers
    deliberately migrate to the constructors. No serialized wire type or spelling changes.

- Self-adversarial pass:
  - The `ConditionalGuard` canonicalizer matches every variant explicitly and recursively descends
    through `Not`, `AllOf`, and `AnyOf`; no wildcard can hide a future condition variant.
  - Overlay insertion canonicalizes a nested guard before binary insertion, so the mutation path
    cannot bypass the constructor invariant. Whole-tree searches find no production direct overlay
    or implication struct literal.
  - Merge construction recursively flattens while iterating left-to-right, so precedence is
    preserved. All production variant creation outside the constructor was removed; remaining
    occurrences are destructures and two deliberate private tests.
  - The four schema changes reproduce byte-for-byte across final2 and final3, while IR remains
    unchanged and the full-depth result repeats. This rules out stale-binary or mixed-dump adoption.
  - No compatibility wrapper, cached parallel vector, lint suppression, or fallback identity was
    introduced.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0; the two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0; 48/48 feature combinations pass across three targets in 230.94 seconds.
  - `cargo nextest run --workspace`: exit 0; 1,334/1,334 pass in 104.409 seconds.
  - `task test:integration`: exit 0; 564/564 pass in 741.742 seconds; 24 tests skipped by profile.
  - `task test:all`: exit 0; 1,902/1,902 pass in 786.866 seconds; 24 tests skipped by profile and
    all live network tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 24.6 seconds.
  - downstream luup2 `check:local`: exit 0; 32/32 charts in approximately 43.5 seconds using the
    documented macOS shims and `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,760.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: -42 (65,802 to 65,760). This ordinary representation round
  deletes duplicated canonicalization/read-time compatibility and helper bodies; no E-style LOC
  gate applies.

## C3a — immutable loaded chart corpus

- Status: landed in `98edc212` (`refactor(engine): load chart sources once`).
- Contract: representation-only. Classify each discovered chart tree once, read every classified
  source once, and make one immutable `LoadedChartCorpus` the source owner for define indexing,
  manifest/NOTES analysis, static CRD collection, and `.Files.Get` registration.
- Acceptance baseline: `473ac567` (S-C canonical-forms closure commit).
- Baseline production Rust LOC: 65,760.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, public-API, wire, or fixture byte changes.
    File and chart traversal order remains stable and every former text-decoding failure remains a
    hard error at the same semantic consumer.
  - Session preparation constructs the corpus after discovery and Boolean-key validation, then
    passes it through define-index construction and chart analysis. No production consumer calls
    `files_with_role`, `list_chart_files`, or `read_to_string` for a loaded template, NOTES source,
    static CRD, or `.Files.Get` source.
  - Binary `.Files.Get` sources remain silently absent from the UTF-8 file-source index; text-only
    roles reject invalid UTF-8 rather than silently skipping it.
  - The corpus is crate-private and changes no public API or wire format. Candidate-accepts/
    Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - Session preparation now constructs one `LoadedChartCorpus` immediately after discovery and
    declaration validation. Each chart calls `list_chart_files` once; every classified file is read
    once into corpus-owned UTF-8 state before define indexing or analysis begins.
  - Define indexing, static CRD collection, manifest analysis, NOTES analysis, and `.Files.Get`
    indexing consume borrowed paths and sources from that snapshot. Whole-tree search finds no
    production `files_with_role` rescan and no loaded template/CRD/NOTES source `read_to_string`.
  - Binary `.Files.Get` inputs remain omitted from the text index, while a non-UTF-8 source reaching
    a text-only role returns a typed path-bearing error. A private snapshot test mutates the backing
    VFS after loading and proves analysis retains the original corpus-owned source.
  - All 84 schema artifacts and all 18 symbolic-IR artifacts are recursively byte-identical to
    `473ac567`. The full-depth battery checks 120,837 probes over 60 charts with zero flips, zero
    candidate-accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and
    7,465/7,465 mandatory third-level probes.
  - Clean corpus dump wall time is 156.656 seconds versus 156.407 seconds at the immediately prior
    canonical round (+0.16%, noisy-host range). Release Airflow is 85.18 seconds wall / 84.80 CPU,
    versus 85.49/85.39 after C2; the host was not isolated, as authorized by the user.

- Deviations:
  - The first all-target compiler pass was intentionally compiler-driven after the production seam
    moved: it rejected 25 private analysis-test call sites still passing `include_tests` instead of
    the snapshot. They were migrated mechanically to construct and share the same corpus shape;
    no archive or dump existed from that state.
  - That pass also exposed a local name collision between `LoadedChartCorpus` and `DefineCorpus` and
    one now test-only role helper. The define corpus was named explicitly and the dead production
    helper deleted rather than suppressed.
  - The prober's disclosed bounded category records 25,718 reductions and four more emitted probes
    than the prior round despite byte-identical schemas; mandatory categories remain complete and
    acceptance remains identical. This is reported as battery sampling variance, not normalized.

- Adjudication evidence: Helm 4.2.3 remains pinned and was enabled in the final full-depth run.
  Schema and IR bytes are exact and all acceptance cells are unchanged, so no fixture or individual
  Helm verdict was adopted. Candidate-accepts/Helm-aborts and mandatory coverage drops are zero.

- Producer/route coverage:

  | Route | Final owner and proof |
  | --- | --- |
  | Template/helper sources | Snapshot feeds `DefineIndex`; helper and corpus identity. |
  | Manifest templates | Snapshot feeds structural contract and template CRD extraction; 84 schemas/18 IR. |
  | NOTES templates | Snapshot feeds text-program contract lane; NOTES regressions and corpus identity. |
  | Static CRDs | Snapshot feeds local schema universe; CRD tests and downstream charts. |
  | `.Files.Get` sources | Snapshot preserves UTF-8 filtering and relative keys; file-backed helper tests. |

- Review dossier:
  - Focused proof: 109/109 `helm-schema` tests pass in 103.587 seconds; whole-workspace lint passes
    in 38.43 seconds with only the two pre-existing ast-grep warnings.
  - Immutable build: C3a `final1`, 90 binaries and 128 files.
  - Clean schema dump: final1 archive and step-local `TMPDIR`; exit 0, 62/62 in 156.656 seconds;
    all 84 artifacts are byte-identical to the baseline.
  - Clean IR dump: same archive and step-local `TMPDIR`; exit 0, one test in 4.561 seconds; all 18
    artifacts are byte-identical.
  - Full-depth proof: same archive, baseline `473ac567`, Helm adjudication enabled; exit 0 in 86.438
    seconds, 60 charts, 120,837 probes, zero flips, zero unallowed accepted-abort cells, zero
    mandatory drops, and 25,718 disclosed bounded reductions.
  - Public/wire decision: none. The snapshot and its lookup errors are crate-private; public
    analysis artifacts and serialized formats are unchanged.

- Self-adversarial pass:
  - A file with both static-CRD and `.Files.Get` roles stores one source and serves both consumers;
    role overlap therefore cannot reintroduce a second read.
  - Snapshot keys use the discovered chart directory identity, while iteration remains the
    pre-existing stable path order. A missing chart entry is a typed invariant error rather than an
    empty iterator that could silently drop analysis.
  - Invalid UTF-8 is skipped only for `.Files.Get`; text-only roles call `source()` and fail. The
    owned-source mutation test proves consumers do not fall back to the VFS after preparation.
  - Values, Chart metadata, and Boolean-key scans remain outside this source snapshot because they
    have distinct composition/validation semantics; the removed repeated reads are precisely the
    roles named by C3.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 38.43 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0; 48/48 combinations in 140.93 seconds.
  - `cargo nextest run --workspace`: exit 0; 1,335/1,335 pass in 105.506 seconds.
  - `task test:integration`: exit 0; 564/564 pass in 762.724 seconds; 24 skipped by profile.
  - `task test:all`: exit 0; 1,903/1,903 pass in 808.746 seconds; 24 skipped and live tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 12.37 seconds.
  - downstream luup2 `check:local`: exit 0; 32/32 charts using the documented macOS shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,796.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +36 (65,760 to 65,796). The typed snapshot adds explicit source
  ownership and invariant errors while deleting all repeated role scans/reads in its scheduled
  lanes; this is an ordinary representation round, so no E-style LOC gate applies.

## C3b — shared lazy parsed defines

- Status: landed in `320ad739` (`perf(ir): share parsed helper programs`).
- Contract: representation-only. Extend the loaded-source boundary with one shared
  `ParsedDefines` artifact: define bodies are discovered once, while each helper program's
  tree-sitter tree and flattened expression list initialize lazily and are shared by every
  chart-local `SymbolicIrContext`.
- Acceptance baseline: `18761889` (C3a ledger closure commit).
- Baseline production Rust LOC: 65,796.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, public wire, or fixture byte changes.
    Stable file order, last-define-wins resolution, implicit `@file:` names, and multi-owner
    optional-dependency attribution remain exact.
  - `IrAnalysisDb` no longer clones file sources or reparses define blocks per chart context, and
    its recognizers consume cached expressions instead of re-running `parse_action_expressions` on
    the same helper body.
  - Trees and expressions remain lazy per body. A helper never inspected by analysis must not pay
    either parse cost; expression-only consumers must not force the tree cache.
  - Measure clean corpus and release Airflow against C3a. Any material eager-work regression rejects
    the implementation for redesign even if bytes remain exact.
  - Candidate-accepts/Helm-aborts allowance and mandatory base/third-level drops remain zero.

- Measured results:
  - One `ParsedDefines` discovers define boundaries and Helm's last-definition winner once for the
    whole chart tree. Every chart-local `SymbolicIrContext` clones its `Rc` handle instead of
    cloning file sources, define bodies, implicit-file maps, and tree caches.
  - Each cached body owns independent `OnceCell<Option<Tree>>` and `OnceCell<Vec<TemplateExpr>>`
    cells. Tree consumers initialize both views through `parsed_helper_body`; expression-only
    wrapper analysis initializes only expressions. Six repeated body-wide expression parses and
    the per-context tree map are deleted.
  - `DefineCorpus` borrows the parsed artifact for winning bodies and uses its complete stable
    source-path set for multi-owner attribution. The former public `define_bodies_in_source`
    projection and its second full-source parse disappear.
  - All 84 schema and 18 symbolic-IR artifacts are byte-identical to C3a. The full-depth battery
    checks 120,837 probes across 60 charts with zero flips, zero candidate-accepts/Helm-aborts,
    112,260/112,260 mandatory base probes, and 7,465/7,465 third-level probes.
  - Clean corpus wall time improves from 156.656 to 154.426 seconds (-1.42%). Release Airflow moves
    from 85.18/84.80 to 85.13/84.72 seconds wall/CPU. Both measurements used the authorized
    non-isolated host; there is no eager-work regression.

- Deviations:
  - The first compiler-driven all-target pass rejected one leftover `&&[TemplateExpr]` loop after
    expression ownership narrowed to a borrowed slice. The redundant reference was removed before
    any archive or dump.
  - The first lint preflight then found one needless borrow of the now-borrowed cached tree in
    fragment-summary fact collection. It was removed directly; no suppression and no artifact from
    that state was adopted.
  - The frozen plan described `ParsedDefines` as a session-owned extension. Cross-crate sharing
    requires the carrier itself and its read-only winner/owner queries to be public in
    `helm-schema-ir`; this deliberately replaces, rather than layers beside, the broader public
    `define_bodies_in_source` projection.

- Adjudication evidence: Helm 4.2.3 remains pinned and enabled. Exact schema/IR bytes and zero
  acceptance flips require no fixture or per-cell adoption; candidate-accepts/Helm-aborts and both
  mandatory coverage-drop categories remain zero. The disclosed bounded category is 25,718.

- Producer/route coverage:

  | Route | Final owner and proof |
  | --- | --- |
  | Define discovery/winner | `ParsedDefines::new`; duplicate-name and helper-resolution suites. |
  | Owner attribution | Complete source-path sets consumed by `DefineCorpus`; optional dependency tests. |
  | Helper tree consumers | Lazy tree cell shared by fragment/control recognizers; 504 focused tests and IR identity. |
  | Expression-only wrapper analysis | Lazy expression cell without tree initialization; nats wrapper re-audits. |
  | Implicit `@file:` programs | Parsed artifact owns implicit map and bodies; `.Files.Get`/file-template tests. |
  | Per-chart policies | Shared parsed programs plus distinct immutable policy maps; full corpus identity. |

- Review dossier:
  - Focused proof: 504/504 engine/IR tests pass in 102.250 seconds; corrected whole-workspace lint
    passes in 35.58 seconds with only the two pre-existing ast-grep warnings.
  - Immutable build: C3b `final1`; exit 0, 90 binaries and 128 files.
  - Clean schema dump: final1 archive and step-local `TMPDIR`; exit 0, 62/62 in 154.426 seconds; all
    84 artifacts are byte-identical to C3a.
  - Clean IR dump: same archive and step-local `TMPDIR`; exit 0, one test in 4.787 seconds; all 18
    artifacts are byte-identical.
  - Full-depth proof: same archive, baseline `18761889`, Helm enabled; exit 0 in 85.536 seconds, 60
    charts, 120,837 probes, zero flips, zero unallowed accepted-abort cells, zero mandatory drops,
    and 25,718 disclosed bounded reductions.
  - Public/wire decision: add the read-only `ParsedDefines` carrier and remove
    `define_bodies_in_source`. This is an intentional source-API replacement required for
    cross-crate single ownership. No serialized wire type or bytes change.

- Self-adversarial pass:
  - Stable `DefineIndex::file_sources` order still drives definition overwrite order, while every
    defining path is retained separately for ownership. Winner and owner information therefore do
    not collapse into one lossy map.
  - `program_wrapper_sentinels` calls the expression accessor directly; a whole-tree search finds
    no helper-body `parse_action_expressions` call except the single cell initializer. Subrange
    expressions remain separately parsed because they are different source slices.
  - `IrAnalysisDb` owns only the cheap parsed handle plus policy-dependent caches. File sources,
    helper bodies, trees, and flattened expressions each have one chart-tree owner.
  - Byte identity, a faster corpus dump, and flat Airflow CPU jointly rule out semantic drift and
    eager parsing hidden by the representation change.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in 35.58 seconds; two pre-existing ast-grep warnings remain informational.
  - `task lint:fc`: exit 0; 48/48 combinations in 190.37 seconds.
  - `cargo nextest run --workspace`: exit 0; 1,335/1,335 pass in 104.238 seconds.
  - `task test:integration`: exit 0; 564/564 pass in 741.797 seconds; 24 skipped by profile.
  - `task test:all`: exit 0; 1,903/1,903 pass in 786.523 seconds; 24 skipped and live tests pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 20.65 seconds.
  - downstream luup2 `check:local`: exit 0; 32/32 charts with the documented host shims.
  - `task tokei:core`: exit 0; production Rust LOC is 65,867.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +71 (65,796 to 65,867). The explicit shared artifact and its
  lazy cells replace repeated source/body/tree/expression ownership; this ordinary representation
  round has no E-style LOC gate.

## C1a — total schema-node runtime type and default relaxation operations

- Status: landed in `32171b72` (`refactor(gen): move schema operations onto typed nodes`).
- Contract: representation-only. Move runtime-type inference and recursive removal of
  default-supplied `required` members from raw `serde_json::Value` keyword walks onto exhaustive
  `SchemaNode` operations.
- Acceptance baseline: `1a526b93` (C3b ledger closure commit).
- Baseline production Rust LOC: 65,867.
- Pre-registered acceptance expectations:
  - Zero schema, symbolic-IR, diagnostic, acceptance, wire, or fixture byte changes.
  - Runtime types use the typed `JsonSchemaType` domain and exhaustively traverse Boolean schemas,
    explicit object/array nodes, typed keywords, combinators, `const`, and `enum`. Unknown extra
    keywords remain lossless and cannot be mistaken for an unsupported schema shape.
  - Default relaxation mutates typed `required`, `properties`, and `allOf`/`anyOf`/`oneOf` children
    directly, preserving legacy recursion and emitted key ordering exactly.
  - Any changed byte rejects this representation round. Candidate-accepts/Helm-aborts allowance
    and mandatory base/third-level drops remain zero.

- Measured results:
  - `SchemaNode::runtime_types` now owns the exhaustive JSON runtime-domain projection. Typed
    object/array nodes, Boolean schemas, typed keyword schemas, `type`, `const`, `enum`, and every
    combinator preserve the deleted raw walk's exact domain, including integer membership in the
    JSON `number` domain.
  - `SchemaNode::relax_required_members_supplied_by_default` now removes defaulted names and
    descends through typed properties and `allOf`/`anyOf`/`oneOf` children without serializing the
    tree to JSON for inspection. Unknown keywords remain in `extra_keywords` and round-trip
    untouched.
  - Provider-fragment string admission and fail-requirement compatibility now compare the typed
    `JsonSchemaType` domain. The raw string-domain helper and the generator-level raw recursive
    mutation function are deleted.
  - Two private regression tests cover combinator/type/const/enum/Boolean runtime domains and
    recursive nested-default relaxation with an unknown keyword. All 84 schema artifacts and all
    18 symbolic-IR artifacts are byte-identical to `1a526b93`.
  - The full-depth battery checks 120,837 probes over 60 charts with zero flips, zero candidate-
    accepts/Helm-aborts cells, 112,260/112,260 mandatory base probes, and 7,465/7,465 mandatory
    third-level probes. The disclosed bounded category remains 25,718 reductions.

- Deviations:
  - The first compiler/lint preflight exposed merged exhaustive arms, one flattenable optional-
    combinator loop, and two implicit string clones after the type move. They were simplified
    directly; no suppression, archive, dump, or fixture was produced from that state.
  - Final1 was sealed and proved byte-exact, but the subsequent full lint gate found that the new
    YAML test fixture used an escaped multiline string. The test now constructs the same value
    structurally. Final1 is rejected evidence: final2 rebuilt the archive and repeated the clean
    schema dump, IR dump, and full-depth battery before any gate result was adopted.
  - The two call sites that still receive raw schema values perform the existing lossless
    `SchemaNode::from_value` boundary conversion. C1b/C1c type their owning carriers; C1a does not
    layer a second runtime-domain or mutation representation in the meantime.

- Adjudication evidence: Helm 4.2.3 remains pinned and was enabled in the final full-depth run.
  Exact schema/IR bytes and zero acceptance flips leave no fixture or per-cell verdict to adopt.
  Candidate-accepts/Helm-aborts and mandatory coverage drops are zero.

- Producer/route coverage:

  | Route | Final owner and proof |
  | --- | --- |
  | Fail requirement compatibility | Typed `JsonSchemaType` subset comparison; focused requirement suites and 84 exact schemas. |
  | Provider string requirements | `SchemaNode::runtime_types`; provider synthesis suites and exact corpus. |
  | Root/default relaxation | Typed required/property/combinator descent; recursive private test and nullability/default suites. |
  | Boolean and unknown schemas | Exhaustive Boolean arms plus lossless `extra_keywords`; private round-trip/domain tests. |

- Review dossier:
  - Focused proof: 6/6 schema-node tests pass, including the two new total-operation tests.
  - Immutable build: C1a final2; exit 0, 90 binaries and 128 files. Final1 is rejected as described
    above.
  - Clean schema dump: final2 archive and absolute step-local `TMPDIR`; exit 0, 62/62 in 159.523
    seconds; all 84 artifacts are byte-identical to C3b.
  - Clean IR dump: the same archive and its own step-local `TMPDIR`; exit 0, one test in 5.031
    seconds; all 18 artifacts are byte-identical.
  - Full-depth proof: same final2 archive, baseline `1a526b93`, Helm enabled; exit 0 in 89.962
    seconds, 60 charts, 120,837 probes, zero flips, zero unallowed accepted-abort cells, zero
    mandatory drops, and 25,718 disclosed bounded reductions.
  - Public/wire decision: none. `SchemaNode`, `JsonSchemaType`, and both operations remain
    crate-private; serialized schemas, symbolic IR, diagnostics, and public APIs are unchanged.

- Self-adversarial pass:
  - `number` continues to admit both integer and non-integer JSON numbers, while `const` and `enum`
    intersections retain the old numeric spelling distinction. Union arms are joined before the
    outer intersection; `allOf` arms intersect one by one.
  - Boolean false remains the empty runtime domain and Boolean true remains the total domain.
    Untyped object hosts stay total rather than being accidentally narrowed by their storage
    representation.
  - Default relaxation removes only string names represented by the typed `required` carrier and
    follows only mapping defaults. It preserves non-defaulted order and recursively applies the
    same complete default document to combinator branches, matching the deleted function.
  - Whole-tree search finds one runtime-domain implementation and one default-relaxation
    implementation. No raw keyword walker, compatibility table, lint suppression, or fixture
    rewrite remains.

- Gates on the final tree:
  - `cargo fmt --check`: exit 0.
  - `task lint`: exit 0 in approximately 107 seconds; the two pre-existing ast-grep warnings in
    `helm-schema-ast` remain informational.
  - `task lint:fc`: exit 0; 48/48 feature combinations across three targets in 441.45 seconds.
  - `cargo nextest run --workspace`: exit 0; 1,337/1,337 pass in 105.879 seconds.
  - `task test:integration`: exit 0; 564/564 pass in 787.131 seconds; 24 skipped by profile.
  - `task test:all`: exit 0; 1,905/1,905 pass in 983.005 seconds; 24 skipped and live network tests
    pass.
  - `cargo install --path ./crates/helm-schema-cli/`: exit 0 in 16.75 seconds.
  - downstream luup2 `check:local`: exit 0; 32/32 charts with the documented host shims and
    `/Users/roman/.cargo/bin/helm-schema`.
  - `task tokei:core`: exit 0; production Rust LOC is 65,906.
  - `git diff --exit-code bb61a78f -- plan/architecture-review-v4.md`: exit 0.
  - `git diff --check`: exit 0.

- Measured production LOC delta: +39 (65,867 to 65,906). This ordinary representation round adds
  exhaustive typed operations and focused tests while deleting the parallel raw walkers; no
  E-style LOC gate applies.
