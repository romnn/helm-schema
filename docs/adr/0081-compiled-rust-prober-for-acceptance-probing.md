# 0081. Acceptance probing uses a compiled Rust prober at three granularities

Status: Accepted
Date: 2026-08-02 (CLAUDE.md, 4d2aba0f)

## Context

Old-vs-new acceptance probing validates many generated documents against very
large schemas. Python `jsonschema` is about 100x too slow on the large
schemas (CLAUDE.md).

## Decision

Old-vs-new acceptance probing uses a compiled Rust prober (the `jsonschema`
crate in an integration test), at three granularities: top-level deletions,
second-level deletions, and empty member/item probes, all composed over chart
defaults with null-deletion merge semantics (0076). In the battery, Rust
screening finds the probes the two schemas judge differently; only screened
flips go to live Helm adjudication.

## Consequences

- Screening is cheap enough to cover the whole corpus every landing; Helm runs
  only on flips.
- The probe set is deletion-anchored on defaults, so a stably wrong answer or
  a value-class cell never generated is invisible to it; the frozen-witness
  gate (0083) covers that blind spot.
- Rules out Python validators for corpus-scale probing.

## References

- CLAUDE.md "Fixture regeneration and flip adjudication" (compiled Rust prober bullet)
- `.claude/skills/acceptance-battery/SKILL.md` intro ("Rust screening finds the probes the two schemas judge differently")
- `plan/schema-bug-hunt-v1.md` "Why this hunt exists" (deletion-anchored battery); `plan/schema-bug-hunt-v1-progress.md` ~2920–2925 (flip-only blind spot)
