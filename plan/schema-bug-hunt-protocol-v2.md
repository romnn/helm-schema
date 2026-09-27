<!-- PROTOCOL v2, adopted 2026-09-26 05:20 local from round8/protocol-v2-sol.md (sol review of v1).
     Existing clones /Volumes/T7/dev/round8-<track> and their exclusive target dirs ARE the slots;
     agents launched before this adoption keep patch checkpoints; new agents use branch commits.
     v1 is preserved as PROTOCOL-v1.md. -->

# PROTOCOL v2 — helm-schema schema-bug-hunt

## 0. Purpose

- Treat a landing as four bounded claims:
  1. **Artifact identity:** Generate every expected chart, profile, and IR artifact from identified inputs; match adopted fixtures to the complete manifest by size and SHA-256.
  2. **Contract preservation:** Evaluate every frozen witness against its recorded Helm oracle and expectation; prove each fix with a regression test that failed before it.
  3. **Corpus preservation:** Account for every changed schema and input; adjudicate changed acceptance and retain exact known-open cases.
  4. **Shipping compatibility:** Check the bytes Helm receives, including compact-output fallback, file size, `helm lint`, and a schema-free render control.
- Scope “no regression” to the identified corpus, witnesses, and probes. Never infer universal correctness from a green fixture comparison or flip battery.
- Read the clone’s `AGENTS.md`. The orchestrator owns landings and the main repository.

## 1. Workspace

- Reuse assigned `/Volumes/T7/dev/round8-slot-N` clone **slots**. The orchestrator creates each slot once with `git clone`, removes `origin`, and runs `mise trust mise.toml`. Never create a fresh clone per track or use `git archive`; baseline fixtures need Git history.
- Keep each slot’s warm `CARGO_TARGET_DIR` exclusive to that slot. Never share an active target directory, copy a large target tree, or delete another slot’s target.
- Keep evidence outside the clone at `/Volumes/T7/dev/round8-<track>-evidence/`; keep logs, matrices, `handoff.md`, and `final.patch` there. Do not put durable evidence in `/private/tmp`.
- Set `CARGO_BUILD_JOBS=3` and `NEXTEST_TEST_THREADS=3`. Keep incremental compilation enabled for edit loops; the landing runner controls its own environment. Do not enable `RUSTC_WRAPPER=sccache`: the trial found zero cross-clone hits, and disabling incremental compilation slowed edit rebuilds 3.7×.
- Start work on an authorized track branch at the **actual** assigned base commit. Record the slot, branch, base SHA, target directory, and evidence directory in `handoff.md`.
- Reset a slot only after its owner hands it back. First commit all wanted work, export `final.patch`, inspect `git status --short` and untracked files, and confirm the index and worktree are clean. Inspect intent-to-add paths; stage their real contents with `git add` and commit them before any branch switch. Never run `git checkout`, `git restore`, or `git reset` over an intent-to-add path: that emptied a pinned CRD schema and carried the truncation through a patch into five clones.
- Have the orchestrator obtain branch-operation authorization required by `AGENTS.md`, fetch the actual landing HEAD, and create the next track branch only from a clean slot. Preserve the previous branch and evidence. Do not discard unresolved work to free a slot.
- At handover, give the orchestrator the branch and commit SHAs, clean/dirty status, target ownership, `final.patch` SHA-256, and the current `handoff.md`. Do not mutate a candidate after its runner receipt is finalized.

## 2. Before coding

- For **every semantic mechanism**, run a small discriminating Helm v4.2.3 matrix before coding. Include positive and negative controls, defaults, missing versus null, relevant transports (`--set`, `--set-string`, values file), dependency activation, and the pinned Kubernetes versions that distinguish the claim.
- Remove shipped `values.schema.json` and `templates/tests` for the render oracle; preserve the real chart tree and packaged dependencies. Compose over Helm defaults with null-deletion semantics. A bare `{}` can delete declared keys; do not assume it means “use defaults.”
- Compare Helm’s exact coalesced values, render result, and diagnostic with the proposed schema behavior. Distinguish template aborts, schema rejection, Kubernetes pinned-schema violations, and uncertainty.
- Use the Kubernetes-version owner: choose the first pinned version admitted by the root chart’s `kubeVersion`; abstain if none is decidable. Keep Helm rendering and offline validation on the same version. If its pinned validation bundle is missing, record uncertainty. Do not claim the battery checks 1.33.0 while its Helm oracle still uses 1.29.0.
- Obtain a cross-vendor **design review before coding** each semantic mechanism. Resolve counterexamples and boundary decisions in the matrix; reuse an already reviewed contract only when its inputs and claim are identical.
- Write the chosen structural mechanism in `handoff.md` with `file:line` references, the Helm matrix path, the review path, and the predicted regression. Record policy choices explicitly; a schema cannot always distinguish equal JSON values delivered through different Helm transports.

