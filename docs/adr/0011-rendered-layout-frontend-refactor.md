# 0011. YAML ownership from a rendered-layout skeleton, not source lines

Status: Accepted
Date: 2026-09-27 (user decision, ledger 14:35)

## Context

After the unified frontend ([0010](0010-unified-templated-yaml-frontend.md)), audits found 15 source-line ownership
heuristics and 22 text-reparsing sites: indentation-based parentage,
block-scalar/header repairs, scalar-blind `---` handling and `{{-3}}`
wrong trimming. Patching this D5 defect class would add more heuristics.
A June attribution rewrite had already failed by deleting fallbacks before
explicit slots existed: annotations attached to the wrong parent and nested
container fields disappeared. The unified CST later supplied that prerequisite.

## Decision

Replace the line model with one guarded rendered-layout skeleton per control
arm: exact literals, typed holes, CST trim effects and a YAML parse with
provenance; use `Uncertain` wherever a placeholder could change the parse.
`tree-sitter-yaml` must replace the line parser. Remaining line-model defects
become red acceptance cells of this refactor instead of more line patches.

Migrate shadow-first with serialized-byte differentials at each checkpoint.
Overflow widens; migrated constructs gain no uncertainty fallback. Claim only
what each step proves (crossing soundness before precision); delete old fields
and representations after their last consumer migrates.

## Alternatives considered

The June rewrite substituted path ranking and probe selection for a structural
slot model and was backed out; only blanking standalone YAML-injecting actions
survived. The lesson is to model mapping-value/sequence-item slots and
whole-slot versus descendant rendering before deleting compatibility logic.

## Consequences

- Retire text/control reparsing using typed expressions and CST delimiters;
  then parse skeletons and compose trim/indent/helper effects before parent
  assignment; finally delete the line/adoption/repair model.
- Phase 1 landed cb42c017 (2026-09-28), including Helm-proven `{{-3}}` and
  compact `{{if(...)}}` corrections. Phase 2.1 landed shadow-only in
  2f9e60d4 (2026-09-29), with no external syntax-crate consumer or fixture move.
- Phase 2.1 lands separately; 2.2/2.3 follow the in-flight semantic tracks.

## References

- `plan/document-attribution-rewrite-rollback.md` "The concrete regressions",
  "The real missing abstraction", "Decision rule used here"; 3230c4e7
- `plan/schema-bug-hunt-v1-progress.md` ~4018–4045 (audit/user decision),
  ~6112–6126 (22:40 Sep 28), ~6245 (phase-2 plan), ~6871 (landing 8)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §3; cb42c017, 2f9e60d4
