# 0022. One typed selection-reachability carrier beside `EvalResult`

Status: Accepted
Date: 2026-08-05 (decision point D3 resolved, option 1; v3 decision register)

## Context

Whether a value is selected (reaches its consumer) was decoded in several
lanes — raw predicate decoding, `TruthCondition`, `ScalarValueDispatch`,
local `DefaultPrimarySelection`, string-consumer capture scoping — which could
disagree. Round 74 fixed a false rejection by keeping rendered `printf` truth
in `ScalarValueDispatch`; two attempted parallel provenance flags tightened
documents Helm rendered and were deleted. v3 decision point D3 offered: (1) a
carrier owned beside `EvalResult`, (2) moving rendered truth into the raw
`Predicate` algebra, (3) keeping the lanes and sharing helpers.

## Decision

Option 1, chosen by the user: one typed `SelectionReachability` beside
`EvalResult` with explicit states — always selected, never selected, selected
under an exact predicate, approximately selected with an optional sound
subset — and a separate `SelectionTruthSource` that distinguishes raw input
truth from rendered scalar truth. Eager execution effects are retained even
when output selection is dead. Every condition and consumer lane queries this
carrier.

## Consequences

- Rejected: option 2, because representing output strings as raw values blurs
  the distinction that caused the Round 74 bug; option 3, because shared
  helpers do not make disagreement impossible.
- Truth, selection, scalar dispatch and string-consumer scope migrated one
  family per commit (Steps 6b.1-6b.5); a string requirement carries its
  selection predicate until it becomes a conditional capture, never a Boolean
  that erases scope.
- Exactness routes through the `TruthCondition` boundary (wave 2 R2).

## References

- `plan/architecture-review-v3.md` "D3. Owner of raw/rendered selection
  semantics", Steps 6a/6b
- `plan/architecture-review-v3-progress.md` "Decision register" (D3), "Step
  6a", "Wave 2 R2", "Step 6b.1"-"6b.5"
- Commits `779b64d4`, `70bc42d2`, `3f6c8291` (2026-08-05), `c6721b8c`,
  `add353e3`, `46b7f24b` (2026-08-06); code:
  `crates/helm-schema-ir/src/eval_effect.rs`