## 3. While coding

- Use typed structural facts. Do not use regex, chart names, helper names, source-line shapes, rescue paths, or witness-shaped exceptions as substitutes for semantics. Prefer deleting a redundant representation to adding another one.
- Add a minimal regression test for **each fixed behavior**, one per contract cell where needed. Show it red on the pre-fix tree and green on the fixed tree; record both commands, exits, and logs. A fixture change alone is not a regression test.
- Follow repository test conventions: private tests under `src/tests/`, public tests under `tests/`, `sim_assert_eq!(have:, want:)`, full-schema equality where applicable, debug builds only.
- Make checkpoints as commits on the track branch after each green step. Record commit SHA and current next action in `handoff.md`; do not use numbered patch files as checkpoints.
- Commit new files with their real bytes. Do not use `git add -N`; the runner refuses intent-to-add paths. Inspect suspicious zero-byte files immediately.
- Export exactly one `<evidence>/final.patch` from the branch’s committed base-to-HEAD diff for the orchestrator. Check that it includes new files and matches the handoff commit range. Keep the branch as the authoritative checkpoint history.
- Add no `#[allow]` or `#[expect]`. Report an unreasonable lint repair as an exact `file:line` blocker.
- Report the production LOC delta; use `task tokei:core` for refactors.

## 4. Gates per hand-back

- Run focused red/green tests and `cargo fmt --check` before hand-back. Run other local gates only when the orchestrator schedules their resource use. Report **each command’s own exit code**, test count, log path, and final-tree SHA; never report a pipe’s exit or call an unrun gate green.
- List these full-tree gates individually in every handoff, with `not run — orchestrator` until the runner supplies a receipt:
  1. `cargo fmt --check`
  2. `task lint`
  3. `task lint:fc`
  4. `task lint:ast-grep`
  5. `cargo nextest run --workspace`
  6. `cargo nextest run --profile integration -p helm-schema-cli --test schema_dialect_hygiene --run-ignored all`
  7. `cargo nextest run --workspace --profile integration --no-fail-fast`
  8. The runner’s dump, battery, and sweep steps.
- Require the dialect hygiene test (no longer `#[ignore]`d; `--run-ignored all` still selects it) to run at least one test and pass every selected test. Require the battery to report exactly `1 test run: 1 passed`; omitting `--run-ignored all` makes it vacuous.
- `task lint` and `task lint:fc` must pass with no diagnostics. The former accepted residual (`control.rs:604 too_many_lines` in `branch_steps`) was fixed on main in 756e4f59, so no lint residual is accepted any more.
- Before declaring the campaign round final, satisfy the repository’s additional `task test:integration` and `task test:all` requirements; for schema-semantic changes, run its downstream luup2 gate. Report their exits separately. Do not substitute a runner step silently for a differently specified `AGENTS.md` command.

## 5. Heavy steps

- Hand back at a dump, battery, workspace integration run, corpus Helm sweep, or full lint sweep. Only the orchestrator starts these steps through `/Volumes/T7/dev/round8/runner/run-landing.sh`, using a candidate-specific `LANDING_ENV`.
- Run one candidate on the actual landing base in this order:
  `dump`, `unit`, `lint`, `battery`, `integration`, `sweep`, `finalize`.
  Run each step as its own invocation; stop on its nonzero exit. Keep the finalized `$E/receipt.json` and its printed SHA-256.
- Let `dump` generate all five fixture lanes once, classify fixture-only mismatches, require a complete nonzero-byte manifest, adopt only manifest artifacts, and freeze both `generation_inputs` and `full_tree`. Re-run the dump after any generation-input change. Re-run stale steps after a test-only change.
- Let the runner refuse intent-to-add paths, zero-byte new or changed files, missing or extra dump files, changed configuration, mismatched adopted fixtures, stale gates, and changed candidate identity. Never override these refusals with manual fixture copying.
- Let the runner hold `/Volumes/T7/dev/round8/heavy.lock` with its PID and unique owner token. Exit 75 means busy: record it and reschedule. Do not wait in a polling loop, take or remove the lock yourself, or infer ownership from a track-name process search.
- Do **not** run manual all-lane dumps, the round-74 battery, full integration, corpus Helm sweeps, or alternate landing scripts. Do not launch a second heavy chain on a predicted base.
- Treat a receipt as evidence for exactly its recorded clone, HEAD, hashes, manifest, baseline, Helm binary, settings, steps, and final tree. A finalized receipt from another candidate proves nothing about this one.

## 6. Landing

