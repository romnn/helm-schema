# Architecture Review v3 Wave 2 Remediation Contract

This addendum is the frozen remediation reference for Architecture Review v3
Wave 2. It is created and frozen by R0 before implementation begins. The
architecture plan at `plan/architecture-review-v3.md` and the emission plan at
`plan/schema-emission-profiles.md` remain independently frozen.

The remediation steps land before frozen-plan Step 6b.1. A representation-only
step fails its preflight if fixture bytes change. A behavior-bearing step uses
one authoritative clean schema dump, one clean IR dump, the full-depth compiled
acceptance battery, live Helm adjudication for every flip and adjacent state,
the repository gates, and luup2 when schema semantics change.

## Sequence

The default order is R5 → R7 → R4 → R2 → R6 → R1 → R3. R1–R6 may be reordered
only when the progress ledger records the dependency argument. Every
remediation step must land before Step 6b.1.

## R5 — restore comments and documentation

Contract: representation-only, one commit. Fixture bytes must remain identical
because this step changes comments only.

1. Split the fused documentation at
   `crates/helm-schema-ir/src/function_semantics.rs:358-398`. Re-home the
   measured nil-behavior discussion on `NilBehavior` and `nil_aborts` rather
   than attaching it to `strict_parser_operand_pattern`.
2. Restore the coercing-arithmetic guard rationale near
   `crates/helm-schema-ir/src/function_semantics.rs:209-210`: division and
   modulo stay excluded because a zero denominator is a genuine precondition
   that analogy must not widen.
3. Restore the rationale near `function_semantics.rs:160-162` for assigning
   `StringOperands::All` to total stringifiers such as `quote` and `urlquery`.
   Document `string_operand_indices` near line 270, including that
   `argument_count` includes a pipeline input.
4. Restore the piped-ternary role comment near
   `crates/helm-schema-ir/src/expr_call_eval/mod.rs:120-125`: the piped operand
   is the condition, so its strict Boolean contract and effects flow but its
   value is not a result arm. Restore the `deepCopy` copystructure rationale at
   its current arm.
5. Document `CallInvocation` and `PipedOperand` near
   `expr_call_eval/mod.rs:57-66`. The piped value is already evaluated, plays
   Go's final-argument role, and must never be evaluated again.
6. Correct the stale reference near
   `crates/helm-schema-ir/src/fragment_eval/control.rs:647`; the nil-tolerant
   early return is in `record_string_transform_effects`, not the deleted
   `string_call_operand_facts`.
7. Document `is_files_get` near `function_semantics.rs:3-5`. Restore the
   deleted guard-algebra module documentation sentence at the module
   declaration in `crates/helm-schema-core/src/lib.rs`.
8. Document `BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT` near
   `crates/helm-schema-ir/src/analysis_db.rs:1135` with the measured Step 2
   rationale: nearest under-bound width 31, over-bound width 58, and the
   256-leaf blowup preflight.
9. At
   `crates/helm-schema-ir/src/contract_signal_builder/contract_rows.rs:765-779`,
   either restore `checked_sub` short-circuiting or document why
   `saturating_sub` is equivalent: a single-segment path cannot match both
   `metadata` and a metadata field name.

Acceptance criteria:

- Production behavior is unchanged.
- All schema and IR fixture artifacts are byte-identical to the parent.
- Every restored comment explains a current invariant or measured rationale
  and follows the repository's Rust comment style.
- The complete per-step verification protocol passes.

## R7 — ledger corrections and LOC re-forecast

Contract: documentation-only, one commit. Append a `Wave 1 review corrections`
section to `plan/architecture-review-v3-progress.md`; do not rewrite historical
step sections.

1. Correct Step 3 accounting. The frozen estimate covers all of Step 3, while
   the measured Step 3 total was +43 LOC from 61,293 to 61,336; the claim that
   Step 3b landed at the lower boundary of −100..−40 is false.
2. Reproduce the D5 capability-oracle cost band. Either enumerate co-owned
   spans that close the gap between the cited approximately 723 LOC and the
   850-LOC lower edge, or adjust the band.
3. Disclose Step 4 edge-arity semantic deltas: malformed piped `fromYaml`,
   `fromJson`, and `join` spellings changed from decode/shape erasure to
   passthrough/widening, while Helm aborts every such spelling. Disclose Step 5
   micro-deltas: `urlquery` now reports string indices; `mustUniq` and
   `mustDeepCopy` gained `Preserve`; and `mustDateModify` lost its arity-at-
   least-two guard for the string-contract claim.
