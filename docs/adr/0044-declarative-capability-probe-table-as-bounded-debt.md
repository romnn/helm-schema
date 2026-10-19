# 0044. Keep a declarative capability probe table as bounded, validated debt

Status: Accepted
Date: 2026-05-18 (debt recorded in CLAUDE.md, cc2662e6; table isolated in ef3513dd, 2026-06-13; rows validated in 0b2e7345, 2026-08-25)

## Context

The capability oracle (ADR 0043) must answer the two-part form
`.Capabilities.APIVersions.Has "group/version"`. The upstream schema source is
per-file (one JSON per kind, fetched on demand) with no bundle manifest to
enumerate, so the probe needs one canonical kind per api version whose
presence proves the version exists (for example `PodDisruptionBudget` for
`policy/v1`). The three-part form `group/version/Kind` probes the kind
directly and needs no table.

## Decision

Keep a manually maintained, declarative `(api_version, kind)` table in
`crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs`, used
only for api-version-only queries, and treat it as acknowledged
architectural debt. Every row is validated by a corpus-gated test that
resolves it against upstream OpenAPI documents spanning live and removed API
versions. Architecture review v4 kept the table: all four reviewers agreed
that removing it via an eager bundle pre-fetch is a bad trade because it
reintroduces the cache-as-oracle antipattern.

## Consequences

- New Kubernetes api versions and api groups require a table edit.
- A wrong row is the table's remaining failure mode (it would silently kill a
  live `.Capabilities` branch); the row-validation test covers it without
  changing capability lookup, upstream-first probing, negative-cache
  authority or the permissive `None` outcome.
- New guard shapes that need a similar lookup should target the kind
  directly rather than grow the canonical-kind map.
- Removing the table cleanly requires an upstream bundle that ships a
  per-version manifest of kinds, or eager enumeration of the primary-version
  bundle.

## References

- CLAUDE.md "Known architectural debt: capability probe table"
- `plan/architecture-review-v4.md` Part D, G3;
  `plan/architecture-review-v4-progress.md` "Round G1+G3" (landed 0b2e7345)
- Commits cc2662e6 (2026-05-18), ef3513dd (2026-06-13), 0b2e7345 (2026-08-25)
