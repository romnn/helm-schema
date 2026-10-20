# 0049. Runtime `tpl` programs stay open; no allowlists or regex template parsers

Status: Accepted
Date: 2026-08-02 (fifty-seventh corpus round, 9fb00bad)

## Context

Charts pass values strings through `tpl`, so a user can supply a Go-template
program in a values file that Helm parses and evaluates only at render time.
Findings F30/F82/F89/F100 asked for those runtime programs to be constrained
like ordinary values.

## Decision

A program supplied later through a values file is not a generation input, and
its result is left open. Draft-07 has no operation that can parse the string
as a Go template, evaluate it against the coalesced values document, and apply
a caller-slot regex, requiredness rule or provider schema to the result; this
is "a language boundary, not an unimplemented bounded parser case".
Chart-authored defaults and statically constructed finite programs remain
generation inputs and are analyzed; action-free inputs still receive their
exact identity placement contract.

## Consequences

- Rejected alternatives: treating strings that resemble known corpus programs
  specially (a source-text allowlist, i.e. a heuristic) and decoding them with
  schema regexes (a second, heuristic template parser). Both violate the
  parser-first design (ADR [0006](0006-parsers-over-string-heuristics.md)) and still could not relate context-dependent
  output to sibling values.
- Pinned by `post_tpl_regex_admits_template_programs` and
  `tpl_rendered_slots_keep_the_raw_program_open`.
- Reopening needs an explicit model extension (for example a custom
  validation vocabulary), not more of the same analysis.

## References

- `plan/chart-corpus-status.md` "Rejected (invalid or won't fix by design)",
  F30/F82/F89/F100 (lines ~2652-2672)
- `plan/chart-corpus-expansion.md` "Why arbitrary runtime programs stay open"
  (lines ~11935-11945)
- Commit 9fb00bad (2026-08-02) "docs(plan): record rounds fifty-one through
  fifty-seven"
