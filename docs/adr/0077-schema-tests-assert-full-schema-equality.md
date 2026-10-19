# 0077. Schema tests assert full-schema equality against explicit fixtures

Status: Accepted
Date: 2026-02-18 (earliest CLAUDE.md/AGENTS.md text, 49bf0731)

## Context

Generated schemas are large; partial assertions of a few fields let
unexamined parts of the output change silently, and snapshot tools that
auto-regenerate expectations turn any output into the new truth.

## Decision

- Schema integration tests assert full JSON schema equality with a diff-based
  assertion (`test_util::sim_assert_eq!` with `have:`/`want:` labels).
- Full-schema equality is never replaced by selective assertions of a few
  fields.
- No snapshot testing or auto-regeneration: intentional output changes update
  the full expected fixtures explicitly.

## Consequences

- Every output change is visible as a fixture diff and must be reviewed and
  adjudicated (0073, 0078).
- Fixture equality pins bytes only; it is not evidence that the bytes are
  correct, which is why the acceptance battery and the frozen-witness gate
  exist (0073, 0083).
- Fixtures must stay readable for diffs to be reviewable (readable `$defs`
  names in fixtures; see the emission-policy ADRs).

## References

- CLAUDE.md "Schema tests"; "Equality assertions in tests"
- 49bf0731 `readme and scaffolding` (2026-02-18); 7c8269a3 `enforce diff-based sim_assert_eq in tests via clippy` (2026-06-20)
- `.claude/skills/acceptance-battery/SKILL.md` intro ("Fixture equality tests only pin bytes")
