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

- Every value records the runner's CPU model (`/proc/cpuinfo`) in its benchtrail
  environment as `cpu`, beside `rustc`. The dashboard splits the times into one series per
  CPU model with benchtrail's `splitEnvironment` setting, so change detection compares a
  value only with earlier values from the same CPU. The CPU is environment, not a
  dimension: it is not a variant measured side by side but the hardware a run happened to
  get, and as environment the values recorded before it read as CPU "unknown", a series of
  their own, instead of a separate benchmark. Splitting is generic in benchtrail: any
  environment name can split a suite's series.
- The executed instruction count of one run under valgrind's cachegrind
  (`--cache-sim=no`) is recorded for kubeview full and bitnami-redis full, a suite of its
  own ("helm-schema instructions", metric "Instructions (cachegrind)", lower is better)
  whose settings ignore the CPU, so it stays one series across runners. Wall and CPU time
  stay as they were.
- valgrind is an exact Ubuntu 24.04 package (`1:3.22.0-0ubuntu3`, from the release pocket)
  installed by apt in the bench job, and its version is part of the counts' environment.

## Consequences

- A regression is a jump within one CPU's series. Each series needs eight values before
  benchtrail judges it, so with three CPU models in the pool a time series is judged after
  about two dozen pushes; the instruction counts, one series, are judged after eight.
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
- benchtrail v0.3.0 has no `splitEnvironment` and rejects the setting, so until the
  `pages.yaml` pin moves to a release with it the dashboard ignores the CPU
  (`ignoreEnvironment`), as before it was recorded; `tools/bench/README.md` holds the
  settings to add with that pin.
- valgrind does not run on macOS on Apple silicon: `task bench` there leaves the counts out.

## References

- Bench runs of 2026-10-20 on romnn/helm-schema (EPYC 7763, 9V74 and 9V45 runners)
- romnn/benchtrail branch `feat/split-environment` (`splitEnvironment` setting)
- `tools/bench/bench.py` (`INSTRUCTION_CASES`), `.github/workflows/bench.yaml`
