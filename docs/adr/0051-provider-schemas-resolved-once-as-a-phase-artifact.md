# 0051. Provider schemas are resolved once, into a phase artifact keyed by the complete use

Status: Accepted
Date: 2026-08-25 (architecture review v4, bb61a78f; A1 landed 1146a3d7; C4 landed c5ddc78d, 2026-09-02)

## Context

The generator consulted providers through private memo caches. Review v4
finding A1: `ProviderSchemaLookupKey` omitted `stringified`, but the cached
candidate embedded a policy that branches on it, so two uses at one slot
differing only in stringification shared one preimage and the first-resolved
use pinned the schema for both (order-dependent, tighter or wider). Finding
C4: the path resolver and four provider-requirement synthesis passes each
kept their own lookup cache.

## Decision

Provider lookups happen once, during emission-plan preparation, into one
immutable `ProviderSchemaResolutions` artifact: a
`BTreeMap<ProviderSchemaUse, ProviderSchemaResolution>` keyed by the
complete use, covering base uses, conditional-overlay uses and every concrete
kind alternative, and retaining authoritative misses (`None`) as well as
hits. Later phases consume the artifact; they may apply use policy to a
cloned fragment but may not call the provider or keep a memo. As the A1 fix,
preimage lowering consumes the typed policy subkey embedded in the cache key,
so a policy field cannot be read outside memo identity.

## Consequences

- Reversing two uses cannot change the generated schema; first-resolved
  cache order no longer matters.
- The five private caches and direct provider calls were deleted.
- C4 was representation/performance-only: zero schema, IR, diagnostic,
  acceptance or fixture byte changes were pre-registered and held.
- Matches CLAUDE.md's rule that any cache affecting generation is keyed by
  every input that can change the result.

## References

- CLAUDE.md "Design goal: fast, deterministic, cache-safe output"
- `plan/architecture-review-v4.md` A1, C4;
  `plan/architecture-review-v4-progress.md` "Round A1", "C4 — provider
  resolution phase artifact"
- Commits 1146a3d7 (2026-08-25), c5ddc78d (2026-09-02)
