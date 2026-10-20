# 0020. Direct calls and pipeline stages share one invocation model

Status: Accepted
Date: 2026-08-04 (architecture review v3 Step 4); landed 2026-08-05 (`02e8e4a3`)

## Context

Expression evaluation handled `f a b` and `b | f a` through separate code:
`eval_pipeline_with_helper_calls` carried its own semantic match plus nine
pipeline-specific evaluator twins, so a function's semantics could differ
depending on call syntax.

## Decision

Every function application is one `CallInvocation` — function name, explicit
arguments, and an optional already-evaluated piped operand — consumed by a
single semantic dispatcher. The pipeline driver only sequences stages and
evaluates the primary. A pipeline is never rewritten into an AST call that
reorders evaluation: Helm evaluates the pipeline primary before passing it as
the final argument, and eager failures and diagnostic precedence stay pinned.
Constructs whose Go-template evaluation order differs (`and`, `or`, `default`,
helper calls, mutation) keep explicit special forms.

## Consequences

- The pipeline-specific twins and their duplicate operand-fact helpers were
  deleted (`collections.rs`, `comparisons.rs`, `serialization.rs`,
  `strict_operands.rs`).
- Each migrated family is tested in call and pipeline syntax across deleted,
  wrong-type and consumed states; any fixture difference counts as
  behavior-bearing and needs Helm adjudication.
- Prerequisite for the single function-semantics catalog ([0021](0021-one-typed-function-semantics-catalog.md)).

## References

- `plan/architecture-review-v3.md` "Step 4 — normalize direct calls and
  pipeline stages into one invocation"
- `plan/architecture-review-v3-progress.md` "Step 4 — unified call
  invocation"
- Commit `02e8e4a3` (2026-08-05); code:
  `crates/helm-schema-ir/src/expr_call_eval/mod.rs`
