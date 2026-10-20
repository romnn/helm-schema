# 0041. A generated schema must accept the chart's own defaults and the `helm lint` document

Status: Accepted
Date: 2026-07-12 (own `values.yaml` acceptance in the corpus); extended 2026-09-24 (lint document)

## Context

A schema that rejects the values a chart ships with blocks every install.
The chart corpus tracked charts whose generated schema rejected their own
`values.yaml` in `KNOWN_VALUES_REJECTIONS`, with the goal of emptying it; a
chart still rejecting its own values counts as a new finding, not something
to force away. In September 2026 the F23/D3a candidate newly failed
`helm lint` on apisix, datadog, and signoz-signoz while `helm template`
passed: Helm v4.2.3's lint values rule validates the raw, un-coalesced root
`values.yaml`, not the coalesced document.

## Decision

The generated schema must satisfy BOTH documents Helm validates:

- the coalesced values document `helm template` / `install` builds from
  the chart's defaults (including dependency defaults), and
- the raw root `values.yaml` that `helm lint` validates.

The root `values.yaml` is a second must-accept document, handled through
the same mechanism that keeps the chart's own defaults accepted. Every
constraint withdrawn to satisfy it stays diagnosable and is recorded per
chart.

## Consequences

- Charts whose defaults genuinely make Helm abort are the exception: their
  schema must reject under `full` (the emission-profiles lint floor is
  conditioned on Helm rendering).
- The generator models lint documents explicitly
  (`crates/helm-schema-gen/src/uncoalesced_root.rs`, `LintDocument` /
  `LintWithdrawal` in the emission report).
- Test harnesses validate the coalesced document, not root-only defaults;
  the CLI harness carries a port of Helm's `ProcessDependencies` +
  `CoalesceValues`.
- Runner v10 (2026-10-19) makes it a landing gate: a shipped schema must
  accept its own chart defaults whenever the schema-free control renders,
  with only a shrink-only `ACCEPTED_DEFAULTS_FAILURES` list as exception.

## References

- `plan/chart-corpus-expansion.md` "Round-1 results" and the F12 wrap-up
  (`KNOWN_VALUES_REJECTIONS` to empty)
- `plan/schema-emission-profiles.md` "Policy classes and the laws" (lint floor)
- `plan/schema-bug-hunt-v1-progress.md` line ~2349 (POLICY, user decision
  2026-09-24) and the Oct 19 runner v10 entries (lines ~8796-8798, 18:32 ~8821-8828)
- CLAUDE.md "Verification gates" (schemas validate the COALESCED document)
- Commits 3a3871dd (2026-07-12), 8e8757ae (ledger, 2026-09-24),
  83247085 (`uncoalesced_root.rs`, 2026-09-26)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
