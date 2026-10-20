# Continuous benchmark

`bench.py` runs a release `helm-schema` offline against the committed provider bundle on four
corpus charts and records CPU and wall time, peak RSS, schema sizes, the `$defs` count and phase
times from the CLI's tracing spans. Why each chart is there is written beside `CASES` in the
script; the design is recorded in the continuous-benchmarks ADR under `docs/adr/`.

## Run it locally

```sh
task bench                              # every chart, with the toolchain CI uses
task bench -- --chart bitnami-redis     # one chart
task bench -- --json out.json           # also write the history entries CI records
```

`task bench` sets `MISE_ENV=bench`, so mise installs and selects the exact Rust and Python that
`mise.bench.toml` pins. A local number is a trend check, not a performance claim; claims follow
the measurement protocol of ADR 0064.

## In CI

`.github/workflows/bench.yaml` runs on every push to main. It records the results with
github-action-benchmark in `bench/data.js` on the `gh-pages` branch, then calls the pages
workflow, which installs the benchtrail dashboard at `/bench/` beside the documentation.

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
starts a new segment of the history. The actions in both workflows are pinned to commits, with
their release in a comment.
