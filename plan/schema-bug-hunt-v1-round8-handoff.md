# schema-bug-hunt-v1 — round 8 hand-off (started 2026-09-26; §6 current as of 2026-10-19, end of session 5)

> **§6 is the current state and ends with the resume prompt — read it first.** §1–§5 are the history of the
> round as of 2026-09-26 (earlier landing plans, superseded statuses); use them only when §6 or the ledger points there.
>
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

## 6. Status (rewritten 2026-10-19 ~16:45 local, end of session 5 — resume here)

Read the ledger entries from "Session 5" (2026-10-17) onward for detail; this section is the map. The resume prompt for session 6
is at the end of this section.

- **Nothing landed in session 5. main code = b51c8589** (landing 9; only plan/ commits since; nothing pushed).
- **Strict verified closures, CORRECTED: 3/83 on main = F17, F54, F77.** Earlier sessions reported 4 (F17, F30, F54, F77): F30 still
  carries 19 roster cells and F74 is open (five fixtures exceed Helm's 5 MiB chart-file limit). After landing 10a merges: **9/83 =
  F17 F32 F35 F45 F54 F57 F60 F61 F77** (witness freeze, LAND ×2). F73/F80 closed by policy (not counted).
- **Critical defect class found this session (inherited from main, in scope):** main's shipped schemas break `helm lint` on their OWN
  chart defaults for 13 charts (aws-ebs-csi-driver, eck-stack, headscale, imgproxy, kube-starrocks, nacos, netbox, nginx-ingress,
  openldap-stack-ha, stacks-blockchain-api, synapse, yourls, gitea), airflow/oncall fail the draft-07 metaschema (`\u` patterns), and
  five schemas exceed Helm's 5 MiB limit (openebs, kube-prometheus-stack, oncall, milvus, gitea). Scan: `round8/defaults-lint-main-results.tsv`;
  attribution per chart: `round8/triage-defaults-sol.md`. The landing sweep never caught it because it fails only on NEW lint failures.

### Landing 10a — assembled, approved, BLOCKED on battery speed
- Clone `/Volumes/T7/dev/round8-landing10a`, branch `landing-10a-v2` (36 commits on main 7c28756f, HEAD 0e7a601c): battery lint-document
  lane phase 1 (21 commits, 8 review rounds, LAND ×2), tracing spans + `trace.rs` recorded with the user's fork romnn/tracing-perfetto
  (git rev d4c54ab), helmsweep validating with the romnn/jsonschema fork (indexed queue), perf fix for `append_selected_constraints`
  (validator reuse, lazy; redis −10.7%, openebs −14.5%, outputs byte-identical), helmsweep v9 (reports the schema bytes Helm loaded),
  witness freeze (117+ rows, traefik enrolment, F74 default-output obligations). Each part LAND ×2 (reviews in round8/review-*.md).
  One hand-resolved conflict (tracing span on the battery's restructured `screen_chart`).
- Run 1 (`round8-landing10a-run1`, env `runner/landing-10a.env`): dump 8 min (0 of 204 adopted), unit 7, lint 8 — OK; battery: killed by
  an EXTERNAL SIGTERM at 03:29 (83 min; also killed W1's battery — never found who; all briefs now forbid pkill by name), then
  TIMED OUT at nextest's 3 h ceiling (06:31). Landing 9's battery took 26 min: the lint lane's live Helm adjudication volume.
- **Unblocking:** the battery builder (Codex astra, agentmux run `20261017T185515-78d55437`, turn 13, clone `/Volumes/T7/dev/round8-battery`,
  14 uncommitted files = work in progress) measures per chart/lane and fixes the dominant cost WITHOUT weakening verdicts (target
  ≤ 45 min, coverage verdicts byte-identical to a slow reference run). On hand-back: `git apply -R --check` its transfer patch, commit
  in round8-battery (the sandbox cannot), re-review with astra + sol, cherry-pick into landing-10a-v2, then launch a FRESH chain
  (runner is now v9 → fresh E, start at dump): derive E=`/Volumes/T7/dev/round8-landing10a-run2` in `runner/landing-10a.env`, commit it.
- The live runner is **v9** (`/Volumes/T7/dev/round8/runner` c6a52a4: schema-bearing chart copies must LOAD the shipped schema despite
  `.helmignore`; CLI cache keyed by the prepared copy); suite 14/14 green with
  `GUARD_SRC=/Volumes/T7/dev/round8-target-guard/scripts/target-guard.py zsh tests/run-all.sh`.

### Semantic tracks
- **W1 rework 14 (membership/emptiness; 317 false acceptances left vs roster f7be7ba5, 0 false rejections):** clone `/Volumes/T7/dev/round8-w1`
  (track/w1-main; target `/Volumes/T7/dev/round7-f9/target`). R81 propagation repairs committed (31312880). Part 2 (R78–R80: abstract
  mergo merge model, aggregate reads, ASCII `lower` partial truth, vault literal-tpl fact, nats NoMarker proof, nginx key/value boundary)
  under BINDING OBLIGATIONS. Resume brief `round8/brief-w1-rework14-part2-resume.md`; current state in
  `round8-w1-evidence/rework8/handoff.md`. Known landing blocker: bitnami-redis generation +50% (CPU gate within 10% of w12).
- **W4 rework 17 (scalar preimages, CONSOLIDATION):** design rev 3 BUILDABLE ×2; implementing. Clone `/Volumes/T7/dev/round8-w4-r9`
  (w4-rebase, base b51c8589). Resume brief `round8/brief-w4-rework17-impl-resume.md`; state `round8-w4-evidence/rebase/r17/handoff.md`.
- **Track G (D1/D5 execution guards; fixes kube-starrocks, openldap-stack-ha, nginx-ingress defaults; milvus/openebs-loki):** design
  released with binding obligations; red tests committed (21505846). Clone `/Volumes/T7/dev/round8-guards` (guards-d1-d5 on 10a).
  Resume brief `round8/brief-guards-resume.md`; state `round8-guards-evidence/handoff.md`.
- **Size-aware output (F74; USER DECISION):** `--output-layout auto|pretty|compact`, default auto = pretty unless > 5,242,880 bytes, then
  compact + short `$defs` handles with a stderr note; forced modes or still-oversized output warn and exit 0 (never fail on size).
  Committed 5640581b in `/Volumes/T7/dev/round8-size` (size-aware-output on 10a); equivalence 156 charts / 270,261 probes / 0 flips;
  reviews running: astra `20261019T143700-106056ac`, sol `20261019T143700-427a9774` → on LAND ×2 add to the next landing.
- **Investigated, queued:** F15 rabbitmq (`round8/investigation-f15-astra.md`; after W4 — same IR files); D4 subchart scope (netbox,
  stacks-blockchain-api, yourls), F4/F31 provider projection (synapse, openebs), imgproxy aliases (F49), eck-stack F50, headscale /
  aws-ebs (NEW) — all from the defaults triage; the k8s provider track (after W4); R46 perf (after W1); definitions emission (superseded
  for size by the output layout; keep only if needed for other reasons).
- **Process items queued:** a runner rule that FAILS when a shipped schema rejects its chart's defaults (control renders) or exceeds
  5 MiB, regardless of main; investigate why `run-all.sh` once printed exit 0 with a failed suite; `perf`: the 22.7% duplicate
  validator inputs in the conditional-constraint gate (generation-scoped reuse, keyed by every input) as the next perf step.

### Standing decisions made this session (user)
- Use Codex (astra, sol) extensively; Opus builders for large semantic builds; Fable/Opus complexity challengers only when they help.
- Inherited (main-too) defects stay in scope and blocking; fixes must simplify the architecture (playbook self-attack item 10).
- Never cancel Codex runs without the user's explicit go; resume FAILED/DEAD turns with follow_up (capacity → retry; content flag →
  rephrase without security wording).
- Launch chains without waiting for an idle host; tell the user when a chain > 3 h or a step > 50% slower than landing 9.
- Delete idle build folders proactively (pgrep check first).
- Forks: romnn/jsonschema (helmsweep) and romnn/tracing-perfetto (git dep, user-maintained; issues/fixes on fix/ branches there).
- Output layout auto/pretty/compact as above.
- Keep ~3 cargo-heavy builders busy; a builder waiting on review gets review-independent work.

### Resume prompt for session 6 (copy verbatim)

Resume as the ORCHESTRATOR of the helm-schema `schema-bug-hunt-v1` campaign, round 8, session 6, in /Volumes/T7/dev/helm-schema.
Read first, in order: plan/campaign-orchestration-playbook.md (incl. "Self-attack before every hand-back", item 10 "Simpler, not
bigger"); plan/schema-bug-hunt-v1-round8-handoff.md §6 (this section); the ledger plan/schema-bug-hunt-v1-progress.md from the
"Session 5" entries (2026-10-17) to the end; /Volumes/T7/dev/round8/PROTOCOL.md; CLAUDE.md; the skills landing-workflow,
acceptance-battery, corpus-fixtures, verification-gates; the memory index. Session 5's native agent ids are DEAD: spawn fresh Opus
builders from the resume briefs, reusing clones and targets. Codex runs survive: check them first with
`python3 /Volumes/T7/dev/round8/watch-codex.py | grep -v '^done'` and collect finished answers (turns/NNNN/last-message.md;
`collect-codex.py` saves only turn 0000). Reviewers: astra = gpt-6-astra, sol = gpt-6.1-sol (never gpt-6-sol), effort xhigh,
sandbox read_only, cwd /Volumes/T7/dev; follow_up for re-checks while the session is younger than 24 h; plain correctness wording.
Do this first:
(1) `git status --short; git log --oneline -5; ls /Volumes/T7/dev/round8/heavy.lock` (must be absent); `uptime; df -h /Volumes/T7`.
(2) Battery speed (blocks landing 10a): the Codex battery builder `20261017T185515-78d55437`; when it hands back, commit its
    transfer in /Volumes/T7/dev/round8-battery, review with astra + sol, cherry-pick into round8-landing10a (landing-10a-v2), set
    E=/Volumes/T7/dev/round8-landing10a-run2 in runner/landing-10a.env, commit, and launch the full chain
    `(cd /Volumes/T7/dev/round8/runner && LANDING_ENV=landing-10a.env nohup zsh -c 'for s in dump unit lint battery integration sweep finalize; do echo "=== step $s start $(date +%H:%M)"; ./run-landing.sh $s || { echo "=== step $s FAILED rc=$? $(date +%H:%M)"; exit 1; }; echo "=== step $s done $(date +%H:%M)"; done; echo chain green' >> <E>/chain.log 2>&1 &)`;
    on "chain green" commit the adopted fixtures and `git merge --no-ff` into main with components + receipt sha256; never push;
    then re-tally closures (expected 9/83).
(3) Spawn fresh Opus builders: W1 from round8/brief-w1-rework14-part2-resume.md, W4 from round8/brief-w4-rework17-impl-resume.md,
    Track G from round8/brief-guards-resume.md (≤ 3 cargo-heavy builders at once). Their FINAL hand-backs go to astra + sol
    (W1 also a Fable challenger) before any landing; then rebase onto main after 10a merges.
(4) Size-aware output: collect the two reviews (143700-106056ac astra, 143700-427a9774 sol); on LAND ×2 add 5640581b to the next
    landing.
(5) Add the runner rule (shipped schema must accept its chart's defaults and be ≤ 5 MiB) via a Codex builder in a runner dev clone,
    applied between chains; then queue D4, F4/F31, F15, imgproxy per round8/triage-defaults-sol.md.
Recreate the 20-minute heartbeat (CronCreate) on start; `date` before every ledger entry; commit ledger entries `chore(plan): …`.
