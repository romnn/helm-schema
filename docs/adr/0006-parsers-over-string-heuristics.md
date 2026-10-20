# 0006. Parsers over string heuristics; one tree-sitter parser path

Status: Accepted
Date: 2026-05-17 (commits 05f095ed, 76d5dfb3 route extractors through the typed expression AST); rule recorded 2026-05-18 (cc2662e6)

## Context

Go-template action text and Helm/YAML structure had been parsed with
regexes and `starts_with` / `strip_prefix` chains, which mishandle quoting,
pipelines, nested calls and trim markers. A second, fused Rust/yaml-rust
parser coexisted with the tree-sitter path.

## Decision

- Structured input (Go-template actions, YAML, JSON, schema-like text) is
  always parsed with a proper parser, never with regexes or prefix chains.
- Go-template action text goes through
  `helm_schema_ast::parse_action_expressions` and pattern matching on
  `TemplateExpr` (`Call`, `Pipeline`, `Selector`, `Literal`, ...).
- Template structure (define blocks, control flow, document boundaries,
  nesting, sequences, comments) comes from the tree-sitter-backed AST, not
  from line scans for `{{- if }}` / `{{- end }}`.
- There is one parser path: the legacy fused YAML parser crate was removed
  and the tree-sitter AST is used everywhere (a8ee4315).

## Consequences

- Line-oriented detectors are tolerated only for narrow single-token,
  single-line checks; anything involving control flow, pipelines, helper
  resolution or multi-line composition must use the AST. "It works for the
  cases I tested" is not an argument.
- Later audits treat source-line shape decisions as violations of this rule
  (15 such violations were listed before the rendered-layout refactor,
  ADR [0011](0011-rendered-layout-frontend-refactor.md)).

## References

- CLAUDE.md "Parsers over string heuristics" (cc2662e6, 2026-05-18)
- `plan/next-priorities.md` "Current state" (parser ownership unified)
- `plan/schema-bug-hunt-v1-progress.md` line ~4020 (line-heuristics audit)
- Commits 05f095ed, 76d5dfb3 (2026-05-17), a8ee4315 (2026-06-10)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
