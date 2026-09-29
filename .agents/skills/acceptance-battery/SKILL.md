---
name: acceptance-battery
description: >-
  Run and interpret helm-schema's schema acceptance / flip-adjudication battery
  (`round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced` in
  crates/helm-schema/tests/schema_emission_profiles.rs) and its sibling
  maintenance tests. Use when a change moves corpus or lean fixture bytes, when
  asked to "run the battery", "adjudicate flips", check "acceptance
  equivalence", pick a SCHEMA_ACCEPTANCE_BASELINE_REF, set up
  SCHEMA_ACCEPTANCE_CANDIDATE_DUMP / ADJUDICATE_WITH_HELM /
  SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS, edit KNOWN_FALSE_ACCEPTANCES /
  KNOWN_UNDECIDED_ACCEPTANCES / ROSTER_BASELINE, read a probe coverage report
  or HELM_FLIP evidence, or debug a battery failure (false acceptances, roster
  rows "no longer fail alike", probe caps, vacuous zero-flip runs).
---

# Acceptance battery

The battery answers one question: **which values documents does the candidate
schema accept differently from the baseline schema, and is each change
right?** It builds a probe battery per chart over the chart's coalesced
defaults. Rust screening finds the probes the two schemas judge differently.
In live mode, each screened flip is then judged by pinned Helm v4.2.3 and an
offline Kubernetes v1.29 strict bundle plus the CRD catalog. Fixture equality
tests only pin bytes. This battery is the evidence that changed bytes are
correct.

- Harness in `crates/helm-schema/tests/common/`: `emission_profile_harness.rs`
  (probes, screening, coverage), `helm_pool.rs` (worker/memory pool), and
  `known_false_acceptances.rs` (rosters, `ROSTER_BASELINE`). The shared Helm
  layer lives in `crates/helm-schema-test-support/src/helm/` (also used by the
  `cell_matrix` tool): `adjudication.rs` (pinned Helm chart copies, offline
  Kubernetes validator), `invocation.rs` (Helm engine, replay store),
  `cache_policy.rs` (render replayability) and `kubernetes_version.rs`.
- Details: [references/env-vars.md](references/env-vars.md) lists every
  variable. [references/verdicts-and-rosters.md](references/verdicts-and-rosters.md)
  covers probe classes, verdicts, evidence files, roster rules, and baseline
  adoption.

Related skills: `corpus-fixtures` produces the candidate dump and adopts
fixtures. `verification-gates` covers the full gate list this battery belongs
to. `landing-workflow` covers the landing chain that runs the battery as one
step.

## The tests

All tests below live in the `schema_emission_profiles` binary of package
`helm-schema`. The `default` nextest profile filters out every integration
binary (`not kind(test)`), so always pass `--profile integration`. Ignored
tests need `--run-ignored ignored-only`. Pass `--no-capture` to see the summary
line and the `HELM_FLIP` lines, because nextest hides a passing test's output.

| Test | Ignored | What it proves | Use it? |
|---|---|---|---|
| `round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced` | yes | the full check: probe coverage caps, every flip adjudicated and matched, rosters exact, and the count gate unless matched flips are allowed | **yes, this is "the battery"** |
| `external_schema_pair_flips_are_helm_adjudicated` | yes | the same adjudication for any chart dir plus a baseline/candidate schema file pair; prints `ADJUDICATED <flip>` lines | for charts outside the corpus or hand-made schema pairs |
| `round73_…`, `round68…72_…`, `early_provider_definition_pruning_…` | yes | older variants: expect zero flips or a hard-coded flip list tied to a historical baseline; no roster-baseline check | no, historical witnesses. `round73_…` also demands zero flips, so it cannot pass live against the roster baseline |
| `round70_oauth2_proxy_tpl_change_kept_the_eager_string_tooth` | yes | two oauth2-proxy probes stay rejected against a hard-coded ref | historical |
| `middle_lean_transition_has_only_preregistered_tightenings` | yes | lean-profile transition against `LEGACY_LEAN_SCHEMA_DIR` | only for emission-profile work |
| `temporal_middle_policy_measurements` | yes | records measurements for the temporal-wrapper control | measurement, not a gate |
| the non-ignored tests in the same file (verdict accounting, roster matching, coverage validation, monotonicity, lean fixture lane) | no | the harness logic itself | run by `task test:integration` automatically |
| `schema_emission_profile_live` binary (`replay_*`, `helm_embedded_validator_*`) | yes | replays semantic controls against the `helm` on `PATH` (v4.2.3); one test also needs `kubeconform` | when touching those controls |

