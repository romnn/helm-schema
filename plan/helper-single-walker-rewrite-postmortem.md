# Helper Single-Walker Rewrite Postmortem

This note records why the attempted "one helper walker for both helper value
analysis and fragment-output analysis" was backed out.

## What was attempted

The goal was to delete the remaining duplicate helper-body traversal logic
after extracting shared helper control-flow planning.

Two designs were tried:

1. One helper walker carrying both value-analysis and fragment-output state in
   a single runtime object.
2. One generic helper-body walker with two runtime implementations
   (`helper_value_analysis` and `helper_fragment_output_uses`).

Both were intended to reuse the same structural traversal over helper AST
nodes while keeping the domain-specific output logic separate.

## What regressed

The attempted collapse regressed IR fixture equality in real helper-heavy
charts, especially the Bitnami common helper chain:

- `common.tplvalues.render`
- `common.tplvalues.merge`
- `common.labels.standard`

The concrete failure shape was:

- extra pathless scalar rows for helper-derived values such as `commonLabels`
  and `nameOverride`
- extra guards on those pathless rows
- duplicated or widened rows that should instead have been represented only as
  structured fragment output under paths like `metadata.labels`

The failure was visible immediately in
`crates/helm-schema-ir/tests/fixtures/bitnami_redis_networkpolicy.ir.json`
and surfaced through `task test`.

There was also an initial structural bug in the generic walker:

- range iteration count was queried before the runtime had installed its range
  frame, so helper `range` bodies degraded to a single synthetic iteration

That bug was fixed, but the deeper IR regressions remained.

## Why it did not work

The shared control-flow *planning* is valid, but the current runtime contract
is still too weak for a shared *execution* model.

The value pass and fragment pass do not just differ in "what they emit". They
also differ in execution semantics:

- **Assignment semantics**
  - Value analysis always treats assignment expressions as value/dependency
    analysis inputs.
  - Fragment analysis first treats `set`-style mutations as local fragment
    state updates and only falls through to output collection when no mutation
    was applied.

- **Condition semantics**
  - Value analysis records guard paths into `HelperSummary` and applies the
    alternative predicate for `else` branches.
  - Fragment analysis uses predicates only to annotate output metadata and does
    not record the same guard-path facts.

- **Range semantics**
  - Value analysis binds non-exact range variables and uses helper-value dot
    semantics.
  - Fragment analysis skips non-exact variable binding, uses fragment dot
    semantics, and can synthesize destructured mapping-entry fragment outputs
    before evaluating the range body.

- **Output semantics**
  - Fragment analysis depends on `DocumentTracker` site context, mapping-key
    suppression, partial-scalar classification, and fragment placement.
  - Value analysis deliberately ignores rendered document placement and instead
    accumulates dependency/output/type-hint facts.

- **Local state shape**
  - Value analysis owns `local_output_meta` and merges helper-output metadata
    across branches.
  - Fragment analysis owns an output-use stream plus document-attribution
    context.

These differences mean that a generic "enter condition / handle output / enter
range iteration" runtime interface still forces too much semantic coupling.
When the walker was unified, helper-local state and suppression logic leaked
across domains, which is what produced the widened pathless rows.

## What survived and is correct

The shared helper control-flow planning remains a good cleanup:

- `helper_runtime_plan.rs` now owns shared `if` / `with` / `range` planning
  for helper analysis.
- The value and fragment runtimes both consume those shared structural plans.

This keeps the structural static-analysis logic shared without pretending the
runtime semantics are already identical.

## What needs to exist before retrying

Do not retry the single-walker collapse by adding more booleans to the current
runtime interfaces.

The next viable design needs a richer event model than the current runtime
callbacks. At minimum it should make these events explicit:

- assignment expression observed
- local mutation applied
- condition consequence entered
- condition alternative entered
- range frame prepared
- non-exact range binding installed
- destructured-range fragment output synthesized
- output expression observed with document site context

Only after those events are explicit can helper value facts and fragment output
facts become separate sinks over the same structural event stream without
changing semantics.

## Practical conclusion

The correct stopping point for now is:

- keep shared helper control-flow planning
- keep separate helper execution runtimes
- continue simplifying helper facts (`HelperSummary`) and explicit semantics
  boundaries

The full single-walker rewrite is still open, but it needs a better event model
first. Without that, the rewrite is not a simplification; it is a semantic
regression risk.
