# 0092. Fixes must simplify the architecture, not add special cases

Status: Accepted
Date: 2026-10-18 (user requirement, ledger 14:33)

## Context

Under the landing bar of ADR 0091, fixes were growing the code: one track grew
+407/+398/+721 production LOC over three reworks, another sat ~2.4k LOC above `main`.
The user required that defect fixes "are not hotfix patches but ideally simplify and
improve the architecture alongside correctness so the code becomes more simple and more
correct and not a pile to thousands of special cases".

## Decision

A fix addresses the root cause in the shared representation. Each fix names the
representation, pass, flag or special case it deletes or merges, reports its net
production LOC delta (`task tokei:core`), and justifies any growth by the class of
defects it closes. A fix that adds an exception per counterexample, a parallel
representation, or a new flag beside an existing one is sent back; reviewers may
return REWORK on architecture alone.

## Consequences

- Architecture audits of the large tracks look for consolidations that delete special
  cases while keeping every Helm-verified verdict; their proposals become rework targets.
- This applies CLAUDE.md "Simplicity by deletion" to bug fixing specifically.

## References

- `plan/schema-bug-hunt-v1-progress.md` line ~8221, entry 14:33 (Oct 18) "USER
  REQUIREMENT: fixes must simplify the architecture" (commit 296d76ed, 2026-10-18)
- `plan/campaign-orchestration-playbook.md` self-attack item 10 "Simpler, not bigger"
- CLAUDE.md "Design goal: simplicity by deletion"
