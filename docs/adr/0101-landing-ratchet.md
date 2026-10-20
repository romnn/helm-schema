# 0101. Landings ratchet: no new failure, strictly fewer known failures

Status: Accepted
Date: 2026-10-20 (user decision, round 8 session 6)

## Context

ADR 0091 made inherited (main-too) defects block landings. Fixing defects at their
root remained right, but blocking on every inherited counterexample made each track's
scope "every input its mechanism can see": W4 reached rework 17 and W1 rework 14,
nothing semantic landed for three weeks (strict closures stayed at 3/83), and fixes
that would have compounded on `main` sat unlanded. Landing 10a exposed the extreme
case: a test-infrastructure landing with no generator change could not pass because
its new battery lane made existing defects of `main` visible, and one of them could
only be fixed by a track that had to land after it.

## Decision

A candidate lands when it satisfies the ratchet:

1. It introduces no new failure compared with `main` (battery, sweep, witnesses,
   defaults/size rules).
2. It strictly reduces known failures — closed witness obligations, fewer false
   acceptances or false rejections, fewer inherited-list entries — except
   test-infrastructure landings that only make existing failures visible, which record
   them and reduce nothing.
3. It adds no net production code unless the reviewers judge the growth justified by
   the class of defects it closes (ADR 0092).

Every counterexample a candidate does not fix becomes an exact, recorded obligation:
a frozen witness row, a roster row where the roster rules allow it, or an entry of the
battery's exact inherited-failure list (exact identity, owning fix track, no
wildcards). These lists only shrink: an entry that no longer fails is stale and fails
the gate until removed.

## Consequences

- Root-cause fixing continues; inherited defects stay in scope as tracked
  obligations, not as landing blockers.
- Generalisation becomes measurable per landing (closures and removed obligations vs
  net LOC); a fix that removes one row at a time while growing code is visibly a patch.
- Tracks hand back at the next landable improvement instead of after open-ended
  rework rounds; builders get a stopping rule.
- A new oracle or battery lane can land without first fixing what it reveals.

## References

- Supersedes ADR 0091; refines ADR 0089 (the chain is unchanged, its pass criterion is
  the ratchet) and ADR 0092.
- `plan/schema-bug-hunt-v1-progress.md` entries of 2026-10-20 (landing 10a battery-red
  classification; the inherited-failure list; this decision).
- Runner v10 `ACCEPTED_DEFAULTS_FAILURES` (shrink-only) as an earlier instance.
