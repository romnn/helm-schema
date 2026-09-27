---
name: corpus-fixtures
description: >-
  How helm-schema's corpus charts, produced artifacts and committed fixtures fit together. Use when
  running or debugging the corpus producer (`corpus_generation`, `.cache/corpus-generation`,
  `manifest.json`, `.corpus-generation.lock`); when `task test:integration` reports a fixture
  mismatch, a stale or missing manifest, or an owned output directory; when regenerating or
  adopting fixtures under `testdata/chart-corpus-schemas/`, `testdata/emission-profile-schemas/`,
  `testdata/final-output-schemas/`, or `crates/helm-schema-{gen,ir}/tests/fixtures/`; when telling
  an ordering-only fixture diff from a semantic one; when a ledger mentions `SCHEMA_DUMP`,
  `IR_DUMP`, or `SYMBOLIC_DUMP`; or when adding a chart to the corpus.
---

# Corpus fixtures

Paths are relative to the repository root. The code is authoritative: `plan/` ledgers describe
dump mechanisms that no longer exist (see "Legacy dump variables").

Related skills: `acceptance-battery` (flip adjudication of changed fixtures), `verification-gates`
(the gate list and tooling), `landing-workflow` (landing runs that dump, verify and adopt).

## Mental model: one producer, one registry, many consumers

- **Registry** — `crates/helm-schema-test-support/src/registry.rs` is the closed list of every
  fixture-backed artifact: typed id (`ArtifactId`), produced file name, fixture path, and the full
  generation recipe. The `corpus_charts!` macro is the chart roster; both `ChartId` and the
  per-chart tests in `crates/helm-schema-cli/tests/chart_corpus.rs` expand from it.
- **Producer** — the `corpus_generation` bin generates the whole registry (203 artifacts: 202
  with fixtures plus one internal) into an output directory and writes `manifest.json` **last**.
- **Consumers** — `helm_schema_test_support::consume(ArtifactId)` reads a verified artifact from
  the directory named by `HELM_SCHEMA_CORPUS_ARTIFACTS`; when the variable is unset, it generates
  that one artifact locally and asserts it equals its fixture. Only chart artifacts are consumed
  this way (`chart_corpus.rs` and the `chart_*.rs` semantic tests). The template, IR, lean and
  final-output fixture tests always generate locally and compare with the committed file.
- **Artifacts are byte-identical to fixtures.** Pretty JSON with sorted object keys; chart, lean
  and final-output files end in a newline, template and IR files do not. Adopting a fixture is a
  plain `cp`.

The full family table (file names, fixture paths, checking tests, hand-maintained fixtures) is in
`references/fixture-families.md`.

## The producer

```sh
# What `task test:integration` runs first (then the integration profile with
# HELM_SCHEMA_CORPUS_ARTIFACTS set to the same directory):
cargo run -p helm-schema-test-support --bin corpus_generation -- \
  --out "$PWD/.cache/corpus-generation"

# Recheck a finished directory completely (provenance, registry membership, recipe inputs,
# artifact bytes, no foreign files). Run it right before adopting anything.
cargo run -p helm-schema-test-support --bin corpus_generation -- \
  --verify "$PWD/.cache/corpus-generation"

# Reproduce single artifacts with their exact recipe; prints manifest entries as JSON lines and
# never writes a manifest. Use a scratch directory, never the shared one.
cargo run -p helm-schema-test-support --bin corpus_generation -- \
  --out "$PWD/target/corpus-only" --only chart/cilium --only lean/schema-emission-controls
```

- Task variables: `task test:integration CORPUS_ARTIFACTS=/abs/dir CORPUS_JOBS=8 -- <nextest args>`.
  Defaults are `<repo>/.cache/corpus-generation` (gitignored) and a host-sized job count: one
  job per 4.5 GiB of memory, at most cores − 2 (`Machine::corpus_jobs`; an 11-core, 18 GiB
  laptop gets 4). Each job holds a whole-chart analysis in memory, and the largest charts set
  a peak of about 5 GiB on their own. Nextest arguments after `--` filter only the tests; the
  producer still generates the whole registry.
