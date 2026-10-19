# 0002. Preserve ambiguity; prefer "unknown" over a wrong guess

Status: Accepted
Date: 2026-05-18 (CLAUDE.md "Design goal: structural static analysis first", commit cc2662e6)

## Context

Charts often express genuine alternatives (an apiVersion chosen by
capability branches, a helper that resolves to a finite set of literals).
Collapsing those alternatives to one convenient answer produces output that
looks deterministic but may be wrong. The flat-candidate apiVersion resolver
replaced in cc2662e6 "masked real chart bugs" and produced noisy
per-candidate diagnostics.

## Decision

When the chart is genuinely ambiguous, helm-schema preserves the exact
alternatives instead of collapsing them, and when it cannot know, it returns
an explicit "unknown" or "ambiguous" result rather than a deterministic-
looking guess. Abstention is a first-class value with a direction:
`Found/Absent/Unknown` lookups, `Top`/`Union` values and `Opaque` document
regions, each with a defined consumer behavior.

## Consequences

- "Explicit candidate preservation over premature collapsing" and
  "'unknown' or 'ambiguous' over a wrong deterministic-looking answer" are
  standing preferences for every analysis.
- Heuristics must be "willing to abstain" (ADR 0001).
- The same rule governs caches and oracles: uncertainty never silently drops
  a branch (CLAUDE.md "Cache is a speed optimisation", the tri-state
  capability oracle).
- Frontend constructs the parser cannot place soundly degrade to `Opaque`
  instead of being guessed (ADR 0015).
- Rules out picking the "first plausible" alternative to make output
  simpler.

## References

- CLAUDE.md "Design goal: structural static analysis first" ("What this
  means concretely", "Project standard")
- `plan/from-scratch-architecture.md` §4 "Design commitments", item 4
  ("Abstention is a value with a direction")
- Commit cc2662e6 (typed apiVersion-branch model; message explains the
  flat-candidate problem)
