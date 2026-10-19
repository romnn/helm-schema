# 0006. Plain Rust (KISS) and `indoc!` for multiline strings

Status: Accepted
Date: 2026-06-21 (KISS rule, commit 50fe8327); multiline-string rule 2026-07-24 (commit 3a7d1a01)

## Context

Generic helpers, iterator tricks, macros and tiny adapters had been used to
save a few lines at the cost of readable control flow. Separately, multiline
string literals in Rust either carried the surrounding source indentation
into their value or broke the source indentation.

## Decision

- Rust code follows KISS strictly: no generic helpers, iterator tricks,
  macros, wrapper functions or adapter layers unless they clearly remove real
  duplication or make control flow easier to understand. Prefer the obvious
  local loop or direct `match`. A helper that needs a closure or type
  parameter to spell an obvious operation is a sign it is too clever.
- Rust string literals that intentionally contain line breaks use `indoc!`,
  or `formatdoc!` when they interpolate, for ordinary and raw literals alike.
  Trailing-backslash continuations need neither. A direct literal stays
  where Rust syntax or an outer macro requires a literal token (attributes,
  patterns, `$literal` macro arguments).

## Consequences

- Abstractions are justified by correctness or clarity, never by line count.
- The string convention is enforced by ast-grep rules
  (`.ast-grep/rules/multiline-string-use-indoc.yaml`,
  `multiline-format-use-formatdoc.yaml`) added in 3a7d1a01.

## References

- CLAUDE.md "Rust guideline: do not be cleverer than necessary" (50fe8327,
  2026-06-21)
- CLAUDE.md "Multiline Rust strings" (3a7d1a01, 2026-07-24)
