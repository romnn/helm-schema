# 0047. Size-aware output layout; never fail on size

Status: Accepted
Date: 2026-10-19

## Context

F74 is a blocker: a `values.schema.json` above Helm's 5,242,880-byte
chart-file limit cannot be installed, so it provides zero validation however
correct it is. An opt-in-only shortening policy left large default output
uninstallable. The shipping-size policy was
put to the user as a product decision.

## Decision

A CLI output-layout option with values `auto` (default), `pretty`,
`compact` (compact JSON + short `$defs` handles):

- `auto` writes pretty, and switches to compact with short handles only when
  the pretty schema would exceed the limit, with a note on stderr.
- Forced `pretty`/`compact`, and an `auto` result still over the limit, write
  the file with a WARNING and exit 0.
- Never fail on size: users feeding their own validator are not bound by
  Helm's limit; there is no typed size error.

## Alternatives considered

The September 27 policy forbade automatic shortening: `shorten` or
`--shorten-defs` plus `--defs-map` renamed definitions to short base-62 keys
(most referenced shortest) and translated diagnostics back to readable names.
That policy required caller intervention for large charts and was replaced
for default CLI output. Explicit shortening remains useful at Helm boundaries.

## Consequences

- Implemented on landing 10a (round8-size), not yet merged to main as of
  2026-10-19; main's docs still describe opt-in-only shortening. Fixtures
  remain readable and pretty at any size ([0045](0045-defs-names-are-readable-source-paths.md)).
- F74 obligations are measured on the final emitted CLI bytes (annotations,
  layout, trailing newline); the shortener reports whether it shortened.
- The build brief requires existing compaction/short-handle flags to be
  mapped or deprecated into the option, and a pretty-vs-compact semantic
  equivalence proof for every corpus chart.
- The landing gate rejects shipped bytes over 5,242,880 at dump time
  (runner v10); all five formerly oversized schemas fit compact.

## References

- `plan/schema-bug-hunt-v1.md` F74
- `plan/schema-bug-hunt-v1-progress.md` 14:41 Oct 19 USER DECISION (~8735,
  8483218a) and 18:32 Oct 19 entry (~8815-8828)
- `/Volumes/T7/dev/round8/brief-size-aware-output.md`
- Commits e70be667, d7ab4078 (round8-size, on landing-10a-v2);
  c7cd9fc5 (2026-09-27 opt-in policy); output reference "Size limit"
