# 0026. Dependency values keys and installed-chart identity are validated separately

Status: Accepted
Date: 2026-08-05 (D2 resolved, option 3); policy refined 2026-08-25 (v4 A2/G4)

## Context

Discovery mapped each installed chart name to one `Chart.yaml` dependency
entry, last write wins, so `{name: redis, alias: cache}` plus
`{name: redis, alias: queue}` silently lost the `cache` subtree's context and
activation, and two different names sharing one alias silently unioned two
subcharts under one `.Values` key. v3 decision point D2 asked how to treat
duplicate aliases; the user chose option 3: measure Helm's installed-entry to
alias association first, then model the measured identity. The Step 11b
measurement against Helm 4.2.3 found the association for multiple installed
entries sharing one internal name ambiguous (nondeterministic).

## Decision

Two identities, two checks:

- Values keys (`alias` or else `name`) must be unique across declarations;
  violations are rejected in one aggregated, deterministically ordered
  diagnostic before discovery.
- Installed-chart lookup stays keyed by internal chart name. One vendored chart
  declared under several unique aliases is modelled as one chart context per
  alias (`name -> Vec<metadata>`), each with its own values prefix and
  activation.
- Only the measured nondeterministic case — two directory/tgz installed
  entries with the same internal name — is rejected as an installed-identity
  ambiguity.

## Consequences

- No last-entry-wins behaviour and no ambiguity claim for cases Helm handles
  deterministically.
- The sole discovery map is not re-keyed by alias, which would break ordinary
  `{name: redis, alias: cache}` lookup.
- Step 11b itself landed no production change: measurement preceded modelling,
  as D2 required.

## References

- `plan/architecture-review-v3.md` "D2. Duplicate dependency aliases"
- `plan/architecture-review-v3-progress.md` "Decision register" (D2), "Step 11b"
- `plan/architecture-review-v4.md` A2, G4; `plan/architecture-review-v4-progress.md`
  "Round A2/G4 — separate values-key uniqueness from installed-chart identity"
- Commit `3e9dbf8e` (2026-08-25)
