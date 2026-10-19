# 0100. Test and error-handling conventions

Status: Accepted
Date: 2026-02-18 (Result types, commit 49bf0731); extended 2026-06-20 to 2026-07-23

## Context

Production LOC is used as a simplicity metric (CLAUDE.md "Simplicity by deletion"),
inline test modules blur it, large schema values make plain `assert_eq!` failures
unreadable, and application errors need precise types.

## Decision

- **Layout**: public API and end-to-end tests live in the crate's `tests/`; private API
  tests live under `src/tests/`. No sibling `foo_test.rs` files, no inline
  `#[cfg(test)] mod tests { … }` blocks (only a `mod tests;` bridge), and no test-only
  helpers in production modules. `src/tests/**` is excluded from core LOC metrics.
- **Equality**: tests assert with `test_util::sim_assert_eq!(have: …, want: …)`, a
  diff-printing wrapper over `similar_asserts`; `clippy.toml` disallows
  `std::assert_eq!` and the workspace denies `clippy::all`.
- **Schema tests** (full JSON schema equality, no snapshots) are recorded separately
  in ADR 0077.
- **Errors**: production code uses typed `thiserror` enums through every boundary,
  including `main`; `color_eyre` is for tests and test support only, written as
  `eyre::Result<T>`, with `ok_or_eyre` for missing options. Fallible test setup returns
  errors with `?` instead of `expect`/`unwrap`/`panic!`.

## Consequences

- These are enforced mechanically where possible (clippy disallowed macros, the
  `sim_assert_eq!` label grammar); the rest are reviewed.
- CLAUDE.md keeps the full wording; this ADR records that they are project decisions.

## References

- CLAUDE.md "Rust test layout and source LOC hygiene" (commit d7f0307b, 2026-06-22),
  "Equality assertions in tests" (commit 7c8269a3, 2026-06-20), "Schema tests" and
  "Result types" (commit 49bf0731, 2026-02-18; typed-error/`ok_or_eyre` wording in
  commit 9d777594, 2026-07-23)
- `clippy.toml` `disallowed-macros`
