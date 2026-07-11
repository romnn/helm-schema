# Unified frontend redesign: global-maximum architecture

Status: IMPLEMENTED (2026-07-04), commits from Stage A through B5+C. All
stages landed fully validated (byte-identical-or-adjudicated fixtures, full
suite, zero lint warnings, luup3 aggregate at every commit). Final state:
one frontend CST (helm-schema-syntax), one abstract interpreter
(fragment_eval, helpers in-domain), one identity projection, one claim
terminal; the line model, AttributionIndex layer, three walkers, the wide
HelperSummary, the emission/prune algebra, and the node_eval runtime are
deleted. Core production Rust LOC: 24,940 — the new design's floor carries
the ~1,700-line frontend the old design lacked while being categorically
simpler downstream. Signoz release generation ~1.53s median (~15% faster
than before the redesign). Remaining floor items (documented, deliberate):
rows flatten guard trees into Vec<Guard> (the public row currency —
collapsing it is public-API surgery); capability conditions live outside
PathCondition (extend the predicate lattice if ever needed); the gen
conditional-append deep-clone is the next perf lever; the double
self-truthy normalization is corpus-pinned fixpoint insurance.

Post-redesign audit (2026-07-11): an end-to-end old-vs-new schema diff over
the luup3 corpus found one real regression the corpus fixtures never covered
— Stage B4's precise root-to-leaf guard stacks put self-truthy members into
placed rows, which the signal builder's overlay lowering treated as
unsupported, poisoning every overlay for the path and pushing typed
constraints to the unconditional base (dict-config helper calls:
`include "common.x" (dict "config" .Values.y)` under call-site guards).
Fixed by (a) folding self-truthy members out of overlay keys (they are
nullability evidence), (b) guard-set algebra in the signal builder
(equal-evidence branch resolution/absorption; complements prove renders
unconditional), (c) lowering truthy guards over any values-declared path
(the "boolean-like flags only" launch policy was the other half of the
regression — its pinned limitation test now pins the fixed behavior),
(d) size-safe emission: same-guard conditionals conjoin, same-content
conditionals disjoin their guards, guard-set disjunctions minimize, and
the helm-truthiness condition is one shared `$defs` entry, and (e) branch
schemas never reject the chart's own declared default (uniform
default-acceptance fallback). Along the way the audit surfaced and fixed a
pre-existing narrowing: an evidence-less declared-empty map with structural
map use (`runtimeEnv: {}` + `range $k, $v := .Values.runtimeEnv`) lowered to
the closed empty object; it is an open map now, while list-ranged sources
with descendant rows keep the exact-empty off-state. Pinned by the
`dict-config` corpus chart and `fragment_dict_config_guards` golden tests.
Core LOC 25,310.

Originally: approved direction (2026-07-03). This supersedes the incremental
consolidation campaign recorded in
`plan/full-core-architecture-simplification-handoff.md`, which closed at the
current model's floor (23,848 production LOC, commit `c46f7af`). That campaign
optimized the plumbing between representations; this plan replaces the
representations themselves.

There are NO backward-compatibility requirements. Internal APIs may change
fully. The only requirements are correctness (generated schemas equal or more
precise, never less), simplicity, determinism, and performance parity.

## Problem statement, first principles

helm-schema is type inference for an untyped template language where the
program's INPUT type is inferred from how the input is used: `.Values.*` flows
forward into typed sinks (K8s/CRD schema positions in rendered manifests), and
sink types flow backward into the values schema, combined with structural
guards, defaults, literal shapes from values.yaml, and comment metadata.

## Why the current model is a local maximum

1. **Dual-document problem.** Templates are YAML-with-holes. The template half
   has a real grammar (tree-sitter go-template); the YAML half is recovered by
   the `StructuralDocument` indentation line model in
   `crates/helm-schema-ast/src/document_attribution.rs`. Attribution tables
   keyed by byte offsets, per-document re-parses in resource identity, and
   header-line scanning are all consequences.
