---
name: verification-gates
description: How to run and correctly read helm-schema's verification gates and day-to-day tooling. Use before declaring any analyzer/generator work done; when running fmt, `task lint`, `task lint:fc`, ast-grep, typos, nextest profiles, `task test:integration` or `task test:all`; when a test filter, profile or alias behaves unexpectedly; when upgrading dependencies with cooldown (`task upgrade`/`outdated`); when building, testing or re-pinning the helmsweep Helm driver or moving to a new Helm release; and when reporting the production LOC delta with `task tokei:core`.
---

# Verification gates and tooling

AGENTS.md (CLAUDE.md) owns the gate list; this skill explains what each gate really runs, what it
catches, and how to read its result without fooling yourself. Sibling skills own the rest:
`corpus-fixtures` (producer, dumps, fixture regeneration), `acceptance-battery` (flip
adjudication, `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP`), `landing-workflow` (campaign runner, receipts).

## Iron rules for reading results

1. **A gate's result is its own exit code.** `cmd | tail` reports `tail`'s status. Redirect to a
   log and read `$?`, or use `set -o pipefail` / `${PIPESTATUS[0]}`. A gate not run on the final
   tree after the last edit is a failed gate.
2. **`task` turns any failing command into exit 201.** `task -x <name>` passes the real code
   through (nextest: 100 = test failures, 4 = no tests ran). `task -n <name>` prints what would run.
3. **Several tools exit 0 while reporting problems.** Read their output:
   - `cargo lint` / `task lint`: rustc lints (`missing_docs`, unused items, …) and clippy
     `missing_errors_doc` are *warnings*; only denied clippy groups fail. Grep for `^warning`.
   - `task lint:fc`: summary rows are `PASS`/`WARN`/`FAIL`/`SKIP`; `WARN` exits 0 (no
     `--pedantic`). Every row must be `PASS` (or `SKIP` for pruned combinations).
   - `ast-grep scan`: every rule has `severity: warning`, so hits print but exit 0.
   - Conversely `cooldown` exits 3 (not 0) when no project of the selected tool is in scope —
     expected for Go here; see Dependency upgrades.
4. **Count tests.** Every nextest invocation in the taskfile and the `cargo t*` aliases passes
   `--no-tests warn`, so a filter that matches nothing exits 0. Check the `N tests run` line.
5. **Lint is green only when the whole workspace compiled under clippy.** A clippy error in one
   crate stops cargo from checking every crate that depends on it; their diagnostics are
   invisible, not absent. Fix the first failure and re-run to the end.

## The gate ladder (run from the repo root, in this order)

| # | Command | Actually runs | Catches | Cost |
|---|---|---|---|---|
| 1 | `cargo fmt --check` | rustfmt over every workspace member (root invocation) | formatting | seconds |
| 2 | `task lint` | `cargo lint --workspace --all-features` → clippy-wrapper → `cargo clippy --no-deps --all-targets --all-features --workspace`; then `task lint:ast-grep` (rule tests + scan) | clippy `all`+`pedantic` (deny), `unwrap/expect/panic/indexing` in non-test code, disallowed macros, reasonless `#[allow]`, indoc/formatdoc/label policy | ~0.5–1 min warm |
| 3 | `task lint:fc` | `cargo fc lint` (every package × feature combination × Linux, Windows-gnu, macOS-arm64 via cargo-zigbuild), then ast-grep | cfg/feature/target-specific breakage invisible to `--all-features` on the host | ~1–3 min warm; slowest lint |
| 4 | `cargo nextest run --workspace` (≈ `task test`) | nextest `default` profile: `not kind(test)` = lib/bin unit tests incl. `src/tests/` | private-API and unit regressions | seconds + rebuild |
| 5 | `task test:integration` | `task build:helmsweep`, the corpus producer, then nextest `integration` profile (all `tests/` binaries except `network_*`) | corpus fixtures, whole-chart schemas, Helm adjudication, dialect hygiene | the long pole: roughly 8–15 min |
| 6 | `task test:all` | `test:unit`, `test:integration`, `test:network` once each; prints all three exit codes, fails if any failed | everything above plus live upstream fetch | sum of the three |
| 7 | downstream consumer check (AGENTS.md step 7) | `cargo install --path ./crates/helm-schema-cli/` then the consumer's task | real-world schema regressions | only if that checkout is available in your environment |
| 8 | `task tokei:core` | tokei over `crates/`, minus test code | production-LOC delta for refactors | ~1 s |

