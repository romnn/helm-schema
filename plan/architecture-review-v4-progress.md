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
