# Continuous benchmark

`bench.py` runs a release `helm-schema` offline against the committed provider bundle on four
corpus charts and records CPU and wall time, peak RSS, schema sizes, the `$defs` count and phase
times from the CLI's tracing spans, each with the compiler and the host's CPU model as its
environment. For the cheap configurations (`INSTRUCTION_CASES`) it also counts the executed
instructions of one run under valgrind's cachegrind, a figure no runner CPU changes. Why each
chart is there is written beside `CASES` in the script; the design is recorded in the
continuous-benchmarks ADRs under `docs/adr/` (0067, 0068).

## Run it locally

```sh
task bench                              # every chart, with the toolchain CI uses
task bench -- --chart bitnami-redis     # one chart
task bench -- --json out.json           # also write the history entries CI records
task bench -- --instructions-json i.json  # and the instruction counts (needs valgrind)
```

Instructions are counted whenever valgrind is on `PATH` (or given with `--valgrind`); valgrind
does not run on macOS on Apple silicon, where the counts are left out. On such a machine, a Linux
container runs them: build the binary for Linux and run `bench.py` in an `ubuntu:24.04` container
with `valgrind` and `python3` installed.

`task bench` sets `MISE_ENV=bench`, so mise installs and selects the exact Rust and Python that
`mise.bench.toml` pins. A local number is a trend check, not a performance claim; claims follow
the measurement protocol of ADR 0064.

## In CI

`.github/workflows/bench.yaml` runs on every push to main, on its head commit. It records the
results with github-action-benchmark in `bench/data.js` on the `gh-pages` branch, as two suites:
"helm-schema end-to-end" (the times, sizes and phases) and "helm-schema instructions" (the
cachegrind counts). It then calls the pages workflow, which installs the benchtrail dashboard at
`/bench/` beside the documentation.

The hosted `ubuntu-24.04` runners are a pool of different CPUs (AMD EPYC 7763, 9V74 and 9V45 have
been seen), whose times differ by up to a factor of two on the same commit. Every value records
its CPU model in its environment, so the dashboard can show one series per CPU model and a
regression is a jump within one CPU's series. That split needs benchtrail's `splitEnvironment`
setting, which v0.3.0 lacks: until the pin moves to a release with it, the pages workflow ignores
the CPU model (`ignoreEnvironment`), as the dashboard did before it was recorded. The instruction
counts do not depend on the CPU, so their suite ignores it for good.

## Splitting the times by CPU model

`pages.yaml` pins benchtrail v0.4.0, the first release with `splitEnvironment`, and gives the
time suite its own settings beside the shared ones:

```json
"ignoreEnvironment": ["cpu"],
"suites": {
  "helm-schema end-to-end": {
    "splitEnvironment": ["cpu"],
    "label": "{{chart}} / {{profile}} / {{cpu}}",
    "dimensions": { "cpu": { "label": "CPU", "order": 2 } },
    "colorBy": "cpu",
    "dashBy": "profile"
  }
}
```

Each CPU model then has its own series, colored by CPU and dashed by profile, with one chart per
chart and metric. The values recorded before the CPU model was show as CPU "unknown", a series of
their own; every series needs eight values before benchtrail judges a change in it.

## One-time setup

github-action-benchmark needs the `gh-pages` branch to exist before the first run. The bench
workflow checks for it before building and fails with this instruction when it is missing.
romnn/helm-schema has the branch (created empty on 2026-10-20); a fork creates it once:

```sh
git switch --orphan gh-pages
git commit --allow-empty -m "Benchmark history"
git push origin gh-pages
```

The branch only stores the history: GitHub Pages must publish from GitHub Actions
(Settings, Pages, Source), and the branch must accept pushes from the workflow's `GITHUB_TOKEN`
(no branch protection on it).

## Moving a pin

Rust and Python move in `mise.bench.toml` (then `MISE_ENV=bench mise lock`); a new compiler
starts a new segment of the history. valgrind is an exact Ubuntu 24.04 package version in
`bench.yaml`; a new one starts a new segment of the instruction counts. The actions in both workflows are pinned to commits, with
their release in a comment.
