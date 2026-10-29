# 0068. Benchmarks compare like with like across runner CPUs, and count instructions

Status: Accepted
Date: 2026-10-20 (user request)

## Context

The benchmark of ADR [0067](0067-continuous-benchmarks.md) runs on `ubuntu-24.04` hosted
runners, a pool of different AMD CPUs. Commits that changed no Rust code measured
kube-prometheus-stack/full at 42.8 and 43.8 CPU-seconds on an EPYC 7763 (Zen 3), 35.3 on
an EPYC 9V74 (Zen 4) and 22.1 on an EPYC 9V45 (Zen 5), while runs on one CPU agree within
2-6%. The dashboard recorded only the compiler as the environment, so a change of runner
read as a regression or an improvement.

## Decision

- CPU, wall and phase times record the runner's CPU model (`/proc/cpuinfo`) in their benchtrail
  environment as `cpu`, beside `rustc`, without its noise (vendor word, trademark marks,
  `CPU @ <frequency>`, a trailing `<N>-Core Processor`): "EPYC 7763". The family word and
  every model token stay, so two models never share a name, and a name of an unknown shape
  stays as it is. The dashboard splits the times into one series per
  CPU model with benchtrail's `splitEnvironment` setting, so change detection compares a
  value only with earlier values from the same CPU. The CPU is environment, not a
  dimension: it is not a variant measured side by side but the hardware a run happened to
  get, and as environment the values recorded before it read as CPU "unknown", a series of
  their own, instead of a separate benchmark. Splitting is generic in benchtrail: any
  environment name can split a suite's series.
- The executed instruction count of one run under valgrind's cachegrind
  (`--cache-sim=no`) is recorded for kubeview full and bitnami-redis full, metric
  "Instructions (cachegrind)", lower is better, in the same suite as the times. Its own
  environment omits CPU, so it stays one series across runners, and it is the first chart.
  Wall and CPU time keep their CPU series.
- `bench.py` owns each metric's environment: peak RSS, pretty and compact schema sizes, `$defs`
  counts and instruction counts omit CPU.
  RSS measures memory use; allocator and kernel accounting differences can add noise, but CPU
  model is not a useful comparison boundary within the hosted runner pool.
  Every metric retains `rustc`, since a toolchain change can legitimately move memory use,
  output sizes and counts, and starts a new segment.
  Benchtrail applies `splitEnvironment` only to names a metric records somewhere in the loaded
  history; a name absent throughout that metric is inapplicable, so filters and label templates
  leave it out and a line reads `full` or `lean`.
  Within metrics that do record CPU, earlier readings without it remain an "unknown" series.
  This makes the recorded data authoritative without duplicating metric policy in dashboard
  settings.
- The suite keeps the newest 500 runs (github-action-benchmark's `max-items-in-chart`), and
  the dashboard opens on the newest 50.
- valgrind is an exact Ubuntu 24.04 package (`1:3.22.0-0ubuntu3`, from the release pocket)
  installed by apt in the bench job, and its version is part of the counts' environment.

## Consequences

- A regression is a jump within one CPU's series. Each series needs eight values before
  benchtrail judges it, so with three CPU models in the pool a time series is judged after
  about two dozen pushes; the instruction counts, one series, are judged after eight.
- Recorded history needs a one-time removal of CPU environment from the five independent metrics
  with benchtrail's idempotent `scripts/suites.mjs --remove-environment METRIC=cpu` operation.
  Release benchtrail, update the dashboard action pin, then rewrite the then-current `gh-pages`
  head while benchmark writers are idle; every main push records and pushes new history.
- The count is deterministic up to under 0.1% (measured: kubeview 552.92-553.07 million over
  seven runs, bitnami-redis full 25.556-25.572 billion over six): std's per-process random
  hash seeds change hash-table probing; disabling address space randomization changes
  nothing. helm-schema generates a schema on one thread. valgrind hides AVX-512 from the
  program, so CPU feature dispatch takes the same paths on every runner. The counted run
  must write the schema the timed runs wrote, or the benchmark fails.
- Cost: cachegrind ran 27 times slower than natively in an Ubuntu 24.04 container on an
  Apple M3 Pro (kubeview 5 s, bitnami-redis full 95-110 s, lean 70-80 s). On a hosted runner,
  up to about twice as slow, the two counted configurations add about 2-4 minutes plus the
  valgrind install; bitnami-redis lean was left out for that reason, the heavy tails for
  tens of minutes.
- The counts first ran as a suite of their own ("helm-schema instructions"); its runs were
  folded once into the one suite with benchtrail's `scripts/suites.mjs`, on `gh-pages`.
- valgrind does not run on macOS on Apple silicon: `task bench` there leaves the counts out.

## References

- Bench runs of 2026-10-20 on romnn/helm-schema (EPYC 7763, 9V74 and 9V45 runners)
- romnn/benchtrail v0.4.0 (`splitEnvironment`) and branch `feat/metric-overrides`
  (per-metric settings, `lineLabel`, `range`, `scripts/suites.mjs`)
- `tools/bench/bench.py` (`INSTRUCTION_CASES`), `.github/workflows/bench.yaml`
