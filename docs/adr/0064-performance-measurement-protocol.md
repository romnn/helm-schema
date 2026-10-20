# 0064. Performance claims follow one binding measurement protocol

Status: Accepted
Date: 2026-09-03 (performance review v1, commit 1ce9e660)

## Context

Earlier performance numbers mixed installed and freshly built binaries, online and
offline runs, loaded and idle hosts, and traced and untraced runs. Performance review
v1 was attacked on its own protocol by one review lens and folded the corrections into
a protocol it declares "binding on the implementor".

## Decision

Every performance number is produced this way:

- **Binary**: a release build of the exact tree, copied out of `target/`; both A and B
  are built before either is timed.
- **Cache state**: a private snapshot of the Kubernetes and CRD caches, warmed once
  online and verified byte-identical against an `--offline` run; all timed runs are
  offline with a pinned `--k8s-version`. A number produced with network access is not
  a timing number.
- **Identity**: schema bytes, stdout, stderr (`--diag-format json`) and exit status are
  hashed; schema-only identity is not identity.
- **Statistics**: repeated runs, interleaved A/B pairs for final decisions, median and
  min–max of CPU and wall; a gain whose paired range crosses zero is not a gain; load
  is recorded and loaded hosts are marked.
- **Attribution**: perfetto traces for phase proportions only, never mixed with untraced
  timings; short sampling windows inside the phase of interest.
- **Byte gate**: every candidate is compared with the baseline on the reference charts
  before it is timed; a candidate that changes one byte leaves the performance track
  for the adjudication track (ADR [0055](0055-fixture-change-contracts.md)).

## Consequences

- Performance items are classed as representation-only (byte-exact) or semantic; a
  semantic speed-up (e.g. the unchanged-variable join shortcut A3) needs Helm
  adjudication like any behavior change.
- Lowering an analysis cap is rejected as a performance lever: it is semantic and
  guaranteed to flip results.
- Per-template parallelism is deferred until a re-trace after the byte-exact fixes
  shows a serial floor above the target; the serial hot spot (repeated
  `predicate_bdd::normalize` rebuilds, 55–86% of CPU on the slow charts) is removed
  first. The recorded entry ticket for a later attempt: the mechanism is
  intra-template, the shared `IrAnalysisDb` is `Rc`/`RefCell`/`OnceCell`, and
  byte-exactness would need ordinal-tagged results. Hash-based `$defs` naming and a
  global `ValuesPath` interner were rejected in the same review.

## References

- `plan/performance-review-v1.md` "Measurement protocol (binding on the implementor)",
  "Target and definition of done", "Verdict", "Anti-findings (considered and
  rejected for wave 1, with reasons)" (lowering any cap, per-template parallelism)
- Commit 1ce9e660 (2026-09-03) "chore(plan): add performance review v1"
