# 0007. Vendor and own the go-template grammar

Status: Accepted
Date: 2026-07-24 (commit ba8c79bd "package vendored tree-sitter sources"; grammar in-tree since 0145e851, 2026-01-28)

## Context

The typed expression analysis (ADR [0006](0006-parsers-over-string-heuristics.md)) rests on a tree-sitter grammar for
Go templates. Fixing parse defects requires editing the grammar itself, and
generated parsers differ by tree-sitter CLI version.

## Decision

The go-template grammar is fully vendored and locally owned at
`crates/helm-schema-template-grammar/grammars/tree-sitter-go-template/`.
`make_grammar.js` is the source; `src/parser.c` and friends are generated.
Regeneration uses the CLI pinned in that directory's `package-lock.json`
(`npm ci && npm run generate`), never a globally installed tree-sitter.
The build compiles only the `gotmpl` dialect; the `dialects/helm` tree is
regenerated in lockstep but unused.

## Consequences

- Grammar defects are fixed at the grammar level (ADR [0008](0008-grammar-agrees-with-go-text-template.md)) rather than
  with rescue layers in Rust.
- Release packaging must include the vendored sources (ba8c79bd).
- Grammar corpus tests run with the pinned `node_modules/.bin/tree-sitter`.
- The pinned CLI version moves only through deliberate dependency upgrades
  (120419e9 moved it to 0.27.0).

## References

- Memory note `grammar-regeneration.md` (vendoring, pinned CLI, dialects)
- `plan/unspaced-pipe-grammar-fix-review.md` "The fix" (vendored, locally
  owned; regenerate with the pinned CLI)
- Commits 0145e851, dcd36f18, 12f06092, ba8c79bd, 120419e9
