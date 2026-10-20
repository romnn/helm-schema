# 0013. Each evidence kind has a fixed error polarity; uncertainty widens

Status: Accepted
Date: 2026-06-11 (from-scratch architecture v2, `e41f21f4`)

## Context

The analyzer combines many kinds of evidence (reads, sink schemas, `default`
and `with` admissions, object openness, sink attribution, requiredness,
branch liveness). Each can be wrong, and a wrong narrowing rejects a values
document Helm would render. Helm validates the coalesced values document
`V = coalesce(defaults, user)`, where a user `null` deletes a key.

## Decision

To meet the soundness contract of ADR [0002](0002-soundness-never-reject-a-working-configuration.md) (`AccK8s(C, E, K) ⊆ L(S)`; every
narrowing backed by provenance-carrying structural evidence or an explicit
caller policy input), each evidence kind may err in exactly one direction,
fixed by a polarity table, and uncertainty always moves `L(S)` wider:

- unknown type evidence is omitted (`Any`); falsey admission is
  over-approximated; objects are closed only on proof plus explicit policy;
- sink attribution is exact (or wildcard-anchored over a uniform target) or
  abstains;
- requiredness is a drastic under-approximation;
- unknown branch liveness is live;
- `Lookup::Unknown` never prunes and never narrows; source precedence never
  overrides structural facts; disagreement widens with a diagnostic.

## Consequences

- Preserve exact alternatives when the chart is ambiguous; never select the
  first plausible candidate for convenience. Abstention is typed
  (`Found/Absent/Unknown`, `Top`/`Union`, `Opaque`), with consumer behavior
  fixed by the polarity table; unplaceable frontend regions stay `Opaque`.
- Conditional lowering is restricted to predicates decidable over the values
  document alone (see the emission-policy ADRs for the precision ladder).
- Determinism is claimed given identical oracle answers; cache state may move
  output only in the widening direction.
- Later bound fixes follow the same rule: the A8 round (`90c037b6`,
  2026-08-26) made over-cap fanout and unproven exactness abstain instead of
  tightening (see [0019](0019-analysis-bounds-are-structural-and-only-widen.md)).

## References

- `plan/from-scratch-architecture.md` §2 "The correctness contract" (soundness
  obligation, precision objective, polarity table) and §4 item 4
- `e41f21f4` (2026-06-11, polarity table); cc2662e6 (2026-05-18,
  typed apiVersion branches replacing noisy flat candidates)
- CLAUDE.md "Design goal: structural static analysis first" ("prefer a
  principled 'ambiguous' or 'unknown' result over a wrong guess")
- `plan/full-core-architecture-simplification-handoff.md` "Non-negotiable
  design rules" ("Unknown branch liveness preserves alternatives")

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
