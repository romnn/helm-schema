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

## Round A1 — key provider memoization by stringification policy

- Status: landed in this round's `fix(gen)` commit.
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

- Status: landed in this round's `fix(engine)` commit.
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

- Status: landed in this round's `fix(ir)` commit.
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

- Status: landed; commit pending.
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

- Status: landed; commit pending.
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

- Status: landed; commit pending.
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

- Status: landed; commit pending.
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

- Status: complete; commit pending.
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