- Land independently useful test infrastructure and oracle corrections separately and first. Test a changed gate against the previous gate **and** fixed positive and negative Helm counterexamples; prove it rejects missing or wrong evidence.
- Land one semantic candidate on the actual HEAD at a time. Combine mechanisms only when their attribution is demonstrably separable or an intermediate tree would be unsound. Run a battery per patch unless exact case identity, schema hashes, oracle context, and independent regressions prove attribution separately.
- Obtain a cross-vendor **pre-landing patch review** for every candidate. Give the reviewer the final diff, Helm matrix, red/green evidence, manifest, receipt, fixture diff, witness changes, and unresolved questions. Use a fresh agent for each rework round.
- Read fixture diffs with stable, readable `$defs` names. Explain every changed definition and reference; prove a pure rename preserves references and acceptance. Do not treat large churn, a green battery, or a compact shipping form as semantic proof.
- Compare changed probes by complete case identity, schema hash, transport, Helm version, chart tree, Kubernetes version, and oracle context. Send only unexplained residuals for further analysis. Do not retire a known-open case because a flip disappeared.
- Run the sweep on **shipped schema bytes**: pretty JSON plus newline when within 5,242,880 bytes, otherwise compact plus newline. Require the schema-free Helm template control, including removal from packaged dependencies, and reject a new lint failure when the control renders. Check row completeness and errors, not only exit status.
- Evaluate every frozen witness on every landing, including unchanged charts. Keep known-open and policy-unresolved rows visible. Count a family closed only when every registered obligation is `Fixed` or `Met`, passes, and has no unfrozen note, roster allowance, or oracle conflict. Report both closed families and inventory coverage; the current baseline is 3/83 closed with rows for 21/83 families.
- Update exact witness inputs, transport, oracle result, owner, and adjudication evidence when a contract changes. Preserve fixed rows after promotion. Do not turn a false acceptance roster label into an allowance without an exact document.
- Let the orchestrator update the roster, witness catalog, and `plan/` ledger after review. Use one ledger entry:
  `time with zone | track | base SHA -> landed SHA | mechanism | red/green tests | receipt SHA | fixture/flip verdicts | witnesses closed/open/policy-unresolved and coverage | decision or blocker | evidence links`.
- Record what actually landed. Do not claim family closure from a flip-only battery or call pinned-schema validation Kubernetes admission.

## 7. Reports

- Keep `handoff.md` **current-state only** and under 60 lines. Include slot, branch, base/HEAD SHAs, target, final.patch path and SHA, mechanism with `file:line`, matrix/review links, red/green tests, individual gate results, fixture and witness status, exact blocker, and exact next command. Link historical commits and logs; do not append old “resume” sections.
- Keep the final message under 60 lines. Preserve the report format:

1. Contract (what Helm does, with the matrix you ran and its file).
2. Mechanism (the structural fact, the files, what representation was deleted).
3. Tests added (names, file).
4. Gates: one line each with exit code and counts.
5. Fixtures: how many moved, how many adjudicated, every unmatched flip with chart, path,
   direction and the Helm verdict.
6. Patch path + handoff path. Anything unfinished: exact witness and blocker.
Do not paste diffs or logs into the message; point at files.

## 8. Token discipline

- Do not poll locks, background jobs, load, or other agents. Run bounded focused work in the foreground; hand back at a heavy step with its exact next command.
- Start rework with a fresh agent reading the short current-state handoff, matrix, review, commits, and receipt. Do not spend a long continuation rediscovering recorded evidence.
- Do mechanical manifest checks, stable-name fixture diffs, and keyed outcome comparisons directly. Ask Codex for an independent semantic challenge, a counterexample, or the required cross-vendor design or pre-landing review; do not ask it to restate mechanical logs.
- Identify the account before starting any new cross-vendor consultation, as required by `AGENTS.md`. Keep the consultation’s source evidence and decision linked from the handoff.

## 9. Prohibited

- Never push, force-push, edit `plan/`, or write to the main `/Volumes/T7/dev/helm-schema` tree. Reading its HEAD and clean status is allowed.
- Never work around a denied command, runner refusal, missing oracle evidence, or failed gate. Report the command, exit, reason, and blocker.
- Never take, remove, or “repair” the heavy lock; only its owning runner releases its token.
- Never `pkill` another agent’s processes or delete another slot’s files.
- Never use `git checkout` or another index-based restore over an intent-to-add path. Never transfer a candidate by an unchecked patch or accept a zero-byte pinned schema.

## 10. Machine budget

- Budget for 11 cores and 18 GB RAM. Run at most four concurrent Rust builders, each with `CARGO_BUILD_JOBS=3`; count the orchestrator’s runner as a builder.
- Count Helm sweeps and lint sweeps against the same machine budget. Keep `SWEEP_JOBS` bounded, initially 3. Schedule one locked heavy step at a time and leave capacity for the active landing.
- Reuse fixed clone slots and their exclusive warm target directories. The sccache trial found 0/209 cross-clone Rust hits; same-path rebuilds hit, but disabling incremental compilation made edit loops 3.7× slower. Do not trade warm incremental builds for speculative cache savings.
- Stop launching work when the budget is full. Hand back a ready candidate and let the orchestrator schedule it.
