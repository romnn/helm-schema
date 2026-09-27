# Ledger and hand-off formats

Match the format already used by the ledger you append to. Placeholders are in `<angle
brackets>`; hashes and numbers in examples are illustrative only.

## Decision register (top of every progress ledger)

```markdown
# <Campaign name> progress

## Decision register

- Frozen plan: `plan/<campaign>.md` at `<freeze-sha>`.
- Frozen-plan policy: the plan remains byte-identical to `<freeze-sha>`; this ledger is the
  only campaign prose surface.
- Wave scope and order: <items in execution order>.
- Starting tree: clean `main` at `<sha>`. Starting production Rust LOC: <n> (`task tokei:core`).
- Helm adjudicator: Helm <version> (commit, Go version); Kubernetes version pinned for adjudication.
- <Dn = option k>: <decision and who made it>.
```

## Round section (frozen-plan campaigns)

```markdown
## Round <id> — <imperative title>

- Status: landed in `<sha>` (`<commit subject>`).
- Contract: <what changes and what must not; "test infrastructure only" / "byte-exact" / "semantic">.
- Acceptance baseline: `<sha>`.
- Baseline production Rust LOC: <n>.
- Pre-registered acceptance expectations:
  - <written before measuring; e.g. "fixtures byte-identical", "zero flips", "no candidate-accepts/Helm-aborts cell">
- Measured results:
  - <what was measured, with numbers>
- Deviations:
  - <every departure from the plan or expectations, and which artifacts were discarded>
- Adjudication evidence:
  - <Helm version, flip counts, per-flip verdicts or "zero flips">

### Producer and route coverage
| Route | Final construction | Verification |

### Review dossier
- <claim>: `<exact reproducer command>`; exit <n>, <counts>.

### Self-adversarial pass
- <the strongest objection to the change and why it does not hold, or what was changed>

### Gates
- `cargo fmt --check`; exit 0.
- `task lint`; exit 0.
- … one line per CLAUDE.md gate, each with its own exit and counts …
- `git diff --exit-code <freeze-sha> -- plan/<campaign>.md`; exit 0.

- Measured production LOC delta: <before> to <after>.
```

A claim without a reproducer command is unverified. The frozen-plan check line belongs in
every round's gates.

## Timestamped entries (orchestrated landing campaigns)

Insert before the trailing `Next:` line; keep the time zone consistent (the orchestrator
shell may print UTC while entries use local time).

```markdown
- 17:40 — **Landing-1 chain stopped at lint (two small corrections in flight).** dump exit 0
  (10 min), unit exit 0, lint: `task lint` passed with the accepted residual, `task lint:fc`
  rc 201 with `too_many_lines` in four test functions … Fix: … (red on the old parser: 2 FAIL;
  green now). Consequence: the runner sha is part of the binding, so the chain restarts from
  dump.
```

Shape: time, bold headline carrying the verdict, then facts with exits and counts, cause,
fix, consequence for the chain, evidence paths, next action. Landing entries add per-step
timings and the receipt sha:

```markdown
- 09:30 (Sep 27) — **LANDING 2 ON MAIN (<merge-sha>): <components>.** Run6 on runner <ver>
  (receipt sha <sha>…, finalized 09:21): dump 3.5 min, unit 22 s, lint 1.7 min, battery 13.8
  min, integration 4 min, sweep 31.7 min (<fast pass stats>, CLI differential 543/543 agree,
  0 new lint failures, 2 accepted Helm refusals), finalize 4 s. adopted.tsv empty.
```

The builder protocol also defines a one-line landing record:
`time with zone | track | base SHA -> landed SHA | mechanism | red/green tests | receipt SHA |
fixture/flip verdicts | witnesses closed/open/policy-unresolved and coverage | decision or
blocker | evidence links`.

## Trailing `Next:` line

```markdown
Next: <the next action in one sentence>; first run `<exact command>`.
```

Update it with every entry that changes what comes next. A stale `Next:` line is worse than
none: the next agent follows it.

## Track hand-off (`handoff.md` in the evidence dir, ≤ 60 lines, current state only)

```markdown
# <track> hand-off
- Slot/clone: <path>   Branch: <name>   BASE: <sha>   HEAD: <sha>   Target: <dir>
- final.patch: <path> (sha256 <sha>), = `git diff <BASE>..<HEAD>`
- Mechanism: <structural fact>, `<file>:<line>`; representation deleted: <…>
- Helm matrix: <path>   Design review: <path>   Pre-landing review: <path or "not yet">
- Red/green: <test names>; red `<cmd>` exit <n> (<log>), green `<cmd>` exit 0 (<log>)
- Gates: fmt <exit> | task lint <exit> | task lint:fc <exit> | ast-grep <exit> |
  unit <exit, count> | dialect hygiene <exit> | integration <exit, count> |
  dump/battery/sweep: not run — orchestrator
- Fixtures: <n moved, n adjudicated, unmatched flips with chart/path/direction/Helm verdict>
- Witnesses: <closed/open/policy-unresolved>
- Blocker: <exact, or none>
- Next: `<exact command>`
```

## Campaign hand-off (`plan/<campaign>-handoff.md`)

1. "Start here" pointer to the playbook and protocol.
2. Where things stand (ledger, protocol, runner, lock, review registry).
3. Landing sequence: each landing's candidate patches with sha, evidence, reviews, env file,
   and the exact chain commands.
4. Candidates not ready: each with its evidence dir, what is done, what blocks it.
5. Queued work with briefs already written.
6. Standing decisions and hazards.
7. **Status at hand-off time (date, time)** — rewritten at session end: what is on `main`
   with merge shas and receipts, chain duration, open design items, next landings, queued
   user requests, hygiene follow-ups, review debt, what can be deleted.

## Commit subjects for plan/ changes

- `chore(plan): record <event>` for ledger and hand-off updates.
- `docs(plan): <new document>` for a new playbook, protocol or plan.
- Landing merges historically used `Land <components>` with the receipt path and sha in the
  body; follow the `commit` skill for new subjects.
