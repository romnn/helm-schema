# Dependency upgrades with cooldown

`cooldown` refuses to adopt any dependency version younger than a minimum release age, across
ecosystems, from one policy file. `cooldown.toml` at the repo root is the source of truth; tasks
pass `--sync` so native configs follow it.

## Commands

| Task | Runs | Mutates |
|---|---|---|
| `task outdated` | `cooldown outdated --sync` — "adoptable now" vs "in cooldown", incl. new majors | no (report) |
| `task outdated:minor` | same, within current majors | no |
| `task upgrade` (alias `update`) | `cooldown upgrade --major --sync` — newest version older than the window, cross-major, re-locks | manifests + lockfiles |
| `task upgrade:minor` | `cooldown upgrade --no-major --sync` | manifests + lockfiles |
| `task upgrade:latest` | `cooldown upgrade --latest` — **bypasses the cooldown**; prompts | deliberate, audited use only |

Useful read-only helpers: `cooldown config` (resolved policy and which layer set it),
`cooldown explain <pkg>` (why a package is held), `-n/--dry-run`, `--offline`, `--tool cargo`
(or `--cargo`) to restrict to one ecosystem, `-p <glob>` to scope packages.

Exit codes: 0 clean/nothing to do; 1 `check` violation or incomplete mutation; 2 usage/config
error; **3 no tool detected**; 4 stale/absent lock, registry unreachable, or a tool failed.

## This repo's policy (`cooldown.toml`)

- `min-age = "14d"` for everything.
- `[global] exclude-folders = ["/docs", "/grammars", "/testdata"]`: patterns are
  `.gitignore`-style and a leading `/` anchors to the repo root. `/docs` holds only the Hugo
  theme's Go module; `/testdata` holds vendored third-party charts. There is no root-level
  `grammars/`, so the nested npm project under `crates/helm-schema-template-grammar/grammars/`
  **is** managed (cargo + npm are the detected tools).
- `[tool.go] exclude-folders = ["/tools/helmsweep"]`: helmsweep's go.mod must stay exactly Helm's
  release requirement set (see `helmsweep.md`); a cooldown bump would make it diverge from the
  real Helm CLI. Move it only with the full Helm re-pin procedure.
- Consequence: no Go project is in scope. `cooldown … --tool go` prints
  `no supported tool detected under .` and exits 3. That is by design, not a breakage.

## Per-package rules (when you need to hold or relax one dependency)

```toml
[package."serde_*"]            # glob; `*` crosses `/`
min-age = "30d"

[tool.cargo.package.some-crate] # ecosystem-qualified: more specific than [package.x]
max-major = 2                   # absolute ceiling, even under --major; integer; package rules only

latest = true                   # sugar for min-age = "0d" (in a rule or at top level)
freeze = "<YYYY-MM-DD>"         # absolute publication cutoff instead of a rolling window
allow = ["acme-*"]              # audited exemption set (shown by `explain`)
floor = "3d"                    # minimum no nearer layer can weaken (max-clamped)
```

Precedence: layers (default < global < native < repo cooldown.toml < `--config` < env < CLI);
within a layer the most specific selector wins (tool-qualified package > package > registry >
project > tool > default). `max-major` has no CLI override: raising it in `cooldown.toml` is the
only way across. `upgrade` reports held rows with their reason (`bound …`, `max-major N`).

## After a Rust dependency bump

Run the whole gate ladder, including `task test:integration`; `cargo check` passing proves little.
Typical breakage classes:

- **API ports**: a new major changes signatures (e.g. a parser-combinator major changing its
  `Parser` trait usage) — compile errors, easy to see.
- **Stricter validation semantics**: e.g. a jsonschema release whose metaschema `format: "regex"`
  check uses a strict ECMA-262 (`u`-mode) parser. Committed schemas that carried RE2-isms then fail
  the integration-profile `schema_dialect_hygiene` gate, and the fix belongs in the generator's
  pattern-dialect respelling plus a fixture regeneration (`corpus-fixtures`), with every acceptance
  flip adjudicated (`acceptance-battery`). This class is invisible to the unit profile.
- **Output drift**: serialization/order or regex-engine changes alter emitted bytes; corpus
  fixtures catch it only in gate 5.
- **Lint drift**: a new toolchain/clippy adds pedantic lints; `task lint` and `task lint:fc` must
  be re-run to the end.

Commit lockfile/manifest bumps separately from the code that adapts to them when practical, and
never mix a cooldown bypass (`upgrade:latest`) into a routine upgrade.