4. Correct the claim that AST retains only parsing. Public semantic evaluators
   `renders_yaml_fragment`, `fragment_indent_width`, `printf_eval`, and
   `semver_constraint` remain in AST. Record this as known debt for a later
   wave without migrating it now.
5. Add a campaign LOC re-forecast. Wave 1 measured +46 against the frozen
   aggregate band of −910..−390; re-forecast the remaining steps honestly from
   that evidence.

Acceptance criteria:

- The corrections are additive and preserve the Wave 1 historical sections.
- Every corrected measurement has a command that reproduces it.
- The remaining-step re-forecast uses whole-step, like-for-like accounting.
- No production or test code changes.

## R4 — restore battery exactness

Contract: test infrastructure, one commit.

`adjudicate_round74_flip` near
`crates/helm-schema/tests/schema_emission_profiles.rs:1284-1287` currently
accepts `schema_accepts || !rendered`. This globally permits a candidate schema
to accept documents Helm aborts without accounting.

Make accepted-but-Helm-aborting cells an explicit machine-report category. The
run fails when their count exceeds a pre-registered per-step allowance, whose
default is zero.

Acceptance criteria:

- A focused test proves an unregistered accepted/Helm-aborting cell fails.
- A focused test proves an explicitly registered allowance is counted and
  bounded.
- The existing zero-flip full-depth battery passes unchanged with default-zero
  allowance.
- Machine-readable coverage records the category and count.
- No schema or IR fixture byte changes.

## R2 — harden the selection carrier before producers land

Contract: representation-only. All primary changes are in
`crates/helm-schema-ir/src/eval_effect.rs` unless stated otherwise. Schema and
IR fixture bytes must remain identical.

1. `SelectionReachability::exact` near lines 880-892 must demote predicates
   containing approximation to `Approximate { sound_subset }`, mirroring
   `TruthCondition::exact` near `scalar_value.rs:855-861`. It must never mint
   invertible exactness from an approximate predicate.
2. Add `complement`: Always ↔ Never, Exact → negated Exact, and Approximate →
   `Approximate { sound_subset: None }`. Tighten field visibility so consumers
   cannot directly invert a sound subset; `complement` is the natural spelling.
3. `default_primary_selection` in `expr_call_eval/collections.rs` must preserve
   its truth source. The printf-identity arm uses rendered truth, while
   `ValuesPath`, `JsonDecodedPath`, and `FirstTruthy` arms use raw truth. Return
   the source with the classification or carry it in the variant, and extend
   the four-state test to pin per-arm sources.
4. Prefer `Option<SelectionReachability>` on `EvalResult`: `None` means a
   producer has not computed reachability and consumers must abstain. If this
   is measurably too invasive, record that decision and require a per-family
   producer-coverage audit in every Step 6b commit.
5. Canonicalize `approximate(Some(Predicate::True))` to Always. Delete the
   caller-less `Not` implementation for `SelectionPolarity` or give it a real
   caller.

Acceptance criteria:

- Approximate predicates cannot enter `SelectionState::Exact`.
- Approximate complements cannot invert a sound subset.
- Raw and rendered default-primary arms retain distinct truth-source labels.
- Forgotten producers are compiler-visible through the preferred `Option`
  representation, or the measured fallback obligation is explicitly recorded.
- Focused tests cover every state, source, complement, and canonicalization.
- No producer is migrated and every fixture remains byte-identical.

## R6 — catalog and hygiene cleanups

Contract: one commit. Any behavior-bearing change requires full adjudication.

1. `CollectionShape::Sequence` and `CollectionShape::Mapping` are currently
   unconsumed. Consume or delete them. Reconcile the sequence-family routing
   list near `crates/helm-schema-ir/src/expr_call_eval/mod.rs:98-115` with the
   catalog partition, or document the semantic reason they differ.
2. Add a drift check: every function name recognized by a dispatcher
   special-form arm is either present in the catalog or in a maintained,
   test-asserted intentional-exceptions list.
3. Replace the hand-spelled string-transform match near
   `crates/helm-schema-ir/src/fragment_eval/assignments.rs:65-68` with
   `is_string_transform`.
4. Remove the dead `uniq → false` distinction in the sequence nil-aborts flag
   near `expr_call_eval/mod.rs:278-283`. The piped spelling currently passes
   the primary's directness even though the old pipeline arms pinned false;
   either pin false for piped calls or re-document the contract, with a
   regression test.
5. Remove the inline `#[cfg(test)] mod tests` from
   `crates/helm-schema/tests/common/emission_profile_harness.rs:1076-1092` and
   convert `.expect(...)` setup to the required `eyre::Result` style.

