# 0010. One templated-YAML frontend CST replaces the line model

Status: Accepted
Date: 2026-07-03 (`plan/unified-frontend-redesign.md`, Stage A landed in d81a6c89)

## Context

Templates are YAML with holes. The template half had a grammar, but the YAML
half used the `StructuralDocument` indentation model, byte-offset attribution
and resource-header re-parses. Earlier AST and production line detectors also
answered the same identity question differently; tests missed production bugs.

## Decision

`helm-schema-syntax` parses each template into one `TemplatedDocument` CST:
YAML structure, control regions and expression holes with byte spans. It
reuses tree-sitter actions and `TemplateExpr`, with recursive descent over
indentation. Controls that provably violate YAML alignment become `Opaque`.
Production and tests consume the same structural resource detector, reading
root `apiVersion`/`kind` in either order, document boundaries, typed helper
results and AST capability branches.

For the exact Kubernetes `v1` `List` envelope, descend into `items[*]`, derive
each item's identity and rebase its values uses to the inner resource path.
The wrapper receives no uses, lookup or diagnostic. Custom resources named
`List` remain resources; other `*List` kinds are outside this envelope rule.
Missing item identity is handled like missing top-level identity.

## Consequences

- Delete the line model, duplicate detectors, lookup-chain List short-circuits
  and resource-identity re-parses after byte identity holds (ece942cf).
- Nested List envelopes follow structural descent; they were not separately
  tested when this rule landed.
- IR fixtures must be byte-identical, or a strictly more precise slot must
  be adjudicated and justified; other changes are parser bugs.
- This reversed the July 2 hand-off's instruction to keep the line model;
  remaining line-shape decisions motivated the rendered-layout work ([0011](0011-rendered-layout-frontend-refactor.md)).
- Interpreter consolidation is a separate decision ([0012](0012-one-abstract-interpreter-owns-helm-semantics.md)).

## References

- `plan/unified-frontend-redesign.md` status, "Stage A", "Risks"
- `plan/unify-resource-detector.md` "Why", "Goal", "How"
- `plan/list-envelope-items-descent.md` "Why", "Goal", "Out of scope"
- `plan/full-core-architecture-simplification-handoff.md` lines 1119–1123
  (8a5443e9, 2026-07-02)
- Commits cc2662e6 (detector plan), 04fc1789 (2026-06-09), c942b0b5
  (List descent, 2026-06-10), d81a6c89, ece942cf (2026-07-03), 9624f587
