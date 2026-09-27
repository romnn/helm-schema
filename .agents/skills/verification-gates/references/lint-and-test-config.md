# Lint and test configuration, in detail

## Where things live

| Concern | File |
|---|---|
| Tasks | `taskfile.yaml` (`task --list`, `task -n <name>` to dry-run) |
| Cargo aliases | `.cargo/config.toml` |
| nextest profiles | `.config/nextest.toml` |
| Workspace lints, cargo-fc matrix | root `Cargo.toml` (`[workspace.lints.*]`, `[workspace.metadata.cargo-fc]`) |
| Clippy settings | `clippy.toml` |
| ast-grep | `sgconfig.yaml`, `.ast-grep/rules/*.yaml`, `.ast-grep/tests/*.yaml` |
| Spellcheck | `.typos.toml` |
| Tool versions | `mise.toml` / `mise.lock` |
| CI | `.github/workflows/{lint,test,build,pages,release}.yaml` |

## Cargo aliases

| Alias | Expands to | Note |
|---|---|---|
| `cargo c` | `check --workspace --all-targets` | |
| `cargo t` | `nextest run -P default --retries 0 --no-tests warn --all-targets` | no `--workspace` → only `default-members` (helm-schema-cli) from the root |
| `cargo ti` | same with `-P integration` | same caveat |
| `cargo tnet` / `cargo tci` | `-P network` / `-P ci` (profile retries kept) | same caveat |
| `cargo lint` | `run --package clippy-wrapper -- lint` | builds the wrapper first (git dependency `clippy-shim`) |
| `cargo fixit` | `… -- fixit` | clippy `--fix --allow-dirty --allow-staged`: rewrites the tree |

`clippy-shim` adds `--no-deps`, `--all-targets`, `--all-features`, and `--workspace` only when run
from the workspace root without a narrower scope (cargo-fc runs it per package directory). It does
**not** add `-D warnings`: rustc warnings pass with exit 0.

## Workspace lints (root `Cargo.toml`)

- rustc: `unsafe_code` deny, `unused_must_use` deny, `missing_docs` warn, everything else at its
  default level.
- clippy: `all` + `pedantic` deny; `unwrap_used`, `expect_used`, `panic`, `unreachable`,
  `unimplemented`, `todo`, `indexing_slicing`, `await_holding_lock`,
  `allow_attributes_without_reason` deny; `missing_errors_doc` warn; `map_unwrap_or`,
  `assigning_clones`, `struct_excessive_bools` allowed.
- Every crate opts in with `[lints] workspace = true`.

`clippy.toml`: `allow-{unwrap,expect,panic,indexing-slicing}-in-tests = true` (tests may panic —
but AGENTS.md still wants fallible test code to return `eyre::Result` instead);
`disallowed-macros` = `std::assert_eq` (use `sim_assert_eq!(have:, want:)` from
`test_util::prelude`) and `std::dbg`; `std::assert_ne!` stays allowed (similar-asserts has none).
Since `disallowed_macros` is a clippy `style` lint and the workspace denies `clippy::all`, a bare
`assert_eq!` is a hard lint error.

## cargo-fc (`task lint:fc`, `task check:fc`, `task test:fc`, `task unused`)

- Matrix: every workspace package × its feature powerset (only `helm-schema` and
  `helm-schema-gen` have a non-default feature, `bench-support`) × three targets
  (`x86_64-unknown-linux-gnu`, `x86_64-pc-windows-gnu`, `aarch64-apple-darwin`).
- cargo-fc passes `--no-default-features` and enables each row's features.
- Non-host targets use `cargo-zigbuild` as the driver (needs zig + cargo-zigbuild from mise);
  `install_missing_targets = true` lets it `rustup target add` missing targets (network,
  toolchain mutation).
- `lint` subcommand is `diagnostics_only`; `dedupe = true` collapses identical diagnostics across
  rows.
- Summary rows: `PASS`, `WARN` (warnings, still exit 0), `FAIL` (sets the exit code), `SKIP`
  (pruned as implied by another row). Read every row.
