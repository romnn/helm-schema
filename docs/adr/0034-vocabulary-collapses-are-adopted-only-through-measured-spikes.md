# 0034. Vocabulary collapses are adopted only through measured E-gated spikes

Status: Accepted
Date: 2026-08-25 (architecture review v4, Part E)

## Context

v3's carrier unifications never deleted the LOC its estimates promised.
Architecture review v4 traced this to the cross-product of semantic
vocabularies (`Guard` 26 variants, `FailValueRequirement` 26, `CaptureKind`
19, `ConditionalGuard` 18, transforms as ~12 Booleans and ~8 path-set
channels) times the interpreters that must understand each. It proposed
collapsing to three vocabularies (conditions, requirements, provenance), but
warned that banded LOC promises select for forcing, and forcing selects for
deleted semantics.

## Decision

Every vocabulary-collapse step starts as a spike on a branch and is adopted
only if it shows (a) at least one representation deleted, (b) non-positive
whole-tree production LOC by `task tokei:core`, (c) byte-exact schema and IR
fixtures, and (d) flat-or-better corpus wall-clock. A miss records the result
and abandons the spike; it is never forced, and an abandoned design is never
restored by layering compatibility state onto current carriers.

## Consequences

Outcomes recorded under this rule:

- B2/E1 `Transform` vocabulary: abandoned after two attempts (+134 and +67
  LOC), 2026-08-29.
- E2 split of fail conditions from execution context: abandoned (attempt 1
  changed 22/56 corpus artifacts; attempt 2 stopped at LOC), 2026-08-30. E3
  is blocked until a design avoids that split.
- E4 study: the execution `Guard` and schema-lowerable `ConditionalGuard`
  vocabularies are a sound phase boundary, not a local maximum — "execution
  predicates preserve what the chart says; conditional guards preserve only
  what schema emission may soundly enforce". Both kept, 2026-09-02.
- Activation-DNF spike: abandoned (+1 and +3 LOC), 2026-09-02.
- S-E `&mut dyn HelperCallValueResolver` dispatch: rejected for +40.3 %
  corpus wall-clock, 2026-09-02.
- Per-atom usable polarity (exact vs sound-subset) must be carried as data in
  any unified vocabulary, or a collapse erases a soundness contract.

## References

- `plan/architecture-review-v4.md` Part E ("Why v3's LOC estimates failed",
  "The rule that prevents another miss"), "Success metrics"
- `plan/architecture-review-v4-progress.md` "B2/E1", "E2", "Wave 2 decision
  register", "E4", "Activation-DNF spike", "S-E"
- Commits `3de22377` (2026-08-29), `7f1f54a2` (2026-08-30), `ac7e7ac2`,
  `a9922a11`, `2fb91c55` (2026-09-02)
