---
name: lane-builder
description: How a builder implements one interpreter or generator change in helm-schema quickly and lands it on the first review. Use when you are a builder (or brief one) working a lane in its own clone; when iterating on an analyzer change and choosing which check to run next; when measuring false-acceptance closures, reopenings or false rejections against the line or main; when checking CPU against main; when preparing a hand-back (patch, handoff, gates); or when a lane has run many full gates without converging.
---

# Lane builder

A builder owns one change in one clone. The time goes into verification loops, review rounds
and late surprises, not into writing code. In Nov 2026 one lane ran 73 full corpus dumps, 360 lint
runs and 376 test runs for 266 file edits over two days, while a complete landing proof took
about 40 minutes. These rules exist to cut that time.

Related skills: `verification-gates` (what each gate runs), `corpus-fixtures` (producer and
dumps), `acceptance-battery` (the round-74 battery and roster rules), `landing-workflow`
(landing chain and ledgers), `rearchitecture` (parity before deletion).

## 1. Get the direction before coding

- A design or analysis pass (root cause, file:line, the structural fix, what gets deleted, the
  law that pins it) comes before any implementation, CPU work included. Starting without one is
  how a lane ends up iterating for days.
- The fix is structural in the interpreter (CLAUDE.md, ADR 0070/0072/0073). No recognizers of
  template shapes, helper names or charts, no post-hoc filters, no caps that drop facts. A bound
  widens only its own object and is counted.

## 2. Inner loop: the cheapest signal that answers the question

Full gates are for hand-back. While iterating, use these, cheapest first:

| Question | Signal | Cost |
|---|---|---|
| Does my mechanism do the right thing? | the focused law: `cargo nextest run -p <crate> -E 'test(<name>)'` | seconds |
| What does one chart emit now? | `corpus_generation --out <scratch> --only chart/<dir> --helm-ready`, or the release CLI on that chart | seconds to a minute |
| Does it close or reopen anything, anywhere? | `corpus_replay` over saved Helm evidence (below) | ~1 minute for the whole corpus |
| Is it fast enough? | the CPU protocol (§4) on the charts the change touches | ~10 minutes |
| Did a schema verdict move at all? | the round-74 schema-only screen against your base (acceptance-battery) | ~5 minutes |

**`corpus_replay`** re-judges saved Helm-adjudicated documents (about 100k occurrences from an
earlier live preview) against a dump, with no Helm run:

```sh
cargo build --release -p helm-schema-test-support --bin corpus_replay   # from YOUR tree
corpus_replay --candidate <dump> --baseline-ref <base sha> \
  --evidence <saved preview root> --out <dir> --jobs 2 --include-unchanged
```

- It prints `fa_closed / fa_reopened / new_fr_candidates` overall and per chart (`<dir>/replay.md`,
  `replay.json`). `--baseline-ref main` measures distance to main instead of the line.
- Build it from the tree under test. The tool re-derives each chart's Helm-ready companion with
  its own minifier, so a binary from an older tree refuses newer dumps
  (`producer companion differs for chart/<x>`).
- A dump for the replay can come from `--only` runs while iterating; only the final proof needs
  one clean full dump.

Do not run a full release dump, `task lint:fc`, the full unit suite or the integration suite more
than once per checkpoint. If you have run the same full gate several times without a new closure,
stop and ask for a re-plan instead of trying another variant.

## 3. Your own counterexample pass before hand-back

Write `transfer/semantics-table.md`: one row per Helm 4.2.3 behaviour your change models, with the
Helm source file:line, how the code models it, and the case where it abstains (unknown, never a
guessed rejection). Run at least one adversarial input per row through real Helm and record the
receipt. Reviewers check the table; counterexamples found one at a time in review cost a full
round each (three rounds on one activation change, Nov 2026).

## 4. CPU before full proof

The landing bar is CPU per chart ≤ main (charts under 2 s on main are exempt) and corpus total
≤ main. Measure it before the long steps:

- Release CLI, five interleaved runs per chart of candidate, line and main, medians in CPU seconds
  (load recorded as a qualifier, never a gate). Use the corpus flags: `--no-config --offline
  --strict-k8s-version --k8s-version v1.29.0-standalone-strict --profile full --exclude-tests`
  with the provider bundle as `--k8s-schema-cache-dir`/`--crd-catalog-cache-dir`.
- Report every pair when the margin is small; a median 0.3 s under main with three pairs over it
  is a coin flip.
- Locate cost with `tracing` spans and the Perfetto layer, then a native sample of the hot
  process (`/usr/bin/sample <pid> 5`), not guesses.

## 5. Proof and hand-back

On the final tree, after the last edit (verification-gates has the details):

1. `cargo fmt --check`, `task -x lint`, `task -x lint:fc`, `cargo nextest run --workspace`. Compare
   failure IDs with the base's list: no new IDs; list the closed ones.
2. The integration tests you touch, by ID (integration profile); the full suite by ID if fixtures move.
3. If fixtures may move: one clean release dump (`--helm-ready --jobs 2`), `--verify`, then
   `corpus_replay` against the base. Report closed / reopened / new FR. Never adopt fixtures yourself.
4. The CPU table from §4.
5. Every fix has a minimal red-then-green law; record the red run on the pre-fix tree.
6. `task tokei:core` delta.

Hand back `transfer/<lane>.patch` (`git diff <base>` plus `/dev/null` hunks for new files) with its
sha256, and `transfer/handoff.md` under 60 lines: the Helm contract with receipts, the mechanism
with file:line, laws red and green, one line per gate with its exit, IDs closed and new, replay
counts, CPU table, LOC delta, anything unfinished and its exact blocker.

## 6. Working in a shared builder clone

- No git writes in a builder clone (commit, add, checkout, stash, reset, merge, rebase, push); the
  orchestrator commits. Read-only git is fine; `git show HEAD:path` reads a pristine file.
- Use your own `CARGO_TARGET_DIR` per tree; never share one across trees.
- Never kill processes by name or pattern; only PIDs you started. Run heavy commands under the
  memory wrapper the orchestrator names, one at a time. A memory-cap kill is a censored result:
  rerun smaller, never count it as pass or fail.
- Keep a resume file in `transfer/` and continue after context compaction. Short context is
  never a reason to hand back early.
- When you need something only the orchestrator can do (a native sample, a decision), ask once,
  carry on with independent work, and name the file you will read the answer from.
