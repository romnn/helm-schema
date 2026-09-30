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

## 6. Status (rewritten 2026-09-30 20:18 local, end of session 3 — resume here)

Read the ledger entries from "22:15 (Sep 27) — Session 2" onward for the detail; this section is the map. The
resume prompt for session 4 is at the end of this section.

- **main code = b51c8589** (landing 9 merged 2026-09-30 07:08: F1 global root + authoring policy v2 + Helm two-pass
  dependency-root model + policy-attributed battery verdict, K17, cell_matrix, catalog promotions; post-merge gates
  green; plan-only commits since). Nothing pushed. **Strict verified closures 4/83 = F17, F30, F54, F77** (every
  frozen witness `Fixed`, no unfrozen witness, ≥ 1 verdict row); F74 size-only (not counted); F73/F80
  CLOSED-BY-POLICY; L1/L2 outside the 83. F1 stays open (`common` umbrella witness; nginx-ingress KnownFalseRejection
  filed against D1). Not run on main for landing 9: `task test:all`, luup2 — owed.
- **Landing order: W4/F75 = landing 10 → k8s D1–D3 = landing 11 → W1 = landing 12 → frontend 2.2.** One
  fixture-moving landing at a time; every candidate rebases onto the current main before its final dump + battery.
- **Landing 10 (W4/F75) — NOT launched; rework 12 CHECKPOINTED; its battery-v17 chain was KILLED at 21:11 (user
  decision) and session 4's replacement builder re-runs it.** Clone
  `/Volumes/T7/dev/round8-landing10`, branch landing-10 at c6f9f663 = main + the 51 rework-11 commits (tip 4fc19662) —
  STALE: re-stage on the rework-12 tip once it is reviewed LAND ×2. Env `/Volumes/T7/dev/round8/runner/landing-10.env`
  committed (bd9a486; header names 4fc19662 and 51 commits — update tip, count and move count; E=round8-landing10-run1,
  TARGET=round8-landing10-target). Rework 11 got REWORK ×2 (`review-w4-rework11-{astra,sol}.md`: R58 the depth bound
  claimed unrecognised programs missing — grafana/spinnaker `include (((((print "x"))))) .` renders, main accepts, r11
  rejects; R59 literal-named missing include inside a computed operand must still reject; R60 nested `not`; R61 CR /
  `\r` in double-quoted literals). Rework 12 (brief `brief-w4-rework12.md`) is CHECKPOINTED in `round8-w4-r9` at
  1e3c2bdf (clean; 54 commits on b51c8589; a5a1f33a's trie patterns were replaced after they broke argo-cd/openebs
  regex compilation and openebs' 5 MiB budget): R58–R61 done in code — NOTE the missing-callable claim is again a
  COMPLEMENT, of `resolvable_calls_program` argued as a total word language (tpl_programs.rs:264); the re-check must
  adjudicate that totality explicitly — with the 13-cell red→green test, 18 gen fixtures + the R51 CLI fixture regenerated, fmt/lint 0; the
  battery-v17 chain (lint:fc → nextest → FULL integration → clean dump → verify → Go-regexp → helmsweep → live battery →
  release CLI → matrix v13 → fixture diff/moves/size table → final-v13.patch + sha256 into
  `round8-w4-evidence/rebase/battery-v17/status.txt`) was restarted 21:07 on 1e3c2bdf and killed at 21:11 during `task test:integration` (partial status.txt: remove it
  before re-running). Resume file
  `round8-w4-evidence/rebase/r12/handoff.md` (exact next commands). The builder a4a462316266893c4 dies with session 3:
  spawn the replacement from `round8/brief-w4-rework12-resume.md` (same clone/target) unless status.txt already says
  `done` with a green battery — then only the final handoff rewrite is missing. Re-check reviewers with FRESH runs on
  both models pointed at review-w4-rework{8,9,10,11}-*.md and the r12 hand-off. On LAND ×2: re-stage the landing clone,
  update the env header, commit, launch the chain (command in the resume prompt), merge `--no-ff`, post-merge gates,
  hand-off. R53 residual: n16/n18 pinned as known false acceptances; the `RenderedToken(YamlKind)` provider-schema-use
  mechanism (corpus sinks redis-ha hostPath.path, open-webui storageClass, external-dns mountPath, main-too) is queued as
  its own track after landing 10.
- **k8s D1–D3 (landing 11)** — clone `/Volumes/T7/dev/round8-k8s-r8`, branch k8s-rebase fcdcd54f (61 commits on
  b51c8589), target `round8-k8s-r8-target`, builder a21b0ee7640a3112b (idle after its K18/K19 hand-back), evidence
  `/Volumes/T7/dev/round8-k8s-evidence/rework9/handoff.md` (60 lines, self-sufficient), `final-v9.patch` sha256
  77cf404e…. K18 pins the template recipes' Kubernetes version; K19 factors guard disjunctions exactly (signoz
  readable pretty 6.63 → 4.19 MB; openebs 13.49 → 8.77 MB). The zalando ingress `service.port` false rejection fix
  is PARKED (`rework9/k18-foreign-range-lowering.parked.patch`): lowering a range over a foreign path unmasked
  prometheus-node-exporter and openebs/mayastor attribution defects — design review requested. Reviews RUNNING
  (fresh): astra 20260930T174441-d3cb7d38, sol(6.1) 20260930T174500-e7223302 → `review-k8s-rework9-{astra,sol}.md`
  (collect with `python3 /Volumes/T7/dev/round8/collect-codex.py` or agentmux `result`). Battery on fcdcd54f fails
  exactly on the 9 W4-dependent cells (nacos `ingress.apiVersion` ×8, kps namespaceOverride); after landing 10
  merges: rebase, one clean dump, battery (zero unmatched), final-v10 → landing 11.
