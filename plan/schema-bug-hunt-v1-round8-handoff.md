# schema-bug-hunt-v1 — round 8 hand-off (2026-09-26)

> **Start here:** `plan/campaign-orchestration-playbook.md` explains how the campaign is run (roles, landing loop, standing decisions, mistakes to avoid, resume checklist). The builder protocol is `plan/schema-bug-hunt-protocol-v2.md`.

Written at the end of the round-8 orchestration session. It is the resume point for the next
session. Everything below is referenced by absolute path on this machine; nothing was pushed.

## 1. Where things stand

- Ledger (source of truth, chronological): `plan/schema-bug-hunt-v1-progress.md` — read the
  round-8 section (from "Round 8 — pre-registration") and the 2026-09-26 entries last.
- Protocol for builders/reviewers: `/Volumes/T7/dev/round8/PROTOCOL.md` (v2; v1 kept beside it).
- Landing runner (fail-closed, v3): `/Volumes/T7/dev/round8/runner/` (its own git repo, branch
  `runner-v3-fail-closed`, HEAD 3564c94; README.md documents the chain, receipt, dump modes,
  sweep gate and lock). Suite: `cd /Volumes/T7/dev/round8/runner && zsh tests/run-all.sh` (9 min).
- Strict verified closures: **3/83** (offline-verified; the live oracle check is part of the
  unstarted gate round 3). F73/F80 are CLOSED-BY-POLICY (reported separately, not counted).
- Heavy lock: `/Volumes/T7/dev/round8/heavy.lock` (legacy owner file `d3f23-f 02:35 sweep`
  belongs to the old F23 lint sweep in `round8-d3f23-evidence/lint-final7/`; remove the lock
  directory by hand only after that sweep's `one.sh` processes are gone; runner v3 never touches
  legacy owner files).
- Codex (agentmux) reports: `/Volumes/T7/dev/round8/*.md`, registry `round8/codex-runs.tsv`,
  collector `python3 /Volumes/T7/dev/round8/collect-codex.py` (reads each run's
  `turns/0000/last-message.md`; run it first in a new session — a run may still be pending).

## 2. Landing sequence (user decision, narrowed 2026-09-26 16:00: land what is ready, no new work)

### Landing 1 — F23 (+D3a 4f, witness fix, kube-version v1, restored CRD) + okteto roster + Helm pool
- Candidate: `/Volumes/T7/dev/round8-d3f23-evidence/candidate-4f.patch` (= `git diff f7be7ba5`,
  sha256 prefix f1c12aca, 137 files). Frozen; battery-final-7 (`battery-final-7.log`) failed only
  on okteto's pre-existing roster debt; fixture freshness 6/6 (`fixture-freshness-final11.log`);
  pre-landing review `round8/review-f23-prelanding-sol.md`.
- Okteto roster patch: `/Volumes/T7/dev/round8-okteto-evidence/final.patch` (sha256 5ad56cc5…,
  2 test files, 0 production lines; applies on the candidate). All 62 run-3 cells registered one
  at a time; okteto slice exit 0 (`battery-okteto.log`, 9,120 s), graylog slice exit 0
  (`battery-graylog.log`); `handoff.md`, `cells.tsv`, `datree-route-lookup.txt` (why the six
  OpenShift Route cells are UNDECIDED, not false acceptances).
- Helm pool + invocation cache (test-only accelerator, battery 2 h 53 m → 55 min): v1
  `/Volumes/T7/dev/round8-pool-evidence/final.patch` (BASE a8668313 = main + candidate) reviewed
  REWORK by sol (`round8/review-pool-prelanding-sol.md`: tpl-assembled actions, worker-panic
  deadlock, result.json integrity, executable identity; policy: `lookup` cacheable only under
  client-only `helm template`, random/clock functions never replayed). v2 = the four fixes +
  policy + rebase onto candidate + okteto: `round8-pool-evidence/final-v2.patch`,
  `handoff-v2.md`, parity battery `full-battery-v2/` (see §6 for status at hand-off time).
