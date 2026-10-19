# 0017. Keep separate helper runtimes until an explicit event model exists

Status: Superseded by 0020
Date: 2026-06-21 (`plan/helper-single-walker-rewrite-postmortem.md`, commit c714ee06)

## Context

Helper bodies were traversed twice: once for value analysis and once for
fragment-output analysis. Two attempts to merge them (one walker with both
states; one generic walker with two runtimes) regressed, producing widened
pathless rows. The passes differ in execution semantics, not just output:
assignment/`set` handling, condition and `else` guard recording, range
binding and dot semantics, document-site output context, and local state.

## Decision

Back out the single-walker rewrite. Keep the shared helper control-flow
planning (`helper_runtime_plan.rs` owns `if`/`with`/`range` planning) and keep
separate value and fragment execution runtimes. Do not retry by adding
booleans to the runtime interfaces; a retry first needs an explicit event
model (assignment observed, local mutation applied, condition consequence /
alternative entered, range frame prepared, non-exact binding installed,
destructured-range output synthesized, output observed with site context).

## Consequences

- Structural planning stayed shared without pretending runtime semantics
  were identical.
- Superseded: the unified frontend redesign replaced the walkers with one
  abstract interpreter over one fragment domain, with helpers evaluated as
  memoized summaries in the same domain, and deleted the three walkers
  (`plan/unified-frontend-redesign.md` status note and "Target
  architecture"; commit 6ac85577).

## References

- `plan/helper-single-walker-rewrite-postmortem.md` ("Why it did not work",
  "What needs to exist before retrying", "Practical conclusion")
- `plan/unified-frontend-redesign.md` status note
- Commits c714ee06 (2026-06-21), 6ac85577 (2026-07-03)
