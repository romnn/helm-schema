# Agents

## Goal

helm-schema is **one symbolic interpreter that executes Helm templates** ([ADR 0070](docs/adr/0070-one-symbolic-interpreter-is-the-only-source-of-constraints.md)).
For every value it tracks four things:
- what the value can be;
- which input it came from, read at which moment;
- every transform applied to it (`default`, `merge`, `toYaml`, `tpl`, `include`, …);
- where it lands in the rendered output, under which guard.
Every schema constraint is a consequence of those facts and of what the destination demands (the Kubernetes/CRD schema
at that placement, or a Go/Helm operation that aborts).

Never add a recognizer for a template shape, helper name or chart, a generator-side re-derivation, a store beside the
value, or a post-hoc filter. Earlier rounds did, and paid for it every round after. A missing constraint means an
incomplete mechanism: complete the mechanism, so the whole class closes. A rewrite proves corpus-wide parity with main
before it deletes the old path ([ADR 0071](docs/adr/0071-rearchitectures-prove-corpus-parity-before-deleting.md)).

Recover chart meaning through typed, structural static analysis first. Preserve exact alternatives
and ambiguity; prefer an explicit unknown over a wrong guess. Bounded, explicit heuristics are allowed only
as opt-in policy outside the interpreter's semantics (for example tiered apiVersion inference, ADR 0034), after
precise structural signals run out. They never override structural facts and never stand in for a missing
interpreter mechanism.
Prefer simplicity by deletion: one semantic model per fact, fewer representations and fallback layers.
Generate fast, deterministic, ordered output; key caches by every semantic input. Upstream is
authoritative: caches are fetch-on-miss optimizations, never correctness or capability oracles.
Treat shipped chart/dependency `values.schema.json` as output, never inference evidence; do not
ingest it or intersect it with generated output. Explicit override schemas are caller policy.

## Every session

- Load the [code-quality](.agents/skills/code-quality/SKILL.md) skill before writing or reviewing code: parsers over
  regexes, direct Rust, test layout, `sim_assert_eq!`, error types, lint suppressions, exhaustive destructuring, and
  Helm-derived test expectations.
- Expected test schemas come from Helm's behaviour, main's fixtures or the destination schema, never from snapshotting
  current output.
- Rewrites and producer deletions follow the `rearchitecture` skill. False rejections against main are fixed, never
  rostered.

## Verification gates

For analyzer/generator work, run every applicable gate on the FINAL tree after the last code or
fixture edit and report each result individually. A gate not run is a failed gate; check the
gate command's own exit code, never a trailing pipe's. Follow the
[verification-gates skill](.agents/skills/verification-gates/SKILL.md) for profiles and result interpretation.

1. `cargo fmt --check`
2. `task lint` — fix failures and rerun until the whole workspace compiles under clippy.
3. `task lint:fc`
4. `cargo nextest run --workspace`
5. `task test:integration` — corpus fixtures run ONLY in the nextest `integration` profile.
6. `task test:all` — required before declaring a round final; includes live network tests.
7. When schema semantics change: `cargo install --path ./crates/helm-schema-cli/`, then
   `task -t /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml check:local`.
8. For refactors, report the production LOC delta with `task tokei:core`.
9. For rewrites, and before deleting any constraint producer, run the corpus parity preview against main. That is the
   live battery against `ROSTER_BASELINE` over every chart, plus the unit and integration suites with the dump's
   fixtures adopted ([rearchitecture](.agents/skills/rearchitecture/SKILL.md), ADR 0071).

### Fixtures and acceptance

- Produce one clean dump after the final build; never mix code states ([corpus-fixtures](.agents/skills/corpus-fixtures/SKILL.md)).
- Every fixture byte change requires the battery after that dump exists, with `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` naming it in every mode; committed fixtures are never the candidate ([acceptance-battery](.agents/skills/acceptance-battery/SKILL.md)).
- Adjudicate every corpus acceptance flip against real `helm template` before adopting fixtures; a tightening is a direction, not a verdict.
- Probe coalesced values composed over chart defaults with null-deletion semantics; bare `{}` means every declared key null-deleted, not defaults applied.

## Where to read more

Start with the [ADR index](docs/adr/README.md); read the relevant decisions before changing a subsystem.

- The interpreter and how to change it: [0070](docs/adr/0070-one-symbolic-interpreter-is-the-only-source-of-constraints.md),
  [0069](docs/adr/0069-value-algebra-is-the-interpreter-semantic-model.md), [0071](docs/adr/0071-rearchitectures-prove-corpus-parity-before-deleting.md).
- Analysis and ambiguity: [0001](docs/adr/0001-structural-static-analysis-first.md), [0013](docs/adr/0013-evidence-error-polarity-uncertainty-widens.md).
- Simplicity and phases: [0003](docs/adr/0003-simplicity-by-deletion.md), [0014](docs/adr/0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md), [0015](docs/adr/0015-contract-ir-is-the-semantic-seam.md).
- Inference inputs and parsing: [0004](docs/adr/0004-values-schema-json-is-output-not-evidence.md), [0006](docs/adr/0006-parsers-over-string-heuristics.md).
- Caches and capability probes: [0028](docs/adr/0028-schema-cache-is-fetch-on-miss-never-an-oracle.md), [0029](docs/adr/0029-tri-state-upstream-first-capability-oracle.md).
  Choose canonical probe kinds present from the API version's inception.
- Helm adjudication and values: [0051](docs/adr/0051-real-helm-is-the-correctness-oracle.md), [0054](docs/adr/0054-validate-the-coalesced-values-document.md).
- Dumps and candidates: [0056](docs/adr/0056-one-verified-producer-one-clean-dump.md).
- Performance and landing criteria: [0063](docs/adr/0063-deterministic-output-and-the-performance-law.md), [0066](docs/adr/0066-landing-ratchet.md).

Load the relevant skill for procedures:

- [verification-gates](.agents/skills/verification-gates/SKILL.md): gate ladder, lint policy, nextest profiles, tooling.
- [corpus-fixtures](.agents/skills/corpus-fixtures/SKILL.md): producer, manifests, regeneration and adoption.
- [acceptance-battery](.agents/skills/acceptance-battery/SKILL.md): baseline/candidate setup, flip adjudication, rosters, vacuous runs.
- [landing-workflow](.agents/skills/landing-workflow/SKILL.md): campaigns, ledgers, hand-offs, landing chain and receipts.
- [rearchitecture](.agents/skills/rearchitecture/SKILL.md): rewriting a pipeline part without losing what the old path
  got right (inventory, parity preview, deletion gate, test reconciliation).
- [code-quality](.agents/skills/code-quality/SKILL.md): house rules for Rust code and tests in this repo.
- [delegating-to-models](.agents/skills/delegating-to-models/SKILL.md): which model to use for direction, building,
  reviews and hard decisions, and how to brief it.
- [lane-builder](.agents/skills/lane-builder/SKILL.md): implementing one change fast: direction first, a cheap inner loop
  (focused laws, `--only` dumps, `corpus_replay` in about a minute), CPU before full proof, a Helm-semantics table before
  hand-back.

Persist generic learnings (process, tooling, pitfalls) in this file or the relevant skill as soon as they are confirmed,
so a fresh session never has to rediscover them. Campaign state belongs in `plan/` ledgers, not here.