No gate task runs the ignored battery. `task test:integration` and
`task test:all` run only non-ignored tests, so run it explicitly.

## Choose the mode and the baseline

| Situation | Mode | Baseline | Candidate | Allow matched flips |
|---|---|---|---|---|
| Refactor or performance work that must not change acceptance | schema-only (no `ADJUDICATE_WITH_HELM`) | the commit your work started from, whose fixtures are the pre-change output | the one clean dump of the final build (required) | no. Pass means zero screened flips. |
| Any change that moves fixture bytes and may change acceptance (correctness work, landings) | live (`ADJUDICATE_WITH_HELM=1`) | `ROSTER_BASELINE`, required: other baselines are refused | the one clean dump of the final build (required) | **yes**. Roster rows are flips against the roster baseline, so the count gate (`PREREGISTERED_ACCEPTANCE_FLIP_ALLOWANCE = 0`) cannot hold. |
| A schema-only run found flips | switch to live | `ROSTER_BASELINE` | same dump | yes |

Why the roster baseline is pinned: a roster row is a cell `ROSTER_BASELINE`
rejected and the candidate accepts. Against any other baseline, and especially
one equal to the candidate, no such flip is screened, so every row looks
"fixed". The live battery therefore resolves the ref with `git rev-parse` and
bails unless it equals `ROSTER_BASELINE`. Moving that baseline is a user-level
design decision with a re-derivation procedure (see the references). Never
change it to make a run pass.

`SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS` skips **only** the final count
comparison. Every flip must still be live-adjudicated and matched by Helm or
Kubernetes evidence, and the false and undecided acceptances must equal the
rosters exactly. It therefore allows proven changes, not outcome-tuned ones. A
non-zero pre-registered allowance, if ever needed, must be committed before the
live run it covers.

## Prerequisites

- **Candidate dump (every mode).** `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` is
  required whether or not Helm adjudicates; without it the battery fails
  immediately, before reading any schema. The committed fixtures are never a
  candidate. Run one clean producer run of the final build:
  `cargo run -p helm-schema-test-support --bin corpus_generation -- --out <dump>`
  (`--jobs <n>` overrides the host-sized default).
  It writes `helm-schema.cli.chart-corpus.<chart>.schema.json` and
  `helm-schema.emission-profile.lean.<chart>.schema.json` flat into `<dump>`,
  which is exactly what the battery reads. See `corpus-fixtures`. Never mix
  dumps from different builds.
- **Helm engine (live only).** Run `task build:helmsweep`, which needs the
  pinned Go through mise and writes `helmsweep` into the cargo target
  directory. Alternatively set `HELM_SCHEMA_HELMSWEEP=<path>`, or use
  `SCHEMA_HELM_ENGINE=cli` with Helm v4.2.3 on `PATH`, which is slower. The
  runner refuses any other Helm build.
- **Room under the cargo target directory.** Evidence and Helm roots live in
  `<target>/scratch/<crate>/<label>-<pid>-<nonce>-<n>` (`crates/test-util/src/scratch.rs`)
  and are swept after the run's process exits, by the next test process;
  failure evidence is copied to `<target>/evidence/` (kept seven days). A
  `TMPDIR`, if set, must already exist (see Pitfalls).

## Commands

Set up a per-run directory and read the pinned roster baseline from the code,
so the command never goes stale:

```sh
RUN="$PWD/target/acceptance-battery/<name>"; mkdir -p "$RUN/tmp"
ROSTER=$(sed -n 's/.*ROSTER_BASELINE: &str = "\([0-9a-f]*\)".*/\1/p' \
  crates/helm-schema/tests/common/known_false_acceptances.rs)
BATTERY=(cargo nextest run -p helm-schema --profile integration --test schema_emission_profiles
  -E 'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture)
```

**1. Iterate on one chart first.** Charts are screened on a worker pool and
folded in sorted order. A chart error or adjudication failure only surfaces
after the whole run. Start with a chart your change moves:

