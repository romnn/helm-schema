# 0001. Structural static analysis first, heuristics last

Status: Accepted
Date: 2026-05-18 (CLAUDE.md "Design goal: structural static analysis first", commit cc2662e6)

## Context

helm-schema generates JSON Schema for a Helm chart's values. Charts can be
read either structurally (parsed Helm/YAML, helper bodies, control flow,
manifest shape) or approximately (regexes, text scans, line shape). The
resource-detector / apiVersion-resolution path was repeatedly bitten by the
approximate approach (CLAUDE.md "Parsers over string heuristics").

## Decision

helm-schema is a static analyzer first and a heuristic inference engine
second: "No heuristic should exist for a problem that can be solved by typed
structural analysis." Whatever can be known from parsed structure, helper
bodies, control flow or explicit manifest shape is derived from that
structure deterministically. Heuristics are allowed only after structural
analysis has run out of precise signals, and then they must be bounded,
explicit, lower-priority than structural facts, willing to abstain, and
diagnosable when they materially affect resolution.

## Consequences

- Typed Helm expression analysis over regexes; YAML/AST structure over line
  shape; helper expansion over filename guessing.
- Examples the project commits to: `kind:` before `apiVersion:` is detected
  from structure; `if`/`else` apiVersion branches and helper-returned
  apiVersions are analyzed structurally; `kind: List` items are the real
  resources; `.Values` guards (`if`, `with`, `range`, `default`, `eq`, `not`,
  `or`) shape the schema through typed control-flow analysis.
- Acceptable fallbacks are named: inferring an apiVersion for a known kind
  after the chart failed to reveal it, cache-root schema scans after exact
  resolution failed, bounded shortlists for well-known kinds, Kubernetes
  version fallback for removed APIs.
- A heuristic never silently overrides a precise structural result.
- The rule was restated as a ground rule of later redesigns
  (`plan/unified-frontend-redesign.md` "Ground rules").

## References

- CLAUDE.md "Design goal: structural static analysis first" (introduced in
  cc2662e6, 2026-05-18)
- CLAUDE.md "Parsers over string heuristics"
- `plan/unified-frontend-redesign.md` "Ground rules (unchanged from the campaign)"

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
