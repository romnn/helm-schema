# Acceptance battery environment variables

Every variable below is read by the battery family in
`crates/helm-schema/tests/schema_emission_profiles.rs`, its
`crates/helm-schema/tests/common/` harness and the shared Helm layer in
`crates/helm-schema-test-support/src/helm/`. Re-grep before trusting this table
after a harness change:

```sh
rg -n 'env::var(_os)?\("' crates/helm-schema/tests/
```

"Presence" means the code only checks that the variable exists: any value,
including `0` or the empty string, switches the behavior on. Unset it to turn
it off.

## Comparison inputs

| Variable | Read by | Meaning | Default | Required |
|---|---|---|---|---|
| `SCHEMA_ACCEPTANCE_BASELINE_REF` | corpus battery tests (`round74_…`, `round73_…`, `round68..72_…`, `early_provider_definition_pruning_…`) | Git ref whose committed fixtures are the **baseline**. Each chart's fixture is read with `git show <ref>:testdata/…` and regex-normalized (`read_schema_at_ref`). Any ref spelling works; live runs resolve it with `git rev-parse` and compare the full sha with `ROSTER_BASELINE`. | none | yes (the test fails with "must name the comparison commit") |
| `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` | corpus battery tests | Directory holding the **candidate** schemas as producer artifacts: `helm-schema.cli.chart-corpus.<chart>.schema.json` and `helm-schema.emission-profile.lean.<chart>.schema.json`. Produce with `--out <dump> --helm-ready`; retain the producer manifest and hash-verified Helm-ready companions, required by the lint lane in every mode. | none: required in every mode | the battery fails immediately without it ("SCHEMA_ACCEPTANCE_CANDIDATE_DUMP must name the producer dump of the candidate build"), live or schema-only; the committed fixtures are never a candidate |
| `SCHEMA_ACCEPTANCE_CONTEXT_ROSTER` | `common/lint_probes.rs` | Optional version-2 JSON roster of complete overlays with transport, Kubernetes version, and input hashes; every row is imported without a dependency-key filter. See the format below. | unset: built-in dependency rows still run | no; if supplied, malformed, empty, oversized, unselected, or hash-mismatched rows fail |
| `SCHEMA_ACCEPTANCE_CHART` | corpus battery tests | Screen only the corpus chart whose fixture stem equals this value exactly (`oncall`, `bitnami-redis`). The four `lean/<chart>` controls are skipped entirely under any filter. A value matching no fixture fails the nonempty-selection guard. | all corpus charts plus the four lean controls | no |
| `ADJUDICATE_WITH_HELM` | `round74_…`, `external_schema_pair_…` (required there), `middle_lean_transition_…` | Presence. Hands every screened flip to pinned Helm v4.2.3 and the offline Kubernetes validator. Also turns on the roster-baseline check and roster matching. | unset: schema-only screening, no Helm | required for `external_schema_pair_…`; for the corpus battery, see the SKILL decision table |
| `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS` | `round74_…` | Presence. Skips the final count gate `flips.len() == PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE` (currently `0`). Every other check still runs: each flip must be adjudicated and matched, and every false or undecided acceptance must be exactly on its roster. | unset: count gate enforced | effectively required for every live run against `ROSTER_BASELINE`, because the roster rows are flips against it |
| `SCHEMA_PROBE_COVERAGE_REPORT` | `round74_…`, `round73_…` | File path for the JSON coverage report (`baseline_ref`, per-chart `charts[]` probe coverage, `helm_adjudication` outcome counts). The parent directory is created. It is written **before** the adjudication checks, so a failing run still leaves it behind. | none | yes ("must name the report file") |
| `SCHEMA_ACCEPTANCE_EXTERNAL_CHART` | `external_schema_pair_…` | Chart directory of an arbitrary chart. Defaults are coalesced from it. | none | yes for that test |
| `SCHEMA_ACCEPTANCE_BASELINE_SCHEMA` | `external_schema_pair_…` | Baseline schema file. It is **not** regex-normalized (unlike `read_schema_at_ref`). | none | yes for that test |
| `SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA` | `external_schema_pair_…` | Candidate schema file. | none | yes for that test |
| `LEGACY_LEAN_SCHEMA_DIR` | `middle_lean_transition_has_only_preregistered_tightenings` | Directory of historical lean-profile schemas for the four lean controls; they are regex-normalized. | none | yes for that test |

