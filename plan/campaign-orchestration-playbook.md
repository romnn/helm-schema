# Campaign orchestration playbook

How the schema-bug-hunt campaign was run once it started landing work reliably
(round 8, 2026-09-26/27: two landings with twelve components in ~14 hours of
wall-clock, most of it unattended). Read this first when resuming; then the
hand-off, then the ledger's last entries.

## Where the state lives

| What | Where |
|---|---|
| Resume point, open work, final status | `plan/schema-bug-hunt-v1-round8-handoff.md` (§6 = status) |
| Chronological ledger (every decision, result, timing) | `plan/schema-bug-hunt-v1-progress.md` — append before the line starting `Next: resume d3f23 first` |
| Bug-hunt findings (F1…F83) and campaign rules | `plan/schema-bug-hunt-v1.md` (never edit) |
| Builder/reviewer protocol | `plan/schema-bug-hunt-protocol-v2.md` (copy of `/Volumes/T7/dev/round8/PROTOCOL.md`) |
| Landing runner (its own git repo) | `/Volumes/T7/dev/round8/runner/` — `README.md`, `run-landing.sh`, `landing.py`, `landing-*.env`, `tests/run-all.sh` |
| Codex reviews, designs, briefs | `/Volumes/T7/dev/round8/*.md`, registry `round8/codex-runs.tsv`, collector `python3 /Volumes/T7/dev/round8/collect-codex.py` |
| Per-track work | clone `/Volumes/T7/dev/round8-<track>`, evidence `/Volumes/T7/dev/round8-<track>-evidence/` (`handoff.md`, `final.patch`, logs) |
| Landing receipts | `/Volumes/T7/dev/round8-<landing>-run*/receipt.json` + `chain.log` |

## Roles

- **Orchestrator (one session).** Owns main, the ledger, the hand-off, landings,
  merges and the runner env files. Reads hand-backs, decides, briefs. Does small
  fixes itself when briefing would cost more than doing (a one-line test
  expectation, a test registry entry).
- **Builders: Opus subagents, few and focused.** One track per agent, in its own
  clone with an exclusive `CARGO_TARGET_DIR`. Resume the same agent with
  `SendMessage` for rework (its context survives quota kills); start fresh only
  when a transcript is very large or the task changes.
- **Reviewers and designers: Codex via agentmux** (`gpt-6-sol` / `gpt-6-astra`,
  effort `xhigh`, sandbox `read_only`, cwd `/Volumes/T7/dev`). Cheap relative to
  Claude quota and cross-vendor, so they find what same-vendor review misses.
  Use them for pre-landing reviews, designs, landing-order plans, and "why did
  this fail" diagnosis. Every review in round 8 returned REWORK with real defects.

## What changed the throughput

1. **Fewer, focused builders beat wide fan-out.** Early rounds ran ~20 parallel
   agents: huge token use, load of 130 on 11 cores, and work that had to be
   re-derived because landings are serial. Round 8 settled on ≤ ~5 concurrent
   builders, each with a narrow brief and a hand-back format.
2. **Speed up the gate before landing semantics.** The landing chain went from
   13–40 h (sweep) + 3 h (battery) to **55 min** end to end, and every later
   landing got cheaper. Order that paid off: pool the battery's Helm calls →
   row-level sweep workers → an in-process Helm driver on Helm's own modules
   (`tools/helmsweep`, one memo patch applied at build time, CLI differential as
   the safety net) → corpus producer → frozen producer binary.
3. **Group changes that must land together** and land test infrastructure first,
   semantic fixes after, each on the fast chain. When a patch depends on a runner
   change (helmsweep v2 ↔ runner v6.3), fold them into one landing rather than two
   chains.
