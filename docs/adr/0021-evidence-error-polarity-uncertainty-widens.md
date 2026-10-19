# 0021. Each evidence kind has a fixed error polarity; uncertainty widens

Status: Accepted
Date: 2026-06-11 (from-scratch architecture v2, `e41f21f4`)

## Context

The analyzer combines many kinds of evidence (reads, sink schemas, `default`
and `with` admissions, object openness, sink attribution, requiredness,
branch liveness). Each can be wrong, and a wrong narrowing rejects a values
document Helm would render. Helm validates the coalesced values document
`V = coalesce(defaults, user)`, where a user `null` deletes a key.

## Decision

To meet the soundness contract of ADR 0003 (`AccK8s(C, E, K) ⊆ L(S)`; every
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

- Abstention is a typed value with a direction (`Found/Absent/Unknown`,
  `Top`/`Union`, `Opaque`), and consumers follow the polarity table.
- Conditional lowering is restricted to predicates decidable over the values
  document alone (see the emission-policy ADRs for the precision ladder).
- Determinism is claimed given identical oracle answers; cache state may move
  output only in the widening direction.
- Later bound fixes follow the same rule: the A8 round (`90c037b6`,
  2026-08-26) made over-cap fanout and unproven exactness abstain instead of
  tightening (see 0028).

## References

- `plan/from-scratch-architecture.md` §2 "The correctness contract" (soundness
  obligation, precision objective, polarity table) and §4 item 4
- `e41f21f4` (2026-06-11, "plan: from scratch architecture v2", polarity
  table introduced)
- CLAUDE.md "Design goal: structural static analysis first" ("prefer a
  principled 'ambiguous' or 'unknown' result over a wrong guess")
- `plan/full-core-architecture-simplification-handoff.md` "Non-negotiable
  design rules" ("Unknown branch liveness preserves alternatives")