Details, config and per-profile semantics: `references/lint-and-test-config.md`.

Gate 5 is the **only** place corpus fixtures run: `tests/` binaries are `kind(test)` and the default
profile filters them out, so a default-profile run is vacuously green while every corpus fixture
mismatches. `task test:integration` also needs Go (for helmsweep) and runs the producer into
`.cache/corpus-generation`; producer/manifest semantics belong to `corpus-fixtures`.

CI parity extras (not in the AGENTS.md list, but CI runs them): `task typos` (alias
`spellcheck`), `task test:doc` (**nextest never runs doctests**), `task unused` (nightly udeps via
cargo-fc), `task audit`, `task test:helmsweep`.

## nextest profiles (`.config/nextest.toml`)

| Profile | Default filter | Retries | Use |
|---|---|---|---|
| `default` | `not kind(test)` | 0 | unit tests |
| `integration` | `kind(test) and not binary(/^network_/)` | 0, cores − 3 threads | every `tests/` binary |
| `network` | `binary(/^network_/)` | 3, exponential backoff, serialized | live upstream fetch |
| `ci` | `all()` | 0; network binaries get the network override | everything in one run (`cargo tci`) |

- **CLI filters intersect the profile's default filter.** `cargo nextest run --test chart_corpus`
  under `default` selects zero tests and exits 0 with a warning. Use `-P integration` for any
  `tests/` binary: `cargo nextest run -P integration -p helm-schema-cli --test schema_dialect_hygiene`.
- **`cargo t` / `cargo ti` / `cargo tnet` / `cargo tci` have no `--workspace`.** From the root they
  test only `default-members` (= `helm-schema-cli`). Add `--workspace` or `-p <crate>`.
- `#[ignore = "maintenance: …"]` / `"live maintenance lane: …"` tests need `--run-ignored only` and
  their env vars; an unignored run of them is not evidence (see `acceptance-battery`).
- Offline tests read the vendored `testdata/provider-bundle/` with downloads disabled, which is why
  retries are 0: a failure there is a defect. Only `network_*` binaries may touch the network.
- Timeouts: slow is *reported* at `period`; a test is *killed* at `period × terminate-after`.

Useful env knobs for integration runs: `SCHEMA_HELM_ENGINE=helmsweep|cli` (default helmsweep),
`HELM_SCHEMA_HELMSWEEP=<path>` (else `<target>/helmsweep`), `HELM_SCHEMA_CORPUS_ARTIFACTS=<dir>`
(consume a producer run; a stale manifest fails the consumers).

## helmsweep and the Helm pin (summary)

`tools/helmsweep` is the in-process Helm oracle: `helm lint` / `helm template` verdicts from the
tagged Helm module (currently v4.2.3) plus one performance patch (`helm-memo.patch`). The
integration suite's Helm adjudication renders through resident `helmsweep serve` processes.

- `task build:helmsweep`: checks the exact Go toolchain (`HELMSWEEP_GO`, `GOTOOLCHAIN=local`),
  `stage.sh` builds a patched copy in `<target>/helmsweep-build` (download → go.sum verify →
  content-digest check → strict `git apply`), builds `<target>/helmsweep` with release ldflags,
  then checks `helmsweep version` against pinned lines (Go, Helm module + replace, `helm-build`,
  jsonschema/v6 version, patch sha256).
- `task test:helmsweep`: `gofmt -l`, stage, `go vet`, `go test -race` — in the staged copy. The
  source tree alone does not compile (it calls functions only the patch adds); never `go build`
  or `go test` in `tools/helmsweep` directly.
- `go.mod` must stay **exactly Helm's release requirement set** (a subset of Helm's go.mod with
  identical versions). Helm derives `.Capabilities.KubeVersion` from the linked `k8s.io/client-go`
  version and `.Capabilities.APIVersions` from client-go's scheme, so any dependency bump makes
  helmsweep render differently from the real Helm CLI. That is why cooldown excludes it.

Moving to a new Helm release touches ~a dozen pin sites in lockstep; follow
`references/helmsweep.md` (procedure, pin inventory, requirement-set check, failure symptoms).

## Dependency upgrades (summary)