- Landing clone: `/Volumes/T7/dev/round8-d3f23-e` (main at f7be7ba5). Env:
  `/Volumes/T7/dev/round8/runner/landing-f23.env` (DUMP_MODE=legacy, kube overrides okteto and
  jupyterhub 1.33.0, SWEEP_JOBS=6, `ACCEPTED_SWEEP_UNRESOLVED='cert-manager=unresolved:loader'`,
  accepted lint residual `control.rs:604 too_many_lines`). Target dir `round7-d3f23/target`.
- Chain (each step takes the lock; exit 75 = lock held):
  ```sh
  cd /Volumes/T7/dev/round8-d3f23-e && git checkout -b landing-f23 f7be7ba5 \
    && git apply /Volumes/T7/dev/round8-d3f23-evidence/candidate-4f.patch && git add -A && git commit -qm "F23 candidate-4f" \
    && git apply /Volumes/T7/dev/round8-okteto-evidence/final.patch && git add -A && git commit -qm "okteto roster" \
    && git apply /Volumes/T7/dev/round8-pool-evidence/final-v2.patch && git add -A && git commit -qm "helm pool v2"
  export LANDING_ENV=/Volumes/T7/dev/round8/runner/landing-f23.env; R=/Volumes/T7/dev/round8/runner/run-landing.sh
  $R dump && $R unit && $R lint && $R battery && $R integration && $R sweep && $R finalize
  ```
  Then adopt the fixtures the receipt lists (`$E/adopted.tsv`), fast-forward `main` in
  `/Volumes/T7/dev/helm-schema` to the landing commits (never push), and record it in the ledger.
- Old lint sweep of the same candidate (legacy, not a v3 receipt, useful as evidence):
  `round8-d3f23-evidence/lint-final7/results-final11.txt` (156 charts; new-lint-failures gate =
  rows with base lint=0 and cand lint=1).

### Landing 2 — test infrastructure, rebased on the new main (all REVIEWED and ready, DUMP_MODE=legacy)
1. Coalescer (exact Helm value composition, three documents): `round8-coalesce-evidence/final.patch`
   (BASE 61f6db73 = main 97341df2 + candidate; sha 5051d717…, 78 files), `handoff.md`,
   `handoff-long.md`; reviews `review-coalesce-checkpoint1-sol.md`, `review-coalesce-final-sol.md`
   (all three items fixed; `task lint` not re-run after the rework). Gate-clone call sites to
   change are listed in its handoff (`family_witnesses.rs:229`, `common/family_witnesses.rs:267`, `:480`).
2. Corpus producer: `round8-producer-evidence/final.patch` (BASE 286c08a2 = main; sha 77bded4e…),
   `handoff.md`; review `review-producer-prelanding-astra.md` (all P1/P2 fixed; every gate green;
   202/202 fixtures byte-identical). The runner's producer dump mode reimplements the OLD
   manifest digests (`manifest.rs@a2cf765a`) and must be extended for the new build-provenance
   manifest (`--verify` after the producer and before integration) — until then use
   `DUMP_MODE=legacy`; the producer's runner diffs are in `round8-producer-evidence/runner/`.
3. Stable `$defs` naming: `round8-naming-evidence/final-code-only.patch` (+ hygiene commit
   6e2e85f4); reviews `review-naming-impl-astra.md`, `review-naming-turn2-astra.md`; design
   `design-defs-naming-astra.md`.
4. Perf1 (compiled-validator memo): `round8-perf1-evidence` (42df7e85 → 52695528, sha 25af56df…);
   review `review-perf1-prelanding-sol.md` (rework accepted: external `$ref` bypass).
5. Runner v3 itself is already in place (`round8/runner`, evidence `round8-runner-evidence/`:
   `handoff.md`, `green/run-all.txt`, `red/summary.txt`, `helm-matrix/`).
Not ready (needs work before it can join): gate round 3 (`round8-gate-evidence/final.patch` +
`review-gate-round2-sol.md`: live oracle check, F74 size-only, digest coverage, six null-override
rows; NOT started), kube-version v2/v3 (`round8-kubever-evidence`, `review-kubever-prelanding-sol.md`:
gates only, drop the stray mongodbcommunity hunk), regexp (`round8-regexp-evidence/final.patch`,
`review-regexp-prelanding-sol.md`: Go gate mandatory, panics → eyre).

