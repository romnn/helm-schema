# 0097. Profile with tracing spans and Perfetto, not custom timers

Status: Accepted
Date: 2026-06-09 (commit 12e482a6 replaced the phase profiler); reaffirmed 2026-10-18

## Context

helm-schema had a hand-maintained `ProfilePhase` / `--profile-phases` timing path. In
October 2026 a separate phase-timer infrastructure was being built for the landing
runner to explain landing-chain slowdowns.

## Decision

Perfetto traces from `tracing::instrument` spans are the source of truth for runtime
analysis, in the product and in the verification tooling. Phase boundaries are
`#[tracing::instrument]` annotations written through the existing `--trace-output`
`tracing_perfetto` layer. No custom phase-timer infrastructure is added. The user on
2026-10-18: "instead of complex phase timing infra, id suggest using our existing
perfetto tracing … simple tracing::instrument should be enough".

## Consequences

- The old `ProfilePhase` / `--profile-phases` path was removed.
- The corpus producer and the battery reuse the CLI's tracing layer behind one switch,
  with spans per chart, per flip adjudication and per Helm subprocess call; output stays
  byte-identical.
- Traces are for attribution only and are never mixed with untraced timings (ADR 0096).
- Span begin/end correctness depends on the tracing-perfetto layer; see ADR 0098.

## References

- `plan/next-priorities.md` "Performance observability" and "Temporal performance/RSS pass"
- Commit 12e482a6 (2026-06-09) "replace phase profiler with tracing spans via perfetto"
- `plan/schema-bug-hunt-v1-progress.md` ~line 8281, entry 16:38 (Oct 18), and ~8346
  (tracing switch for `corpus_generation` and the battery)
