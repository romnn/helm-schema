# 0090. Every fix lands with a red-then-green regression test

Status: Accepted
Date: 2026-09-24 (user rule during the schema bug hunt)

## Context

Corpus fixtures move whenever schema output changes, so a moved fixture shows that
something changed but not that a specific defect is fixed or will stay fixed. The user
asked for "minimal regression tests for all things that get fixed too - so nothing
ever regresses".

## Decision

Every fix carries a minimal regression test per fixed behavior: chart-free where the
mechanism allows, one test per contract cell, shown red on the pre-fix tree and green
on the fixed tree, with both commands, exits and logs recorded. A fixture change on
its own is not a regression test.

## Consequences

- A hand-back that does not name its tests is not landed.
- Landing protocol v2 makes "prove each fix with a regression test that failed before
  it" part of the contract-preservation claim of every landing.
- Tests follow the repository's test conventions (ADR 0100).

## References

- Memory `regression-tests-per-fix.md` (user statement, 2026-09-24)
- `plan/schema-bug-hunt-protocol-v2.md` §0 item 2 and line 41 (adopted 2026-09-26;
  commit 7b85b2d8)
- `plan/campaign-orchestration-playbook.md` "Standing decisions" ("Every fix lands with
  a red-then-green test; fixture drift is not a test")
- `.claude/skills/landing-workflow/SKILL.md` §7 "Standing rules"
