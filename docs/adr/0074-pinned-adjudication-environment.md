# 0074. Adjudicate in a pinned Helm and Kubernetes environment

Status: Accepted
Date: 2026-08-25 (Helm adjudication version pinned, 82f0ea11); setup rules 2026-09-03; register 2026-09-05

## Context

A finding is a disagreement between Helm and the generated schema, so both
sides must be reproduced exactly. An unpinned `helm template` defaulted to a
newer Kubernetes version than the corpus and produced a false-rejection filing
that a pinned run withdrew; leaving a chart's shipped `values.schema.json` in
place made Helm enforce the author's schema instead of ours (332 bogus hits in
one sweep) (`plan/schema-bug-hunt-v1.md` "For the implementor").

## Decision

Adjudication runs in one pinned environment:

- Helm v4.2.3 (commit `43e8b7fe`, Go 1.26.5), pinned in `mise.toml` ("Schema adjudication depends on Helm's exact rendering behavior").
- `--kube-version 1.29.0`, matching the corpus generation bundle
  `v1.29.0-standalone-strict`.
- Private chart copies with the shipped `values.schema.json` removed and
  `templates/tests/` removed, including in dependencies.
- Test scope: the corpus keeps `--exclude-tests` (`include_tests: false`),
  and adjudication removes `templates/tests/` consistently with it.

## Consequences

- Verdicts are reproducible across runs and machines; any Helm upgrade is a
  deliberate re-pin (helmsweep and the battery share the pinned version, 0086).
- The test-scope policy is a known trade-off, not an accident: Helm renders
  `templates/tests/*` before filtering them, so real abort constraints there
  (e.g. kyverno `test.image.repository: null`) are outside the generated
  schema by policy.

## References

- `plan/schema-bug-hunt-v1-progress.md` "Decision register" (Helm pin; "Adjudication pins Kubernetes 1.29.0 …"; "Test-scope policy default"), 3ed93525, 2026-09-05
- `plan/schema-bug-hunt-v1.md` "For the implementor" items 1–3; "Policy question surfaced, not a bug"
- 82f0ea11 `chore(tooling): pin helm adjudication version` (2026-08-25)
- `.claude/skills/acceptance-battery/SKILL.md` (pinned Helm v4.2.3 + Kubernetes v1.29 strict bundle)
