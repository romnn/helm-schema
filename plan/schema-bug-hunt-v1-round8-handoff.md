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

## 6. Status (rewritten 2026-10-01 12:30 local, end of session 4 — resume here)

Read the ledger entries from "21:21 (Sep 30) — Session 4 started" onward for the detail; this section is the map. The resume
prompt for session 5 is at the end of this section.

- **Nothing landed in session 4. main code = b51c8589** (landing 9; only plan/ commits since; nothing pushed). **Strict verified
  closures 4/83 = F17, F30, F54, F77**, unchanged. Not run on main for landing 9: `task test:all`, luup2 — still owed.
- **Why:** every review round and the one landing chain found genuine defects. W4 reached LAND ×2 once (rework 13, R58–R67);
  its landing chain (run1) passed dump, unit, lint, battery and integration and FAILED the sweep gate after ~2.8 h: 19 new real
  `helm lint` failures (oauth2-proxy with redis-ha enabled) — `helm lint` validates the values WITHOUT dependency defaults and a
  claim fired on that document. Lesson, now mandatory (playbook section "Self-attack before every hand-back"): **builders
  attack their own patch with the reviewers' methods before every hand-back** (lint-document check over dependency rows,
  multi-path probes in non-default contexts, positive claims only, evaluator == emitted encoding, every emitter,
  compose-and-invert, registry completeness, `then`-branch diff classification, generation-time comparison) and record it in a
  `## Self-attack` section; a hand-back without it is returned unread, and reviewers are asked to check it first.
- **Landing order: first-ready-first between W4 (landing 10) and W1 (landing 12); k8s (landing 11) follows W4** (its 9 failing
  battery cells are W4-dependent). One fixture-moving landing at a time; the later lander rebases onto the earlier one before
  its final dump + battery. Then R46 (perf) as its own landing, then frontend 2.2.
