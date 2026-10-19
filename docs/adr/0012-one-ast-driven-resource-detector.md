# 0012. One AST-driven resource detector

Status: Accepted
Date: 2026-05-18 (plan `plan/unify-resource-detector.md`, commit cc2662e6); implemented 2026-06-09 (04fc1789)

## Context

`helm-schema-ir` had two detectors answering "which Kubernetes resource does
this document represent?": the AST-based `DefaultResourceDetector` used by
many IR tests, and a line-based detector inside `SymbolicWalker` used in
production. Tests exercised the simpler detector and said nothing about the
production path, and the line-based walker was re-implementing a partial AST
(`---` boundaries, `inline_block_depth`, `ActionDirective` keyword matching).

## Decision

Collapse both into a single AST-driven detector used by production and
tests alike. It reads `apiVersion` / `kind` pairs from the YAML document
root regardless of source order, handles multi-document streams through
the AST's document boundaries, resolves helper-returned apiVersions through
typed helper evaluation, and reads capability `if`/`else` chains from the
AST `If` node (no keyword matching).

## Consequences

- `DefaultResourceDetector`, its trait, `scan_for_resource` and the
  line-based machinery in `symbolic.rs` (`InlineIfState`, `ActionDirective`,
  the dual-buffer split, `process_line`) are deleted.
- The detector returns multiple resources per template (multi-document from
  the start), which sets up the `kind: List` descent (ADR 0014).
- Resource identity was later re-based on the unified CST, reading
  documents and List items from it (ece942cf, ADR 0015).

## References

- `plan/unify-resource-detector.md` (status "implemented"; "Why", "Goal",
  "How")
- `plan/next-priorities.md` "Current state" (detector split gone)
- Commits cc2662e6 (plan), 04fc1789 (2026-06-09), ece942cf (2026-07-03)