2. **Three interpreters, partial output models.** One language, but a document
   walker (`SymbolicWalker`), a helper walker (`HelperAnalysisRuntime`), and a
   resource-output walker (`ResourceOutputRuntime`). Their outputs (output
   slots, `HelperSummary` rows, string outputs, apiVersion branch trees) are
   partial projections of an artifact that never exists: the abstract rendered
   document.
3. **Guards flattened too early.** Path conditions become flat `Vec<Guard>` per
   row, then get repaired by the sibling/suppression/related-sources algebra
   (~400 lines, probe-pinned). Tree-structured guards make that machinery
   unnecessary by construction.

## Target architecture

```text
0 FRONTEND    one templated-YAML parser -> one CST: YAML structure, control
              regions, and expression holes are first-class nodes with spans
1 BINDING     define index, helper table, file roles (symbol table)
2 EVALUATION  one abstract interpreter, one domain:
              AbstractFragment = Guarded tree of
                { Mapping | Sequence | Scalar(AbstractString)
                | Splice(values_path, meta) | Opaque(taint) }
              helpers = memoized summaries IN THE SAME DOMAIN
              (the AbstractValue expression lattice survives as the scalar
              sub-domain; expr_call_eval transfer functions survive)
3 PROJECTION  resource identity, slot placement, value uses, and guard sets
              are all read off the fragment tree (root-to-leaf path
              conditions); replaces attribution lookups, HelperSummary
              projections, witness lowering inputs, sibling repair
4 CONSTRAINTS per-values-path typed constraints; k8s/CRD provider chain
              unchanged (tri-state oracle contract inviolable)
5 LOWERING    constraints -> JSON Schema; merge algebra kept; deterministic
```

## Stage A: the unified templated-YAML parser

New crate `helm-schema-syntax` (frontend only, no IR deps). Deliverable: parse
one template source into a `TemplatedDocument` CST.

Design:

- **Token layer.** Split source into a line-and-action stream using the
  existing tree-sitter go-template parse for action spans and internals (KEEP
  `TemplateExpr` and `parse_action_expressions` — the expression grammar is
  good). Each source line decomposes into indent, literal YAML text runs, and
  action references. Standalone-action lines (actions whose line holds nothing
  else) are control/output candidates; inline actions are scalar holes.
- **Layout parser.** Recursive-descent over the indent structure (the compiler
  lesson: layout-sensitive parsing like Haskell/Python). Nonterminals:
  `Document*` (split at top-level `---`), `Mapping{entries}`,
  `SequenceItem`, `Scalar{parts: [Text|Hole]}` (partial scalars are parts
  lists), `BlockScalar{suppressed holes}`, `ControlRegion{kind: If|With|Range,
  header: TemplateExpr, branches: [(guard-arm, Body)]}`, `Output(Hole)`,
  `Comment`, `Opaque`.
- **Well-nested assumption + fallback.** Helm control actions almost always
  align with YAML structure boundaries (a branch emits whole entries/items/
  mappings). Parse under that assumption; where an action provably violates it
  (e.g. a branch closing mid-entry), degrade THAT region to `Opaque` with the
  raw span — never guess. Opaque regions must round-trip spans so downstream
  can still attribute conservatively.
- **Spans everywhere.** Every node carries byte spans; the CST must be able to
  answer today's queries: slot at action span, control site path, entire-vs-
  partial scalar, mapping-key position, block-scalar suppression, comment
  position, document boundaries, `items:` sequences.

Validation (the stage gate): an adapter in `helm-schema-ast` builds today's
`AttributionIndex` (output slots + control sites) FROM the CST, replacing
`build_attribution_index`'s StructuralDocument internals. Acceptance:

- IR corpus fixtures byte-identical; all 867+ tests pass; luup3 aggregate
  chart check passes; zero lint warnings.
