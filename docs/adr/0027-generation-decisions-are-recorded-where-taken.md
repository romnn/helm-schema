# 0027. Generation decisions are recorded where they are taken and exposed by `explain`

Status: Accepted
Date: 2026-09-29 (explain C1, landing 7, 58bf21a3)

## Context

CLAUDE.md requires that "when a heuristic materially affects resolution, it
should be diagnosable". The generator takes such decisions in several places
(policy resolution, path resolution, overlay lowering, member projection, base
schema), and reconstructing them afterwards would mean a second model of the
same facts.

## Decision

One typed `GenerationDecisions` record (`helm-schema-gen`
`generation_decisions.rs`) is captured at the sites where the decisions are
taken. It holds references by index into the owning signals, makes no copies,
and is never read back by generation. `ResolvedContract` carries it, and
`AnalysisSession::explain_generation` (format version 1) exposes it beside the
existing `ValuePathExplanation` fields. No CLI surface was added in this step.

## Consequences

- Explanation cannot drift from generation, because it reads the record the
  generator wrote rather than recomputing.
- Generation output stays byte-identical: landing 7 adopted 0 of 202 fixtures.
- Pinned by `tests/explain.rs` (JSON and text for prometheus-redis-exporter
  `service.type` and jaeger `commonAnnotations`) and
  `gen/src/tests/generation_decisions.rs`.

## References

- CLAUDE.md "When heuristics are appropriate" (diagnosability)
- `plan/schema-bug-hunt-v1-progress.md` 04:15 (Sep 29) entry, lines ~6509-6516
- Commit 58bf21a3 (2026-09-29) "landing 7: explain C1 (GenerationDecisions
  record + AnalysisSession::explain_generation, …)"

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