## Context roster version 2

The file is JSON with exactly `version` and `rows`; unknown fields are refused.
Each row has exactly these fields:

```json
{
  "version": 2,
  "rows": [{
    "id": "redis-ha-enabled",
    "chart": "oauth2-proxy",
    "overlay": {"redis-ha": {"enabled": true}},
    "transport": "values-file",
    "kubernetes_version": "1.29.0",
    "chart_sha256": "<chart-tree-sha256>",
    "baseline_sha256": "<shipped-baseline-sha256>",
    "candidate_sha256": "<helm-ready-candidate-sha256>",
    "canonical_overlay_sha256": "<compact-overlay-json-sha256>",
    "values_file": "{\n  \"redis-ha\": {\"enabled\": true}\n}\n",
    "values_file_sha256": "<original-values-file-bytes-sha256>",
    "context_overlay": {},
    "context_overlay_sha256": "<compact-context-overlay-json-sha256>"
  }]
}
```

`id` is nonempty and unique within its chart; `overlay` must be an object.
`chart` names a selected fixture stem, and `kubernetes_version` is nonempty.
Only `values-file` transport is supported; every other transport errors.
All supplied rows are imported, including complete activation-plus-child-toggle
contexts, regardless of whether their top-level keys resemble dependencies.
A chart filter requires a matching roster; rows for other charts are errors.
Rows sort by chart and id; identical file-byte/version executions retain all origins.
`context_overlay` is the explicit control before the tested payload mutation,
including dependency activation where applicable; use `{}` for defaults-only rows.
Its hash follows the same compact JSON contract as the tested canonical overlay.

Compute `chart_sha256` with `helm::invocation::tree_sha256` over the staged chart.
The schema hashes bind the exact bytes installed at the chart root.
The baseline preserves the committed fixture bytes when its size is at most
`HELM_MAX_CHART_FILE_BYTES` (5 MiB), exactly like the sweep.
Above that limit it uses `helm-schema shorten --compact` spelling plus its newline.
Validator-only regex normalization never changes these shipped bytes or their hash.
The candidate hash binds the producer's verified Helm-ready companion.

`canonical_overlay_sha256` hashes `serde_json::to_vec(&overlay)`; use Rust's compact
JSON serialization and key ordering rather than assuming another serializer matches.
`values_file` embeds the original UTF-8 file contents exactly, including whitespace
and final newlines; `values_file_sha256` hashes those original bytes unchanged.
The importer parses those bytes through Helm's values-options port and requires
that the resulting complete overlay equals `overlay`.
The same original bytes reach real Helm adjudication; they are never re-encoded.
These two hashes deliberately differ for pretty JSON or YAML files with the same
canonical overlay, allowing the sweep to export its existing file-byte hashes.
The coverage report records both identities, each origin's explicit context, and
the SHA-256 of the complete roster file bytes.
All hashes and the overlay/file agreement are checked before screening.
Version 1 is refused; exporters must write the explicit version-2 contract.

The file is limited to 64 MiB, with at most 8,192 rows per chart and at least one row.
The combined built-in/imported distinct file-byte/version set is capped at 8,192 per
chart and charged three document attempts per entry against the remaining
50,000-probe budget.
A cap that would omit work fails coverage; there is no lane-disable variable.

## Oracle and engine