- The line model (`StructuralDocument` and friends) is deleted in the same
  stage once byte-identity holds. If byte-identity is unreachable for a case,
  the difference must be adjudicated by the fixture-review rules (a strictly
  more precise slot is acceptable WITH written justification; anything else is
  a parser bug).
- Performance: attribution build time must not regress materially (the line
  model is O(n^2); the parser should win).

Sub-stages: A1 parser + tests for the corpus's real templates (dump-based
golden tests inside `helm-schema-syntax`); A2 adapter to `AttributionIndex` +
byte-identity across the IR corpus; A3 cutover (delete StructuralDocument,
line-context code, and the resource-identity re-parses that the CST obviates —
document spans and `items:` descent come from the CST).

## Stage B: the Guarded<AbstractFragment> domain

Build the new interpreter BESIDE the current pipeline (module
`helm-schema-ir::fragment_eval` or a new crate), never by mutating the three
runtimes in place. Both prior failures (attribution rewrite, single walker)
mutated in place; the replacement playbook is parallel-build + corpus-diff +
per-class cutover + delete.

- `AbstractFragment` as above; guards live on branch nodes (`Guarded<T> =
  Vec<(PathCondition, T)>` with an unconditional arm); `Splice` carries the
  values path and meta (defaulted, encoding, provenance).
- One `eval_fragment(cst_node, env) -> AbstractFragment` interpreter,
  parameterized by scope context (document vs helper); helper calls are
  memoized summaries in the same domain keyed like today's
  `BoundHelperCallCacheKey`.
- Projections: (a) resource identity = read kind/apiVersion scalars from the
  fragment per document subtree, branch-aware by construction; (b) value uses
  = walk fragments, emitting (values_path, yaml_path, root-to-leaf guards,
  meta) — feeds the EXISTING EmissionWitness terminal; (c) scalar summaries
  are the degenerate single-node case.
- Cutover: run old and new per corpus fixture; diff ContractIr rows; classes
  that match byte-identically switch immediately; divergent classes get
  fixture-review adjudication (more precise guards/branch schemas are expected
  improvements here — this stage is WHERE branch schemas get better).
- Deletion targets once cutover completes: `HelperAnalysisRuntime`,
  `ResourceOutputRuntime`, `HelperSummary` + output-use rows + meta prune
  algebra, `helper_fragment_output_uses/`, `helper_runtime_plan`,
  `helper_walk_state`, most of `symbolic/output.rs`, `OutputSlot` consumers
  (slots become CST queries), sibling/suppression machinery.

## Stage C: constraints and lowering cleanup

After B, per-path evidence arrives already guard-tree-shaped. Fold
`ContractSchemaSignals` overlays into guard-tree lowerings; simplify
`contract_signal_builder` routing; keep merge.rs and the provider chain.

## Ground rules (unchanged from the campaign)

- Structural analysis first; heuristics bounded and diagnosable; unknown over
  wrong; comments are user-facing metadata; deterministic ordering;
  cache is never correctness evidence; `values.schema.json` is output only.
- Commit only fully-validated stages: fmt, zero-warning check+lint, full
  `task test`, byte-identical-or-adjudicated fixtures, release build, luup3
  aggregate chart check. No partial commits.
- Every deletion pairs with a producer-tracing argument or corpus proof. The
  verified-facts ledger in the old handoff still binds Stage B reviewers.

## Risks

- Templated-YAML parsing is genuinely hard: partial scalars
  (`foo: {{a}}:{{b}}`), block scalars containing actions, flow collections,
  `---` inside branches, ill-nested actions. The Opaque fallback is the safety
  valve; byte-identity against today's behavior is the ratchet.
- Stage B fixture diffs are expected and must be human-adjudicated; budget
  review time. Emission ORDER feeds normalization; preserve it or prove the
  normalized output equal.
- LOC may transiently rise while old and new coexist; only post-deletion
  numbers count.
