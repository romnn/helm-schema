# 0030. One typed function-semantics catalog owned by the IR

Status: Accepted
Date: 2026-08-04 (architecture review v3 Step 5); landed 2026-08-05 (`888f2749`)

## Context

Facts about Helm/Sprig functions (operand roles, nil behaviour, output
stringification, collection shape, provenance, predicate meaning) were spread
over independent `is_*_function` predicates, nil tables and operand-position
tables, some of them in the AST (parser) crate. Separate tables could disagree
about the same function.

## Decision

`FunctionSemantics` is the single, IR-owned, exhaustive match giving one row
per recognized Helm/Sprig function for facts consumed by more than one phase.
Unknown functions get an explicit all-unknown row. Parser crates classify
syntax only. Truly special evaluation stays in explicit match arms. There is
no generic registry, macro DSL or order-dependent list of facets.

## Consequences

- AST no longer exposes semantic function classifiers or expression output
  typing; former `is_*_function` callers read the catalog.
- A table-driven test asserts each known function has exactly one row and
  that overlapping facets are intentional.
- Changes in a consumer's type, nil or predicate behaviour are adjudicated
  against live Helm.
- The catalog is part of the irreducible Helm idiom surface (v4 Part E), not
  a target for further collapse. A wrong row is a semantic bug (wave 2 R1
  corrected `unset` nil behaviour, `b6d9c6f3`).

## References

- `plan/architecture-review-v3.md` "Step 5 — replace function facets with one
  typed semantic catalog"
- `plan/architecture-review-v3-progress.md` "Step 5 — typed function
  semantics", "Wave 2 R1 — correct `unset` nil behavior"
- `plan/architecture-review-v4.md` Part E ("What is genuinely irreducible")
- Commits `888f2749`, `b6d9c6f3` (2026-08-05); code:
  `crates/helm-schema-ir/src/function_semantics.rs`
