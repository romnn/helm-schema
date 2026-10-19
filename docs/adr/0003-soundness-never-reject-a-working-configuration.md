# 0003. Soundness: never reject a working configuration

Status: Accepted
Date: 2026-06-11 (`plan/from-scratch-architecture.md` §2, commit e41f21f4)

## Context

The schema can err in two directions: accept values Helm or Kubernetes would
reject (imprecision), or reject values that render and validate (a false
rejection that breaks users). The from-scratch architecture made the
correctness contract the "spine of the design".

## Decision

Helm validates the coalesced values `V = coalesce(D, U)` (null deletes
keys), so the schema's domain is coalesced documents. With `AccK8s` the
values whose render succeeds and whose resolvable manifests validate:

- Soundness obligation (hard): the generated schema never rejects a
  working configuration, `AccK8s ⊆ L(S)`, for every environment the oracle
  does not refute.
- Precision objective (soft): minimize `L(S) \ AccK8s`; every narrowing must
  be justified by provenance-carrying evidence.

Guarded constraints are emitted on a named precision ladder (lowered
conditionals where the guard is decidable over the values document,
otherwise a guard-insensitive projected floor) rather than silently.

## Consequences

- A tightening is not automatically an improvement: CLAUDE.md "Fixture
  regeneration and flip adjudication" requires every acceptance flip to be
  checked against real `helm template`, because "a tightening that rejects
  something helm renders is a false rejection".
- Test instances compose over chart defaults; a bare `{}` means every
  declared key null-deleted (CLAUDE.md "Verification gates").
- Existing `values.schema.json` files are outside the accepted-set
  definition (ADR 0007).
- Named policy exceptions to the contract (e.g. strict authoring defaults)
  are recorded as their own decisions, not as silent behavior.

## References

- `plan/from-scratch-architecture.md` §2 "The correctness contract" (policy
  P1 precision ladder); first in e41f21f4 (2026-06-11)
- CLAUDE.md "Fixture regeneration and flip adjudication"; "Verification
  gates" (coalesced values document)