- **Landing 10 (W4/F75) — run1 FAILED at the sweep gate; run2 STAGED, NOT launched; rework 15 CHECKPOINTED.** Clone
  `/Volumes/T7/dev/round8-w4-r9`, branch w4-rebase, HEAD b10d54b3 (67 commits on b51c8589, clean), target
  `round8-w4-r9-target`. History this session: rework 12 (R58–R61, complement by "totality" → refuted) → rework 13 (R62–R67:
  positive missing-callable pattern via a 32-character name trie, Go-decoded define names, positive parse-error claims, block
  names; LAND ×2 at f9da148e) → run1 failure → rework 14 (R68, 5325b7d1: a pattern guard is false where its value is absent;
  REWORK ×2) → rework 15 (`brief-w4-rework15.md`): R69 84193438 numeric guards read an absent value as 0 (openebs
  `engines.replicated.mayastor.enabled: null`), R71 e888412e `Absent`/`TypeIs` read explicit null as the encoding does, R70
  b10d54b3 the declared sample never narrows a branch a range runs in (datadog `traceAgent.envDict: []`; datadog's schema is
  byte-identical to pre-R68 again). Checkpoint state (`round8-w4-evidence/rebase/r15/handoff.md`, 42 lines, exact next
  commands): table test `guard_evaluation_agrees_with_the_emitted_encoding` green (49 mismatches on 5325b7d1; floats like `1.0`
  still disagree for numeric/Eq-int/TypeIs-integer — outside the brief's table, to decide), clean dump `dump-r15` verified,
  lint-document check over dependency rows done (4 rows newly rejected, schema-free Helm aborts on all 4 → 0 false
  rejections), unit 1766/1766, fmt 0. NOT done: test-level red runs on 5325b7d1 for R69/R70, the `then`-branch classification
  over the 32 changed artifacts (prober exists; milvus/oncall/synapse/nacos conditionals unsatisfied), task lint, lint:fc,
  ast-grep, FULL integration, live battery on dump-r15, final-v17.patch. Replacement builder: `round8/brief-w4-rework15-resume.md`.
  After its FINAL hand-back: follow_up BOTH W4 reviewer sessions (astra 20261001T013326-7cb09804, sol
  20261001T013344-c93683c5; they expire 24 h after start, i.e. ~03:30 on Oct 2 — after that, FRESH runs pointed at
  review-w4-rework{11,12,13,14}-*.md). On LAND ×2: re-stage `/Volumes/T7/dev/round8-landing10` (currently landing-10 6808a8a1 =
  main + the 64 commits up to 5325b7d1 — STALE), update the header of `runner/landing-10.env` (committed 741e1eb; E already
  `/Volumes/T7/dev/round8-landing10-run2`), launch the chain. Evidence of run1: `/Volumes/T7/dev/round8-landing10-run1/`
  (chain.log, sweep/new-lint-failures.tsv, battery: 9 687 flips, 1 passed). Reviews: `review-w4-rework{12,13,14}-{astra,sol}.md`.
- **k8s D1–D3 (landing 11) — LAND ×2 (`review-k8s-rework9-{astra,sol}.md`), waits for landing 10.** Clone
  `/Volumes/T7/dev/round8-k8s-r8`, k8s-rebase fcdcd54f (61 commits on b51c8589, clean), target `round8-k8s-r8-target`.
  Conditions: rebase onto the main carrying W4, one clean dump, fixture adoption, zero unmatched flips, bidirectional roster
  gate green. Builder brief ready: `round8/brief-k8s-rework10-resume.md`. The zalando foreign-range lowering stays PARKED
  (`rework9/k18-foreign-range-lowering.parked.patch`): both reviewers require typed iteration facts per range occurrence
  (`ContainsTruthyMember` ≠ non-empty iteration) and the node-exporter / mayastor attribution fixes first.
- **W1 (landing 12) — rework 9 CHECKPOINTED mid-R48.** Clone `/Volumes/T7/dev/round8-w1`, track/w1-main, HEAD 8e98b12c =
  `W1 rework 9 (wip): R48 …` which DOES NOT COMPILE; last green commit f375cde6 (unit gates 0, nextest 1830/1830); base
  6f1ce577 (code == main b51c8589); target `round7-f9/target` (trimmed to ~14 GB). Done and reviewed PASS: R29–R35 (rework 8),
  R36 (output-metadata join by outcome selection with typed absent/unconditional/unknown states — the pre-existing defect
  behind 979 bitnami security-context false acceptances), R37 (per-path token claims), R38/R45 (traefik abstention proof),
  R39–R44. Reviewed REWORK: R47 (f375cde6) → R48 = quote claims need a POSITIVE `verbatim` provenance (Addendum 2 of
  `brief-w1-rework9.md`; counterexamples (a)–(d) in `review-w1-rework9b-{astra,sol}.md`): in the wip commit `TextProvenance`
  (Unknown/Verbatim/Templated/StrictText/Alphabet) replaces the negative flag, helper rows carry it, the classifier
  (`fragment_eval/hole_text.rs`) is a draft; none of (a)–(d) is fixed, no R48 tests yet. Hand-off
  `round8-w1-evidence/rework8/handoff.md` (59 lines, exact next commands). The final-evidence run on f375cde6 was stopped
  mid-battery (73 adjudications, no verdict). Replacement builder: `round8/brief-w1-rework9-resume.md`. Reviewer sessions
  (astra 20261001T033616-8c4a53c9, sol 20261001T033641-8d519810) expire ~05:36 on Oct 2. Open with W1: the R46 perf
  regression is recorded, not fixed (bitnami-redis 2.26× user CPU vs main; `rework9/perf.txt`).
- **R46 (perf) — queued as its own landing right after W1** (`round8/brief-r46-perf.md`, `design-w1-r46-{astra,sol}.md`): step 1
  representation-only (full memo key kept, `MutationFacts` shared behind `Rc`, helper summaries return only mutation deltas;
  fixtures byte-identical); step 2 semantic only if needed (typed object identity, alias-aware reachability).
- **Runner = v6.12** (`/Volumes/T7/dev/round8/runner`, branch runner-v3-fail-closed; last commit 741e1eb). A full chain now
  takes ~2.8 h: dump+unit+lint ~45 min, battery ~25 min, integration ~10 min, sweep ~90 min (fast pass 29 min; the real-Helm
  stage is dominated by openebs — one real `helm lint` of openebs takes ~11 min even with main's schema).
- **Codex:** astra = `gpt-6-astra`, sol = `gpt-6.1-sol` (never `gpt-6-sol`), xhigh, read_only, cwd /Volumes/T7/dev; registry
  `round8/codex-runs.tsv` (3 tab-separated columns). agentmux transcript FILES lag the service state: detect finished turns
  with `status`/`result` (offset = previous byte count), not file sizes; `collect-codex.py` re-saves the previous turn for a
  follow-up that has not finished. Follow-ups on the same session worked well (minutes instead of an hour).
- **Builders:** native Opus subagents; ids die with the session — spawn replacements from the resume briefs above (same clone
  and target). Both session-4 builders ended at 390–700 k tokens; start fresh ones, do not try to revive them. Rules that held:
  restore files with `git show HEAD:path > path`; in-test fixtures regenerate through a temporary scratch test module, never
  by editing `crates/test-util`; `rm`/`cargo clean` of whole dirs is denied by the permission classifier (builders may
  `cargo clean -p` their workspace crates; other deletions need the user); a builder's login can expire mid-turn (resume with
  SendMessage after `/login`).
- **Queued items (owners to assign):** battery coverage gaps found this session — (1) lint-document validation for
  dependency-enable rows, (2) probes in non-default contexts (conditional restrictions); validation time of shipped schemas
  under real Helm (openebs ~11 min per lint, main-too); RenderedToken(YamlKind) vs provider schema (after landing 10); CLI
  default output vs Helm's 5 MiB limit; the parked zalando lowering + node-exporter/mayastor attribution; inherited defects
  recorded with cells: redis-ha `securityContext.privileged: true` accepted (K8s rejects), jupyterhub prePuller hook guard
  pair (`define_block_from_node` keeps whitespace the closing `{{- end }}` trims), `t | b64dec` false quote rejection
  (main-too), W4's retained UNKNOWN limits (o12, m19–m23, `include ("missing")`, null redis-ha name), float `1.0` in numeric
  guard evaluation; an umbrella witness for `common`; D1 nginx `usageReport` guard; main's `eq`-form resourcesPreset false
  rejection; F23/F79 unfrozen witnesses; F69's KnownFalseAcceptance; `task test:all` + luup2 on main; runner artifact-kind
  discovery.
- **Hazards:** `date` before every ledger entry; concurrent Bash calls share one shell (absolute paths); T7 free space swings
  between 75 and 125 GB with other activity on the volume (floor 80; builders trim with `cargo clean -p`); six untracked
  Kubernetes 1.35 cache files in `round8-w1/testdata/provider-bundle/...` (never stage; the user may delete them);
  `/Volumes/T7/dev/round8-timing` and `/Volumes/T7/dev/round8-landing10-target` are disposable with the user's approval.

### Resume prompt for session 5 (copy verbatim)

Resume as the ORCHESTRATOR (Fable) of the helm-schema `schema-bug-hunt-v1` campaign, round 8, session 5, in
/Volumes/T7/dev/helm-schema. Read first, in order: plan/campaign-orchestration-playbook.md (incl. "Self-attack before every
hand-back"); plan/schema-bug-hunt-v1-round8-handoff.md §6 (rewritten at the end of session 4 — the map, incl. this prompt); the
ledger entries of plan/schema-bug-hunt-v1-progress.md from "21:21 (Sep 30) — Session 4 started" to the end;
/Volumes/T7/dev/round8/PROTOCOL.md; CLAUDE.md; the skills landing-workflow, acceptance-battery, corpus-fixtures,
verification-gates; the memory index. Rules: Claude builders only as native Opus subagents, few and focused (session 4's agent
ids are dead: spawn replacements from round8/brief-w4-rework15-resume.md and round8/brief-w1-rework9-resume.md, reusing their
clones and targets; the k8s builder from round8/brief-k8s-rework10-resume.md only after landing 10 merges); every review and
design goes to BOTH Codex reviewers via agentmux (astra = gpt-6-astra, sol = gpt-6.1-sol, never gpt-6-sol; effort xhigh,
sandbox read_only, cwd /Volumes/T7/dev; follow_up on the same session for re-checks — it cannot change models, sessions expire
24 h after their start; detect finished turns with status/result, not transcript file sizes); merge findings into a brief
before the builder resumes; NO hand-back is accepted without a `## Self-attack` section (lint-document check over dependency
rows, multi-path probes in non-default contexts, positive claims only, evaluator == emitted encoding, every emitter,
compose-and-invert, registry completeness, `then`-branch diff classification, generation-time comparison) and reviewers are
asked to check that section first; landings run the runner chain (dump→unit→lint→battery→integration→sweep→finalize, ~2.8 h) in
the staged clone with its own env and target and merge into main with `--no-ff` naming the components + receipt path +
sha256; NEVER push; deletions need the user's approval; `date` before every ledger entry (python datetime.now(), insert before
the trailing `Next:` line, commit `chore(plan)`); one fixture-moving landing at a time; first-ready-first between W4 and W1,
k8s after W4; report closures strictly (4/83 = F17, F30, F54, F77). Do this first: (1) `git status --short`, `git log
--oneline -5`, `ls /Volumes/T7/dev/round8/heavy.lock` (must be absent), `uptime`, `df -h / /Volumes/T7`. (2) W4 (landing 10):
read /Volumes/T7/dev/round8-w4-evidence/rebase/r15/handoff.md and `git -C /Volumes/T7/dev/round8-w4-r9 log --oneline
5325b7d1..HEAD`; spawn the replacement builder from round8/brief-w4-rework15-resume.md to finish rework 15 (test-level red
runs for R69/R70, the `then`-branch classification over the 32 changed artifacts, the float gap decision, all gates, the live
battery on a clean dump, final-v17.patch, a Self-attack section); on its FINAL hand-back send the re-check to both models
(follow_up on astra 20261001T013326-7cb09804 and sol 20261001T013344-c93683c5 if they are younger than 24 h, else FRESH runs
pointed at round8/review-w4-rework{11,12,13,14}-*.md and the r15 hand-off); on LAND ×2: `git -C
/Volumes/T7/dev/round8-landing10 fetch /Volumes/T7/dev/helm-schema +main:main && git checkout landing-10 && git reset --hard
main && git fetch /Volumes/T7/dev/round8-w4-r9 +w4-rebase:w4-rebase && git cherry-pick main..w4-rebase`, update the header of
/Volumes/T7/dev/round8/runner/landing-10.env (tip, commit count; E is already round8-landing10-run2 — if run2 exists use a
fresh run3) and commit it in the runner repo, confirm the lock is absent and load < 40, `mkdir -p` the evidence dir, launch
`(cd /Volumes/T7/dev/round8/runner && LANDING_ENV=/Volumes/T7/dev/round8/runner/landing-10.env nohup zsh -c 'for s in dump
unit lint battery integration sweep finalize; do ./run-landing.sh $s || exit $?; done; echo chain green' >>
<E>/chain.log 2>&1 &)`, heartbeat every ~25 min with `grep -E '^(OK|FAIL|REFUSE|chain green)' <E>/chain.log | tail`; on a
step failure read only that step's log; on "chain green" adopt exactly column 1 of `<E>/adopted.tsv` (skip the header) in the
clone, commit the fixtures, fetch the branch into main and `git merge --no-ff` with components + receipt sha256, run the
post-merge gates on main one at a time (cargo fmt --check; task lint; task lint:fc; cargo nextest run --workspace; task
test:integration; CARGO_TARGET_DIR=/Volumes/T7/dev/helm-schema/target), retire the env, rewrite §6, re-tally closures. (3) W1
(landing 12), in parallel with (2): read /Volumes/T7/dev/round8-w1-evidence/rework8/handoff.md; spawn the replacement builder
from round8/brief-w1-rework9-resume.md to finish R48 (HEAD 8e98b12c is a non-compiling wip; last green f375cde6), then re-check
with both models (follow_up on astra 20261001T033616-8c4a53c9 and sol 20261001T033641-8d519810 if younger than 24 h, else
FRESH runs pointed at round8/review-w1-rework{8,9,9b}-*.md), then the final evidence (integration, clean dump, live battery,
matrix11, final-v11.patch); if W1 is LAND ×2 with a green battery before W4, land it first in a fresh clone
/Volumes/T7/dev/round8-landing12 with runner/landing-12.env derived from landing-10.env. (4) k8s (landing 11): after landing
10 merges, spawn the builder from round8/brief-k8s-rework10-resume.md (rebase, clean dump, live battery zero unmatched,
final-v10), then its chain. (5) After W1: R46 per round8/brief-r46-perf.md. Keep T7 > 80 GB free and load < ~40, record every
decision in the ledger, schedule wakeups (ScheduleWakeup, 20–30 min) while things run, and never launch tracks outside the
hand-off's queue.
