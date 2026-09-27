---
name: landing-workflow
description: How multi-round analyzer/generator work in helm-schema is planned, executed, recorded and landed. Use when starting or resuming a campaign round, finding "what's next", reading a hand-off or the plan/ directory, writing a progress-ledger entry or a hand-off, briefing builders or reviewers, assembling a landing, running or restarting the landing chain (dump → unit → lint → battery → integration → sweep → finalize), reading a receipt or adopted.tsv, or merging a landed branch into main.
---

# Landing workflow

helm-schema work above a one-line fix is run as **campaigns**: a frozen plan or findings
document, many rounds or tracks of implementation, and serial **landings** that each pass a
fail-closed gate chain before they reach `main`. The durable record lives in `plan/`. This
skill says where state lives, how to write it down, and what "landed" means.

Sibling skills own the mechanics this one only points at:

- `acceptance-battery`: the round-74 flip battery, flip adjudication, roster baselines.
- `corpus-fixtures`: the corpus producer, fixture lanes, dumps and fixture regeneration.
- `verification-gates`: the CLAUDE.md gate list, task commands, tooling (lint, nextest
  profiles, helmsweep build, luup2, `tokei:core`).

Read `CLAUDE.md` first; its gates and git rules outrank anything here.

## 1. Orient in five minutes

1. Find the active campaign with git, not mtimes (a checkout gives every file one mtime):
   `git log --format='%h %ad %s' --date=short -20 -- plan/`
   The newest `chore(plan): record …` commits name the ledger being written.
2. If a playbook exists for that campaign (`plan/*-playbook.md`), read it first. It names
   the hand-off, the ledger, the protocol and the resume checklist.
3. Read the hand-off's **status section** (for example "§6 Status at hand-off time").
   It is the resume point: what landed, what is ready, what is blocked, standing decisions.
4. Read the last ~10 entries of the progress ledger (`tail -150 plan/<campaign>-progress.md`).
5. `git status --short` and `git log --oneline -8`. Landings appear as `--no-ff` merges whose
   message names the components and the receipt sha.
6. Cross-check before acting: the ledger's trailing `Next:` line and hand-off paths can be
   stale (a `Next:` line may still say "resume X" after X has landed). Trust
   `git log` and the newest dated entry over any "next" pointer.

Evidence (patches, logs, matrices, receipts, per-track `handoff.md`) lives **outside** the
repo in the orchestrator's workspace; the ledger records its paths. Those paths are
machine-specific. If one does not resolve, say so and ask; never reconstruct evidence.

## 2. What lives in plan/

| Kind | Naming | Rule |
|---|---|---|
| Frozen plan / findings | `<campaign>.md` (e.g. `performance-review-v1.md`, `schema-bug-hunt-v1.md`) | Never edited after freezing; check with `git diff --exit-code <freeze-sha> -- plan/<campaign>.md`. Corrections go in the ledger. |
| Progress ledger | `<campaign>-progress.md` | The campaign's only prose surface. Append-only history, plus a decision register at the top. |
| Hand-off | `*-handoff.md` | Resume point for the next session; status section is rewritten, not appended. |
| Playbook / protocol | `*-playbook.md`, `*-protocol-v<N>.md` | How the campaign is run: roles, landing loop, builder rules. |
| Reviews / designs | `*-review*.md`, `architecture-review-v<N>*.md` | Review outcomes and the plans they produce. |
| Post-mortems | `*-postmortem.md`, `*-rollback.md` | Why an approach failed; read before retrying it. |
| Bug-hunt reports | `schema-bug-hunt-reports/report-*.md` | Raw per-batch findings (class, status, schema says / template says / why). |
| Scripts | `*-scripts/` | Survey and measurement scripts referenced by the documents. |

