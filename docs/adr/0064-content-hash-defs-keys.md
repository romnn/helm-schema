# 0064. Content-hash `$defs` keys for stable fixtures

Status: Superseded by 0065
Date: 2026-09-27 (landing 2)

## Context

Every fixture dump scrambled the base-62 ordinal `$defs` names, producing
multi-million-line fixture diffs that nobody could review, so landings were
adjudicated only by the battery. The user proposed stable identifiers as
`$ref` keys (2026-09-25).

## Decision

Landing 2 (test infrastructure 2) shipped "stable `$defs` names" as
content-hash keys (`h<12hex>`, `providerSchema_<12hex>`).

## Consequences

The keys were stable but not readable; the user rejected them ("hashes as
keys that nobody understands"). Stable is necessary but not sufficient: a
fixture key must say what the definition is. Replaced the same day by
source-path names (ADR 0065).

## References

- Session memory `fixture-defs-naming.md` (2026-09-25 request, 2026-09-27
  correction); `fix-uninterpretable-output-first.md`
- Commit 74139f94 (2026-09-27, Land test infrastructure 2 ... stable $defs names)