- A sandbox that forbids writes to zig's global cache, or a missing cross toolchain, fails every
  non-host row before clippy runs. That is an environment failure: fix access and re-run the
  exact command; do not report it as a source diagnostic or as green.
- Changing the matrix itself is a design task (the `cargo-fc-matrix-design` skill, if available),
  not a gate fix.

## ast-grep (`task lint:ast-grep`, also run by `lint`, `lint:fc`, `lint:fix`)

`ast-grep test --config sgconfig.yaml --skip-snapshot-tests` (rule unit tests in
`.ast-grep/tests/`; `sim-assert-needs-labels` has none), then `ast-grep scan`. All rules are
`severity: warning`: findings print, exit stays 0.

| Rule | Flags | Not flagged (built-in escape hatches) |
|---|---|---|
| `multiline-string-use-indoc` | string/raw-string literal containing a real newline | literals inside any macro token tree (`indoc!{…}`, `json!`, `println!`, …), attributes (`#[doc = …]`), match patterns, `if let`/`let … else` patterns; trailing-backslash continuations (no newline in the value) |
| `escaped-newline-string-use-indoc` | literal with a `\n` escape that is not only the final character | byte strings (`b"…"`), same macro/attribute/pattern contexts, a single trailing `\n`, escaped backslash `\\n` |
| `multiline-format-use-formatdoc` | `format!`/`std::format!` whose format literal contains a real newline | — use `formatdoc!` |
| `sim-assert-needs-labels` | `sim_assert_eq!` without a leading `label:` argument | — write `sim_assert_eq!(have: a, want: b)` |

AGENTS.md "Multiline Rust strings" says when a direct literal is required (attributes, patterns,
`$literal` macro arguments); the rule exclusions mirror it. ast-grep also honors a
`// ast-grep-ignore: <rule-id>` comment on the preceding line, but the repo currently has none;
prefer restructuring the literal, and justify any suppression like a lint suppression.

## typos (`task typos`, alias `spellcheck`; CI job `spellcheck`)

`.typos.toml` excludes `testdata/`, `plan/`, grammars, helmsweep `go.mod`/`go.sum`, generated docs
assets. Inline escapes: `spellcheck:ignore-line`, `spellcheck:ignore-next-line`,
`spellcheck:ignore-block` (until the next blank line), `spellcheck:ignore-start` …
`spellcheck:ignore-end`. Genuine domain words go in `[default.extend-words]` /
`[default.extend-identifiers]`.

## nextest details (`.config/nextest.toml`)

- `default`: `not kind(test)`, retries 0, fail-fast off (`max-fail = "all"`), slow at 120 s, killed
  at 30 min.
- `integration`: `kind(test) and not binary(/^network_/)`, retries 0, `test-threads = -3`, i.e. cores
  minus 3 (whole-chart analyses are CPU-heavy; memory is not the limit), slow at 120 s, killed at 3 h (some tests shell out to `helm`).
- `network`: `binary(/^network_/)` (currently `helm-schema-k8s/tests/network_fetch_on_demand.rs`),
  3 retries with exponential backoff, test-group `upstream-schemas` (one at a time), killed at
  2 min.
- `ci`: `all()`, fail-fast off, network binaries get the network retry/serialization override.
- Filters given on the command line (`-E`, test names, `--test`) are **intersected** with the
  profile's default filter; `--ignore-default-filter` lifts it.
- `--no-tests`: default `auto` (= fail, exit 4); the taskfile and aliases pass `warn` (exit 0).
- `--run-ignored only|all` for `#[ignore]` maintenance lanes; they need their documented env vars.

`task test:fc` (CI `test` job) runs the default profile across cargo-fc feature rows;
`task test:doc` runs doctests with plain `cargo test --doc` (nextest cannot).

## Timing, relative

fmt and ast-grep: seconds. `task lint`: under a minute warm. `task lint:fc`: one to a few minutes
warm (three targets). Unit profile: seconds of test time after the rebuild. `task
test:integration`: the long pole, typically many minutes (producer + integration profile). Network:
a few minutes, dominated by upstream latency. A cold `target/` adds a large one-off build to
whichever runs first; do not compare timings across cold and warm runs.