## 3. Semantic candidates (none landable today; each dir has handoff.md = resume point)
- F1 global + policy options (`--open-root`, `--declared-types`): `round8-f1-evidence/checkpoint.patch`
  (BASE 1c449bd9 → d5b9bf4c, clone `round8-f1`); brief `brief-f1-global-and-policy-options-astra.md`,
  cross-check `crosscheck-f1-brief-sol.md`. F1 done; `fixture_verdicts` red (5 rows); "reads
  members of `global`" still over-approximate; lint/corpus not run.
- W1 membership/emptiness (F2/F45/F66): `round8-w1-evidence/final.patch` (rework DONE, green,
  needs Codex re-review: `review-w1-prelanding-astra.md` was the REWORK) and `checkpoint-f2.patch`
  (F2 present-null, RED, 9 tests); designs `design-f2-present-null-haskey-sol.md`,
  `crosscheck-f2-haskey-astra.md`.
- W4 comparison operands (F34/F65/F75): `round8-w4-evidence` (BASE 822bbb35 → 99c6824c; overlay
  guard rework done); F75 DONE but unreviewed: `final-f75.patch` (8 files, +240 net LOC, over
  budget; one regression vs the rework HEAD on helper-rendered `tpl` in plain slots; see ledger 17:25);
  `review-w4-prelanding-sol.md`, `design-f75-tpl-typed-output-astra.md`, `crosscheck-f75-tpl-sol.md`,
  `briefs-w4-astra.md` (W4a/W4b).
- B6 stack (F4 + MLS r5 + B6/L1 + L2 + n4): `round8-stack-evidence/final-stack.patch` (31 files
  on d9e75b1c), `handoff.md`, matrices; BLOCKED: 7 cells worse than base (agent-container
  defect). Fix brief: `brief-agent-container-defect-astra.md` (repair truthiness at the
  control-flow join; `joined_truthy_reduction_arms` sole owner). MLS round-6 rework brief:
  `brief-mls-round6-rework-sol.md` (3 P1s from `review-mls-round5-astra.md`). Components:
  `round8-f4-evidence`, `round8-mls-evidence`, `round8-b6-land-evidence`, `round8-l2-evidence`,
  `round8-n4-evidence`, `plan-b6-stack-finishing-sol.md`.
- F6 range accumulation: `round8-f6-evidence`, `round8-f6-land-evidence`, `design-f6-range-accumulation-sol.md`,
  `crosscheck-f6-list-astra.md`, `review-f6-final-sol.md` — blocked behind F9.
- F9: `round8-f9-evidence`, `round8-f9-land-evidence` (re-prep needed on the new main).
- F13: `round8-f13-evidence`, `plan-f13-corrections-sol.md`. F31: `round8-f31-*-evidence`.
  F69: `round8-f69-evidence`, `round8-f69x-evidence`. F5: `round8-f5-evidence` (transport union
  kept; four `-f` false acceptances documented).
- k8s provider D1–D3 (cache-dependence): NOT started; spec `spec-k8s-provider-red-tests-astra.md`,
  review `review-k8s-provider-astra.md`, clone `round8-k8s` (BASE 22813ad1), `round8-k8s-evidence/handoff.md`.
- Frontend rendered-layout refactor (replaces D5): brief `brief-frontend-rendered-layout-astra.md`
  (commits 1.1–4.3, witness ledger A1–A15), audit `audit-line-heuristics-astra.md`, design
  `design-d5-rendered-layout-sol.md`; clone `round8-frontend`, target `round8-frontend-target`.
  Phase 1 DONE, unreviewed: `round8-frontend-evidence/final.patch` (sha bf518fea…, HEAD 83927a5b,
  five commits 1.1–1.5, 0 fixtures moved, +101 LOC), `handoff.md`; conflicts with F9 and W4;
  one open phase-2/3 finding (ledger 18:25). Next: Codex review, then the runner chain. D5 itself is superseded (`round8-d5-evidence`, `round8-d5-land-evidence`).
