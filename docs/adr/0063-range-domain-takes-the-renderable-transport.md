# 0063. Integer `range` operands follow the renderable transport (the F5 transport union)

Status: Accepted
Date: 2026-09-26 (user decision; transport conflict recorded 2026-09-05)

## Context

F5 found integers admitted at `range`-only sinks. Pinned Helm v4.2.3 turned
out to treat an integer `range` operand differently by values transport:
`--set items=0` renders a minimal `range .Values.items` (the value arrives
as int64), while `-f` with the same numeric zero in a YAML/JSON file aborts
(float64, `range can't iterate over 0`). Both serialize as the same JSON
number, so JSON Schema cannot tell the channels apart. The register's
default was to record this expressiveness limit and not claim
transport-independent integer rejection from file-based witnesses alone.

## Decision

The schema keeps the transport union: the domain Go's `range` iterates
without aborting admits integer counts (the renderable `--set` channel
wins), unless the loop body reads member structure an integer cannot
provide. Strings and non-integral numbers stay rejected in every channel.
Values-file (`-f`) false acceptances that follow from this are documented
as a transport limitation and rostered under F5.

## Consequences

- Positive integers that abort on member access in every transport must
  stay rejected; a cell pins that.
- Transport-dependent residuals of the same class (e.g. `eq $.Values.count 1`
  float64 vs int) are recorded as F5-class rather than fixed by guessing a
  channel.
- The rule lives once, in `runtime_iterable_schema`
  (`crates/helm-schema-gen/src/lib.rs` ~line 239).

## References

- `plan/schema-bug-hunt-v1.md` F5
- `plan/schema-bug-hunt-v1-progress.md` "Open questions" (F5 integer
  iteration, fc49d2a8, 2026-09-05), round-1 transport witness (~506),
  13:00 entry ~3849 (2026-09-26), ~4041, 17:52 Oct 19 entry ~8808
- `crates/helm-schema-gen/src/lib.rs` (`runtime_iterable_schema` doc comment)
