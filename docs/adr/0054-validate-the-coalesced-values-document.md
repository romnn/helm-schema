# 0054. Test instances validate the coalesced values document obtained from Helm

Status: Accepted
Date: 2026-08-02 (coalesced-document rule); harness port landed 2026-09-27 (d734629c)

## Context

Helm validates the fully coalesced document: root values, subchart defaults
and propagated globals. Raw-root validation manufactured phantom errors and
hid real ones (gitlab: 49 phantom errors and a masked issuer-email rejection).
The round-74 battery initially probed root-only defaults too. A Rust Helm
composition port improved coverage but remained an approximation.

## Decision

Schema tests and probes judge the coalesced document. Compose probe overlays
over chart defaults with null-deletion semantics; bare `{}` denotes deleting
every declared key, not applying defaults. A user null is deleted only when
the innermost owning chart's own `values.yaml` declares the key.

The render adjudicator obtains the exact coalesced document from Helm itself:
`HelmAdjudicator::adjudicate_as` calls `coalesce_in` before rendering and
compares schema verdicts on those returned values. Helm is the composition
oracle, rather than a prescription to share a handwritten port everywhere.

## Consequences

- The Rust port in `crates/test-util/src/helm_values/` remains useful for
  screening and `acceptance_documents`: Helm does not print lint's two
  validation documents. Distinguish those approximations from the exact
  template document returned by the Helm coalescence path.
- A coalescence abort cannot coexist with a successful render; the adjudicator
  checks this invariant and preserves invocation evidence.
- The port initially matched 155/158 charts; moving the battery to it exposed
  the inherited kyverno `grafana=null` rejection. Those results explain the
  migration, not a claim that the port is the current render oracle.
- Lint's uncoalesced must-accept document is a separate emission policy ([0041](0041-schema-must-accept-chart-defaults-and-lint-document.md)).

## References

- CLAUDE.md "Fixture regeneration and flip adjudication" (4d2aba0f, 2026-08-02)
- `crates/helm-schema-test-support/src/helm/adjudication.rs` lines 267–313
  (`adjudicate_as`, `coalesce_in`, `acceptance_documents`)
- `plan/corpus-expansion-v1.md` "Methodology correction" (2026-09-03)
- `plan/schema-bug-hunt-v1.md` "For the implementor" items 4–5
- `plan/schema-bug-hunt-v1-progress.md` ~2349–2365, ~4480–4490
- d734629c (2026-09-27, initial composition port)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