- Go validator helper for the battery: DONE, unreviewed — `round8-govalidate-evidence/final.patch`
  (sha ca58d80d…, on candidate + coalescer), `handoff.md`; design `design-go-validator-helper-sol.md`.
  Two open items before it can land: (a) NEW DEFECT — the emitted airflow and oncall schemas do not
  compile in Helm (`\u` escapes in URL `pattern`; Go regexp rejects them) → emitter fix + test;
  (b) Helm verdicts are ~20× slower than the Rust screen, so running Go on every probe is not
  affordable — decide sampling/differential use (ledger 17:45).

## 4. Queued work with briefs already written (post-landing)
`brief-corpus-hygiene-sol.md` (after the producer lands), `brief-cli-dx-corrections-sol.md`
(after naming), `brief-timing-and-benchmarks-sol.md`, `design-changed-chart-selection-astra.md`
(after the pool), `plan-allocation-batches-astra.md`, `brief-ir-refactor-step1-astra.md`,
`brief-gen-refactor-d4-sol.md` (post-campaign), `design-incomplete-analysis-result-astra.md`
(exit 3 default; deferred past the campaign by the user), `design-kube-version-policy-astra.md`,
`design-gate-time-budget-{astra,sol}.md`, `audit-determinism-sol.md`, `plan-landing-order-astra.md`.

## 5. Standing decisions (user) and hazards
- Strict authoring default stays; F1 is a correctness bug; F73/F80 policy-exception rows;
  four-builder cap waived for perf work; incomplete-analysis result deferred; Codex used heavily.
- Every landing: runner v3 chain, adjudicated flips, red-then-green test per fix, byte-identical
  fixtures for refactors, ledger entry before the marker line, never push.
- Hazards: shared target dirs serve stale crates after switching trees (touch `crates/**/src/*.rs`
  before trusting a build); `date` in the orchestrator shell may print UTC; mixing dump batches
  from different binaries; the Claude work account weekly limit (reset Oct 2, 01:00) kills agents
  mid-turn — resume them with SendMessage, their context survives.

## 6. Status (rewritten 2026-09-28 23:45 local, session 3 — resume here)

Read the ledger entries from "22:15 (Sep 27) — Session 2" onward for the detail; this section is the map.

- **main code = cb42c017** (landing 6 = frontend phase 1 merged 22:52; before it landing 5 d42a1f33, landing 4
  4b5db9c3, `fix(test)` 9716e080; lint + unit green on cb42c017; plan-only commits since). Nothing pushed. Strict
  verified closures 3/83 (landing 6 closes none of the 83; W1 → 6, W4/F75 → 9 are the next movers).
- **Landing 6 LANDED** as cb42c017 (receipt `/Volumes/T7/dev/round8-landing6-run1/receipt.json` sha256 68285d51…,
  v6.6, own target, 0 fixtures). Hazards learned today: `mise trust` every fresh clone; one cargo target per clone
  AND per worktree (cargo aliases artifacts between checkouts with the same relative layout).
- **Concurrency (user 22:50/22:55: optimise landings per token):** keep ~3 compile-heavy builders; heavy set now =
  W1 rework 6, W4 rework 5, k8s rework 3. PAUSED with checkpoints: F1 (`round8-f1` 0b443ab0 rebased onto 9716e080,
  `rebase/state.md` has the exact next command) and frontend phase 2 (plan `round8-frontend-evidence/phase2/plan.md`
  under Codex cross-check; worktree `round8-frontend-phase2` exists, no code). Resume them when a heavy slot frees.
- **F10** (`frontend-main-f10` 987bb4f6, one fixture) and **partial-kind** (98cf9a8b, superseded by phase 2/3.2 if the
  plan lands) are parked side branches.
