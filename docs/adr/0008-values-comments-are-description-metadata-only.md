# 0008. Values-file comments are description metadata only

Status: Accepted
Date: 2026-06-10 (commit abc666c8 "derive schema descriptions from values comments"; `plan/next-priorities.md`)

## Context

`values.yaml` comments carry useful documentation for chart users, and the
generated schema has `description` fields that editors show. Comments are
free text, not a typed language helm-schema can analyze structurally.

## Decision

Values-file comments are carried into generated JSON Schema descriptions as
metadata only. They do not create values paths and do not influence
inferred types, nullability, requiredness or object shape.

## Consequences

- Comment text never changes which values the schema accepts.
- The inputs list of ADR 0007 names "comments/descriptions as metadata
  only"; the unified frontend redesign kept "comments are user-facing
  metadata" as a ground rule.
- Descriptions can be removed from output without changing validation
  (schema-aware `--strip-descriptions` output, a8ee4315).

## References

- `plan/next-priorities.md` "Current state" (comments bullet)
- CLAUDE.md "`values.schema.json` is output, not inference evidence" (input
  list)
- `plan/unified-frontend-redesign.md` "Ground rules"
- Commits abc666c8 (2026-06-10), a8ee4315 (2026-06-10)
