# Continuous benchmark

`bench.py` runs a release `helm-schema` offline against the committed provider bundle on four
corpus charts and records CPU and wall time, peak RSS, schema sizes, the `$defs` count and phase
times from the CLI's tracing spans, each with the compiler and the host's CPU model as its
environment (shortened by `short_cpu`: "AMD EPYC 7763 64-Core Processor" is "EPYC 7763"). For the cheap configurations (`INSTRUCTION_CASES`) it also counts the executed
instructions of one run under valgrind's cachegrind, a figure no runner CPU changes. Why each
chart is there is written beside `CASES` in the script; the design is recorded in the
continuous-benchmarks ADRs under `docs/adr/` (0067, 0068).

## Run it locally

```sh
task bench                              # every chart, with the toolchain CI uses
task bench -- --chart bitnami-redis     # one chart
task bench -- --json out.json           # also write the history entries CI records
python3 -m unittest discover tools/bench  # the helpers' tests
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
results with github-action-benchmark in `bench/data.js` on the `gh-pages` branch, as one suite,
"helm-schema end-to-end", which keeps the newest 500 runs (`max-items-in-chart`). It then calls the
pages workflow, which installs the benchtrail dashboard at `/bench/` beside the documentation.

## The dashboard

The hosted `ubuntu-24.04` runners are a pool of different CPUs (EPYC 7763, 9V74 and 9V45 have been
seen), whose times differ by up to a factor of two on the same commit. Every value records its CPU
model in its environment, and the dashboard (settings in `pages.yaml`) splits each benchmark into
one series per CPU model (`splitEnvironment`), so a regression is a jump within one CPU's series.
Values recorded before the CPU model show as CPU "unknown"; every series needs eight values before
benchtrail judges a change in it. Lines are colored by CPU and dashed by profile, read as
`full (EPYC 7763)` inside a chart, and each chart of the corpus has a chart per metric.

The instruction counts do not depend on the CPU: their metric's own settings neither split nor
segment on it, so the counts stay one series across runners, and they are the first charts. The
page opens on the newest 50 runs; its range buttons widen it.

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
`bench.yaml`; a new one starts a new segment of the instruction counts. The actions in both
workflows are pinned to commits, with their release in a comment.
