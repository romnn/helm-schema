# 0015. One templated-YAML frontend CST replaces the line model

Status: Accepted
Date: 2026-07-03 (`plan/unified-frontend-redesign.md`, approved and Stage A landed in d81a6c89)

## Context

Templates are YAML with holes. The template half had a real grammar, but the
YAML half was recovered by the `StructuralDocument` indentation line model;
byte-offset attribution tables, per-document re-parses in resource identity
and header-line scanning followed from that dual-document problem. The
incremental consolidation campaign had reached the old model's floor.

## Decision

A new frontend-only crate, `helm-schema-syntax`, parses each template into
one `TemplatedDocument` CST in which YAML structure, control regions and
expression holes are first-class nodes with byte spans. It reuses the
tree-sitter action parse and `TemplateExpr`, and parses layout by recursive
descent over indentation. It assumes control actions align with YAML
structure; where an action provably violates that, the region degrades to
`Opaque` with its raw span, "never guess".

## Consequences

- The line model (`StructuralDocument` and friends) and the resource-identity
  re-parses were deleted once byte identity held; documents and `items:`
  come from the CST (ece942cf).
- Revision: on 2026-07-02 the full-core hand-off still said "Do not replace
  the line model itself; the rollback postmortem stands"
  (`plan/full-core-architecture-simplification-handoff.md` lines 1119-1123,
  8a5443e9); this decision reversed that a day later.
- Stage acceptance: IR fixtures byte-identical (or adjudicated: a strictly
  more precise slot needs written justification, anything else is a parser
  bug), full suite green, zero lint warnings.
- Remaining line-shape decisions inside the frontend were later targeted by
  the rendered-layout refactor (ADR 0018).
- The same redesign also replaced the interpreters with one abstract
  interpreter; that decision is recorded separately (IR theme).

## References

- `plan/unified-frontend-redesign.md` status note, "Why the current model is
  a local maximum", "Stage A", "Risks"
- Commits 3c9e037f (plan, 2026-07-03), d81a6c89, ece942cf (2026-07-03),
  9624f587 (completion record, 2026-07-04)
