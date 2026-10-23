# 0066. Landings ratchet: no new failure, strictly fewer known failures

Status: Accepted
Date: 2026-10-20 (user decision, round 8 session 6)

## Context

Blanket blocking on inherited counterexamples expanded each track to every
input its mechanism could see: W4 reached rework 17, W1 rework 14, and no
semantic change landed for three weeks (strict closures 3/83). Landing 10a's
new battery lane exposed main's defects, one requiring a later track to fix.
Meanwhile fixes grew hundreds or thousands of production lines per rework.

## Decision

A candidate lands when it satisfies the ratchet:

1. No new failure versus `main` (battery, sweep, witnesses, defaults/size).
2. Strictly fewer known failures: closed witness obligations, fewer false
   acceptances/rejections or inherited-list entries. Test-infrastructure
   landings may instead only record the existing failures they make visible.
2b. Consolidation-only landing (amendment, 2026-10-23, user decision D3 of
   `plan/value-algebra-r0.md` §6). A behavior-preserving consolidation
   satisfies clause 2 without closures when every other gate here and the
   fail-closed chain still pass, and all of:
   (i) frozen coverage: same artifact set, admission-suite cells and
   `ROSTER_BASELINE` as `main`; every fixture and the IR dump byte-identical
   to `main`'s clean dump; schema-only battery 0 flips; every admission
   verdict equal to `main`'s;
   (ii) frozen classification: the owner inventory gate
   (`plan/value-algebra-r0-inventory/`) passes against its baseline, with no
   class change and no fact/snapshot/re-execution row relabeled as a
   view/derived/observation row;
   (iii) reviewed consolidation: every deleted fact/snapshot/re-execution
   row has a reviewed producer mapping to a surviving declaration, every new
   such row names the rows it replaces, and the surviving producers are
   strictly fewer than the deleted rows (or are pre-existing);
   (iv) net production LOC (`task tokei:core`) strictly negative, coming
   from the region consolidated in (iii).
3. No net production growth unless reviewers justify it by the defect class
   closed. Fix the shared root cause; name the representation/pass/flag/case
   deleted or merged and report net LOC with `task tokei:core`. Per-witness
   exceptions and parallel representations warrant architectural REWORK.

Every unfixed counterexample becomes an exact obligation: frozen witness,
roster row where allowed, or inherited-failure-list entry with identity and
owning track, without wildcards. Lists only shrink: an entry that no longer
fails is stale and fails the gate until removed.

## Alternatives considered

The October 18 rule blocked on every Helm-verified counterexample, including
main-too and constructed cases; inherited defects had to enter the same
track's next rework. It was replaced because it prevented correct slices
from compounding on main and created infrastructure/fix ordering deadlocks.
Its root-cause requirement survives; blanket inherited blocking does not.

## Consequences

- Inherited defects stay tracked and in scope; candidates stop at the next
  landable improvement. New oracle lanes can land before their findings' fixes.
- Generalization is measured as closed/removed obligations versus net LOC.
  Architecture reviews seek consolidations preserving every Helm verdict.
- The fail-closed chain ([0062](0062-fail-closed-landing-chain-with-sealed-receipt.md)) is retained with this pass criterion.

## References

- `plan/schema-bug-hunt-v1-progress.md` 14:31/14:33 Oct 18 (~8212–8221,
  inherited blockers and simplification, 296d76ed); 04:20 Oct 20 (ratchet)
- `plan/campaign-orchestration-playbook.md` "Landing ratchet", "Simpler, not bigger"
- Runner v10 `ACCEPTED_DEFAULTS_FAILURES` (earlier shrink-only instance)
