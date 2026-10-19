# 0039. Typed incomplete-analysis result (deferred)

Status: Proposed
Date: 2026-09-26 (bug-hunt round 8 design reviews; deferred the same day)

## Context

On 2026-09-26 (ledger 12:35) the campaign digested a batch of design reviews.
One of them, item (h) "incomplete-analysis result", proposed a typed result for
runs whose analysis is incomplete, and the ledger marked its adoption as a user
decision. A design document,
`design-incomplete-analysis-result-astra.md`, was written outside the
repository.

## Decision

Proposed, not adopted: analysis would return a typed `AnalysisOutcome` carrying
`UnresolvedObligation`s. The CLI would exit with code 3 by default when any
obligation is unresolved; `--allow-incomplete` would write the schema anyway,
marked with `x-helm-schema-incomplete`. Estimated size: 2,000–3,500 LOC.

At the 13:00 decision session of 2026-09-26 the user DEFERRED this result past
the bug-hunt campaign. It is recorded here so the design is not lost; it is
not current behaviour.

## Consequences

- Not implemented: no `--allow-incomplete` flag or `x-helm-schema-incomplete`
  keyword exists in `crates/` as of `8910ea1e`.
- Adopting it is a user decision (the ledger marks it as one) and would add a
  CLI exit-code contract and a schema annotation keyword.
- The design is queued in the round 8 hand-off §4 as "deferred past the
  campaign by the user".

## References

- `plan/schema-bug-hunt-v1-progress.md` ledger 2026-09-26, design-review
  "12:35" entry, item (h) (line ~3828) and "13:00 — User back; decisions adopted"
  (line ~3841); commits `42df7e85`, `00410877`
- `plan/schema-bug-hunt-v1-round8-handoff.md` §4, §5 ("incomplete-analysis
  result deferred")