Acceptance criteria:

- No unconsumed catalog facet remains without a documented disposition.
- Dispatcher/catalog drift becomes test-visible.
- Sequence nil-abort directness is explicit and regression-tested.
- Test support follows the repository test-layout and error-handling rules.
- Every semantic delta, if any, is disclosed and live-adjudicated.

## R1 — fix the wrong `unset` catalog row

Contract: behavior-bearing, one commit.

The `unset` catalog row near
`crates/helm-schema-ir/src/function_semantics.rs:224` incorrectly claims
`AlwaysAborts`. Helm 4.2.3 renders the indirect spelling
`{{ $x := .Values.absent }}{{ unset $x "k" }}` because deleting from a nil map
is a no-op; only direct access aborts. Change `unset` to
`DirectAccessAborts`, matching the `hasKey` class, and restore an arm comment
with this corrected reason.

Acceptance criteria:

- Live Helm controls independently pin direct access as aborting and indirect
  nil-map deletion as rendering.
- Tests cover deleted, present-wrong-type, truly-consumed, and adjacent
  composite states where applicable.
- Expected schema/IR diffs are pre-registered before the single clean dump.
- Every fixture and acceptance flip is adjudicated against Helm.
- The full behavior-bearing protocol and luup2 pass.

## R3 — harden structural bound-helper widening

Contract: behavior-bearing and optionally split into two commits. The frozen
Step 2 stop branch remains binding.

1. The width budget near `crates/helm-schema-ir/src/analysis_db.rs:1142,1177`
   currently applies only to a binding literally named `config`. Generalize
   the budget over every helper binding and helper/fragment dot value. Repeat
   Step 2's pinned OpenTelemetry chart and preflight measurements. If this
   widens small precise values or cannot reproduce bounded runtime, stop and
   record the `config` scope as explicit debt with a test pinning it as
   intentional.
2. Preservation reads near `analysis_db.rs:1091-1100` currently use
   `ValueKind::YamlSerialized` with unconditional guards. After
   `ContractIr::finalize` near `contract/graph.rs:613`, they are
   indistinguishable from genuine YAML-serialized uses and can alter
   `has_non_control_use` or `used_as_yaml_serialized` in
   `contract_signal_builder/contract_rows.rs:624-666` and
   `resolve_policy.rs:150,332-341`. Give widened-abstention reads a dedicated
   kind or carrier consumed only for closed-root admission. Pin the tightening
   vector: a guard-only third-level member under a widened helper whose
   declared default has a different scalar type must remain accepted when Helm
   renders it.
3. Replace the vacuous name-independence test near
   `crates/helm-schema-ir/src/tests/abstract_value.rs:66-73` with a
   `widen_large_config_binding`-level test containing a non-`config` key.

Acceptance criteria:

- The structural width metric, not a binding name, governs widening unless the
  measured stop branch is taken and recorded.
- The pinned OpenTelemetry chart remains within the Step 2 bounded-runtime and
  memory envelope without widening small precise values.
- Widening-preservation facts cannot masquerade as genuine YAML-serialized
  semantic use.
- The guard-only third-level fixture accepts every corresponding document Helm
  renders.
- Every acceptance change is tested in deleted, present-wrong-type,
  truly-consumed, and composite states, then live-adjudicated.

## Reporting integrity

1. Compare whole-step frozen estimates with whole-step measured deltas.
2. Disclose every semantic delta, including malformed-input and edge-arity
   behavior even when Helm aborts the spelling.
3. A move, inline, or pure-move claim must be token-faithful; record any rewrite
   as a deviation.
4. Re-adjudicate inherited semantic facts before placing them in a new single
   source of truth.
5. Every dossier command must reproduce the measurement it supports.

## Self-adversarial pass

Every behavior-bearing remediation step ends with a recorded self-adversarial
pass after its gates. Probe deleted, present-wrong-type, truly-consumed, and
composite states omitted by the first-order tests. Explicitly hunt for an
inherited fact accepted without live adjudication, a contract satisfied only
in wording rather than intent, and a favorably framed ledger claim. Record the
result even when nothing is found.

## Stop conditions

Stop rather than force past any of these conditions:

- A representation-only step changes fixture bytes.
- A candidate rejects a document Helm renders.
- A gate cannot be repaired within the step's contract.
- Any frozen document changes.

When stopping, land the current step cleanly at a commit boundary when possible
and leave an evidence-backed handoff in the progress ledger.