- **Landing 4 LANDED** as 4b5db9c3 (receipt `/Volumes/T7/dev/round8-landing4-run1/receipt.json` sha256
  76f4a5aa…, v6.6, 0 fixtures adopted) after a `cargo clean -p` of all workspace crates: landing-4.env and
  landing-5.env shared `TARGET=/Volumes/T7/dev/round7-integrate/target` and cargo had aliased landing 5's
  artifacts (ledger 14:13). Rule from now on: every landing gets its own `TARGET=/Volumes/T7/dev/round8-landing<N>-target`;
  runner v6.7 (rework 1 in progress) enforces it. Lock busy = exit 75: wait, never touch `round8/heavy.lock`.
- **Landing 5 (wrappers) LANDED** as d42a1f33 (receipt `/Volumes/T7/dev/round8-landing5-run1/receipt.json`
  sha256 7b379a7e…; 0 fixtures adopted).
- **Runner = v6.7 checked out** (`/Volumes/T7/dev/round8/runner` 9e3f47b: merge 2309bc3 of runner-v6.7 423a6148, LAND
  from both after 3 rework rounds; landing-4/5/6 env files `LANDING_FINISHED`). Every new landing: own
  `TARGET=/Volumes/T7/dev/round8-landing<N>-target`, owner marker, receipt /9, env grammar (`NAME=VALUE`, quoted
  values, ASCII only). **v6.8** (readable `helm-schema lint` diagnostics; branch runner-v6.8 rework 2 = 2e968a72)
  under review; merge between chains on LAND. Disk: 163 GB free
  at 16:11 after removing the dormant build caches round7-f13/target and round7-d3f23/target (finished tracks).
- **Candidates and their state** (evidence dirs hold `handoff.md` + `final*.patch`; reviews in
  `/Volumes/T7/dev/round8/review-<track>-<round>-{sol,astra}.md`; briefs `brief-<track>-rework<n>.md`):
  - W1 (`round8-w1` `track/w1-main`, rework 6 in progress off fdb4f8c3, `brief-w1-rework6.md`: argo-workflows
    guard loss, pluck/dig/root-constructed mutation escapes, helper-argument mutations, stale empty dict in
    merge, concat nonemptiness, merge operand requirement, rebuilt corpus probe; R1 abstention accepted, R7
    withdrawn); side branch `track/w1-unset` parked (worktree `round8-w1-unset`); 51 corpus charts drift
    (battery adjudicates); first fixture-moving landing → needs the roster-baseline confirmation
    (recommendation: keep f7be7ba5) and a rebase onto main.
  - Frontend phase 1: LAND from both on e584c2d3 (rework 4) → landing 6 on the chain (above). Side work in
    worktrees: `round8-frontend-f10` (F10) and `round8-frontend-else` (inline `{{else}}` false rejection, WIP).
  - W4/F75 (`round8-w4` `w4-main`, rework 4 in progress off f493bb14, `brief-w4-rework4.md`: anchor token and
    payload modelled separately, provider patterns applied to the payload via parsed regex structure, empty
    intersection kept after guard union); R5/R7 confirmed; the 153-chart blast radius is endorsed by both as the
    right scope (+2–4 % schema size); BASE still c816e318 — rebase needed (worktree `round8-w4-rebase`, stale).
  - k8s D1–D3 (`round8-k8s` `k8s-main`, rework 2 in progress off c42bfa6a, `brief-k8s-rework2.md`: DECISION (b)
    conditional provider contracts for values-driven KubeVersion guards, whole-condition `Has` decoding,
    Masterminds `||` semantics, prerelease-preserving version, K4 missing-else, comparator red evidence);
    BASE c816e318 — rebase needed (`k8s-rebase` in a separate worktree). D2/D3 endorsed by both.
- **Builder agents** (native subagents of this session; resume with SendMessage): W1 `ae88c76cb6bda5467`,
  frontend `a69f98e6d3af1e500`, W4 `a8dd06d301e7c2760`, k8s `a1ff77c400aeaa4df`, runner `a67c3c0781e99d708`
  (v6.7/v6.8), F1 `a1fb0ea167e8d1a85` (started 22:25 from the checkpoint, `brief-f1-resume.md`); done: wrappers
  `ac59ade2d4da19ac0`, scratch `a159f489c3a3fd5c0`. If the session itself is gone, start fresh builders from each track's `handoff.md` +
  brief.
