# 0018. YAML ownership from a rendered-layout skeleton, not source lines

Status: Accepted
Date: 2026-09-27 (user decision, `plan/schema-bug-hunt-v1-progress.md` 14:35 entry, line ~4045)

## Context

After the unified frontend (ADR 0015), a design review and a
line-heuristics audit agreed that YAML ownership in the syntax/IR frontend
was still decided from source-line shape: 15 violations of "parsers over
string heuristics" (indentation implies parentage, block headers via
`starts_with('|')`, block adoption repairs, `---` ignoring scalar context,
`starts_with("{{-")` mis-trimming `{{-3}}`, textual control-keyword
reparsing, among others) and 22 text-reparsing sites. That line model was the
root cause of the D5 defect class; patching it would add more line
heuristics.

## Decision

Replace the line model with one guarded rendered-layout skeleton per control
arm: exact literals, typed holes, CST trim effects and a YAML parse with
provenance, `Uncertain` wherever a placeholder could change the parse.
`tree-sitter-yaml` is justified only if it replaces the line parser. The
remaining line-model parsing is not patched further; defects in it become
red acceptance cells of the refactor (direction change, 22:40 Sep 28).

## Consequences

- Phases: (1) equality-preserving retirement of text reparsing, textual
  control-keyword reclassifiers and the `{{-` prefix check, using CST
  delimiter kinds and typed expressions, plus two Helm-proven corrections
  (`{{-3}}` is a negative literal; compact `{{if(...)}}`); (2) render
  skeleton with YAML parse and provenance; (3) trim/indent/helper
  composition before parent assignment; (4) deletion of the
  line/adoption/repair model.
- Landed so far: phase 1 (cb42c017, 2026-09-28) and phase 2.1 shadow-only
  (2f9e60d4, 2026-09-29). The D5 rework track was superseded by this one.

## References

- `plan/schema-bug-hunt-v1-progress.md` lines ~4018-4032 (audit), 4045
  (14:35 user decision), ~6112-6126 (22:40 Sep 28 direction change), 6871
  (landing 8)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §3 (frontend track)
- Commits cb42c017, 2f9e60d4