Older campaigns are closed; their close-out sections ("Campaign handoff", "Residual
deliberately unscheduled debt") are still the best source for why something was deferred.

## 3. Writing ledger entries

Two formats are in use; match the ledger you are appending to. Templates and a filled
example of each: `references/ledger-and-handoff-formats.md`.

- **Round sections** (frozen-plan campaigns): `## Round <id> — <title>` with Status (landed
  in `<sha>`), Contract, Acceptance baseline, Baseline LOC, Pre-registered expectations,
  Measured results, Deviations, Adjudication evidence, then `###` Producer and route
  coverage, Review dossier, Self-adversarial pass, Gates, and the measured LOC delta.
  Pre-register expectations **before** measuring; record deviations instead of editing them.
- **Timestamped entries** (orchestrated landing campaigns): `- HH:MM (Mon DD) — **headline
  with the verdict.** body`, inserted **before** the ledger's trailing `Next:` line, then
  update that `Next:` line so it is true.

Rules for both:

- Every claim carries its evidence: the command, its own exit code, counts, log path.
  A gate that was not run is written "not run", never folded into "all green".
- Record decisions with who made them ("user decision", "orchestrator"), and failures and
  mistakes as plainly as successes. Corrections are new entries that point back; do not
  rewrite history (a mistaken hash can be fixed in place, as a separate commit).
- End a working checkpoint with ``Next: <action>; first run `<exact command>`.``
- Re-read the file from disk immediately before editing: another agent or vendor session
  may be writing the same ledger. Reconcile, keep foreign entries, renumber your round if
  its number was taken.
- Write ledger text with the Edit tool or a **quoted** heredoc (`<<'EOF'`); an unquoted one
  expands backticks and `$defs`.
- Commit ledger updates separately from code, as `chore(plan): record <what>` (new documents:
  `docs(plan): …`), and only under the CLAUDE.md commit policy.

## 4. Writing hand-offs

- **Track hand-off** (`handoff.md` in a track's evidence dir): current state only, under
  ~60 lines, never an appended history. Slot/clone, branch, base and HEAD shas, target dir,
  `final.patch` path and sha256, mechanism with `file:line`, Helm matrix and review links,
  red/green tests, one line per gate with its exit, fixture and witness status, the exact
  blocker, and the exact next command.
- **Campaign hand-off** (`plan/*-handoff.md`): where things stand; landing sequence with
  candidates and their evidence; candidates not ready and why; queued work with briefs;
  standing decisions and hazards; a final dated **status** section rewritten at session end.
  Put a "Start here" pointer to the playbook at the top.
- Builder hand-back message (≤ 60 lines): contract (what Helm does + matrix file),
  mechanism (structural fact, files, representation deleted), tests added, gates one per
  line with exit and counts, fixtures moved/adjudicated with every unmatched flip, patch and
  hand-off paths, anything unfinished with its witness and blocker. Point at files; no diffs.

## 5. The landing model

Roles: one **orchestrator** owns `main`, the ledger, hand-offs, landings and runner env
files; **builders** each own one track in their own clone with an exclusive
`CARGO_TARGET_DIR`; **reviewers** are independent and read-only. Builders never touch
`plan/`, `main`, the runner, its lock, or another track's files.

The loop:

1. Builder hands back `final.patch` (one committed base..HEAD diff), `handoff.md`, gate exits.
2. Pre-landing review → REWORK items → the same builder reworks → repeat until LAND.
3. Assemble the landing in a fresh clone of `main`: apply patches in order, one commit per
   patch (`git apply --3way`, else fetch the builder's branch); record every conflict
   resolution in `conflicts.md`. Land test infrastructure and oracle fixes first, semantic
   fixes after; land one semantic candidate at a time unless attribution is separable.
4. Write the landing env file (see `references/landing-runner.md`); every acceptance in it
   (residuals, unresolved classes, overrides) carries a comment saying why.
5. Run the chain (§6). On failure read only that step's log, fix the smallest thing,
   restart from the earliest stale step.
6. On "chain green": commit exactly the fixtures listed in `$E/adopted.tsv`, then
   `git merge --no-ff` the landing branch into `main` with a message listing the components
   and the receipt path + sha256. **Never push.**
7. Ledger entry with per-step timings and the receipt sha; update the hand-off status.

Branch creation, `git checkout`, merges and commits follow CLAUDE.md: ask before branch
operations, commit only when authorized, never stage what you did not edit.

## 6. The landing chain

The runner is **not in this repository**. It is a separate git repo in the orchestrator's
workspace (`run-landing.sh`, `landing.py`, `sweep-one.sh`, `landing-*.env`, a README, and
a test suite `tests/run-all.sh`). The in-repo pieces it drives are `tools/helmsweep`
(`task build:helmsweep`; its README documents the sweep contract), the corpus producer
(`corpus_generation`), the round-74 battery test, and the nextest/`task` gates. Interface,
env variables, exit codes and receipt contents: `references/landing-runner.md`.

Each step is its own invocation `LANDING_ENV=<env> <runner>/run-landing.sh <step>`, run in
order, stopping at the first nonzero exit:

| Step | Purpose | Green means |
|---|---|---|
| `dump` | Generate every fixture lane once from one build; freeze the candidate | Only fixture-content mismatches (a panic, timeout, build error, missing/extra artifact aborts); complete non-empty manifest verified; `adopted.tsv` written; tree and producer binary frozen into the receipt |
| `unit` | Default nextest profile | exit 0 |
| `lint` | fmt, `task lint`, `task lint:fc`, ast-grep, dialect-hygiene test | every exit 0, or the sole located diagnostic equals the env's accepted residual (receipt records partial coverage) |
| `battery` | Round-74 flip battery of the dump against `BASELINE` | exactly `1 test run: 1 passed`; no unmatched flip; no stale roster entry |
| `integration` | Producer `--verify` with the frozen binary, then the integration profile | verify passes, every test passes |
| `sweep` | Real-Helm check of the **shipped** schema bytes over every roster row (chart × override) | 0 new `helm lint` failures where the schema-free `helm template` control renders; every unresolved row is an accepted `chart=class`; CLI differential sample agrees 100%; rows == charts × overrides |
| `finalize` | Seal the receipt read-only and print its sha256 | exit 0 |

Restart rules (the runner enforces them; do not argue with a refusal):

- **Any change to generation inputs** (anything outside `**/tests/**`), the env file, or the
  runner itself → fresh evidence dir `E`, restart from `dump`.
- **Test-only change** → the dump stays valid; re-run from the earliest step the runner
  marks stale (it falls back to requiring `dump` if the test change alters what the dump
  generates or an adopted fixture).
- Lock busy (exit 75) → record it and reschedule. Never take, remove or "repair" the lock.
- A receipt proves exactly its recorded clone, HEAD, hashes, baseline, tools and steps —
  nothing about another candidate.

The chain does not replace the CLAUDE.md gates for declaring a round final
(`task test:all`, the luup2 gate, `task tokei:core`); report those separately
(`verification-gates`).

## 7. Standing rules

- **Every fix lands with a minimal red-then-green regression test**, one per fixed
  contract cell: record the red run on the pre-fix tree and the green run, with commands
  and exits. A fixture change is not a test.
- **The shipped schema must pass real `helm lint` on the chart's raw root `values.yaml`
  and `helm template`**, not only the battery's validator.
- Refactors are **byte-identical** on every fixture; any fixture move turns the round into
  a semantic one needing adjudication. Report the production LOC delta (`task tokei:core`).
- Caches are keyed by every input and never an oracle; nondeterministic templates are
  never cached. Every chart and every sweep row runs; sampling applies only to the extra
  real-CLI differential.
- Report closures strictly: verified closures separately from policy closures; never
  claim a family closed from a flip-only battery.
- Never push. Stage only the files you edited — a runner-managed clone contains dump
  leftovers.

## 8. Adjudicating schema changes

A flip's direction is not its verdict. **TIGHTEN is a direction, not a verdict**: a
tightening that rejects values Helm renders is a *false rejection*; a loosening that admits
values Helm aborts on is a *false acceptance*. Adjudicate every flip against real pinned
Helm (pinned Kubernetes version, shipped `values.schema.json` and `templates/tests`
removed, the **coalesced** values document, null-deletion semantics), and distinguish
template abort, schema rejection, pinned Kubernetes schema violation and "undecided".
Known-open cases stay visible; do not retire one because its flip vanished. Before any
semantic mechanism, run a small discriminating Helm matrix (defaults, missing vs null,
transports, kube versions) and record it. Mechanics and roster rules: `acceptance-battery`.

## 9. Review cadence

- A **cross-vendor design review before coding** each semantic mechanism, and a
  **cross-vendor pre-landing review** of every candidate, asking for a LAND/REWORK verdict
  with `file:line` fixes. Brief with exact paths: final diff, Helm matrix, red/green
  evidence, manifest, receipt, fixture diff, open questions. Put the user's policy calls
  in the brief as decided, not as questions.
- Every behavior-bearing step gets an **independent adversarial review** (gate re-runs,
  mechanism review, probes of adjacent states its own tests skipped) before the next
  behavior-bearing step builds on it.
- Do mechanical checks (manifests, stable-name fixture diffs, keyed outcome comparisons)
  yourself; spend reviewers on semantic challenges and counterexamples.
- Anything landed before its review is recorded as **review debt** and queued.
- Model, account and concurrency choices follow CLAUDE.md (the user picks the account).

## 10. Pitfalls

The ones that recur most (full catalogue with symptoms and fixes:
`references/pitfalls.md`):

- **Vacuous battery**: `flips_adjudicated: 0` on a byte-changing round, or "0 tests run".
  Point `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` at the final clean dump and run ignored tests.
- **Broad staging**: `git add -A` in a landing clone swept 157 stale fixtures into a commit.
  Stage named files; check `git diff --stat <base> -- testdata` before committing.
- **Mixed dump batches**: drift that keeps "growing" across runs is a stale binary. One
  clean dump after the final build.
- **Editing the runner while a chain runs from it** broke a live sweep. Change it in a
  worktree; merge between chains.
- **Concurrent ledger edits**: an Edit refused with "file modified" means another session is
  writing `plan/`. Re-read, reconcile, never clobber.
- **Pipe exit codes**: `cmd | tail` reports `tail`'s exit. Check the gate's own exit.