- **Codex runs** (follow_up keeps context; runs expire after ~24 h — the first k8s astra run did): W1
  92580cac/416b8635, frontend 7660883f/5d5c270b, scratch be7d3a37/908ce142, runner 73d3081f/6b442c0c,
  wrappers 7a44129c/b7481bb6, W4/F75 20260928T034515-68054652/-aeb245af, k8s 20260928T133519-3677d240 (sol) /
  20260928T133601-e1cee4f7 (astra). Answers are read from `turns/<latest>/last-message.md` (the transcript
  file grows only after a `result` call); `collect-codex.py` saves and registers them (`codex-runs.tsv`).
- **User decisions**: personal Claude account only, Claude via native subagents, agentmux for Codex only;
  keep ~3 compile-heavy implementors running (cap, 22:50 Sep 28: more compete for the machine; queue new heavy
  work on hand-back when ≥3 are active; reviews/plans are free); spend the weekly quota by its reset and resume after
  (reset happened 10:00 local Sep 28); roster baseline: DECIDED 22:40 Sep 28 (orchestrator under the user's delegation): keep f7be7ba5 for the
  whole campaign; fixture-moving landings may proceed.
- **Process improvements (user mandate 23:50 Sep 28: chase these as first-class work; owner in brackets):**
  1. Shared cell-matrix tool in `helm-schema-test-support`: a TSV of (chart, override JSON, kube-version) → one row per
     cell with verdicts from N CLI binaries + Helm 4.2.3 render + kubeconform strict, emitted as the `matrix-*.txt`
     files builders and reviewers hand-write today [runner builder: proposal; then a builder slot].
  2. Builders run the schema-only round-74 battery (baseline = their base commit) over their moved fixtures before
     hand-back instead of ad-hoc Python probes (W1's probe2.py coalesced wrongly) [document in corpus-fixtures skill].
  3. Adjacent-spelling cell bank per feature class (anchors, comments, flow indicators, quoting, trims, mid-line
     boundaries) as reusable test fixtures, run before hand-back — most REWORK rounds today found one adjacent cell
     [W4/frontend builders after their tracks land].
  4. Target-dir owner guard for builders: generalise the runner's `.landing-owner` marker into a `task` pre-check that
     refuses to build when the target was last built from another checkout path [runner builder: proposal].
  5. Family-witness gate: size checks must apply main's `$defs` shortening (F1 builder found 5 spurious F74 rows)
     [F1 builder, in its track].
  6. Codex re-checks of a single item at `high` effort instead of `xhigh` [orchestrator, immediate].
  7. Every fresh clone: `mise trust`; every worktree: own target — both now in memory and the runner [done].
  8. helm-schema feature: `helm-schema explain <chart> <values-path>` — print the evidence behind a path's emitted
     schema (source rows with file:line, guards/arm predicates, provider documents and pointers, overlay ownership,
     abstention reasons). Reviewers and builders currently trace this by reading source; a typed provenance dump
     from the IR/gen phases (no new heuristics, just exposing the facts the compiler already has) would shorten
     every adjudication round [design cross-check first; builder slot after W1/W4].
  User rule (23:55 Sep 28): do not accept a bad process, bad output, or a genuinely useful missing feature that
  slows us down when it could just be implemented.
- **Follow-ups queued**: sol's scratch P2 (nested `request` paths in copied records); wrappers P3s (scratch
  path in Helm messages, raw error formatting, Windows runtime test); the airflow/oncall `\u` URL pattern
  emitter defect; `if $d`/`empty $d` after `unset` (pre-existing); `required (dict)`/`(list)` false
  rejection on main (hole_effects.rs:150); k8s items outside the spec (forward-incompatible layout,
  cache-write failure, inference trusting a partial inventory, CRD online probe repeats without the memo);
  the runner's readable-log step should call `helm-schema lint` now that expand-defs is gone.
