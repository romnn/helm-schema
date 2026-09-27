# The landing runner: interface as the ledgers establish it

The runner lives **outside this repository**, as its own git repo in the orchestrator's
workspace. It is not vendored here and may not exist on the machine you are on. Everything
below is taken from the progress ledgers, the playbook, the builder protocol and
`tools/helmsweep/README.md`; treat the runner's own `README.md` as authoritative when you
can read it, and say so when you cannot.

## Contents of the runner repo

- `run-landing.sh <step>`: the entry point; one step per invocation.
- `landing.py`: receipt, freeze and binding checks, dump classification, Helm log
  classification (`classify_helm`), sweep planning (`sweep-plan`, `prep`) and the sweep gate
  (`sweep-gate`).
- `sweep-one.sh`, `gen_overrides.py`: the legacy CLI sweep and the Helm-faithful override
  generator (one override file per roster row).
- `landing-<name>.env`: one env file per landing; a template env from the last landing is
  the starting point for the next one.
- `tests/run-all.sh`: the runner's own suite (fakes for Helm, the producer, cargo). Every
  runner change is red-then-green in this suite before it is merged.

## Invocation

```sh
export LANDING_ENV=<runner>/landing-<n>.env
R=<runner>/run-landing.sh
$R status                      # who holds the heavy lock; never removes it
$R dump && $R unit && $R lint && $R battery && $R integration && $R sweep && $R finalize
```

For long chains the orchestrator runs the same sequence detached with a log, stopping at
the first failure:

```sh
nohup zsh -c 'for s in dump unit lint battery integration sweep finalize; do
  ./run-landing.sh $s || exit $?; done; echo chain green' > "$E/chain.log" 2>&1 &
```

Run it from the runner directory, never edit that directory while a chain executes from
it, and build runner changes in a git worktree merged only between chains.

## Env file variables (as used in landing env files)

| Variable | Meaning |
|---|---|
| `R` | The landing clone (fresh clone of `main` with the candidate commits) |
| `E` | The evidence dir for this chain; **fresh for every restart from dump** |
| `TARGET` | `CARGO_TARGET_DIR` used by the chain (exclusive to it) |
| `BASELINE` | Acceptance baseline commit for the battery; the known-false-acceptance roster refuses any value other than the pinned `ROSTER_BASELINE` (`crates/helm-schema/tests/common/known_false_acceptances.rs`) |
| `DUMP_MODE` | `producer` (the corpus producer, default) or `legacy` (per-lane nextest dumps, for candidates older than the producer) |
| kube overrides | Per-chart Kubernetes version overrides for the battery's Helm oracle |
| `ALLOW_MATCHED_FLIPS` | Lets the battery accept flips that are matched by adjudication (maps to `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS`) |
| `ACCEPTED_LINT_RESIDUAL` | `path:line lint_name` of the one located lint diagnostic accepted for this landing |
| `ACCEPTED_SWEEP_UNRESOLVED` | Space-separated `chart=class` pairs of sweep rows accepted without a Helm verdict (e.g. a chart without `Chart.yaml` → `unresolved:loader`) |
| `SWEEP_ENGINE` | `helmsweep` (in-process Helm) |
| `SWEEP_JOBS`, `HELMSWEEP_JOBS` | Worker counts; keep them inside the machine budget |
| `SWEEP_DIFF_CAP` | Max rows per chart whose fast verdicts differ that are re-run through the real Helm CLI |
| `SWEEP_CACHE` | Verdict cache root (speed only, never evidence) |

Every acceptance in an env file carries a comment saying why. The env file is part of the
receipt binding: changing it after the dump forces a fresh `E` and a restart from `dump`.

## Steps in detail

