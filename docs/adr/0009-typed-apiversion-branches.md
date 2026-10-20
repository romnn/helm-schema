# 0009. apiVersion alternatives are typed branches, not candidate lists

Status: Accepted
Date: 2026-05-18 (commit cc2662e6)

## Context

Vendored charts choose `apiVersion` through
`{{- if .Capabilities.APIVersions.Has "X" -}}` / `{{- else -}}` chains,
inline around the `apiVersion:` line or inside a helper. The previous
resolver flattened the alternatives into a candidate list tried in source
order, which masked real chart bugs (a chart emitting `policy/v1` where the
kind is not served there) and produced noisy per-candidate diagnostics.

## Decision

apiVersion alternatives keep their structure: helper outputs are
`HelperOutput::Branched { branches }`, each branch carrying a decoded
`CapabilityGuard` (`Has` / `NotHas` / `Opaque`) and a recursive body
(literals or nested branches), so guards compose through wrapper helpers and
nested `if` bodies. Inline chains around `apiVersion:` are captured the same
way. Resolution evaluates the guards (capability-aware) instead of trying
literals in order.

## Consequences

- If a helper such as `grafana.hpa.apiVersion` statically resolves to a
  finite set of literals, those exact candidates are derived (CLAUDE.md
  examples).
- An undecodable guard is `Opaque`, not guessed (ADR [0013](0013-evidence-error-polarity-uncertainty-widens.md)).
- Capability evaluation depends on the tri-state capability oracle, whose
  cache-safety contract is a separate decision (CLAUDE.md "Cache is a speed
  optimisation, not a correctness oracle").

## References

- Commit cc2662e6 message ("Typed apiVersion-branch model and
  capability-aware resolution")
- CLAUDE.md "Design goal: structural static analysis first" (apiVersion
  examples)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