4. **Codex before every landing.** Brief it with exact paths, the patch, the
   evidence, and explicit questions; ask for a LAND/REWORK verdict with file:line
   fixes. Then resume the builder with the review path and the decisions already
   made (the user's policy calls go in the brief, not back to the reviewer).
5. **Cheap heartbeats.** `ScheduleWakeup` at the pace of what is actually
   running (10 min near a finish line, 30–60 min during long steps), with the
   whole next action written into the wakeup prompt so a tick is one `tail` and
   a reschedule when nothing changed. Hand-backs arrive as notifications; never
   poll agents.
6. **Minimal-token instructions.** Tell builders "minimal tokens, no exploration
   beyond X, report in ≤N lines", name the files they must read, and give them
   the exact gate commands. Tell them what NOT to do (touch the runner, the lock,
   main, fixtures).

## The landing loop

1. Builder hands back: `final.patch` + `handoff.md` + gate exits.
2. Codex pre-landing review → REWORK items → resume the builder → repeat until clean.
3. Assemble the landing in a **fresh clone** from main: apply patches in order,
   one commit per patch (`git apply --3way`; fall back to fetching the builder's
   branch), `conflicts.md` for every resolution.
4. Write `runner/landing-<n>.env` (R, fresh E, TARGET, BASELINE, DUMP_MODE,
   kube overrides, `SWEEP_DIFF_CAP=3`, accepted residual/unresolved lists — each
   acceptance with a comment saying why).
5. Run the chain in the background with a nohup loop that stops at the first
   failing step:
   ```sh
   cd /Volumes/T7/dev/round8/runner && export LANDING_ENV=…/landing-<n>.env
   nohup zsh -c 'for s in dump unit lint battery integration sweep finalize; do ./run-landing.sh $s || exit $?; done; echo chain green' > $E/chain.log 2>&1 &
   ```
6. On failure read only that step's log, fix the smallest thing, re-run from the
   earliest stale step (a test-only change keeps the dump; env or runner changes
   need a fresh E and a restart from dump).
7. On "chain green": commit adopted fixtures, `git merge --no-ff` into main with
   a message listing components and the receipt sha. Never push.
8. Ledger entry with step timings; update hand-off §6.

## Standing decisions (from the user)

- Every chart and every sweep row runs through the fast engine; nothing is
  skipped. The real Helm CLI checks a sample: every 100th row per chart, up to 3
  rows per chart whose fast verdicts differ (`SWEEP_DIFF_CAP=3`), every unresolved
  row. Driver-contract tests guard the configuration class; re-run full parity
  when Helm or the driver changes.
- No vendored Helm sources or submodules: pinned module + one patch applied at
  build time with a digest check.
- Caches are keyed by every input (including the environment); nondeterministic
  templates are never cached; a cache is never an oracle.
- Every fix lands with a red-then-green test; fixture drift is not a test.
- Report closures strictly (verified closures only; policy closures separately).

## Mistakes worth not repeating

- **Editing a runner directory a live chain executes from** broke an 18-chart
  sweep. Build runner changes in a git worktree; merge only between chains.
- **Wrong baseline.** The known-false-acceptance roster is observed as flips
  against `ROSTER_BASELINE` (f7be7ba5); a baseline equal to the candidate made 237
  rows look "fixed". The battery now refuses any other baseline.
- **Shared target dirs.** Other steps rebuild binaries with different feature
  unification; freeze any binary a later step must verify (the producer), and
  touch sources after switching trees in a shared target.
- **Disk.** Killed battery runs leaked 69 GB of temp dirs into the system temp
  folder; the Go linker then failed with ENOSPC. Keep temp dirs under the target
  volume and check `df -h /` when a build fails oddly.
- **Quota kills.** The Claude weekly limit killed five agents mid-turn. Resume
  them with `SendMessage` and a "checkpoint now" instruction; their context is intact.
- **Shell heredocs.** Use quoted heredocs (`<<'EOF'`) for ledger text; an unquoted
  one expanded backticks and `$defs` and mangled an entry.
- **Registry files.** `codex-runs.tsv` must stay exactly three tab-separated
  columns or the collector crashes.
- **Scope drift.** When the user narrows the goal ("land what is ready, no new
  work"), checkpoint in-flight builders immediately instead of letting them finish
  speculative work.

## Resuming from scratch (checklist for a new agent)

1. Read this file, then hand-off §6, then the last ~10 ledger entries.
2. `git -C /Volumes/T7/dev/helm-schema status --short` and `git log --oneline -5`.
3. `python3 /Volumes/T7/dev/round8/collect-codex.py` (pending reviews).
4. `ls -t /Volumes/T7/dev/round8-*-evidence/handoff.md | head` to see recent tracks.
5. Check the lock: `LANDING_ENV=…/landing-2.env /Volumes/T7/dev/round8/runner/run-landing.sh status`.
6. Pick the next landing from hand-off §3/§6; decide the roster-baseline question
   before the first semantic landing.
7. Start the heartbeat and brief at most a few builders plus Codex reviews.