```sh
TMPDIR="$RUN/tmp" SCHEMA_ACCEPTANCE_BASELINE_REF="$ROSTER" \
SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=<dump> SCHEMA_ACCEPTANCE_CHART=<chart> \
SCHEMA_PROBE_COVERAGE_REPORT="$RUN/coverage-<chart>.json" \
SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS=1 ADJUDICATE_WITH_HELM=1 \
"${BATTERY[@]}" > "$RUN/battery-<chart>.log" 2>&1; echo "battery exit $?"
```

To iterate on the code, `corpus_generation --out <scratch> --only chart/<chart>`
regenerates just that chart's artifact. The battery reads dump files by name
and needs no manifest. The final evidence must still come from the one clean
full dump.

**2. Full live run.** Use the same command without `SCHEMA_ACCEPTANCE_CHART`.
It also screens the four `lean/<chart>` controls, which any chart filter
skips. Expect tens of minutes. Add `SCHEMA_HELM_INVOCATION_REPORT="$RUN/cost.json"`
to record pool peaks and per-chart cost.

**3. Schema-only equivalence (refactors).** No Helm is needed:

```sh
TMPDIR="$RUN/tmp" SCHEMA_ACCEPTANCE_BASELINE_REF=<start-commit> \
SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=<dump> SCHEMA_PROBE_COVERAGE_REPORT="$RUN/coverage.json" \
"${BATTERY[@]}" > "$RUN/battery.log" 2>&1; echo "battery exit $?"
```

**4. External pair.** Set `ADJUDICATE_WITH_HELM=1`,
`SCHEMA_ACCEPTANCE_EXTERNAL_CHART=<chart dir>`,
`SCHEMA_ACCEPTANCE_BASELINE_SCHEMA=<file>` and
`SCHEMA_ACCEPTANCE_CANDIDATE_SCHEMA=<file>`, then run the same command with
`-E 'test(external_schema_pair_flips_are_helm_adjudicated)'`.

Check the recorded exit code, not a pipe's. nextest exits 100 on a test failure
and 4 when no test matched: a wrong profile or filter.

### Tuning workers and memory

Both default to the host's size
(`crates/helm-schema-test-support/src/machine.rs`): `SCHEMA_HELM_MEMORY_MIB`
to a third of total memory and `SCHEMA_HELM_WORKERS` to one worker per GiB of
that budget, at most cores minus 2 (an 11-core, 18 GiB laptop gets 6 workers
and 6144 MiB). An explicit worker count is still clamped to cores minus 2. The
memory budget covers concurrently admitted Helm renders. Each chart's first probe reserves the whole budget, and
later probes reserve 1.3x that chart's measured peak. Size the workers to the
free cores on a shared machine. Size the memory to what Helm children may use
without swapping. Raise workers only if the invocation report shows the
memory peak well under budget. Lower both when the host is loaded or other
heavy jobs share it. The values change only speed, never verdicts.

## Reading the result

The summary line on success:

```
charts_checked=<n> probes_checked=<n> flips=<n> listed_false_acceptances=<n> listed_undecided_acceptances=<n>
```

- `charts_checked` counts charts screened: all corpus fixtures plus 4 lean
  controls, or 1 with a filter.
- `probes_checked` counts probes generated over those charts, including
  unreachable ones.
- `flips` counts live-adjudicated, non-collapsed flips. In schema-only mode it
  counts screened flips.
- `listed_false_acceptances` and `listed_undecided_acceptances` count the false
  and undecided acceptances observed this run. The run passed, so each one is
  on its roster.

The coverage report (`SCHEMA_PROBE_COVERAGE_REPORT`) is written before the
adjudication checks run, so it exists even when the run fails. Its
`helm_adjudication` object counts screened, collapsed and adjudicated flips,
matched tightenings (Helm abort / Kubernetes rejection), matched loosenings
(Kubernetes validation / defaults violations), and lists undecided and false
acceptances, `charts_adjudicated` and `unreachable_cases`. `charts[]` holds each
chart's probe accounting.

Failure messages end with `evidence=<dir>`. Read that probe's
`schema-verdicts.json`, `render.stderr` and `values.json` before deciding
anything.

## Flip semantics in one paragraph