| Variable | Read by | Meaning | Default |
|---|---|---|---|
| `SCHEMA_ACCEPTANCE_K8S_CACHE` | `compare_charts` (live only) | Root of the Kubernetes JSON-schema cache the offline validator uses (release `v1.29.0-standalone-strict`). The CRD catalog is always `testdata/provider-bundle/crds-catalog-cache`. Both bundles are hashed at start and re-checked at the end ("pinned schema bundle … changed during the run"). Override only to study a different bundle; verdicts are comparable only against the committed one. | `testdata/provider-bundle/kubernetes-json-schema-cache` |
| `SCHEMA_HELM_ENGINE` | `helm/invocation.rs` | `helmsweep` runs renders in resident `helmsweep serve` processes; `cli` spawns the `helm` found on `PATH` for each render. Both must be Helm v4.2.3 exactly. Lint always uses the pinned CLI, with a separate 1,200-second timeout, even when renders use helmsweep. Any other value is an error. | `helmsweep` |
| `HELM_SCHEMA_HELMSWEEP` | `find_helmsweep` | Explicit path to the `helmsweep` binary. | `<target dir of the test binary>/helmsweep`, which `task build:helmsweep` writes |
| `SCHEMA_HELM_INVOCATION_CACHE` | `HelmRunner::shared` | Persistent store for Helm executions, under `<dir>/v2`. Entries are keyed by engine, program bytes and version, platform, the full child environment, arguments, and chart and values content. Charts whose templates call clocks, randomness, `tpl`, `keys` and similar are never replayed (`helm/cache_policy.rs`). Safe to reuse across runs; only speeds up repeated runs. | unset: a private temporary root per process, nothing replayed |
| `SCHEMA_HELM_WORKERS` | `PoolLimits::from_env` | Worker threads for chart screening and probe adjudication. Clamped to `min(value, cores - 2)`, at least 1. | host-sized: one per GiB of the Helm memory budget, at most cores − 2 (`Machine::helm_workers`) |
| `SCHEMA_HELM_MEMORY_MIB` | `PoolLimits::from_env` | Memory budget in MiB for concurrently admitted Helm renders. A probe reserves 1.3x the largest Helm child measured for its chart, or the whole budget until one was measured, so each chart's first probe runs alone. Screening jobs reserve nothing. | host-sized: a third of total memory (`Machine::helm_memory_bytes`) |
| `SCHEMA_HELM_INVOCATION_REPORT` | `round74_…` | Optional JSON file with the pool limits and peaks, wall time, and per-chart costs (screening, preparation, adjudication, Helm stage totals). Use it to tune workers and memory. | unset: not written |

## Perfetto traces

Set `HELM_SCHEMA_TRACE_OUTPUT=/absolute/path/run.perfetto` for `corpus_generation`
or `round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced`.
The parent directory must exist; the file is overwritten.
Unset leaves tracing unchanged.
This uses the CLI's `--trace-output` layer, `helm_schema*` target filter and
field annotations, installed process-wide so worker threads are captured.
Use a distinct path per process and select just the battery test when tracing it.

Corpus traces include artifact, chart and profile spans around generation phases.
Battery traces include chart screening, schema compilation, probe construction,
per-flip adjudication and Helm driver calls with command, chart and stage fields.
Read the `.perfetto` files in Perfetto or through the Perfetto MCP.

## Process environment

| Variable | Why it matters |
|---|---|
| `TMPDIR` | Not used for test scratch any more: per-chart evidence (`adjudication-*`), the Helm root (`helm-root-*`, without `SCHEMA_HELM_INVOCATION_CACHE`) and staged chart copies live under `<target>/scratch/<crate>/<label>-<pid>-<nonce>-<n>` (`crates/test-util/src/scratch.rs`), and Helm children get `TMPDIR`, `TMP` and `TEMP` = `<helm root>/tmp`. Scratch is kept while its process runs and swept by the next test process after it dies. A failure's reported `evidence=<dir>`, and each case a failing final gate implicates, is copied with the prepared charts it used to `<target>/evidence/<crate>/…`, which no sweep touches; marked entries there are pruned after seven days (an age limit, not a byte bound). `HELM_SCHEMA_SCRATCH_ROOT` overrides the scratch root. cargo and the C compiler still use `TMPDIR`; if you set it, create it **before** starting (a missing `TMPDIR` fails the build with exit 101). |
| `CARGO_TARGET_DIR` | `find_helmsweep` looks in the target directory of the running test binary, so build helmsweep into the same target directory as the test. |

Helm children run with a cleared environment, so the caller's `HELM_*` and
`KUBECONFIG` variables do not leak into verdicts.

## Adjacent variables owned by other skills

- `HELM_SCHEMA_CORPUS_ARTIFACTS` (producer artifacts consumed by `task test:integration`),
  `SCHEMA_DUMP` and `SCHEMA_DUMP_CHART` (the lean fixture lane test writes its lean dumps to
  `<target>/schema-dump/` instead of asserting): see `corpus-fixtures`. The battery reads
  none of them.
