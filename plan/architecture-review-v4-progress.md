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

- Status: landed in `c72e7d1e`.
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

- Status: landed; commit pending.
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

- Status: landed; commit pending.
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
