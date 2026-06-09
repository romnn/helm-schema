# helm-schema-ast

Shared AST and expression parsing for Helm templates embedded in YAML.

This crate owns two related responsibilities:

- parsing YAML templates into `HelmAst`, a common representation used by the
  rest of the workspace;
- parsing individual Go-template actions into typed `TemplateExpr` values so
  callers can inspect helper calls, selectors, pipelines, literals, and control
  flow structurally instead of scanning strings.

The tree-sitter-backed parser produces the shared AST used by the rest of the
workspace. The AST intentionally stays close to the source template: it
preserves Helm control-flow nodes, helper definitions, scalar YAML structure,
and action text, while higher-level semantic interpretation lives in downstream
crates.

Keep new parsing work structural. Prefer extending `TemplateExpr` or `HelmAst`
over adding regex or substring-based parsing in consumers.
