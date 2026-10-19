# 0016. Build the structural model before deleting fallbacks

Status: Accepted
Date: 2026-06-21 (`plan/document-attribution-rewrite-rollback.md`, commit 3230c4e7)

## Context

A larger rewrite of document attribution
(`document_projection/tracker/attribution.rs`) tried to move toward a purely
structural model by deleting the line-driven fallback stack. It replaced the
fallback not with a first-class structural slot model but with more probe
selection and path-preference logic. It caused real regressions: fragments
collapsed to the wrong parent slot (annotations attached to
`spec.template.metadata`), nested container targets lost their field
(`securityContext`), and corpus schemas changed.

## Decision

The rewrite was backed out. The rule applied, and kept for later work:

- keep only changes that are simpler, structurally grounded and fully
  validated;
- back out changes that need new path-ranking exceptions to preserve old
  behavior;
- introduce the missing structural representation first (explicit open
  mapping-value and sequence-item slots, whole-slot vs descendant rendering,
  tests pinning slot semantics), and only then delete compatibility logic.

Only the small sanitizer fix survived: standalone template actions that
inject YAML are blanked during sanitized-YAML preparation instead of being
left as fake scalar placeholders.

## Consequences

- "Delete obsolete fallback paths" (ADR 0004) applies only once the
  structural path is good enough; a deletion that leaves a more complex
  transition state is not a simplification.
- The missing abstraction, an attributed document with explicit slots, was
  later supplied by the unified templated-YAML CST (ADR 0015), after which the
  line model was deleted.

## References

- `plan/document-attribution-rewrite-rollback.md` ("Short version", "The
  concrete regressions", "The real missing abstraction", "What should happen
  before trying again", "Decision rule used here")
- Commit 3230c4e7 (2026-06-21)
