# Acceptance battery environment variables

Every variable below is read by the battery family in
`crates/helm-schema/tests/schema_emission_profiles.rs` and its
`crates/helm-schema/tests/common/` harness. Re-grep before trusting this table
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
| `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` | corpus battery tests | Directory holding the **candidate** schemas as producer artifacts: `helm-schema.cli.chart-corpus.<chart>.schema.json` and `helm-schema.emission-profile.lean.<chart>.schema.json`. The corpus producer's `--out` directory has exactly this layout. | unset: the candidate is the on-disk fixture under `testdata/` | required with `ADJUDICATE_WITH_HELM` ("live adjudication requires SCHEMA_ACCEPTANCE_CANDIDATE_DUMP"); in a schema-only run, needed whenever the fixtures on disk are not already the candidate |
| `SCHEMA_ACCEPTANCE_CHART` | corpus battery tests | Screen only the corpus chart whose fixture stem equals this value exactly (`oncall`, `bitnami-redis`). The four `lean/<chart>` controls are skipped entirely under any filter. A value matching no fixture screens nothing and passes with `charts_checked=0`. | all corpus charts plus the four lean controls | no |
| `ADJUDICATE_WITH_HELM` | `round74_…`, `external_schema_pair_…` (required there), `middle_lean_transition_…` | Presence. Hands every screened flip to pinned Helm v4.2.3 and the offline Kubernetes validator. Also turns on the roster-baseline check and roster matching. | unset: schema-only screening, no Helm | required for `external_schema_pair_…`; for the corpus battery, see the SKILL decision table |
| `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS` | `round74_…` | Presence. Skips the final count gate `flips.len() == PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE` (currently `0`). Every other check still runs: each flip must be adjudicated and matched, and every false or undecided acceptance must be exactly on its roster. | unset: count gate enforced | effectively required for every live run against `ROSTER_BASELINE`, because the roster rows are flips against it |
| `SCHEMA_PROBE_COVERAGE_REPORT` | `round74_…`, `round73_…` | File path for the JSON coverage report (`baseline_ref`, per-chart `charts[]` probe coverage, `helm_adjudication` outcome counts). The parent directory is created. It is written **before** the adjudication checks, so a failing run still leaves it behind. | none | yes ("must name the report file") |
| `SCHEMA_ACCEPTANCE_EXTERNAL_CHART` | `external_schema_pair_…` | Chart directory of an arbitrary chart. Defaults are coalesced from it. | none | yes for that test |
| `SCHEMA_ACCEPTANCE_BASELINE_SCHEMA` | `external_schema_pair_…` | Baseline schema file. It is **not** regex-normalized (unlike `read_schema_at_ref`). | none | yes for that test |
| `SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA` | `external_schema_pair_…` | Candidate schema file. | none | yes for that test |
| `LEGACY_LEAN_SCHEMA_DIR` | `middle_lean_transition_has_only_preregistered_tightenings` | Directory of historical lean-profile schemas for the four lean controls; they are regex-normalized. | none | yes for that test |

## Oracle and engine

| Variable | Read by | Meaning | Default |
|---|---|---|---|
| `SCHEMA_ACCEPTANCE_K8S_CACHE` | `compare_charts` (live only) | Root of the Kubernetes JSON-schema cache the offline validator uses (release `v1.29.0-standalone-strict`). The CRD catalog is always `testdata/provider-bundle/crds-catalog-cache`. Both bundles are hashed at start and re-checked at the end ("pinned schema bundle … changed during the run"). Override only to study a different bundle; verdicts are comparable only against the committed one. | `testdata/provider-bundle/kubernetes-json-schema-cache` |
| `SCHEMA_HELM_ENGINE` | `helm_invocation.rs` | `helmsweep` runs renders in resident `helmsweep serve` processes; `cli` spawns the `helm` found on `PATH` for each render. Both must be Helm v4.2.3 exactly. Any other value is an error. | `helmsweep` |
| `HELM_SCHEMA_HELMSWEEP` | `find_helmsweep` | Explicit path to the `helmsweep` binary. | `<target dir of the test binary>/helmsweep`, which `task build:helmsweep` writes |
| `SCHEMA_HELM_INVOCATION_CACHE` | `HelmRunner::shared` | Persistent store for Helm executions, under `<dir>/v2`. Entries are keyed by engine, program bytes and version, platform, the full child environment, arguments, and chart and values content. Charts whose templates call clocks, randomness, `tpl`, `keys` and similar are never replayed (`helm_cache_policy.rs`). Safe to reuse across runs; only speeds up repeated runs. | unset: a private temporary root per process, nothing replayed |
| `SCHEMA_HELM_WORKERS` | `PoolLimits::from_env` | Worker threads for chart screening and probe adjudication. Clamped to `min(value, cores - 2)`, at least 1. | host-sized: one per GiB of the Helm memory budget, at most cores − 2 (`Machine::helm_workers`) |
| `SCHEMA_HELM_MEMORY_MIB` | `PoolLimits::from_env` | Memory budget in MiB for concurrently admitted Helm renders. A probe reserves 1.3x the largest Helm child measured for its chart, or the whole budget until one was measured, so each chart's first probe runs alone. Screening jobs reserve nothing. | host-sized: a third of total memory (`Machine::helm_memory_bytes`) |
| `SCHEMA_HELM_INVOCATION_REPORT` | `round74_…` | Optional JSON file with the pool limits and peaks, wall time, and per-chart costs (screening, preparation, adjudication, Helm stage totals). Use it to tune workers and memory. | unset: not written |

## Process environment

| Variable | Why it matters |
|---|---|
| `TMPDIR` | Every temporary root goes here: per-chart evidence directories `helm-schema-adjudication-*`, the Helm root `helm-schema-helm-root-*` (without `SCHEMA_HELM_INVOCATION_CACHE`), and staged chart copies. Evidence and Helm roots are **kept on purpose**, so verdicts stay reproducible. They are never cleaned up, and a killed run leaks them too. Point `TMPDIR` at a fresh per-run directory on a large disk, create it **before** starting (cargo and the C compiler also use it; a missing `TMPDIR` fails the build with exit 101), and delete it after reading the evidence. |
| `CARGO_TARGET_DIR` | `find_helmsweep` looks in the target directory of the running test binary, so build helmsweep into the same target directory as the test. |

Helm children run with a cleared environment, so the caller's `HELM_*` and
`KUBECONFIG` variables do not leak into verdicts.

## Adjacent variables owned by other skills

- `HELM_SCHEMA_CORPUS_ARTIFACTS` (producer artifacts consumed by `task test:integration`),
  `SCHEMA_DUMP` and `SCHEMA_DUMP_CHART` (the lean fixture lane test writes its lean dumps to
  the system temp directory instead of asserting): see `corpus-fixtures`. The battery reads
  none of them.
