# 0017. Render conditions are preserved as a guard DNF

Status: Accepted
Date: 2026-07-11 (`8ab60379`, "preserve render conditions as guard dnf")

## Context

The interpreter computes each render site's condition as a disjunction of
conjunctions of predicates (`HelperOutputMeta.predicates`). The projection
used to flatten it into one `ContractUse` row per branch with a flat
`Vec<Guard>`, and the pipeline then reconstructed the disjunction twice
(builder branch algebra, generator emission minimizer). One guarded splice
could emit many rows (minio's `renderSecurityContext`: six rows for one
placement), and the B4 regression was a flatten/reconstruct mismatch.

## Decision

The condition under which a use is rendered survives projection as one
normalized `GuardDnf` (`BTreeSet<BTreeSet<Predicate>>`, normalized by
resolution, absorption and dedup) carried on the contract row
(`ContractUse.condition`), instead of being flattened into per-branch rows
and reconstructed downstream.

## Consequences

- The flatten/reconstruct bug class disappears; one placement is one row.
- `Guard`, `Predicate`, `GuardDnf` and the `ContractUse.condition` encoding are
  public/wire shapes; the v4 E4 study (2026-09-02) explicitly retained them.
- Execution conditions (what the chart says) stay distinct from the
  schema-lowerable `ConditionalGuard` vocabulary (what emission may soundly
  enforce); see the E4 study cited below.
- Later predicate work canonicalizes conjunctions and shares predicate nodes
  in place (v4 B5a-B5c) rather than replacing the representation.

## References

- `plan/architecture-review-v2.md` "Verdict" (local-vs-global answer) and
  "Step 5 — `GuardDnf` as the row condition" (`9e45f9b0`, 2026-07-12)
- Commit `8ab60379` (2026-07-11); code: `crates/helm-schema-core/src/`
  (`GuardDnf`, `contract_use.rs`)
- `plan/architecture-review-v4-progress.md` "E4 — guard-vocabulary
  unification study" (public/wire decision), "B5a"-"B5c"
