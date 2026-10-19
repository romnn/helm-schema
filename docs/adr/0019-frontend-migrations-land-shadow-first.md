# 0019. Frontend migrations land shadow-first; deletions follow the last consumer

Status: Accepted
Date: 2026-09-29 (phase-2 plan revision, `plan/schema-bug-hunt-v1-progress.md` 00:15 Sep 29 entry, line ~6245)

## Context

Phase 2 of the rendered-layout refactor (ADR 0018) introduces a new YAML
parser and ownership model beside the existing line model. Reviews of the
first phase-2 plan returned "revise the plan".

## Decision

The revised plan, adopted for phase 2:

- the new parser lands shadow-only, checked by a serialized-byte
  differential at each checkpoint;
- overflow widens; there is no uncertainty fallback for constructs already
  migrated;
- intermediate steps claim only what they prove (2.2 claims "no false
  rejection" for the crossing cells; precision waits for its consumer);
- deletions follow their last consumer (e.g. `BlockScalar.body.end` readers,
  `document_spans` to resource identity);
- 2.1 lands alone, 2.2/2.3 after the in-flight semantic tracks.

## Consequences

- Landing 8 (2f9e60d4) merged phase 2.1 shadow-only: the tree-sitter YAML
  parser, per-arm rendered pieces, arm skeletons mapped to YAML ownership and
  one copy of the YAML 1.1 divergence rules (`yaml_dialect.rs`), with no
  caller outside `helm-schema-syntax` and zero fixture bytes moved.
- Consistent with ADR 0016: the new structure exists and is verified before
  the old model is deleted.

## References

- `plan/schema-bug-hunt-v1-progress.md` line ~6245 (00:15 Sep 29, "Frontend
  phase-2 plan: REVISE-THE-PLAN"), 6871 (21:05 Sep 29, landing 8 merged)
- Commits e2fcde05, 14ce1610, 2f9e60d4 (2026-09-29)
