# 0079. Fixtures come from one clean dump of one verified producer

Status: Accepted
Date: 2026-08-02 (one-clean-dump rule in CLAUDE.md); producer landed 2026-09-27 (72ce31a2)

## Context

A stale binary or a mix of dump batches from different code states fakes
progressive drift and pins wrong output (CLAUDE.md). Fixtures were first
regenerated per test binary with `SCHEMA_DUMP=1` dumps into the system temp
dir (`plan/chart-corpus-expansion.md` "Goal", "Ground rules" step 4), which
could not prove that an adoption set came from a single build.

## Decision

- Fixtures are regenerated with ONE clean dump run after the final build;
  dump batches from different code states are never mixed, and never
  assembled from `--only` runs or legacy `SCHEMA_DUMP` files.
- One producer, `corpus_generation` in `crates/helm-schema-test-support`,
  generates every registered artifact (a closed, typed `ArtifactId`
  registry) and writes `manifest.json` last: harness version, a digest of all
  non-test generation sources, recipe input digests, size and sha256 per
  artifact. It refuses to start when the tree no longer matches its compiled
  digest, captures input digests before dispatch, and re-verifies before
  publishing; consumers verify every hash and fail closed on staleness.
- Artifacts are byte-identical to fixtures, so adoption is a plain copy of
  exactly the adjudicated rows.

## Consequences

- Revision: the per-binary `SCHEMA_DUMP` regeneration flow is superseded;
  it was removed from the chart, gen and IR corpora when the producer landed
  and survives only in two tests (corpus-fixtures skill "Legacy dump
  variables").
- Editing tests never invalidates a manifest; editing generation code always
  does. Each worktree needs its own producer run.
- `task test:integration` runs the producer first, so the corpus suite
  cannot pass on stale artifacts.

## References

- CLAUDE.md "Fixture regeneration and flip adjudication" (4d2aba0f, 2026-08-02)
- `plan/chart-corpus-expansion.md` "Goal" and "Ground rules for the implementing agent (round 2)" (the superseded flow)
- `plan/schema-bug-hunt-v1-progress.md` lines ~3735, ~4090–4100 (producer hand-back)
- 72ce31a2 `test(support): produce every corpus artifact once behind one verified producer` (2026-09-27)
- `.claude/skills/corpus-fixtures/SKILL.md` "Mental model", "What the manifest binds", "Regenerating and adopting fixtures"
