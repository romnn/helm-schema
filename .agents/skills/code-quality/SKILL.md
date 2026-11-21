---
name: code-quality
description: House rules for writing and reviewing code in helm-schema: parsing with real parsers, plain direct Rust, indoc/formatdoc, where tests live, sim_assert_eq!, thiserror vs eyre, lint suppressions, exhaustive destructuring of semantic carriers, semantic (Helm-derived) test expectations, tracing-based profiling and debug-only tests. Use whenever you write, change or review Rust code or tests in this repository.
---

# Code quality

These rules bind every change in this repository. They were the "Every session" list in AGENTS.md.

- Parse structured input with proper parsers instead of regexes or prefix chains. For action text,
  use `helm_schema_ast::parse_action_expressions` and match `TemplateExpr`; for Helm/YAML structure,
  use the tree-sitter-backed `helm_schema_ast::TreeSitterParser` and `HelmAst`.
- Keep Rust direct and simple: prefer local loops and `match`; add abstractions only for real
  duplication or clearer control flow.
- Use `indoc!` for multiline ordinary/raw Rust strings and `formatdoc!` when interpolating.
  Trailing-backslash continuations need neither; retain direct literals where syntax or a macro
  requires a literal token (attributes, patterns, `$literal` arguments).
- Put public API/end-to-end tests in crate `tests/`, private tests in `src/tests/`. No sibling
  `foo_test.rs`/`foo_tests.rs`/`foo.spec.rs` files or inline test modules in production files;
  only a minimal `#[cfg(test)] mod tests;` bridge to `src/tests/` is allowed.
  Mirrored integration tests may reuse production filenames under `tests/`.
- Keep test-only helpers out of production modules; use `tests/common.rs`, `tests/util.rs`, or
  `src/tests/` for private access. Exclude `src/tests/**` and crate `tests/` from production LOC.
- Import `use test_util::prelude::sim_assert_eq;` in each test module/integration-test file and
  assert equality with `sim_assert_eq!(have: actual, want: expected)`. `std::assert_eq!` is denied.
  Add `test-util.workspace = true` under dev-dependencies if missing.
- Assert full JSON schema equality in schema integration tests, never selected fields or snapshots.
  Update full expected fixtures explicitly for intentional changes; do not auto-regenerate expectations.
- Use typed `thiserror` enums throughout production libraries/binaries, including `main`;
  reserve `color_eyre` reports for tests and shared test support.
- In tests, import `color_eyre::eyre` and write `eyre::Result<T>`, never the fully qualified type
  inline. Import extension traits anonymously, e.g. `use color_eyre::eyre::{self, OptionExt as _};`.
  Use `ok_or_eyre(...)` for missing options; reserve `ok_or_else` for genuinely lazy/dynamic errors.
  Avoid importing a bare `Result` alias; keep result types explicit.
- Return `eyre::Result<T>` from fallible tests/helpers and propagate failures with `?` and useful
  context; do not add suppressed `expect`, `unwrap`, or `panic!` for representable errors.
- Before adding/changing Rust lint suppressions, load `rust-lint`; every `#[allow]` needs a
  concrete `reason = "…"` per the [lint policy](.agents/skills/verification-gates/SKILL.md#lint-policy-pointers).
- Destructure every semantic-carrier field and match owned enums without wildcards in merges, path maps, cache keys and dispatch; collapse hand-synced twins.
- Preserve semantic features during refactors; removing one requires an explicit product decision.
- Rewrites and producer deletions follow the `rearchitecture` skill:
  - inventory what the old path produces;
  - build the replacement beside it;
  - measure parity with the corpus parity preview, never with an admission suite alone;
  - delete only what the preview proves covered.
  False rejections against main are fixed, never rostered.
- Expected test schemas come from Helm's behaviour, main's fixtures or the destination schema, never from snapshotting
  current output. A red test must be semantic: validate discriminating documents against the expected and actual
  schemas before calling a difference a regression.
- Profile product and verification tooling through `tracing::instrument` spans and the existing Perfetto layer; do not add custom phase timers.
- Run tests in debug mode with `cargo nextest run --workspace`; never use `--release` for tests.
