# 0087. Every sweep row runs through the fast engine; the real Helm CLI checks a bounded sample

Status: Accepted
Date: 2026-09-26 (runner v5 rule, ledger 23:05 entry); cap confirmed 2026-09-27 (ledger 04:50)

## Context

The landing sweep checks the shipped schema bytes through real Helm over
every roster row (chart × override): no new `helm lint` failure where the
schema-free `helm template` control renders. With the in-process engine
(0086) the remaining question was how much of the real CLI to re-run as a
cross-check. With no cap on differing rows, a change like F23 would re-run
~318 differing rows through the CLI, 3–8 h for little gain (ledger
~4866–4885).

## Decision

- Every chart and every sweep row runs through the fast engine; nothing is
  skipped.
- The real Helm CLI checks a sample: every 100th row per chart, up to 3 rows
  per chart whose fast verdicts differ (`SWEEP_DIFF_CAP=3`), and every
  unresolved row. The gate refuses on any disagreement between the CLI sample
  and the fast engine.
- Driver-contract tests guard the configuration class; full parity is re-run
  when Helm or the driver changes.

## Consequences

- Sampling applies only to the extra CLI differential, never to sweep
  coverage; rows must equal charts × overrides.
- The sample is a check on the engine, not the source of verdicts; the
  engine's fidelity rests on running Helm's own code (0086) and on parity runs.

## References

- `plan/campaign-orchestration-playbook.md` "Standing decisions (from the user)" (7b85b2d8, 2026-09-27)
- `plan/schema-bug-hunt-v1-progress.md` ~4480–4490, ~4555, 04:50 (Sep 27) entry ~4866–4885
- `.claude/skills/landing-workflow/SKILL.md` §6 "The landing chain" (`sweep` step), §7
- `plan/schema-bug-hunt-protocol-v2.md` §6 (sweep on shipped schema bytes, schema-free control)