- **W1 (landing 12)** — clone `/Volumes/T7/dev/round8-w1`, branch track/w1-main, rework 8 CHECKPOINTED at 5246c574
  (clean tree, every step a `W1 rework 8 (wip)` commit), target `round7-f9/target` (the red worktree now has its own).
  Resume file `/Volumes/T7/dev/round8-w1-evidence/rework8/handoff.md` (60 lines); checkpoint `rework8/final-v9.patch`
  (6f1ce577..5246c574, sha256 9e440e47…). Per the builder all of R29–R35 are implemented (brief `brief-w1-rework8.md`;
  R31 corrected: okteto `probe=null` was a genuine false rejection) and the checkpoint gates are green except FULL
  integration: 145 fixture adoptions + 4 OPEN real diffs caused by R29 (explain redis_exporter…, explain_values_json…,
  nacos_ingress_api_version… ×2) that need review; 232 late R29 regressions were fixed but are not yet pinned by named
  tests; the LIVE battery on dump10 ENDED 20:43 with 1 failed, sole reason "false acceptances missing from
  KNOWN_FALSE_ACCEPTANCES" (`rework8/battery10-live/battery.log` + coverage.json; no unmatched tightening) — the
  expected input for open item 1 (adjudicate and roster the R29 false acceptances), then re-run
  `bash /Volumes/T7/dev/round8-w1-evidence/rework8/battery9.sh live`. Open: roster the R29 false
  acceptances, review the 4 diffs, wrong-rows-v8.txt, regenerate final-v9. The builder (ae88c76cb6bda5467) is dead with
  session 3: spawn the replacement from `round8/brief-w1-rework8-resume.md`. Reviews on the real hand-back: fresh runs on
  both models pointed at review-w1-rework{5,6,7}-*.md and rework8/handoff.md (the old sessions bde21128/b7fd0b5d expire).
- **Runner = v6.12** (`/Volumes/T7/dev/round8/runner`, branch runner-v3-fail-closed; c78845e adds the `authoring`
  artifact kind after landing-9 run1 refused it; v6.11 f5bc5b9 signals/lock token). `tests/run-all.sh` REQUIRES
  `GUARD_SRC` (snapshots `round8-runner-evidence/guard-src-v3..v7-target-guard.py`). Every landing: own TARGET, strict
  env grammar, retired envs quote `LANDING_FINISHED='… (landed; target retired)'`, a runner change → fresh evidence
  dir. Queued runner item: learn artifact kinds from the producer instead of a hand-kept set.
- **Tooling:** target guard PARKED at 163bd01a (branch target-guard in `round8-target-guard`; rework 6 REWORK ×2:
  `-F` masking, relative CARGO_HOME, `HOME` env patches, `[target.*].runner`; no further rounds unless the user
  reopens it; builder a67c3c0781e99d708 idle); cell_matrix landed (landing 9); explain C1 landed (7); frontend 2.1
  landed (8), **2.2 starts only after W1/W4/k8s land**.
