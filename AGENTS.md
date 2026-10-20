# Agents

## Goal

Recover chart meaning through typed, structural static analysis first. Preserve exact alternatives
and ambiguity; prefer an explicit unknown over a wrong guess. Use bounded, explicit heuristics
only after precise structural signals run out, and never let them override structural facts.
Prefer simplicity by deletion: one semantic model per fact, fewer representations and fallback layers.
Generate fast, deterministic, ordered output; key caches by every semantic input. Upstream is
authoritative: caches are fetch-on-miss optimizations, never correctness or capability oracles.
Treat shipped chart/dependency `values.schema.json` as output, never inference evidence; do not
ingest it or intersect it with generated output. Explicit override schemas are caller policy.

## Every session

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
- Profile product and verification tooling through `tracing::instrument` spans and the existing Perfetto layer; do not add custom phase timers.
- Run tests in debug mode with `cargo nextest run --workspace`; never use `--release` for tests.

## Verification gates

For analyzer/generator work, run every applicable gate on the FINAL tree after the last code or
fixture edit and report each result individually. A gate not run is a failed gate; check the
gate command's own exit code, never a trailing pipe's. Follow the
[verification-gates skill](.agents/skills/verification-gates/SKILL.md) for profiles and result interpretation.

1. `cargo fmt --check`
2. `task lint` — fix failures and rerun until the whole workspace compiles under clippy.
3. `task lint:fc`
4. `cargo nextest run --workspace`
5. `task test:integration` — corpus fixtures run ONLY in the nextest `integration` profile.
6. `task test:all` — required before declaring a round final; includes live network tests.
7. When schema semantics change: `cargo install --path ./crates/helm-schema-cli/`, then
   `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml check:local`.
8. For refactors, report the production LOC delta with `task tokei:core`.

### Fixtures and acceptance

- Produce one clean dump after the final build; never mix code states ([corpus-fixtures](.agents/skills/corpus-fixtures/SKILL.md)).
- Every fixture byte change requires the battery after that dump exists, with `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` naming it in every mode; committed fixtures are never the candidate ([acceptance-battery](.agents/skills/acceptance-battery/SKILL.md)).
- Adjudicate every corpus acceptance flip against real `helm template` before adopting fixtures; a tightening is a direction, not a verdict.
- Probe coalesced values composed over chart defaults with null-deletion semantics; bare `{}` means every declared key null-deleted, not defaults applied.

## Where to read more

Start with the [ADR index](docs/adr/README.md); read the relevant decisions before changing a subsystem.

- Analysis and ambiguity: [0001](docs/adr/0001-structural-static-analysis-first.md), [0013](docs/adr/0013-evidence-error-polarity-uncertainty-widens.md).
- Simplicity and phases: [0003](docs/adr/0003-simplicity-by-deletion.md), [0014](docs/adr/0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md), [0015](docs/adr/0015-contract-ir-is-the-semantic-seam.md).
- Inference inputs and parsing: [0004](docs/adr/0004-values-schema-json-is-output-not-evidence.md), [0006](docs/adr/0006-parsers-over-string-heuristics.md).
- Caches and capability probes: [0028](docs/adr/0028-schema-cache-is-fetch-on-miss-never-an-oracle.md), [0029](docs/adr/0029-tri-state-upstream-first-capability-oracle.md).
  Choose canonical probe kinds present from the API version's inception.
- Helm adjudication and values: [0051](docs/adr/0051-real-helm-is-the-correctness-oracle.md), [0054](docs/adr/0054-validate-the-coalesced-values-document.md).
- Dumps and candidates: [0056](docs/adr/0056-one-verified-producer-one-clean-dump.md).
- Performance and landing criteria: [0063](docs/adr/0063-deterministic-output-and-the-performance-law.md), [0066](docs/adr/0066-landing-ratchet.md).

Load the relevant skill for procedures:

- [verification-gates](.agents/skills/verification-gates/SKILL.md): gate ladder, lint policy, nextest profiles, tooling.
- [corpus-fixtures](.agents/skills/corpus-fixtures/SKILL.md): producer, manifests, regeneration and adoption.
- [acceptance-battery](.agents/skills/acceptance-battery/SKILL.md): baseline/candidate setup, flip adjudication, rosters, vacuous runs.
- [landing-workflow](.agents/skills/landing-workflow/SKILL.md): campaigns, ledgers, hand-offs, landing chain and receipts.
