# 0027. `Chart.yaml` dependency activation is a structural guard

Status: Accepted
Date: 2026-06-13 (`24456b57`, "dependency activation evidence")

## Context

A subchart's templates render only when its dependency is active, which Helm
decides from `Chart.yaml` `condition:` and `tags:`. The from-scratch
architecture widened the input universe to real charts, including
`Chart.yaml` conditions, as declarative guard evidence.

## Decision

Dependency activation is modelled as a predicate over the values document and
applied as a guard on everything the subchart contributes, following Helm's
precedence: the first present condition decides; tags are considered only
when every condition is absent; a fully absent activation set keeps Helm's
default-active behaviour.

## Consequences

- Subchart contract uses carry the activation guard instead of being
  unconditional (`964f4145`, 2026-06-17).
- Activation must not erase structurally known facts. Architecture review v4
  A7 found that activation dropped default sources and root-overlay prefixes;
  the fix (`c9967847`, 2026-08-26) keeps the two channels distinct:
  root-overlay implications conjoin the activation predicate, and guarded
  default sources stay out of unconditional values composition and take part
  only in branch-aware prepared-values/absence lowering.
- Open edge (audit §2.9): activation is still projected as guards on
  duplicated contract rows, not first-class guarded witness data. A typed
  activation-DNF applied in place was spiked and abandoned on the E-gates
  (see 0034).

## References

- `plan/from-scratch-architecture.md` revision history ("Chart.yaml
  conditions")
- `plan/from-scratch-architecture-implementation-audit.md` §2.9
- `plan/architecture-review-v4.md` A7; `plan/architecture-review-v4-progress.md`
  "A7 — activation-scoped overlay projection and default sources",
  "Activation-DNF spike"
- Commits `24456b57` (2026-06-13), `964f4145` (2026-06-17), `c9967847`
  (2026-08-26)
