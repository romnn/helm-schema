# 0085. Provider schemas are a committed, offline test input

Status: Accepted
Date: 2026-07-17 (F101, fa946305); test isolation completed 2026-07-25 (927f53a3)

## Context

Corpus schemas depended on cache warmth: provider-backed facts (e.g.
Prometheus `ruleFiles`, Minio `service.port`, Datadog PDB) gained their correct
polarity only when the schemas happened to be cached. That violates the
project's cache contract, and full-schema equality blessed the least-informed
output (`plan/chart-corpus-expansion.md` F101). Several suites also read the
shared per-user cache or fetched live from raw.githubusercontent.com
(927f53a3).

## Decision

- `testdata/provider-bundle/` holds the Kubernetes schema cache (v1.29 strict
  for the CLI corpus, plus v1.24/v1.35 for the gen corpus) and the CRD catalog
  cache, including negative-cache records for authoritative 404s.
- Corpus generation and every offline provider in tests point at the bundle
  (or a per-role cold cache root) with downloads disabled
  (`allow_net: false`); a missing bundle fails loudly.
- The bundle is a recipe input of every chart and template artifact (0079):
  any bundle change makes them stale, and existing fixtures must regenerate
  byte-identical unless the change is meant to move them.

## Consequences

- Fixtures are byte-stable regardless of cache warmth or upstream drift, and
  provider-backed facts are part of the committed expectations.
- Pinning the bundle immediately surfaced four warm-path false rejections,
  which were fixed.
- A kind or CRD missing from the bundle silently drops provider-backed facts;
  growing it is an explicit, additive step (generate against a scratch copy
  with network on, commit only new files).

## References

- `plan/chart-corpus-expansion.md` F101 "Fix direction" (~6445–6460) and resolution (~6590–6615)
- fa946305 `… make provider corpus deterministic` (2026-07-17); 927f53a3 `test: isolate suites from ambient caches, network, and binary paths` (2026-07-25)
- `plan/corpus-expansion-v1.md` "Intake" (bundle grew 83→158 Kubernetes and 27→105 CRD schemas, all prior fixtures byte-identical)
- `.claude/skills/corpus-fixtures/SKILL.md` "Adding a chart to the corpus" step 4
- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle"