**dump.** Builds once and generates every fixture lane (chart corpus, IR, generator cases,
lean profiles, final-output). Producer mode runs `corpus_generation --out <dump> --jobs N`
(`--helm-ready` also writes the short-key schemas Helm receives), checks the manifest shape,
exact file set and bytes, that the target binary is the one the manifest names, and that
`corpus_generation --verify <dump>` passes; it then copies the producer binary into `$E`
read-only and binds its sha (the shared target dir rebuilds it with different feature
unification in later steps). Aborts on anything but fixture-content mismatches. Writes
`adopted.tsv`: the fixtures whose committed bytes differ from the dump, tagged
changed / format-only / new. An empty `adopted.tsv` means the candidate already carries its
fixtures. Freezes two levels: `generation_inputs` (everything outside `**/tests/**`) and
`full_tree`.

**unit.** `cargo nextest run` on the default profile.

**lint.** `cargo fmt --check`, `task lint`, `task lint:fc`, ast-grep, and the dialect-hygiene
integration test with ignored tests included (must run at least one test). A nonzero lint
exit passes only when its sole located diagnostic equals `ACCEPTED_LINT_RESIDUAL`; because a
clippy failure stops dependent crates, the receipt then records partial coverage. The parser
strips ANSI colour before matching.

**battery.** The round-74 battery comparing the dump (`SCHEMA_ACCEPTANCE_CANDIDATE_DUMP`)
against `BASELINE`, Helm adjudication on, ignored tests included. Green: `1 test run: 1
passed`. Common reds: unmatched flip; a roster entry that no longer fails ("remove it");
wrong baseline.

**integration.** Re-runs `corpus_generation --verify` with the frozen producer copy, then the
nextest `integration` profile against the verified artifacts
(`HELM_SCHEMA_CORPUS_ARTIFACTS`). Catches test-expectation drift the default profile never
runs.

**sweep.** Plans a frozen `roster.tsv` (chart × override rows), prepares three copies per
chart (`plain` without any schema, `base` and `cand` with the baseline's and the
candidate's **shipped** schema), and runs per row: `helm lint` on base, on cand, and a
schema-free `helm template --skip-schema-validation` control on plain, all at the row's
pinned kube version. The fast pass uses `helmsweep sweep` (Helm's own code, in process); a
real-CLI differential re-runs a sample (every 100th row per chart, up to `SWEEP_DIFF_CAP`
rows whose fast verdicts differ, every unresolved row) and must agree 100%. The gate:
no new lint failure (cand fails where base passes and the control renders), row count equals
charts × overrides, and every row without a Helm verdict is either absent or listed in
`ACCEPTED_SWEEP_UNRESOLVED`.

**finalize.** Makes the receipt read-only and prints its sha256; the merge message and the
ledger cite that sha.

## Exit codes seen in the ledgers

| Exit | Meaning |
|---|---|
| 0 | Step green |
| 1 | Dump failed beyond fixture mismatches, or a harness failure |
| 3 | Sweep gate failed (new lint failures) |
| 4 | Refused: stale step or broken binding (e.g. "generation inputs changed since the dump", a rebuilt producer binary) |
| 5 | Sweep rows without a Helm verdict (`unresolved:*`) not accepted by the env |
| 75 | Heavy lock held by another step: record, reschedule, do not wait in a loop |
| 100 | nextest failure inside a step (e.g. the battery) |

## Receipt

`$E/receipt.json` binds the clone, HEAD, `E`, dump, target dir, lock token, baseline commit,
runner sha, env, tool shas (Helm build line, helmsweep, producer binary), manifest sha and
the per-step results and timings. It is verified at every step boundary: any tracked or
untracked edit, tampered dump file, intent-to-add path, zero-byte new file, or changed
configuration refuses the next step. A receipt proves exactly its recorded candidate.

## Restart decision table

| What changed | Restart from | New `E`? |
|---|---|---|
| Production code, fixtures, anything outside `**/tests/**` | `dump` | yes |
| Env file or runner | `dump` | yes |
| Test-only code | earliest step the runner marks stale (often `unit` or `battery`) | no |
| Test change that alters generated artifacts or an adopted fixture | `dump` | yes |
| Nothing (lock busy, disk full, killed process) | the failed step | no |
