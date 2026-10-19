# 0042. Resolve capability branches by iterating branch literals, first success wins

Status: Superseded by 0043
Date: 2026-05-18 (earliest evidence: the CLAUDE.md round-7 note, added in cc2662e6; the round itself predates the commit)

## Context

An early structural guard evaluation answered
`.Capabilities.APIVersions.Has "..."` by probing `has_resource`, which is
local-cache-only and fetch-free by contract. On the first lookup of a cold
cache it returned `false` for every API version, so charts collapsed to the
`else`-branch fallback even for APIs present upstream: the cache-as-oracle
antipattern (ADR 0041).

## Decision

Round 7 removed capability evaluation entirely. apiVersion branches selected
by capability guards were not evaluated; the provider chain iterated the
branch literals and returned the first one that resolved successfully.

## Consequences

- Removed the cold-cache collapse, but discarded the guard's meaning: the
  selected branch depended on which literal resolved first, not on what the
  configured Kubernetes version supports.
- Superseded in round 8 by an explicit oracle implemented upstream-first
  with a tri-state answer (ADR 0043).
- A residue of "first available alternative" behaviour for unknown
  capabilities survived in `lookup/chain.rs` and was flagged in 2026-09 as
  letting cache contents decide chart semantics (ADR 0052).

## References

- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle",
  "Historical note (round 7 → round 8 → round 10)" (cc2662e6, 2026-05-18)
- `plan/schema-bug-hunt-v1-progress.md` line ~3821 (finding (f), chain.rs:137)
