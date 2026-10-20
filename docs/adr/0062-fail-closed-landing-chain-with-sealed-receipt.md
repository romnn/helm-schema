# 0062. Landings pass a fail-closed chain that seals a receipt

Status: Accepted
Date: 2026-09-26 (fail-closed landing runner v2, ledger entry 01:00)

## Context

A review of the round-4 landing found the runners were not fail-closed: a dump exit
was recorded and adoption proceeded anyway, adoption skipped missing files, the
sweep ran beside the lock, and there was no immutable record of tree hash, baseline,
corpus hash, tool versions, artifact manifest and gate exits.

## Decision

Every landing into `main` runs a fixed chain of separate steps, in order, stopping at
the first nonzero exit: `dump` (generate every fixture lane once from one build and
freeze the candidate) → `unit` → `lint` → `battery` (round-74 flip battery against the
named baseline) → `integration` (producer `--verify` with the frozen binary, then the
integration profile) → `sweep` (real Helm over the shipped schema bytes for every
roster row) → `finalize` (seal the receipt read-only and print its sha256).

Restart rules are enforced by the runner: any change to generation inputs, the env
file or the runner means a fresh evidence directory and a restart from `dump`; a
test-only change restarts from the earliest stale step. Only the fixtures listed in
the chain's `adopted.tsv` are committed, and the merge message names the receipt.

## Consequences

- A receipt proves exactly its recorded clone, HEAD, hashes, baseline, tools and steps,
  nothing about another candidate.
- Every acceptance in a landing env file (residuals, unresolved classes, overrides)
  must carry a comment saying why.
- The runner lives outside this repository; the in-repo pieces it drives are
  `tools/helmsweep`, the corpus producer, the battery test and the nextest/`task` gates.
- The chain does not replace the final-round gates in
  [AGENTS.md](../../AGENTS.md#verification-gates).

## References

- `.agents/skills/landing-workflow/SKILL.md` §5 "The landing model", §6 "The landing chain"
- `plan/schema-bug-hunt-v1-progress.md` ~line 3250 (review findings: runners not
  fail-closed, no immutable receipt) and line 3326 (01:00, "Fail-closed landing runner
  v2 written"), recorded in commit 0256c69e (2026-09-26)
- `plan/schema-bug-hunt-protocol-v2.md` §0 (a landing as bounded claims: artifact
  identity, contract preservation), commit 7b85b2d8
