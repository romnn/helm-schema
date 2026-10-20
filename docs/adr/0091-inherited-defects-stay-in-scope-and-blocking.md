# 0091. Inherited defects stay in scope and block landings

Status: Superseded by 0101
Date: 2026-10-18 (user decision, ledger 14:31)

## Context

Every review round of the schema bug hunt ended in REWORK. The findings were real
(Helm-executed counterexamples), but the landing bar also blocked on cases that
`main` already had ("main-too") and on constructed cases. The proposal on the table
was to block landings only on regressions relative to `main`.

## Decision

The landing bar stays. Every Helm-verified counterexample remains blocking, including
defects that `main` already has; such defects are fixed, not deferred. In the user's
words: "keep fixing defects that main already has! even if it takes longer it will
hopefully become less over time right?"

## Consequences

- A known main-too false rejection found during a track (the cited example: W1's
  statement-level `set .Values` requirement) goes into that track's next rework.
- Throughput is recovered within the bar rather than by lowering it: fix by class with
  one structural rule, smaller landable slices, and pre-review before hand-back.
- Pairs with ADR 0092: fixing inherited defects must simplify, not accumulate special
  cases.

## References

- `plan/schema-bug-hunt-v1-progress.md` line ~8212, entry 14:31 (Oct 18) "USER
  DECISION: keep fixing defects main already has; the landing bar stays" (commit
  296d76ed, 2026-10-18)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §6 "Standing decisions made this session"
  ("Inherited (main-too) defects stay in scope and blocking")
