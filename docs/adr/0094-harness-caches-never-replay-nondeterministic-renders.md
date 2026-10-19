# 0094. Verification caches never replay nondeterministic renders

Status: Accepted
Date: 2026-09-27 (playbook "Standing decisions", commit 7b85b2d8)

## Context

The acceptance battery and the Helm sweep cache Helm render and lint verdicts to stay
within time budgets. 82 of 164 corpus charts were classified nondeterministic (`lookup`
57, the rest `genCA`/`randAlphaNum`/`now`/…). A review reproduced both sweep caches
replaying nondeterministic verdicts: `randInt`/`now`/dynamic `tpl` alternate pass and
reject on identical inputs. Environment clearing in the driver also happened after
client-go had read the user's kubeconfig location.

## Decision

Verification caches are keyed by every input, including the environment; templates
classified nondeterministic are never cached; a cache is never an oracle. Cacheability
is decided per row from Go's `text/template/parse` and recorded as an explicit
`cacheable` column, where a missing column or file means not cacheable.

## Consequences

- Nondeterministic charts are always re-rendered, which bounds how much caching can
  speed up the battery and sweep (the battery missed its 40-minute target for this reason).
- A change of driver environment handling invalidates the cache namespace.
- This is the verification-harness counterpart of the product cache rule in ADR 0093
  and CLAUDE.md "Cache is a speed optimisation, not a correctness oracle".

## References

- `plan/campaign-orchestration-playbook.md` "Standing decisions (from the user)"
  (commit 7b85b2d8, 2026-09-27)
- `plan/schema-bug-hunt-v1-progress.md` ~lines 4158-4172 (cacheability census, decision
  deferred to review) and ~4766-4772 (P1-2 environment, P1-3 replayed verdicts)
- `plan/schema-bug-hunt-v1-progress.md` 04:50 (Sep 27), runner v6.3 (`cacheable` column)
- `.claude/skills/landing-workflow/SKILL.md` §7 "Standing rules"
