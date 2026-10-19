# 0004. Simplicity by deletion: one semantic model per fact

Status: Accepted
Date: 2026-06-20 (CLAUDE.md "Design goal: simplicity by deletion", commit bdf480cb)

## Context

helm-schema accumulated several partial models of the same idea:
line-driven trackers beside parser-backed structure, parallel
helper/fragment value shapes, generator-side reassembly of facts the IR
already knew, and fallback layers that survived after the precise path
existed. That made correctness harder to reason about.

## Decision

Precision is reached through a design with fewer moving parts:

- one semantic model over multiple projections of the same fact;
- immutable precomputed structure over mutable incremental state;
- parser-backed structural models over line- or text-shape recovery;
- delete obsolete fallback paths once the structural path is good enough;
- a small explicit bounded fallback over a stack of rescue heuristics.

Between two designs of similar correctness, choose the one with fewer
representations to keep in sync, the one that removes code, and the one
whose invariants are explained by the parsed language structure rather than
source layout. An abstraction that does not make the system both easier to
reason about and more structurally correct is the wrong abstraction.

## Consequences

- Refactors report the production LOC delta (`task tokei:core`, CLAUDE.md
  "Verification gates" item 8).
- Redesigns are judged by what they delete: the unified frontend redesign
  replaced representations rather than the plumbing between them and
  deleted the line model, the attribution layer and three walkers
  (`plan/unified-frontend-redesign.md` status note).
- Deletion must be sound: a deletion without the replacing structural model
  is backed out (ADR 0016).

## References

- CLAUDE.md "Design goal: simplicity by deletion" (introduced in bdf480cb,
  2026-06-20)
- `plan/unified-frontend-redesign.md` status and "Originally" paragraphs
- `plan/document-attribution-rewrite-rollback.md` "Decision rule used here"
