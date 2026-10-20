# 0042. The values root is closed by default; `--open-root` opts out

Status: Accepted
Date: 2026-09-26 (user decision; root closure itself predates it, 2026-07)

## Context

helm-schema closes the chart values root (`additionalProperties: false`) so
a key no template reads, such as a misspelled `replicaCont`, is rejected
instead of silently ignored. The corpus plan called root closure the
"strict-mode contract" and excluded it from closure-opening fixes. The bug
hunt then proved F73: Helm accepts and ignores extra root keys, so the
closure rejects documents Helm renders (datadog's own CI values set a root
`securityAgent`); the shape is in 153 of 156 corpus schemas. The campaign
register set a default to retain root closure, and on 2026-09-26 a review
digest listed "F1/F73 open root (SUPERSEDES the strict-mode default)" as an
item for the user. The user instead adopted the reading that KEEPS the
strict default.

## Decision

- Root closure is the default authoring assertion.
- `--open-root` is an explicit caller policy that omits the generated
  root `additionalProperties: false`. Only the root closure changes: nested
  structural and Kubernetes resource closures, member contracts, and
  requirements are kept. Dependency instance roots are open carriers under
  either policy.
- Helm guarantees `global` at every chart/dependency-instance root, so
  reserve it independently of root policy. Register every instance and follow
  Helm's two dependency-enablement/assertion passes; carriers are untyped,
  with the root's own requirement carrying Helm's table assertion.
- Authoring assertions never change a recovered constraint (one that
  follows from what templates, helpers, or resource sinks do).

## Consequences

- F73 witnesses become explicit POLICY-EXCEPTION rows in the frozen gate:
  strict verdict by default, `Accepts` under the option; a family closed
  only by such rows is reported CLOSED-BY-POLICY, separately from CLOSED.
- Missing injected `global` was F1, a correctness defect: ten dependency
  charts failed, including library chart `common` on every use. All ten
  global-image-registry witnesses became `Fixed(Accepts)`; the old
  `top_level_mapping_paths` model was deleted. This reserves one Helm key
  without opening arbitrary root keys.
- The setting is recorded in `x-helm-schema-policy.authoring` (ADR [0040](0040-emitted-schemas-self-identify-their-policy.md)).

## References

- `docs/content/docs/how-it-works.md`, `quick-start.md` (typo rationale)
- `plan/chart-corpus-expansion.md` ~line 380 ("strict-mode contract")
- `plan/schema-bug-hunt-v1.md` F73; `plan/schema-bug-hunt-v1-progress.md`
  Decision register (F73 policy default), ~line 3829-3831 (12:35 proposal,
  42df7e85) and 13:00 entry ~3841-3850 (adopted, 00410877, 2026-09-26)
- `plan/schema-bug-hunt-v1.md` F1; `plan/chart-corpus-expansion.md`
  lines 796–800 (root global opened July 12, 77f0f3db)
- `crates/helm-schema-gen/src/authoring_policy.rs`; 5834fbbd (2026-09-29,
  dependency-root reservation and authoring policy)
