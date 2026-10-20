# 0008. The grammar agrees with Go's text/template

Status: Accepted
Date: 2026-08-31 (memory note `grammar-regeneration.md`; landed in abb753c2, 2026-09-01)

## Context

`{{ toYaml .| nindent 8 }}` was silently misparsed because the vendored
grammar modeled command arguments as pipelines separated by a literal single
space, so argument boundaries depended on whitespace bytes. A Rust rescue
(`call_with_unfolded_pipe`) covered only some shapes, so suites stayed green
while charts got an unsatisfiable schema clause.

## Decision

The grammar must agree with Go's `text/template` on every input Go accepts.
Arguments are Go's "Arguments" operands (never bare pipelines), the argument
separator is any whitespace run, and token adjacency can never change the
tree. Defects are fixed in the grammar following Go's documented grammar,
"not another rescue layer"; the rescue code was deleted.

## Consequences

- Fidelity is verified with a differential harness against real Go
  (`text/template/parse` with SkipFuncCheck) over every action in the chart
  corpus (46,061 actions, zero structural divergences in the 2026-09-01
  review); the harness is rebuilt from that pattern when the grammar changes.
- Go tolerances are matched too (a trailing pipe before `}}` or `)`).
- Deliberate divergences are documented, not hidden: a superset of Go on
  inputs Go rejects; complex/imaginary/rune literals and some escapes
  abstain as `Unknown`; last-ULP hex-float rounding; `range ... =` modeled
  like `:=` (`plan/unspaced-pipe-grammar-fix-review.md` "Accepted
  divergences").

## References

- `plan/unspaced-pipe-grammar-fix-review.md` "Root cause", "The fix",
  "Verification", "Accepted divergences from Go"
- Memory note `grammar-regeneration.md` ("Invariant")
- Commits abb753c2 (fix), df5d6862 (review record), 77987613 (the earlier
  partial rescue)
