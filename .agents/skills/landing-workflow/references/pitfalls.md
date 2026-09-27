# Pitfalls observed repeatedly in the ledgers

Each entry: symptom → cause → fix. Most were paid for with a restarted chain or a wrong
record in the ledger.

## Evidence and gates

- **Vacuous battery.** Symptom: `flips_adjudicated: 0` on a round that changed fixture
  bytes; per-chart `guards_discovered` equal to the baseline-only count; "0 tests run"
  (nextest exit 4). Cause: the battery read its candidate from the on-disk fixtures (schema
  compared with itself) or ran without ignored tests. Fix: set
  `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` to the final clean dump and include ignored tests;
  require `1 test run: 1 passed`. One campaign reported zero flips this way and carried 73.
  See `acceptance-battery`.
- **Wrong baseline.** Symptom: hundreds of known-false-acceptance rows look "fixed". Cause:
  baseline equal to the candidate. Fix: the roster is observed against the pinned
  `ROSTER_BASELINE`, and the battery refuses any other baseline; decide explicitly before
  advancing it (rows older than a new baseline need an absolute home).
- **Pipe exit codes.** Symptom: a "green" gate that failed. Cause: `cmd | tail` reports
  `tail`'s exit. Fix: capture the gate's own exit (`cmd > log 2>&1; echo $?`).
- **Unrun gates reported green.** Symptom: "all gates pass" while `lint:fc`, `test:all` or
  the luup2 gate were never run on the final tree. Fix: one line per gate with its exit, and
  "not run" where it was not.
- **Masked lint.** Symptom: a builder's clippy is clean, the chain's `lint:fc` fails. Cause:
  the builder ran clippy with a lint allowed (`-A clippy::too_many_lines`), or a failing crate
  stopped clippy before dependent crates. Fix: run the real `task lint` and `task lint:fc`
  to the end; no `#[allow]`/`#[expect]` additions.
- **Integration-only tests.** Symptom: the chain's integration step fails on a test whose
  expected JSON was never updated. Cause: the default nextest profile never runs integration
  tests. Fix: run the touched integration binaries under `-P integration` before hand-back.
- **Fixture drift as proof.** Symptom: "tests green, fixtures updated" with no regression
  test. Fix: every fix needs a minimal test that was red before it; fixture diffs are
  adjudicated, not trusted.

## Trees, builds and binaries

- **Broad staging in a landing clone.** Symptom: a commit contains hundreds of fixtures
  nobody edited. Cause: `git add -A` picked up files a previous dump left in the clone.
  Fix: stage named files only; check `git diff --stat <base> -- testdata` before
  committing; restore anything unintended to the base.
- **Mixed dump batches.** Symptom: drift that keeps "growing" run after run, looking like
  nondeterminism. Cause: part of a dump ran against a stale binary. Fix: build to
  completion, then one clean dump with no concurrent cargo; that dump is the authority.
  See `corpus-fixtures`.
- **Shared target dirs.** Symptom: the producer binary's sha differs between dump and
  integration; a build serves stale crates after a tree switch. Cause: other steps rebuild
  with different feature unification; mtimes older than the target. Fix: exclusive
  `CARGO_TARGET_DIR` per clone; the runner freezes the producer at dump; touch
  `crates/**/src/*.rs` after switching trees in a shared target.
- **Intent-to-add paths.** Symptom: a pinned CRD schema became zero bytes in several clones.
  Cause: `git checkout`/`restore` over a `git add -N` path, then a patch carried the
  truncation. Fix: never `git add -N`; commit real bytes; inspect zero-byte files at once.
- **Disk exhaustion.** Symptom: a Go or Rust link fails with ENOSPC or odd errors. Cause:
  killed battery runs leak multi-GB temp dirs into the system temp folder. Fix: keep
  `TMPDIR`/`GOTMPDIR` under the target volume; check `df -h` when a build fails oddly.

## Runner and chain

- **Editing the runner during a chain.** Symptom: sweep workers exit with `usage:` and rows
  go missing. Cause: the runner directory was edited while the chain executed from it. Fix:
  runner changes in a worktree, merged only between chains, red-then-green in its suite.
- **Stale receipt after a fix.** Symptom: the runner refuses `unit` with "generation inputs
  changed since the dump". Cause: a production-side edit after the dump. Fix: fresh `E`,
  restart from `dump`; do not hand-copy fixtures around the refusal.
- **Hidden refusals behind skips.** Symptom: rows that never got a Helm verdict surface only
  when a skip is removed (a chart without `Chart.yaml`; a library chart that is "not
  installable"). Fix: every row runs; accept Helm's own refusals explicitly per chart in the
  env with a comment, and give them proper classes in the classifier.
- **Lock handling.** Symptom: exit 75. Fix: record and reschedule; never remove or take the
  lock, never infer ownership from process names.

## Records and coordination

- **Concurrent plan/ edits.** Symptom: an Edit is refused because the file changed, or a
  round number appears twice. Cause: another agent or vendor session writes the same
  ledger. Fix: re-read from disk, keep foreign entries, renumber your round.
- **Shell-mangled ledger text.** Symptom: an entry with missing backtick spans or `$defs`
  replaced by nothing. Cause: an unquoted heredoc. Fix: quoted heredoc (`<<'EOF'`) or the
  Edit tool; re-read the entry after writing.
- **Stale pointers.** Symptom: the `Next:` line or a hand-off names work that has already
  landed, or paths on another machine. Fix: confirm with `git log` and the newest dated
  entry; update the `Next:` line with every entry.
- **Policy drift through review.** Symptom: builder, reviewer and orchestrator all call a
  result compliant that contradicts the user's stated policy (e.g. hash-named `$defs`
  described as "readable"). Fix: quote the user's policy verbatim in briefs and check the
  hand-back against it, not against the reviewer's summary.
- **Scope drift.** Symptom: builders keep producing speculative work after the user narrowed
  the goal. Fix: checkpoint in-flight builders immediately and hand off.
- **Registry and evidence formats.** Symptom: a collector script crashes. Cause: a
  tab-separated registry gained a column. Fix: keep machine-read files in their exact shape.
- **Quota-killed agents.** Symptom: a builder stops mid-turn. Fix: resume the same agent
  with a "checkpoint now" instruction; its context is intact. Start fresh only for a new
  task or a very large transcript.
