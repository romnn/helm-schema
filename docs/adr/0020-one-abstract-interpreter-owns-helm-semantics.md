# 0020. One abstract interpreter owns Helm semantics

Status: Accepted
Date: 2026-06-09 (abstract-values interpreter plan, `bd238a46`)

## Context

Before this decision the IR's `SymbolicWalker` recovered values paths from
template text, local bindings, helper calls and control flow, and
`helm-schema-gen` then reinterpreted the resulting lossy `ValueUse`s with guard
heuristics, `values.yaml` placeholders and ad hoc fragment rules. Several
narrow evaluators (helper binding, fragment binding, expression facts, local
assignment/range, helper output propagation, generator-side nullability and
open-object reconstruction) walked the same `TemplateExpr`/`HelmAst`
structures and duplicated semantics, so one bug class (`with ... default`,
helper-bound `set`, `toYaml` fragments, map ranges, empty placeholder objects)
kept recurring as separate one-off fixes.

## Decision

Helm control-flow and helper semantics live in exactly one typed abstract
interpreter over the parsed AST: `eval_expr` (abstract value plus proven
effects of an expression) and `eval_node` (control flow, YAML sink
attribution, local assignment, mutation ordering, helper summary
propagation). Every Helm construct is modelled once as a transfer function
over one abstract value/effects lattice. It is not a Helm renderer: it tracks
provenance, shape and falsey states, preserves exact alternatives and abstains
when the chart gives no static answer.

## Consequences

- New Helm semantic support is added as an expression/node transfer function,
  never as a generator heuristic; the generator merges provider schema,
  defaults and explicit IR facts and does not re-derive Helm meaning.
- Non-goals: full rendering, concrete evaluation of arbitrary functions,
  exact runtime strings. Render-then-infer was rejected: rendered samples
  cannot cover the guard space, so rendering is a test oracle only
  (`plan/from-scratch-architecture.md` "Alternatives rejected").
- No chart-name rules; decisions come from typed AST structure and explicit
  abstract facts.
- The from-scratch architecture generalised this into commitment 1, "One
  semantic engine"; later reviews (v4, 2026-08-25) confirmed the core as "a
  faithful instantiation of classical abstract interpretation".
- The Helm idiom catalog (`condition_predicate.rs`, `function_semantics`, the
  `expr_call_eval` families) is treated as essential complexity, not as
  duplication to attack.

## References

- `plan/abstract-values-interpreter.md` ("Design goals", "Current
  architectural problem", "Non-goals", "Success criteria"); created in
  `bd238a46` (2026-06-09)
- `plan/single-abstract-interpreter.md` (intro, "Why this exists",
  "Completion criteria"); created in `f8395e8b` (2026-06-10), first routing
  `0786ba06`
- `plan/from-scratch-architecture.md` §4 "Design commitments" item 1
- `plan/architecture-review-v4.md` Part E ("What is genuinely irreducible")
  and "Blue-sky verdict"
- CLAUDE.md "Design goal: structural static analysis first"
