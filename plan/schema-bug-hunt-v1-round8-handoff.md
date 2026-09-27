# schema-bug-hunt-v1 — round 8 hand-off (2026-09-26)

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

## 6. Status at hand-off time (2026-09-27, 09:30 local) — FINAL

Everything the user queued as "basically ready" is on main. Nothing was pushed.

- **main = 74139f94**: landing 1 (c02c01f8: F23 + okteto/graylog roster + Helm pool v2 + helmsweep)
  and landing 2 (74139f94: corpus producer, perf1, stable  names, Helm value coalescer +
  roster baseline pin, helmsweep v2, battery resident client, two test fixes). Receipts:
  , . Ledger entries 01:25 and 09:30 (Sep 27) hold the step timings.
- **The landing chain now takes ~55 min end to end** (dump 3.5, unit 0.4, lint 1.7, battery 14,
  integration 4, sweep 32 incl. the CLI differential, finalize). Runner = v6.3
  (, branch , HEAD 931672d + env commits;
  , every roster row runs,  by user decision, producer
  dump mode with a frozen producer copy, receipt /6). Template env: .
  Worktrees  can be pruned ( after removing the dirs).
- **Roster baseline coupling (design item):** the known-false-acceptance roster is observed as
  flips against  (f7be7ba5, pinned in ). The first
  SEMANTIC landing that changes production behaviour must keep BASELINE=f7be7ba5 for the battery
  (the pin refuses anything else); when the baseline is advanced, rows that predate it stop being
  observable as flips and need an absolute home (their K8s-rejects evidence is absolute). Decide
  before the first semantic landing.
- **Next landings (semantic; each needs review/rework first, §3):** W1 rework (green, needs a
  Codex re-review), frontend phase 1 (green, 0 fixtures moved, unreviewed; conflicts with F9/W4),
  F75 (over budget, one regression), F1 and F2 checkpoints, k8s D1–D3 (not started), B6 stack
  (blocked on the agent-container fix, brief ).
- **Hygiene follow-ups found while landing:** (1) the battery harness leaks multi-GB temp dirs
  under the system temp folder when killed (69 GB found; fixed for the runner via GOTMPDIR/TMPDIR
  under , not yet in the harness — create them under the target dir and sweep stale ones
  at start); (2) the emitted airflow and oncall schemas do not compile in Helm ( escapes in
  URL , ledger 17:45 Sep 26) — emitter fix + test; (3) the sweep classifiers (Python
   AND Go  must agree) should give "unable to detect chart" a loader
  class and "library charts are not installable" a  class instead of the catch-all
  (today accepted per chart in ); (4) helmsweep's  build step is
  unverified on native Windows; (5) the Go sweep cache rarely hits on this corpus (bitnami
   calls , grafana helpers ) — fine at this speed, but the cacheability list
  could distinguish  (order-only) if the sweep ever needs it; (6) 
  compile time in Helm dominates every lint (openebs 4–10 min): emitting  instead of
   measured 3.5× faster on okteto — a separate output-policy candidate.
- **Review debt:** helmsweep v1 + runner v5/v6 landed on parity evidence before the cross-vendor
  review; the review () was then applied in helmsweep v2 and
  runner v6.2/v6.3. Landing 2's assembly and the runner versions v5.1–v6.3 had no separate Codex
  review; queue one review of the runner () and of the landing-2
  conflict resolutions () when the next session starts.
- Old evidence dirs ,  are history only.
- Codex runs all collected (); no agent running at hand-off.
