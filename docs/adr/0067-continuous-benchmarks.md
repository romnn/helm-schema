# 0067. Continuous end-to-end benchmarks, published with benchtrail beside the docs

Status: Accepted
Date: 2026-10-20 (user request)

## Context

ADR [0063](0063-deterministic-output-and-the-performance-law.md) sets a performance
law, but nothing watched it between performance campaigns: regressions surfaced only
when a later campaign re-traced the slow charts. The user asked for a small,
representative benchmark on every commit to main, its history kept over time, and the
results published on GitHub Pages beside the Hugo site, as romnn/progeny does with
the user's benchtrail dashboard.

## Decision

- `tools/bench/bench.py`, a standard-library Python driver, runs a release
  `helm-schema` offline against the committed provider bundle (ADR
  [0060](0060-provider-bundle-is-a-committed-test-input.md)). A survey of all 164
  corpus charts (one full-profile run each, 345 CPU-seconds in total, a 0.01–41 s
  spread) chose four: kubeview (0.06 s, the fixed cost), bitnami-redis (2.4 s, helper-heavy
  analysis at 40%), kube-prometheus-stack (20 s, subcharts and chart-local CRDs, schema
  generation three quarters of the run) and oncall. Lean runs where it changes the cost:
  bitnami-redis (1.6 s) and kube-prometheus-stack (11 s); oncall runs lean only (16 s
  against 17 s full), the lean path's heavy case, since its full run repeats
  kube-prometheus-stack's phase proportions.
- Per configuration it records the median CPU time, wall time and peak RSS of a stated
  number of untraced runs (9 for kubeview, 5 for bitnami-redis, 3 for the heavy tails), the
  pretty and the compact short-`$defs` schema size (`helm-schema shorten --compact`), the
  `$defs` count, and six phase times read from one separate `--trace-output` run of the
  existing spans; no timers or dependencies are added to the core crates. Untraced runs
  that write different bytes fail the benchmark (ADR 0063).
- Left out for cost without a new signal: openebs (41 s, the corpus maximum, the phase
  proportions of kube-prometheus-stack at twice its cost) and the CRD-heavy charts, whose
  CRD universe costs under 5 ms. The compact output layout is not a timed configuration:
  shortening the largest output takes 0.13 s, so only its size is recorded.
- `.github/workflows/bench.yaml` runs on every push to main, whatever it touches, and
  records the results with github-action-benchmark (`customSmallerIsBetter`, benchtrail's
  typed `extra` payload: chart and profile dimensions, rustc as environment) on the
  `gh-pages` branch under `bench/`, then calls `pages.yaml`, which installs the benchtrail
  dashboard into the Hugo output at `/bench/` with the `romnn/benchtrail` action; the
  pages workflow has no push trigger of its own, so each commit deploys once. A
  "Benchmarks" menu entry links the site to it.

## Consequences

- `gh-pages` only stores the history (`bench/data.js`); Pages still publishes from
  Actions. github-action-benchmark needs the branch to exist: romnn/helm-schema has it
  (an empty orphan, created 2026-10-20), and the workflow checks for it before building
  and fails with the one-time setup, which `tools/bench/README.md` documents for forks.
- Runs queue (`concurrency.queue: max`), so no commit's point is dropped; the docs
  deploy even when a benchmark fails, and before any history exists.
- The compiler and Python are exact versions in `mise.bench.toml` (Rust 1.99.0, Python
  3.14.7, active under `MISE_ENV=bench`, so contributors' toolchains are unaffected): no
  toolchain release moves a value without a commit, and moving the pin starts a new
  history segment. Hosted runners still vary in hardware, so each run notes its CPU model.
- benchtrail marks a change only beyond 10% and four times the series' own noise.
  github-action-benchmark evaluates no threshold (it does so only to comment or fail,
  both off); its job summary lists every value against the previous run with the ratio.
  The history is a trend monitor, not a performance claim: claims still follow ADR
  [0064](0064-performance-measurement-protocol.md).
- Every action in `bench.yaml` and `pages.yaml` is pinned to a commit SHA with its
  release noted, unlike the moving majors of the other workflows: the bench job can
  push to `gh-pages`, and an upstream update must not change what runs.
- The driver is isolated maintainer tooling in the sense of ADR
  [0065](0065-drift-tooling-is-isolated-maintainer-tooling.md); renaming a phase span
  fails the benchmark until `PHASES` follows.

## References

- `plan/schema-bug-hunt-v1-progress.md`, entry 04:44 (Oct 20) "USER REQUEST: continuous
  benchmarks on GitHub Pages via benchtrail" (commit bda45bab)
- `plan/performance-review-v1-progress.md` recommendation 3, "lightweight continuous
  performance infrastructure" (one small, one mid and one large chart; keep CI short)
- romnn/progeny `.github/workflows/bench.yaml`; romnn/benchtrail v0.3.0 README, "Beside
  documentation" and "Typed benchmarks"