- `--only` keys: `chart/<dir>`, `template/<dump_stem>`, `lean/<chart>`, `final-policy/<name>`,
  `ir/<template path>` (see `ArtifactId::key`).
- `--helm-ready` also writes `internal/<name>.helm.schema.json` and `internal/<name>.defs-map.json`
  (short `$defs` keys, what Helm is handed) and records them in the manifest. Landing runs use it.
- Output layout: `<out>/<artifact file>` for fixture-backed artifacts, `<out>/internal/` for the
  internal `signoz-postgresql` chart artifact and helm-ready companions, `<out>/manifest.json`, and
  `<out>/.corpus-generation.lock` while a run owns the directory.

### What the manifest binds, and what invalidates it

`manifest.json` records `harness_version`, `build` (`source_sha256`, target triple, cargo profile),
`producer_binary_sha256` (informational only), the absolute `testdata` path, and per artifact
`key`, `kind`, `file`, `fixture`, `recipe`, `inputs_sha256`, `size`, `sha256`.

- **Source digest** (`src/source_digest.rs`, embedded by the crate's `build.rs`): `Cargo.toml`,
  `Cargo.lock`, `.cargo/config.toml`, the vendored go-template grammar `src/`, and for every crate
  in `GENERATION_CRATES` its `Cargo.toml`, `build.rs` and `src/`, **excluding anything under a
  `tests` directory**. Editing tests never invalidates a manifest; editing generation code always
  does. `helm-schema-cli` is not a generation crate: corpus artifacts are library output
  (`AnalysisSession` plus minimization), never CLI output.
- **Recipe inputs** (`InputDigester`): a chart recipe reads its whole `testdata/charts/<dir>`,
  its values files and the entire `testdata/provider-bundle/`; template recipes read their template,
  values file (unless inline) and helpers plus the bundle; IR recipes read template and helpers.
  Committed fixtures are **not** inputs, so adopting fixtures leaves the manifest valid.
- **Location**: the manifest is bound to its checkout's absolute `testdata` path. Every worktree
  needs its own producer run.
- The producer refuses to start when the tree no longer matches its compiled digest, captures
  every input digest before dispatch, and re-runs the full `--verify` before publishing the
  manifest. A run racing an edit therefore fails closed instead of mixing states.

Error messages and their fixes: `references/producer-errors.md`.

## Regenerating and adopting fixtures

Do this once, after the **final** code edit of the round. Never assemble an adoption set from
two producer runs, from `--only` output, or from legacy `SCHEMA_DUMP` files.

1. Stop editing generation crates. Make sure no other session is using the output directory.
2. Produce: `task test:integration` (produces, then runs the suite), or the producer command
   above alone. Chart-corpus mismatches then show up as `chart_corpus` failures; the template,
   IR, lean and final-output tests show only their first mismatch (see Pitfalls).
3. Verify the dump: `corpus_generation --verify "$OUT"` (command above).
4. List every fixture the dump changes, straight from the manifest:

   ```sh
   OUT="$PWD/.cache/corpus-generation"
   jq -r '.artifacts[] | select(.fixture != null) | [.file, .fixture] | @tsv' "$OUT/manifest.json" |
     while IFS=$'\t' read -r file fixture; do
       cmp -s "$OUT/$file" "$fixture" || printf '%s\t%s\n' "$file" "$fixture"
     done > target/changed-fixtures.tsv
   ```

5. Triage each diff (next section), then adjudicate. **Every fixture byte change needs flip
   adjudication** against real `helm template`: run the battery per the `acceptance-battery`
   skill with `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP="$OUT"` (it reads the chart-corpus and lean
   artifacts under the same file names). The battery does not read template, IR or final-output
   artifacts; review those diffs by hand, reasoning against Helm for schema families.
6. Adopt exactly the adjudicated rows:

   ```sh
   while IFS=$'\t' read -r file fixture; do cp "$OUT/$file" "$fixture"; done < target/changed-fixtures.tsv
   ```

7. Re-run the consumers against the same dump; no new producer run is needed because fixtures are
   not recipe inputs:

   ```sh
   HELM_SCHEMA_CORPUS_ARTIFACTS="$PWD/.cache/corpus-generation" \
     cargo nextest run -P integration --workspace --all-targets --no-tests warn
   ```

   The final gate is still `task test:integration` on the final tree (`verification-gates`).
8. Roster tests in `chart_corpus.rs` are self-enforcing: when a chart's defaults start to validate,
   its test fails and tells you to remove it from `KNOWN_VALUES_REJECTIONS` or
   `QUARANTINED_FALSE_REJECTIONS`. Do the removal in the same change as the fixture. Charts in
   `UNADJUDICATED_INTAKE` pin current output, not correct output: a diff there means "changed",
   never "regressed" by itself.

Fast iteration on one chart without a producer run (local generation, full fixture diff on
mismatch):

```sh
cargo nextest run -P integration -p helm-schema-cli --test chart_corpus -E 'test(=cilium)'
```

## Semantic change or ordering-only?

Object keys are always sorted (`serde_json` without `preserve_order`), so key order never
differs. Array order does, and fixture tests compare `serde_json::Value`, where arrays are
order-sensitive:

- `allOf` and `anyOf` members are sorted by their canonical JSON string
  (`sort_schemas_by_canonical_json` in `crates/helm-schema-gen/src/merge.rs`, key from
  `helm_schema_json_schema_walk::canonical_json_string`). A content change inside one member can
  move it and reorder all its siblings, producing a large diff around a small change.
- Minimized `$defs` get readable source-path names; a name already taken gets an `@2`, `@3`, …
  suffix (`crates/helm-schema-json-schema-minify/src/naming.rs`). A new or removed definition can
  shift suffixes and rename `$ref`s without changing meaning.

Triage recipe: sort the members of keywords whose order carries no meaning, then diff.

```sh
cat > target/canon.jq <<'EOF'
walk(if type == "object" then with_entries(
  if (.key | IN("allOf", "anyOf", "oneOf", "required", "enum", "type")) and (.value | type) == "array"
  then .value |= sort_by(tojson) else . end) else . end)
EOF
diff <(git show HEAD:testdata/chart-corpus-schemas/nats.schema.json | jq -S -f target/canon.jq) \
     <(jq -S -f target/canon.jq "$OUT/helm-schema.cli.chart-corpus.nats.schema.json")
```

An empty canonical diff means ordering-only. It is triage, not a verdict: it also sorts data
arrays under those key names, and it cannot see through `$defs` renames. An ordering-only change
still changes fixture bytes, so it still goes through the battery.

## Adding a chart to the corpus

1. Vendor the chart, dependencies included exactly as packaged, under `testdata/charts/<dir>/`,
   for example `helm pull --repo <repo> <chart> --version <pinned> --untar --untardir <scratch>`
   and move the directory holding `Chart.yaml` into place.
2. Add `(<snake_test_name>, <Variant>, "<dir>")` to `corpus_charts!` in `registry.rs`. This
   creates the `ChartId`, the registry entry and the `chart_corpus` test.
3. Update every hard-coded count: `FIXTURE_COUNTS` in `registry.rs`, and in
   `crates/helm-schema-test-support/tests/registry.rs` the total (`specs.len()`) and the
   `"chart fixtures: N+1, expected N"` string in `validate_names_every_broken_rule`.
4. Provider bundle: generation reads only `testdata/provider-bundle/` (Kubernetes
   `v1.29.0-standalone-strict`, `allow_net: false`). A kind or CRD missing from it silently drops
   provider-backed facts. The corpus expansion grew the bundle by generating with a scratch copy of
   the bundle as cache and network enabled for misses, then committing only the new files. After
   any bundle change, every chart and template artifact is stale, and all existing fixtures must
   regenerate byte-identical unless the change is meant to move them.
5. Produce, then copy the new artifact to `testdata/chart-corpus-schemas/<dir>.schema.json`
   (`fixture_directories_hold_exactly_the_registered_fixtures` requires the file to exist).
6. Classify the chart's own coalesced defaults in `chart_corpus.rs`: Helm refuses them →
   `KNOWN_VALUES_REJECTIONS`; Helm renders them but the schema rejects them →
   `QUARANTINED_FALSE_REJECTIONS`; Helm cannot load the chart → `HELM_UNLOADABLE_CHARTS` with the
   reason substring. A first fixture that has not been adjudicated also goes into
   `UNADJUDICATED_INTAKE`, which must contain every quarantined chart
   (`quarantine_rosters_are_consistent`).

A new workspace crate linked into generation must be added to `GENERATION_CRATES`
(`generation_crates_are_the_linked_workspace_crates` enforces it).

## Legacy dump variables

Checked against the code; ledgers still use the removed forms.

- `SCHEMA_DUMP`: removed from `chart_corpus`, the gen corpus and the IR corpus; the producer
  replaced it. It survives in two tests. `lean_profile_schemas_match_their_separate_fixture_lane`
  (`crates/helm-schema/tests/schema_emission_profiles.rs`, optional `SCHEMA_DUMP_CHART=<chart>`)
  writes `$TMPDIR/helm-schema.emission-profile.lean.<chart>.schema.json` and **skips the
  assertion**, so a pass under it proves nothing; prefer the producer's lean artifacts.
  `generates_schema_for_fixture_chart_without_k8s_provider` (`crates/helm-schema-cli/tests/cli.rs`)
  writes `$TMPDIR/helm-schema.cli.full-fixture.disable-k8s.schema.json` and still asserts.
- `IR_DUMP`: only `eprintln!`s the IR in four `extractor_inline_fixtures.rs` tests. `RANGE_VAR_DUMP`
  does the same in a helm-schema-ast unit test. Neither writes a fixture.
- `SYMBOLIC_DUMP`, the `IrCorpusCase` `dump_env` field, and `schema_roundtrip.rs` no longer exist.
- The other `SCHEMA_ACCEPTANCE_*`, `ADJUDICATE_WITH_HELM`, and `SCHEMA_HELM_*` variables belong to
  the battery and the Helm harness; see `acceptance-battery`.

## Pitfalls

- **Vacuous green unit run.** The default nextest profile is `not kind(test)`, so
  `cargo nextest run --workspace` and `task test` run no `tests/` binary and check no fixture.
  Fixtures, hygiene gates and registry tests run only in `-P integration` (`task test:integration`,
  `cargo ti`).
- **Zero tests is not green.** Tasks pass `--no-tests warn`; a mistyped `-E` filter "passes" with
  zero tests. Read nextest's `Starting N tests` count.
- **First mismatch hides the rest.** `schema_fixtures_match` (gen), `ir_corpus_fixtures_match`
  (IR) and the lean lane loop over every case and stop at the first failing one. Use the manifest
  listing in step 4 for the complete set.
- **"the locally generated artifact differs from its fixture"** inside a `chart_*.rs` semantic
  test means fixture drift (the variable was unset), not a failed semantic assertion.
- **Relative artifact paths.** Nextest runs test binaries from the package directory, so a
  relative `HELM_SCHEMA_CORPUS_ARTIFACTS` or `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` resolves to the
  wrong place. Always pass `"$PWD/..."`.
- **Shared output directory.** A producer run deletes `manifest.json` first. A second session
  running `task test:integration` on the same checkout breaks the first session's consumers. Give
  parallel work its own `CORPUS_ARTIFACTS=/abs/dir`.
- **`--only` into the shared directory** overwrites artifacts under a manifest that no longer
  matches (`artifact ... differs from its manifest entry`). Use a scratch directory.
- **Release builds.** The manifest binds the cargo profile; a producer built with `--release` is
  refused by the debug-built tests as a stale producer.
- **Ad-hoc CLI runs are not corpus evidence.** CLI defaults differ (tests included, Kubernetes
  `v1.35.0`, the user cache, network allowed). Reproduce a corpus artifact with `--only`.
- **`$TMPDIR` fills up.** Legacy `SCHEMA_DUMP` output goes to `$TMPDIR`; chart schemas reach tens
  of MB. When `/tmp` is a full tmpfs, shell commands fail with exit 1 and no output. Point
  `TMPDIR` into `target/` for such runs.
