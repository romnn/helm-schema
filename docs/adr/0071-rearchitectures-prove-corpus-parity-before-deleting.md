# 0071. A rearchitecture proves corpus-wide parity with main before it deletes the old path

Status: Accepted
Date: 2026-10-29 (R3b landing campaign; lesson recorded at the user's request)

## Context

R3b (ADR [0069](0069-value-algebra-is-the-interpreter-semantic-model.md)) replaced the interpreter at checkpoint C02 and
deleted the old producers at the same time. Later checkpoints were meant to re-derive their facts. Progress was measured
on the W1 admission suite: 1,857 isolated snippets, 154 of 173 closed. That looked close to done.

The first corpus-wide preview ran only after C09, on 2026-10-28. It was the live battery against the roster baseline
over every corpus chart. It found:
- 90,586 new false-acceptance cells and 7,534 false-rejection cells against main;
- generated schemas shrunk from 191 MB to 12.6 MB;
- 567 unit and 248 integration regressions.
The landing estimate moved from days to weeks.

Isolated snippets cannot show what real charts combine: helpers inside helpers, serialized merges under guards,
dependency trees.

## Decision

- **Measure parity on the corpus, early and repeatedly.** As soon as a new path produces output, and at every
  checkpoint after, run the corpus parity preview against main:
  - the live acceptance battery against `ROSTER_BASELINE` over every corpus chart and lean control;
  - the unit and integration suites with that build's fixtures adopted.
  Report new false acceptances and false rejections by mechanism, plus unit and integration regressions.
- **Deletion is gated on parity.** An old producer or path is deleted only after the preview shows that the
  replacement covers its class: no new false acceptance or false rejection is attributable to the deletion. Until then
  the old path stays. Compare the two paths with a two-binary differential, never an in-binary dual run (ADR 0069).
- **Admission suites are necessary, not sufficient.** W1-style suites gate each step, but corpus numbers decide
  progress claims and landing estimates.
- **Preview economically.** Re-screen only the charts that carry a fixed class's witnesses. Run the full preview at
  each checkpoint and before every landing chain.

## Consequences

- A full live preview costs about 8–9 hours on the maintainer machine (around 130k Helm adjudications). Targeted
  previews cost minutes to an hour. Either is far cheaper than discovering the gap after the deletions.
- A rewrite carries the old path longer, which temporarily violates "one semantic model per fact" (ADR
  [0003](0003-simplicity-by-deletion.md)). That transitional duplication is accepted; it ends at the parity-proven
  deletion.
- False rejections against main are never rostered; they are fixed before landing (user decision, 2026-10-29). Residual
  false acceptances may be rostered on exact rosters (ADR [0057](0057-exact-rosters-against-a-pinned-roster-baseline.md))
  only by explicit user decision, with matching ignored-test reasons.
- Complements the landing ratchet (ADR [0066](0066-landing-ratchet.md)) and the fail-closed chain (ADR
  [0062](0062-fail-closed-landing-chain-with-sealed-receipt.md)). Those judge a landing; this rule judges a rewrite in
  progress.

## References

- AGENTS.md "Every session" and "Verification gates"
- `plan/schema-bug-hunt-v1-progress.md`, entries 08:02 and 13:36 (Oct 29)
- `plan/value-algebra-r3b.md` (C02 deletion, C09 adapter removal)
- Landing previews: round8-r3b-preview-evidence (63ed5d62) and round8-r3b-preview2-evidence (ad33fed8), maintainer
  workspace