A **tightening** means the candidate rejects a document the baseline accepted,
or both reject and the candidate adds a new reason. A **loosening** means the
candidate accepts what the baseline rejected. **A tightening is a direction,
not a verdict.** It is matched only when Helm aborts or the render adds a new
Kubernetes violation relative to the defaults render. A tightening of a
document Helm renders cleanly is a false rejection and fails the run. A
loosening is matched only when the render is valid, or its violations all
come from the defaults render, and every changed resource is decided. An
accepted document that Helm or Kubernetes rejects is a **false acceptance**
and must be on `KNOWN_FALSE_ACCEPTANCES`. An accepted render Kubernetes cannot
decide must be on `KNOWN_UNDECIDED_ACCEPTANCES` with its exact uncertainty
text. Rows that stop failing alike on an adjudicated chart must be removed.
The full verdict table and the roster editing rules are in the references.

## Historical baselines and regex dialects

`read_schema_at_ref` reads each baseline fixture with `git show` and passes it
through `helm_schema_core::normalize_schema_pattern_dialects`. Older fixtures
can carry Go/RE2 `pattern` spellings, such as a leading `(?i)` or escaped
punctuation. The strict ECMA-262 `u`-mode regex check refuses to compile those,
which would fail the whole chart with `compile full schema: …`. Provider
ingestion now applies the same respelling. It is language-exact, never
widening, so the normalized baseline accepts the same values and still
compiles. `LEGACY_LEAN_SCHEMA_DIR` baselines are normalized the same way. The
external-pair baseline file is **not** normalized, so normalize it yourself if
it predates the dialect fix.

## Vacuous runs (tell-tales)

A green battery proves nothing when the baseline and candidate are the same
bytes. Suspect a vacuous run when:

- `flips_adjudicated: 0` or `flips=0` on a round that changed fixture bytes. The
  usual causes are that `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` names a dump of an
  older build, or that the baseline ref already holds the new fixtures. (An
  unset variable no longer falls back to the on-disk fixtures: the run fails.)
- A chart's `guards_discovered` equals what one schema alone yields. Guard arms
  are deduplicated across both schemas, so a real pair usually discovers more.
- `charts_checked=0`: `SCHEMA_ACCEPTANCE_CHART` matched no fixture stem.
- A live run against the roster baseline reports few or no flips. The roster
  rows alone guarantee thousands of matched flips there.
- The whole roster reports "no longer fail alike": wrong baseline or stale dump.

Run the battery **after** the final clean dump exists, with the dump variable
naming it.

## Pitfalls

| Symptom | Cause | Fix |
|---|---|---|
| nextest exits 4, "no tests to run" | default profile, or `--run-ignored` missing | `--profile integration --run-ignored ignored-only` |
| exit 101 before any test, with a compiler or `ring` temp-file error | `TMPDIR` does not exist | `mkdir -p` it first |
| a `Helm adjudication evidence:` or `HELM_FLIP` dir is gone | successful scratch is swept after its process exits | failure evidence lives on under `<target>/evidence/`; re-run the chart to inspect a success |
| "the false-acceptance rosters are adjudicated against …" | live run with a baseline other than `ROSTER_BASELINE` | use `$ROSTER` |
| "fixture acceptance flips differ from the pre-registered count" | live run against the roster baseline without allowing matched flips | set `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS=1` |
| "fixture flips were not all live-adjudicated" | schema-only run found flips | rerun live to adjudicate them |
| "SCHEMA_ACCEPTANCE_CANDIDATE_DUMP must name the producer dump of the candidate build" | any mode without a dump | point it at the one clean dump of the final build |
| `read <dump>/helm-schema.cli.chart-corpus.<chart>.schema.json` fails | dump incomplete or from a different registry; the chart list comes from the on-disk `testdata/chart-corpus-schemas/` | regenerate the full dump |
| `git show failed for <ref>:testdata/…` | the baseline predates that chart's fixture | there is no skip: filter to other charts, or treat it as a baseline-adoption question |
| `no helmsweep at …` | helmsweep not built into this target dir | `task build:helmsweep`, `HELM_SCHEMA_HELMSWEEP`, or `SCHEMA_HELM_ENGINE=cli` |
| "pinned schema bundle … changed during the run" | something wrote the provider bundle mid-run | rerun with nothing else touching `testdata/provider-bundle/` |
| "tightening rejects a document whose render adds no Kubernetes violation …" | a real false rejection | fix the analyzer; never roster a tightening |
| "false acceptances missing from KNOWN_FALSE_ACCEPTANCES" | a new false acceptance | if your change caused it, fix the change; otherwise adjudicate it and add a row |
