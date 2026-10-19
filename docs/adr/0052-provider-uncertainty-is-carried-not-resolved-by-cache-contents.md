# 0052. Provider uncertainty is carried, never resolved by cache contents

Status: Proposed
Date: 2026-09-26 (review finding and red-test spec, ledger 2026-09-24 round; decision (b) confirmed 2026-09-28)

## Context

The schema-bug-hunt review of the Kubernetes provider (finding (f)) found
cache-dependence left after ADR 0043: an unknown capability resolved to the
"first available alternative" (`lookup/chain.rs:~137`), letting cache
contents decide chart semantics; uncertainty was erased in
`load_source_schema_doc`; and a sticky `NotOwned` memo kept stale answers.
Red-test specs confirmed three defects: HPA v1/v2 under a partial cache,
Pod release/mirror substitution, and memoized `Unavailable` / incomplete
inference. A cold offline cache also made an `Unavailable` provider drop
default-derived typing (a `containerPort` integer became `{}`).

## Decision

- An undecided capability abstains and reports `UnresolvedCapability`
  instead of choosing an alternative.
- An `Uncertain` probe is carried as `ProviderLookupResult::Unavailable` →
  `ChainLookupOutcome::Unavailable` with a `SchemaUnavailable` diagnostic,
  and stops release/mirror substitution.
- `Unavailable` and incomplete inference are never memoized.
- Decision (b): an unavailable provider withholds the provider contract (with
  a diagnostic) but never erases chart-derived typing; only an undecided
  capability counts as unresolved.

## Consequences

- Cold and warm caches yield the same chart-derived schema; the difference is
  visible as diagnostics, as ADR 0041 requires.
- Status: designed, red-tested and reviewed (rework 9: LAND from both
  reviewers on 2026-09-30, for the landing after W4); not on `main` at
  8910ea1e, where these types do not exist yet.

## References

- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle"
- `plan/schema-bug-hunt-v1-progress.md` lines ~3821 (finding (f)), ~3874
  (D1-D3 confirmed), ~4186 (track launched), ~6026-6034 and ~6091-6093
  (decision (b), 2026-09-28), ~6175 (K1-K3), ~7435 (LAND x2, 2026-09-30)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §3 and §6 "Investigated, queued"
- Ledger commits 00410877 (2026-09-26), 7f55ad6b (2026-09-28), 51873feb (2026-09-30)
