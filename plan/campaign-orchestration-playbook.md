# Campaign orchestration playbook

How the schema-bug-hunt campaign was run once it started landing work reliably
(round 8, 2026-09-26/27: two landings with twelve components in ~14 hours of
wall-clock, most of it unattended). Read this first when resuming; then the
hand-off, then the ledger's last entries.

## Where the state lives

| What | Where |
|---|---|
| Resume point, open work, final status | `plan/schema-bug-hunt-v1-round8-handoff.md` (§7 = status) |
| Interpreter rearchitecture (current campaign focus) | `plan/value-algebra-interpreter.md`, `plan/value-algebra-r0.md`, ADR 0069, ADR 0066 clause 2b |
| Chronological ledger (every decision, result, timing) | `plan/schema-bug-hunt-v1-progress.md` — append before the trailing line starting `Next:` |
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
- **Reviewers and designers: Codex via agentmux** (astra = `gpt-6-astra`, sol = `gpt-6.1-sol` — never `gpt-6-sol`;
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

- **Landing ratchet (user, 2026-10-20; ADR 0066, supersedes "inherited defects block landings").** A candidate lands when it adds
  no new failure vs main, strictly reduces known failures (test-infra landings may only record what they make visible), and adds no net
  production code unless reviewers justify the growth by the defect class it closes. Every unfixed counterexample becomes an exact
  obligation (frozen witness row, roster row where allowed, or an entry of the battery's shrink-only inherited-failure list). Builders
  hand back at the next landable improvement and carry a stopping rule; land small slices often.
- **Models (latest, user 2026-10-23; supersedes 2026-10-19 and 2026-10-21):** builders are a mix of Codex `gpt-6.1-sol`
  (workspace_write clones, `round8/codex-builder-sandbox.md`) and Opus subagents; Codex sol reviews everything (high routine,
  xhigh semantic core); `gpt-6-astra` only for the hardest Helm/Go-semantics questions. Re-review with `follow_up`. The orchestrator
  does every git branch/worktree operation for Opus builders (they hang on approval prompts).
- Design reviews of semantic mechanisms go to two reviewers in parallel with distinct lenses (architecture, Helm/Go
  semantics). Two sol reviewers with different lenses are usually as good as sol + astra and much cheaper (user 2026-10-24);
  astra does the hardest design/semantics work itself (e.g. the R3b design), Fable only in the rarest cases. Routine
  pre-landing reviews go to one sol reviewer; re-reviews are follow_ups on the same run. Merge findings before resuming the builder.
- **Parallel by default (user, 2026-10-24; the machine sat idle most of a day).** Keep up to 3 concurrent builders (mixed Codex sol
  workspace_write clones and Opus subagents; the orchestrator creates every branch/clone) plus 1 landing chain, within the
  memory/disk floors. Overlap every landing chain (~2 h) with building the next stages: stack the next stage on the current
  stage's reviewed checkpoint commit instead of waiting for it to land, and land test-infrastructure pieces early as their own
  landing. Design work (no cargo) runs on top. Never block on one agent; the heartbeat and completion notifications drive the
  orchestrator. Codex is effectively unlimited; the Claude quota resets Mondays 10:00. Status updates: 2–3 lines on state changes.
- Test harnesses and tools never write bulk scratch to the system temp folder on the small
  internal disk: temp roots live under the target volume (`$TARGET/tmp`, `GOTMPDIR`) and stale
  ones are swept at start. Killed battery runs once left 70 GB in `/var/folders/.../T`.

## Self-attack before every hand-back (mandatory since round 8, session 4)

Session 4 landed nothing in ~15 hours because every review round and one landing chain found defects the builders could have
found themselves with the reviewers' own methods. A hand-back without a `## Self-attack` section in its `handoff.md` (cells
tried, method, result) goes back to the builder unread. The checklist — a builder runs every item that applies to its change:

1. **Lint document.** `helm lint` validates `CoalesceTables(overrides, values.yaml)` WITHOUT dependency defaults; `helm template`
   has them. For every chart with dependencies and every roster row that enables, disables or deletes a dependency, validate
   the lint document against the new Helm-ready schema with the Rust prober: zero rows rejected that main's schema accepts
   (landing 10 run1 failed its sweep gate on exactly this after 2.5 hours).
2. **Multi-path probes.** The battery probes single paths over defaults and adjudicates with template semantics. For every new
   or changed conditional, build a probe that makes its condition true in a non-default context and vary the constrained path
   (absent, null, false, 0, "", [], {}, wrong type); adjudicate with Helm + kubeconform (the datadog `envDict: []` regression).
3. **Positive claims only.** A rejection claim derived from a complement, a bound or "everything not recognised" is wrong by
   default: express it as a positive pattern and list what stays UNKNOWN (W4 needed five rounds for this).
4. **Evaluator equals encoding.** Wherever Rust code evaluates a guard that is also emitted as schema, table-test evaluator ==
   JSON-Schema evaluation of the emitted guard over every guard kind × {absent, null, wrong type, matching, non-matching}.
5. **Every emitter.** When a flag or provenance gates a claim, enumerate every path that produces the gated thing (helper
   bodies, inline arms, extra splices, locals, helper returns) and test one cell per path; prefer a positive provenance set
   at one place over a negative mark that emitters can bypass (W1's quote claims).
6. **Compose and invert.** For text mechanisms, compose each transform with its inverse and with a second splice
   (`b64enc | b64dec`, `tpl (trunc …)`, a lookup key bound to a local).
7. **Registry completeness.** A "name does not exist" claim needs every way the language registers names (define AND block,
   Go-decoded names, whitespace, empty names) or it abstains.
8. **Branch diff.** Diff the previous dump against the new one per changed artifact and classify every `then` change:
   presence condition added / redundant alternative removed / NEW restriction — each new restriction needs its Helm cell.
9. **Cost.** Time generation main vs candidate on three charts (one bitnami); never run real `helm lint` over the whole roster
   (openebs takes ~11 minutes per lint) — use the prober for breadth and real Helm for named witnesses.

10. **Simpler, not bigger (user, 2026-10-18).** A fix must not be a hotfix patch or a per-cell special case. Fix the root cause
   in the shared representation so the code gets simpler and more correct together: name the representation, pass, flag or
   special case the fix DELETES or merges, report the net production LOC delta (`task tokei:core`) and justify any growth by the
   class of defects it closes. A fix that adds an exception per counterexample, a parallel representation or a new flag beside
   an existing one goes back. Reviewers are asked to judge this explicitly (VERDICT may be REWORK on architecture alone).

The orchestrator repeats this list in every builder brief and asks the reviewers to check the self-attack section first.

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
- **`git add -A` in a runner-managed clone** swept 157 stale fixtures a dump had left behind
  into a commit. Stage only the files you edited.
- **Registry files.** `codex-runs.tsv` must stay exactly three tab-separated
  columns or the collector crashes.
- **Background agents and approvals.** An Opus builder's `git checkout` waited hours for an approval nobody saw; prepare branches
  yourself and forbid branch commands in briefs.
- **Zero-byte files.** The runner refuses any zero-byte file new since the baseline (truncation guard); never commit empty
  placeholder files (give them a comment line).
- **Scope drift.** When the user narrows the goal ("land what is ready, no new
  work"), checkpoint in-flight builders immediately instead of letting them finish
  speculative work.

## Resuming from scratch (checklist for a new agent)

1. Read this file, then hand-off §7 (it ends with the resume prompt), then the last ~10 ledger entries.
2. `git -C /Volumes/T7/dev/helm-schema status --short` and `git log --oneline -5`.
3. `python3 /Volumes/T7/dev/round8/collect-codex.py` (pending reviews).
4. `ls -t /Volumes/T7/dev/round8-*-evidence/handoff.md | head` to see recent tracks.
5. Check the lock: `LANDING_ENV=…/landing-2.env /Volumes/T7/dev/round8/runner/run-landing.sh status`.
6. Pick the next landing from hand-off §3/§6; decide the roster-baseline question
   before the first semantic landing.
7. Start the heartbeat and brief at most a few builders plus Codex reviews.
