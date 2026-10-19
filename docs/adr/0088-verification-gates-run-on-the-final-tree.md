# 0088. Verification gates run individually on the final tree

Status: Accepted
Date: 2026-08-02 (CLAUDE.md "Verification gates", commit 4d2aba0f)

## Context

CLAUDE.md names two ways a round can look done when it is not: a gate
summarized as passing without having been run after the last edit, and a gate
whose exit code was masked by piping its output through `tail`/`grep`. The corpus
fixture suite also lives only in the nextest `integration` profile, so a plain
nextest run can be green while every corpus fixture mismatches.

## Decision

A round of analyzer/generator work is done only when every gate below has been run
on the FINAL tree, after the last code or fixture edit, with each result reported
individually and judged by the gate command's own exit code:

1. `cargo fmt --check`
2. `task lint` (the whole workspace must compile under clippy)
3. `task lint:fc` (feature-combination lint via cargo-fc)
4. `cargo nextest run --workspace` (debug, never `--release`)
5. `task test:integration` (the corpus fixture suite)
6. `task test:all` before declaring a round final (includes live network tests)
7. the downstream luup2 `check:local` gate whenever schema semantics change
8. for refactors, the production LOC delta via `task tokei:core`

## Consequences

- "All gates pass" without per-gate evidence is not an acceptable report; a gate
  that was not run counts as failed.
- The landing chain (ADR 0089) automates gates 1–5 for a landing but does not replace
  gates 6–8, which are reported separately.
- A clippy failure in one crate hides lints in its dependents, so lint is green only
  once the whole workspace has been checked to the end.

## References

- CLAUDE.md "Verification gates" and "Running tests"
- `.claude/skills/verification-gates/SKILL.md`
- `.claude/skills/landing-workflow/SKILL.md` §6 ("The chain does not replace the
  CLAUDE.md gates")
- Commit 4d2aba0f (2026-08-02) "docs(agents): record the verification gates and
  adjudication rules"
