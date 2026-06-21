# Why the larger attribution rewrite was backed out

This note explains exactly why the attempted larger rewrite of
`crates/helm-schema-ir/src/document_projection/tracker/attribution.rs` was not
kept, even though the long-term goal is still a cleaner parser-backed document
attribution model.

## Short version

The rewrite was backed out because it was not yet a strict improvement.

It did remove some old fallback logic, but it did so by introducing a new layer
of probe-ordering and path-preference behavior that was still indirect. That
produced real regressions in fragment attribution, changed schema quality, and
left the design in a more complex transition state rather than a simpler final
state.

The only part that survived was the small sanitizer fix that is both local and
fully validated:

- standalone template actions that structurally inject YAML are now blanked
  during sanitized-YAML preparation instead of being left behind as fake scalar
  placeholder content

That change improved the parser-backed path without changing the overall
attribution semantics.

## What the larger rewrite tried to do

The larger pass attempted to push document attribution further toward a purely
structural model by:

- deleting the old line-driven fallback stack in `attribution.rs`
- removing the `fallback_structural_probe_context` path
- replacing local fallback behavior with broader parser-backed probe search
- changing how fragment paths are selected between mapping-entry and
  sequence-like contexts
- making the sanitized YAML probe logic search more scopes instead of only the
  byte-containing scope

Directionally that sounds right, but the implementation was still missing the
core structural model needed to make those deletions sound.

## Why that was not good enough

The central problem was this:

The old fallback was not actually replaced by a stronger first-class structural
slot model. It was replaced by more probe selection and path preference logic.

That matters because the remaining hard cases are not really "find the nearest
path prefix" problems. They are "what output slot is open here?" problems.

Those are different.

The rewrite still had to infer semantics indirectly from:

- which parsed scope to probe
- whether to prefer a mapping-like or sequence-like result
- whether one path was a prefix of another
- whether a descendant path should be collapsed back to a container path

That is still heuristic control flow around parser results. It is not yet the
desired direct structural model.

## The concrete regressions it caused

These were not theoretical concerns. The rewrite produced real failures.

### 1. Fragment paths collapsed to the wrong parent slot

Examples:

- `conditional_annotations_fragment_stays_under_annotations_path`
- `deployment_annotations_fragment_stays_annotations_map`
- `direct_rendered_annotations_helper_with_empty_default_keeps_open_string_map`

The rewrite caused fragments that should stay attached to
`spec.template.metadata.annotations` to collapse upward to
`spec.template.metadata`.

That is a semantic regression, not a harmless reformat.

It changes which provider schema gets attached to the values path, and that
changes the generated schema shape.

### 2. Nested container fragment targets lost their nested field

Example:

- `deployment_security_context_fragments_keep_nested_provider_paths`

The intended path was:

- `spec.template.spec.containers[*].securityContext`

The rewrite collapsed that to:

- `spec.template.spec.containers`

Again, that is not a cosmetic difference. It weakens the provider evidence and
changes downstream schema lowering.

### 3. Real chart corpus behavior regressed

During the larger pass, full-suite failures showed up in:

- IR fixture tests
- generator fixture tests
- public-surface tests
- CLI chart tests, including Signoz

That is exactly the failure mode we want to avoid for architecture work: a
cleanup that is only locally cleaner but makes the analyzer less correct on
real charts.

## Why the transition state was worse than the original

The attempted rewrite was not just "unfinished". It was structurally awkward in
a specific way.

It mixed:

- the parser-backed structural gap resolver
- sanitized full-document probe insertion
- scope ranking and fallback ordering
- mapping-vs-sequence preference logic
- path prefix reconciliation logic

That is more indirection, not less.

The old fallback code was not elegant, but replacing it with a larger mesh of
probe heuristics does not move the architecture forward. It just changes where
the indirection lives.

That is why it was backed out.

## Why the retained sanitizer change is different

The retained change is this:

- if a standalone template action may inject YAML structure, blank it during
  sanitization instead of leaving a fake placeholder scalar in the sanitized
  document

That was kept because it is a true local improvement:

- it removes an invalid intermediate representation
- it makes the sanitized YAML more faithful to the structural intent
- it does not add new probe ordering rules
- it does not change fragment path selection policy
- it passed the full test suite cleanly

So that change improves the parser-backed infrastructure without pretending to
finish the bigger rewrite.

## The real missing abstraction

The attempted rewrite showed clearly what the actual missing abstraction is.

The missing abstraction is not "more clever probe search".

It is an explicit structural output-slot model.

The clean end state still looks like this:

- one immutable attributed template document
- one pass over parsed template plus sanitized YAML structure
- first-class slot/context facts such as:
  - current path
  - mapping value slot open at this site
  - sequence item slot open at this site
  - whole-scalar slot vs descendant slot
  - block-scalar suppression
  - control-flow body context

Once those facts exist directly, the code can stop guessing between:

- parent mapping vs nested descendant
- sequence container vs sequence item descendant
- byte-containing scope vs structurally preceding open slot

That is the real boundary between a static-analysis model and a probe-ranking
model.

## What should happen before trying again

A future rewrite should not start by deleting more fallback code.

It should start by introducing the missing structural representation and only
then delete compatibility logic.

Concretely, the next serious attempt should first provide:

1. A first-class attributed document model with explicit insertion-slot facts.
2. Structural events or nodes that represent open mapping-value and
   sequence-item slots directly.
3. A clear distinction between:
   - "render here as the whole slot"
   - "render inside the descendant structure below this slot"
4. Tests that pin those slot semantics independently of downstream IR and
   schema generation.

Only after that should the remaining fallback/probe logic be deleted.

## Decision rule used here

The rollback followed this rule:

- keep only changes that are simpler, structurally grounded, and fully
  validated
- back out changes that require new path-ranking exceptions to preserve old
  behavior

By that rule, the sanitizer improvement stayed and the larger attribution
rewrite did not.