`cooldown.toml` sets a 14-day minimum release age; `task outdated` (read-only report incl. new
majors), `task upgrade` (cross-major, within cooldown, re-locks), `task upgrade:minor`,
`task upgrade:latest` (bypasses the cooldown; prompts; deliberate use only). `/docs` and
`/testdata` are excluded globally and `/tools/helmsweep` for Go, so **no Go project is managed**:
`cooldown … --tool go` prints `no supported tool detected` and exits 3. That is expected; do not
remove the exclude to "fix" it. Rules, exit codes and what typically breaks after a Rust bump:
`references/dependency-upgrades.md`. After any bump, run the full ladder including gate 5 —
dependency behavior changes (e.g. a stricter jsonschema metaschema check) surface only in the
integration profile.

## Lint policy pointers

- Workspace lints (`Cargo.toml`): clippy `all` + `pedantic` deny; `unwrap_used`, `expect_used`,
  `panic`, `indexing_slicing`, `todo`, … deny; `allow_attributes_without_reason` deny (every
  `#[allow]` needs `reason = "…"`); rustc `missing_docs` warn; `unsafe_code` deny.
- `clippy.toml`: the unwrap/expect/panic/indexing denies are lifted in tests;
  `disallowed-macros` forbids `std::assert_eq!` (use `test_util::prelude::sim_assert_eq!(have:,
  want:)`) and `std::dbg!`. `disallowed_macros` is in clippy's `style` group, which the workspace
  denies, so a bare `assert_eq!` fails `task lint`.
- ast-grep (`sgconfig.yaml`, `.ast-grep/rules`): `multiline-string-use-indoc`,
  `escaped-newline-string-use-indoc`, `multiline-format-use-formatdoc`, `sim-assert-needs-labels`.
  Built-in exemptions and escape hatches: `references/lint-and-test-config.md`.
- Before adding or changing any suppression, follow the repository's lint-suppression policy
  (the `rust-lint` skill where available).

## `task tokei:core` (production LOC)

Counts `crates/` excluding every directory named `tests` (crate `tests/` and private `src/tests/`),
`fixtures`, `test-util` and `helm-schema-test-support`. Still counted: `examples/`, `build.rs`, and
inline `#[cfg(test)]` modules left in production files. The metric is the **Rust row's Code
column**. Arguments *replace* the default path: pass `task tokei:core -- crates -t Rust`, not
`-- -t Rust` (that counts the whole repo). For a delta, record the number before editing, or
count a pristine base read-only: `git archive <base> crates | tar -x -C <scratch>` and run the same
tokei excludes there. (`task tokei` is the whole-repo count minus grammars/testdata/fixtures.)

## Docs site (only when relevant)

`task docs:schemas` regenerates `docs/assets/schemas/*.json` by running the release binary on
`docs/examples/` (committed snippets, so schema-semantic changes can show up as docs diffs);
`task docs:build` / `task docs:serve` need hugo extended + go. Not part of the gate ladder.

## Pitfalls quick table

| Symptom | Cause | Fix |
|---|---|---|
| `N tests run` is 0 but exit 0 | filter ∩ profile default filter is empty; `--no-tests warn` | use `-P integration` for `tests/` binaries |
| `cargo t` runs only CLI tests | alias has no `--workspace`; `default-members` = cli | add `--workspace` |
| `no helmsweep at …/helmsweep` | integration run without building the driver | `task build:helmsweep` or `SCHEMA_HELM_ENGINE=cli` |
| `helmsweep needs go1.x.y` | pinned Go not installed via mise | `mise install go@<HELMSWEEP_GO>` |
| `module copy has digest …, not the pinned …` | Helm version changed without re-pinning `stage.sh` | see `references/helmsweep.md` |
| `helmsweep version lacks: <line>` | taskfile identity lines, `main.go` constants, ldflags or go.mod disagree | align all pin sites |
| `task lint:fc` 201, all non-host rows fail before clippy | sandbox/filesystem blocks zig's cache dir or cross toolchain missing | tooling failure, not a source diagnostic: fix permissions/tools and re-run the exact command |
| lint "clean" but dependents never checked | clippy error in an upstream crate | fix it, re-run until every crate compiles |
| `cooldown … --tool go` exit 3 | every Go project is excluded by design | expected; nothing to upgrade there |
| integration consumers fail on manifest | stale `HELM_SCHEMA_CORPUS_ARTIFACTS` producer run | re-run `task test:integration` (see `corpus-fixtures`) |