- **Codex (user, 2026-09-29): astra = `gpt-6-astra`, sol = `gpt-6.1-sol`, never `gpt-6-sol`; xhigh, read_only,
  cwd /Volumes/T7/dev.** A follow_up cannot switch models (a re-check on an old sol session becomes a fresh run
  pointed at the old transcript). Runs expire 24 h. Registry `round8/codex-runs.tsv`. A content filter once stopped a
  run (89e63fe7): rephrase neutrally ("render-failure claims"), never escalate. agentmux calls can block up to an
  hour. Subagent JSONL timestamps and agentmux run ids are UTC (local −2 h).
- **Builders (native Opus subagents; an agent id is addressable ONLY inside the session process that spawned it — all
  ids below belong to session 3 and are dead in a new process; spawn replacements from the resume briefs
  `round8/brief-w4-rework12-resume.md`, `round8/brief-w1-rework8-resume.md`, and write one for k8s from the
  `brief-k8s-rework8-resume.md` pattern when landing 11 starts; a dead builder's clone and target are reused as-is):** W4 a4a462316266893c4 (active), W1 ae88c76cb6bda5467
  (active), k8s a21b0ee7640a3112b (idle), runner/guard a67c3c0781e99d708 (idle), F1 a1fb0ea167e8d1a85 (done), old W4
  a8dd06d301e7c2760 (retired; its clone `round8-w4-rebase` holds 43 uncommitted fixture edits, ignore). A builder
  whose transcript ends on a `tool_use` with no result and no live process is blocked on a permission prompt:
  spawn a replacement from a resume brief (pattern: `brief-w4-rework9-resume.md`, `brief-k8s-rework8-resume.md`).
  Builders restore files with `git show HEAD:path > path`, never `checkout --`/`stash`/`reset`. ~3 cargo-heavy
  builders at once; one build at a time each, `uptime`-gated (load < ~40); FULL `task test:integration` in every
  hand-back (chart_corpus alone hid the Traefik regression; schema_emission_profiles alone hid two k8s failures).
- **Standing decisions:** keep `assert` as the default authoring policy with policy-attributed adjudication (user
  informed, may override); F1 attribution = candidate is the assert regeneration ∧ annotate regeneration accepts
  outright ∧ explain reports the rejected members unread (no baseline, no identity); TIGHTEN is a direction, not a
  verdict; `values.schema.json` is output, not evidence; landings per token; the k8s template recipes pin v1.29
  like the corpus; the corpus size gate checks the Helm-ready form the sweep ships while the CLI's DEFAULT output is
  readable pretty JSON.
- **Queued items (owners to assign):** RenderedToken(YamlKind) rendered-token kind vs provider schema (after
  landing 10); CLI default output vs Helm's 5 MiB limit (pretty airflow/synapse/gitea/kps/milvus/oncall/openebs exceed
  it; K19's factoring helps; the compact form fits) — product decision; the parked zalando foreign-range lowering
  + node-exporter/mayastor attribution (k8s, after the reviews); an umbrella witness for `common`; D1 nginx
  `controller.mgmt usageReport` guard (report-batch-03.md:527); main's `eq`-form resourcesPreset false rejection;
  F23/F79 unfrozen witnesses; F69's KnownFalseAcceptance; `task test:all` + luup2 on main; runner artifact-kind
  discovery; W4's > 3-deep parenthesised name residual (R58 covers it).
- **Hazards:** `mise trust` every fresh clone; one cargo target per clone AND per worktree; `date` before every
  ledger entry; concurrent Bash calls share one shell (absolute paths, no leading `cd`); `du` over T7 times out
  under load (background it); disk sits at 100–170 GB — prune idle targets between rounds (removed this session:
  landing 7/8/9 targets, guard targets, old W4/F1 targets); the session idled 11:10–13:50 once with the wakeup firing
  late — re-read `date` before assuming anything.

### Resume prompt for session 4 (copy verbatim)

Resume as the ORCHESTRATOR (Fable) of the helm-schema `schema-bug-hunt-v1` campaign, round 8, session 4, in
/Volumes/T7/dev/helm-schema. Read first, in order: plan/campaign-orchestration-playbook.md;
plan/schema-bug-hunt-v1-round8-handoff.md §6 (rewritten at the end of session 3 — the map, incl. this prompt); the
last ~40 entries of plan/schema-bug-hunt-v1-progress.md; /Volumes/T7/dev/round8/PROTOCOL.md; CLAUDE.md; the skills
landing-workflow, acceptance-battery, corpus-fixtures, verification-gates; the memory index. Rules: Claude builders
only as native Opus subagents (the agent ids in §6 belong to session 3's process and are dead in a new one: spawn
replacement builders from round8/brief-w4-rework12-resume.md and round8/brief-w1-rework8-resume.md, reusing their clones
and targets; write a k8s resume brief from the brief-k8s-rework8-resume.md pattern when landing 11 starts); every review and design to BOTH Codex reviewers via agentmux (astra =
gpt-6-astra, sol = gpt-6.1-sol, never gpt-6-sol; effort xhigh, sandbox read_only, cwd /Volumes/T7/dev; a follow_up
cannot change models; runs expire after 24 h); merge findings into a brief before the builder resumes; landings run
the runner chain (dump→unit→lint→battery→integration→sweep→finalize) in the staged clone with its own env and target
and merge into main with `--no-ff` naming the components + receipt path + sha256; NEVER push; `date` before every
ledger entry (compose entries with datetime.now() in python, insert before the trailing `Next:` line, commit
`chore(plan)`); one fixture-moving landing at a time; optimise landings per token; report closures strictly
(4/83 = F17, F30, F54, F77). Do this first: (1) collect pending Codex results (`python3
/Volumes/T7/dev/round8/collect-codex.py` or agentmux `result`): k8s reviews 20260930T174441-d3cb7d38 (astra) and
20260930T174500-e7223302 (sol) → save as round8/review-k8s-rework9-{astra,sol}.md, record in the ledger. (2) W4
(landing 10): read /Volumes/T7/dev/round8-w4-evidence/rebase/r12/handoff.md and `git -C /Volumes/T7/dev/round8-w4-r9
log --oneline 4fc19662..HEAD` (brief round8/brief-w4-rework12.md R58–R61); unless a complete hand-back with
final-v13.patch + sha256 and a green battery-v17 exists, spawn the replacement builder from
round8/brief-w4-rework12-resume.md and wait for its hand-back; on hand-back re-check with FRESH Codex runs (both models) pointed at
review-w4-rework{8,9,10,11}-*.md and the r12 hand-off; on LAND ×2: re-stage the clone
(`git -C /Volumes/T7/dev/round8-landing10 fetch /Volumes/T7/dev/helm-schema +main:main && git reset --hard main
&& git fetch /Volumes/T7/dev/round8-w4-r9 +w4-rebase:w4-rebase && git cherry-pick main..w4-rebase`), update the
landing-10.env header (tip, commit count, move count) and commit it in /Volumes/T7/dev/round8/runner, confirm the
lock is free (`ls /Volumes/T7/dev/round8/heavy.lock` absent) and load < ~40, `mkdir -p
/Volumes/T7/dev/round8-landing10-run1`, launch `(cd /Volumes/T7/dev/round8/runner &&
LANDING_ENV=/Volumes/T7/dev/round8/runner/landing-10.env nohup zsh -c 'for s in dump unit lint battery integration
sweep finalize; do ./run-landing.sh $s || exit $?; done; echo chain green' >>
/Volumes/T7/dev/round8-landing10-run1/chain.log 2>&1 &)`, poll chain.log every ~40 min (a step failure: read only
that step's log, fix the smallest thing, restart per the runner's refusal text; a runner change → fresh evidence
dir), on "chain green" adopt exactly `$E/adopted.tsv` (column 1 = paths) in the clone, merge `--no-ff` into main
with components + receipt sha256 from finalize, run the post-merge gates on main one at a time (fmt, task lint,
lint:fc, nextest workspace, test:integration; CARGO_TARGET_DIR=/Volumes/T7/dev/helm-schema/target), retire the env,
remove the landing target, rewrite §6, re-tally closures from the catalog. (3) k8s (landing 11): after landing 10
merges, spawn a k8s builder (resume brief from the brief-k8s-rework8-resume.md pattern; clone round8-k8s-r8, target
round8-k8s-r8-target, read rework9/handoff.md) to rebase k8s-rebase onto the new main, one clean dump, live battery (zero
unmatched), final-v10, then the reviews' verdict on the parked zalando fix. (4) W1 (landing 12): read
/Volumes/T7/dev/round8-w1-evidence/rework8/handoff.md and the clone's log; unless rework 8 is complete, spawn the
replacement from round8/brief-w1-rework8-resume.md (brief round8/brief-w1-rework8.md); on hand-back reviews with fresh runs pointed at
review-w1-rework{5,6,7}-*.md. (5) Keep disk > 80 GB (prune idle targets), load < ~40, and record every decision in
the ledger. Schedule wakeups (ScheduleWakeup, 20–45 min) while things run; never launch new tracks without the
hand-off's queue.
