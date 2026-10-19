# 0076. Test instances validate the coalesced values document, composed by a port of Helm

Status: Accepted
Date: 2026-08-02 (CLAUDE.md rule); harness port landed 2026-09-27 (d734629c)

## Context

Helm validates `values.schema.json` against the fully coalesced values
document: root values, every subchart's defaults under its key, and
propagated globals. The repo's own self-validation gate and
`values_validation.rs` read the raw root `values.yaml` instead, which
manufactured phantom errors and hid real ones (gitlab: 49 phantom errors, a
masked `certmanager-issuer.email` rejection) (`plan/corpus-expansion-v1.md`
"Methodology correction"). The round-74 battery also probed over root-only
defaults (ledger ~2349–2365).

## Decision

Schema tests and probes judge the COALESCED document:

- Test instances compose over chart defaults with null-deletion merge
  semantics; a bare `{}` means every declared key null-deleted, not
  "defaults apply". A user `null` is deleted only when the key exists in the
  innermost owning chart's own `values.yaml`.
- The composition is a port of Helm v4.2.3 `ProcessDependencies` +
  `CoalesceValues` in `crates/test-util/src/helm_values/`, shared by the CLI
  harness and the battery, byte-equal to real Helm on 155/158 corpus charts
  when recorded.

## Consequences

- Dependency charts are judged against the document Helm actually validates;
  moving the battery onto the port exposed a pre-existing kyverno
  `grafana=null` false rejection, which was quarantined with its witness.
- `helm lint` validates a different, un-coalesced document; that second
  must-accept document is an emission policy, not part of this decision.
- The port is itself an approximation to maintain; the ledger records the
  plan to take Helm's documents from Helm's code via helmsweep and delete
  the port (ledger ~4480–4490), not yet done.

## References

- CLAUDE.md "Fixture regeneration and flip adjudication" (COALESCED rule, 4d2aba0f, 2026-08-02)
- `plan/corpus-expansion-v1.md` "Methodology correction" (2026-09-03)
- `plan/schema-bug-hunt-v1.md` "For the implementor" items 4–5
- `plan/schema-bug-hunt-v1-progress.md` lines ~2349–2365 (d3f23 finding), ~4480–4490
- d734629c `test(values): port Helm v4.2.3 values composition for the acceptance harness` (2026-09-27)
