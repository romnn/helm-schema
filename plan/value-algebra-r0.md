# helm-schema — value-algebra interpreter, R0 design closure

Status: R0 deliverable, rework 4 (2026-10-22), on branch `r0/value-algebra-design` (base main 65a6d6c7). No production
code. Every `file:line` is against 65a6d6c7; IR paths are relative to `crates/helm-schema-ir/src/`.

Artifacts:
- Helm evidence (scripts with recorded command, output and exit): `plan/value-algebra-r0-probes/`
  - `run.sh`: 42 probes;
  - `selected.sh`: 45 runs;
  - `review.sh`: 50 probes from the three semantics-review rounds.
- Inventory: `plan/value-algebra-r0-inventory/`:
  - `inventory.py`: the fail-closed checker;
  - `classification.tsv`: one reviewed row per declaration;
  - `baseline-65a6d6c7.tsv`: the frozen R0 classification;
  - `landed.txt` and `producers.tsv`.
- ADR draft: `docs/adr/0069-value-algebra-is-the-interpreter-semantic-model.md`.

Admission cells are referenced as **the admission suite (landing 10e)**, which is being assembled on `w1/stage1-min`.
This document names lanes; it does not duplicate cells.

## 0. Review disposition

### Round 3 (rework 4)

astra returned ACCEPT WITH CHANGES, sufficient to start the byte-identical R1. sol returned REWORK on the checker
only. Everything below is accepted.

| Finding | Disposition | Where |
| --- | --- | --- |
| sol P1: baseline rows can disappear | Every baseline key must stay classified; disappearance is valid only through a landed delete stage. Self-test case 4 | §1.4 |
| sol P1: producer targets need not survive (self-mapping passed) | A target must be a scanned candidate or present needle whose delete stage has not landed. Self-test case 5 | §1.4 |
| sol P2: three classifications | `IterationAlternatives` → R; `ValuePathContext.eval_env` → S; `static_semver_numeric_selector` → E (R3a) | TSV |
| astra P1 (pre-R2): pipeline value fills a fixed parameter of a variadic function | Recorded as an R2 obligation. c01 `int 1 \| round 1` gives `1`; c02 `1 \| printf` aborts `expected string; got int` | §4.7 |
| astra P2 (pre-R2): the certificate excludes heap mutation, not legacy guard errors | Recorded. The per-consumer-family no-regression requirement stays binding | §5.3 |
| astra P2 (pre-R3b): fresh allocation is not a closed remainder | Recorded. c03 (`fromYaml .Values.payload`) and c04 (`dict .Values.key 1`) both reach `fail` | §4.4 |

### Round 2 (rework 3)

Both reviews returned REWORK again, with fewer findings. Every finding was accepted, and every astra template
reproduces under pinned Helm (b01-b07b).

| Finding | Disposition | Where |
| --- | --- | --- |
| sol P1: inventory fails open (struct defaults, unused entries) | Accepted. Defaults removed; one explicit row per declaration; consumed-row, frozen-class, producer-mapping and seal checks; the semantic-type closure enumerates wrappers, aliases and variants; `--self-test` proves 3 fail-closed cases | §1.4, §2 |
| sol P1: D3-B allows relabeling and blocks consolidation | Accepted. Frozen baseline classes, reviewed producer mappings, `replaces` for new F/S/E rows, all ADR 0066 gates kept | §6 |
| sol P2: sealing is not deletion | Accepted. `seal` column checked separately; R2 deletes 7 rows and seals 4 | §2, §5.3 |
| sol P2: three misclassifications | Accepted. `FragmentSummary.scalar_dispatch` → K; `ValuePathContext.current_dot_fragment` → S; `document_result_from_expr` → D | TSV |
| sol P3: parent stages, budget arithmetic | Accepted. Parent §4 now points to the refined sequence; R1a −460…−130; R2 −40…+330 | §5 |
| astra P1: literal `tpl` writes (b01) | Accepted. Write-free certificate parses literal `tpl` programs recursively | §5.3 |
| astra P1: `index` returns live slots (b02, b02b) | Accepted. Slot references; only map range snapshots | §4.3-§4.4 |
| astra P1: `.Release`/`.Chart` mutable (b03, b03b) | Accepted | §4.10 |
| astra P1: D4 names round's float parameter (b04, b04b) | Accepted. It is the integer precision parameter | §7 |
| astra P1: nil-map `unset`, absent `index` on typed map, methods before keys (b05-b05c) | Accepted | §4.1-§4.2 |
| astra P1: determinism blacklist (b06: 18 true, 12 false) | Accepted. Audited catalog facet, propagated, default ineligible | §4.6, §4.9 |
| astra P1: pipeline value validated (b07, b07b) | Accepted | §4.7 |

### Round 1 (rework 2)

Two cross-vendor reviews of rework 1 both returned REWORK and both endorsed the direction:
- sol, architecture: agentmux run 20261022T052728-edb2adc2;
- astra, Helm/Go semantics: agentmux run 20261022T052741-f8a5b99b.

Every finding was accepted. The source-derived templates were run under pinned Helm and all reproduce (§3.5).

| Finding | Disposition | Where |
| --- | --- | --- |
| sol P1 R3a byte-identity argument invalid (`Effects::merge` eval_effect.rs:528-536 is order-sensitive; ternary collects condition effects first, comparisons.rs:39; short-circuit refines env, expr_call_eval/mod.rs:1357) | Accepted. R3a now keeps demand, effect-composition order and resolver context; Go order is semantic (R3b) | §5.4 |
| sol P1 inventory incomplete and not reproducible | Accepted. Declaration-level script with classes; added the missed owners | §2 |
| sol P1 R1b fallback, and `joined_truthy_reduction_arms` is not a leaf fold | Accepted. Per-fact derivation contracts; residuals move to R3b explicitly | §5.2 |
| sol P1 R2 seals the wrong boundary | Accepted. Seal `ScalarValueDispatch` itself: private arms, views on every carrier | §5.3 |
| sol P1 cache key loses `seen` (analysis_db.rs:1340, :1004) | Accepted. `seen` stays in the key; partial cycle results are not cached | §4.9 |
| sol P1 independent R1/R3a landings fail ADR 0066 clause 2 | Accepted. Default sequence folds them; amendment shown as alternative (D3) | §6 |
| sol P2 concrete types and temporal boundaries | Accepted | §4 |
| sol P2 recurrence rules not enforceable | Accepted. Privacy first; declaration-qualified lint with fixtures; inventory check | §1.4 |
| sol P2 R3b coupling claim too broad | Accepted. HoleEval collapse → R3a; RootSetState → R1a; earlier semantic flow fixes allowed | §5.4, §5.6 |
| sol P2 LOC optimistic | Accepted. Region-level budgets; `collect_summary_rows` (summary.rs:1026) kept | §5 |
| sol P2 ADR 0069 overstates | Accepted. Counts and arithmetic removed; added snapshot, recursion, widening and performance consequences; renderer non-goal kept | ADR |
| astra P1-1 stale guard over a fresh-operand kind fact | Accepted, probe a01. R2 new facts require a write-free chart | §5.3 |
| astra P1-2 slices have backing identity | Accepted, a02/a02b/a02c | §4.4 |
| astra P1-3 Go kind insufficient (concrete types, uint8, `common.Values` methods) | Accepted, a03/a03b/a03c | §4.1 |
| astra P1-4 typed nil, lookup and grouping modes | Accepted, a04-a04e | §4.2 |
| astra P1-5 catalog outcomes fixed/selected/error | Accepted, a05-a05c | §4.6 |
| astra P1-6 short-circuit and parameter-by-parameter validation | Accepted, a06-a06e | §4.7 |
| astra P1-7 D4 scope | Accepted, a07-a07e. D4 rescoped | §7 |
| astra P1-8 namespaces, frames, `<no value>` boundaries | Accepted, a08-a08c | §4.8 |
| astra P1-9 deepCopy per occurrence, alias per operation | Accepted, a09-a09c | §4.5 |
| astra P1-10 deterministic plus read-only memoization | Accepted, a10 (16/30 succeed, 14/30 abort) | §4.9 |
| astra P1-11 heap initialization follows Helm's graph | Accepted, a11 | §4.10 |
| astra P2-12 Q2 overclaims the consumer defect | Accepted. Claim narrowed | §3.2 |
| astra P3-13 42, not 40, probes | Accepted | header |

## 1. Post-mortem: why the single interpreter grew parallel maps

### 1.1 What the plan specified

`plan/single-abstract-interpreter.md` (June 2026):
- Goal: replace "the current family of parallel symbolic evaluators with one typed abstract interpreter" (:5-7).
- Result type: `EvalResult { value, effects }` (:186-193), with `Effects` as "the single semantic fact carrier" (:196).
  Truth existed only as `Effects.predicates` (:205).
- Invariant (:211-213): facts "flow through `Effects`, not through separate collectors". This governs collectors, not
  the value. Truth, scalar content and Go kind had no home in the value.
- Completion criteria (:995-1011) counted deleted walkers, not stored copies.

`plan/from-scratch-architecture-implementation-audit.md` audits API, engine and provenance (:113, :131, :151, :203,
:242, :464). It never mentions `EvalResult`, truth or dispatch.

### 1.2 How each parallel owner arrived

The earliest commit is found with `git log -S` over `crates/`. Reasons come from the commit body or the ledger.

| Commit | Date | Owner added | Recorded reason |
| --- | --- | --- | --- |
| bd238a46 | 06-09 | `local_output_meta`, `local_set_mutations` | "propagate helper/default/range/fragment semantics" (predates `EvalResult`) |
| 237939c0 | 06-09 | `local_default_paths` | no body |
| 58217d68 | 07-15 | `root_truthy_predicates`, `root_set_mutations`, `root_set_predicates` | F66-F95. `chart-corpus-expansion.md:8199`: "one value plus one truthiness predicate per key" |
| c6d5b8d5 | 07-20 | `root_value_dispatches`, `root_set_value_dispatches` | F107 vault: per-arm literals "join into mutually exclusive, total (condition, literal) arms"; built beside the truth map |
| 18f806ec | 07-25 | `EvalEnv.local_truthy_reductions` | grafana: the reduction "had no channel into expression evaluation" |
| 955e7cc1 | 07-30 | `EvalResult.truth`, `.scalar_dispatch`, `.field_scalar_dispatches`, `EvalEnv.local_scalar_dispatches` | "One `TruthCondition` ladder…": the lanes were unified by adding fields beside `value` |
| caa115ec | 08-02 | `EvalResult.json_payload_truth` | no body |
| 779b64d4 | 08-05 | `EvalResult.selection_reachability` | ADR 0022 option 1, "beside `EvalResult`"; the old lanes stayed as adapters |
| c5aafe86 | 09-05 | `DeferredScalarProjection` | perf: defer helper scalar projection |
| 0bed35bb | 09-07 | `EvalResult.proven_operands` | no body |
| 87034212 | 09-11 | `BindingValue` leaf ownership | deleted two projection types; every map stayed |

### 1.3 Root cause

1. **The value had no slot for the facts consumers needed.** Truth, content and kind arrived through corpus findings.
   Each fix added a carrier beside the value, and each was green on its own finding.
2. **Nothing counted owners or required deletion.** No commit says "temporary", and the first deletion proposal is
   65c7273b (2026-10-21). Consolidations (955e7cc1, ADR 0022, 87034212) stacked a carrier and left the old lanes as
   adapters.
3. **Caches locked the maps in.** ADR 0016 keys summaries by root predicates and dispatches (:22;
   `performance-review-v1.md:218,441-445`). `DeferredScalarProjection` added a second execution for speed.

The W1 stage-1 fence (`value_writes` on four carriers) repeated the pattern, and the reviews found stale reads in each
round.

### 1.4 Rules that make a repeat detectable and costly (enforceable forms)

- **Rule 1, construction privacy (primary).** The value, slot and heap types live in one module (`value/`) with
  private fields and constructors. Consumers get views only:
  - `ScalarValueDispatch::content()` and `::kinds()`;
  - `LocalSlot::binding()`;
  - `Heap::read`.

  A fact stored outside `value/` therefore cannot be built from a value without going through a view, and adding a
  stored fact means editing `value/`, which is a reviewable diff. Every match over leaves and slots is exhaustive
  (CLAUDE.md), so the compiler lists all consumers.
- **Rule 2, fail-closed inventory gate.** `inventory.py` becomes a `task lint` step in R1a and moves to `tools/`.
  It keys every candidate by qualified declaration, never by identifier grep:
  - `field:<file>:<Struct>.<field>`, `variant:<file>:<Enum>::<V>`, `alias:<file>:<Name>`, `fn:<file>:<name>`;
  - `needle:<file>:<text>` for the few re-derivation sites that are not structurally enumerable.

  The candidate set is the closure of the base carriers under "a struct, enum or alias whose body names a semantic
  type", so a new wrapper, alias or variant is enumerated, never skipped. It fails on any of:
  1. a candidate with no explicit row (there are no struct defaults);
  2. a row whose declaration is absent (unused or stale entry) while its delete stage has not landed, or present
     after its delete stage has landed;
  3. a `pub` declaration whose seal stage has landed;
  4. a class that differs from the frozen `baseline-65a6d6c7.tsv`;
  5. a new F/S/E row without `replaces` naming baseline rows deleted by a landed stage;
  6. a deleted baseline F/S/E row without a reviewed `producers.tsv` mapping to a surviving key.

  `--self-test` shows that five cases are each rejected:
  1. an added `Effects.parallel_truths`;
  2. a ghost row;
  3. an F→V relabel;
  4. a declaration removed together with its row before its stage lands;
  5. a landed deletion mapped to itself.

  Baseline rows are never removed from `classification.tsv`. Producer targets must be surviving, scanned
  declarations.
  Moving to `tools/` adds positive and negative ast-grep fixtures.
- **Rule 3, deletion receipt.** Each landing's receipt contains the classification diff, the producer mappings and
  per-class counts. An adapter over a replacement is a row reading a deleted owner. It must either be classified F/S
  (which needs `replaces`) or not exist.
- **Rule 4, review question.** Reviewers ask how many *independently maintained producers* each fact has after the
  change. Immutable snapshots taken at a Helm-defined instant (§4.3) and derived views are allowed. A second producer
  is REWORK (ADR 0066 clause 3).

## 2. Inventory of semantic owners at 65a6d6c7

`python3 -I -B plan/value-algebra-r0-inventory/inventory.py --self-test` exits 0 at 65a6d6c7. It reports 488
enumerated candidates plus 21 needles, giving 509 rows.

Scope:
- struct fields whose type names a closure type or is a String-keyed map or set;
- variants whose payload names one, plus every `AbstractValue`/`ScalarValue` variant;
- aliases;
- the 86 functions that evaluate syntax or helper bodies.

Plain `bool` fields of listed structs, such as `VariableLocalState.truthiness_abstained`, are not candidates; they go
with their struct's rows.

Classes:
- **F** — independently maintained fact store;
- **S** — snapshot or copy of another owner;
- **D** — derived view, borrowed parameter or per-call plan;
- **K** — cache or key;
- **G** — ambient guard context;
- **O** — observation exported to `ObservedFacts`;
- **R** — representation, a type or variant;
- **E** — re-execution or second interpreter;
- **V** — surviving owner;
- **X** — outside the interpreter.

The `delete` column records the stage in which the *declaration* disappears. For example, `EvalEnv.root_truthy_predicates`
is deleted in R1a when its fact moves into the new `RootSlot` row; the new row carries the fact onward to R3b
through `replaces` and `producers.tsv`.

| Class | Rows | Declaration deleted in (rows) |
| --- | --- | --- |
| F | 63 | R1a 27 · R3b 36 |
| S | 55 | R1a 31 · R3a 7 · R3b 17 |
| E | 62 | R2 7 · R3a 13 · R3b 42 |
| K | 14 | R1a 4 · R3b 4 · keep 6 (incl. `seen`) |
| R | 43 | R3b 32 (`AbstractValue` variants, `ProvenOperand`, selection states, iteration aliases, provenance types) · keep 11 |
| D 48 · G 7 · O 117 · V 69 · X 31 | 272 | keep, except D 1 and V 1 (R3b, R1a) |
| **F/S/E deletion obligations** | **180** | R1a 58 · R2 7 · R3a 20 · R3b 95 |
| **Seals (privacy, not deletion)** | 4 | R2: `ScalarValueDispatch.arms`, `EvalResult.scalar_dispatch`, `FragmentSummary.scalar_dispatch`, `HelperArgBindings.scalar_dispatches` |

Owners that rework 1 missed:
- `BoundValueContext.range_domains`/`get_bindings` (bound_value_analysis.rs:23-24): clones of the local maps.
- `HelperArgBindings.{bindings, scalar_dispatches, value}` (expr_eval.rs:461-466): a helper-argument dispatch map.
- `RangeSubject` (value_path_context/mod.rs:35-42) and `RangeValueAlternative` (:48-49): a range-alternatives carrier
  that keeps iterable and member identity apart.
- `RootSetState`'s six maps (fragment_eval/control.rs:3006-3011).
- `RangeIterationAlternative.truth` (control.rs:41), `DotFrame`'s two dot flavors (analysis_db.rs:1116-1117) and
  `IrAnalysisDb.static_root_fields` (:149).
- Second interpreters:
  - `helper_literal_dispatch` (helper_literal_dispatch.rs:25);
  - `HelperOutputEvaluator` (resource_identity.rs:715, apiVersion);
  - the nested file/tpl interpreter (fragment_eval/files.rs:135);
  - the helper-shape recognizers (analysis_db.rs:364, :493, :663, :867, :953);
  - `collect_template_requests_from_helper` (static_file_template.rs:56).
- A correction: the replay at fragment_eval/assignments.rs:434 belongs to `apply_context_copy_value_mutations`
  (:395), not to `apply_helper_scope_set_mutations` (:360).

Independent facts behind the F rows are grouped in the TSV's `fact` column:
- 13 local facts. Each lives in `SymbolicLocalState`, a `VariableLocalState` mirror and `BoundValueContext`, and four
  of them also live in `EvalEnv`.
- 3 root facts (value, truth, dispatch). Each lives in 8 containers: EvalEnv, Interpreter, `*_observed`, Effects,
  FragmentSummary, RootSetState, BoundHelperCallResolution and the cache key.
- 6 result-side facts beside `EvalResult.value`, copied into `HoleEval` and `FragmentSummary`.
- 4 write channels: local writes, object conversions ×4 carriers, set-defaults ×3, root writes.
- 2 dot flavors.
- 1 range-alternatives carrier.

R1a reduces *containers*: the 13 local maps become one slot map. It does not reduce facts; facts become single-owner
only in R1b (where proven) and R3b.

## 3. Helm facts settled by pinned probes (Helm 4.2.3, go1.26.5)

One pitfall: a values key `n:` parses as YAML 1.1 Boolean `false` (ADR 0025), so the probes use `num`.

### 3.1 Q1. Go `int` and Sprig `int64` are distinct; acceptance is decided per parameter (run.sh)

| Probe | Exit | Output |
| --- | --- | --- |
| `trunc (add 1 2) "abcdef"` | 1 | `expected int; got int64` |
| `trunc (len "abc")`, `trunc 3`, direct `trunc 3.0`, `trunc (int 3)`, `trunc (atoi "3")` | 0 | `abc` |
| grouped `trunc (3.0)`; `trunc (int64 3)` | 1 | `got float64`; `got int64` |
| `trunc .Values.num`: YAML `num: 3` / `--set num=3` | 1 / 1 | `got float64` / `got int64` |
| `repeat`, `substr`, `indent`, `abbrev`, `until`, `splitn` over `(add …)` | 1 each | `got int64` |
| `index (list "a" "b") (add 0 1)` | 0 | `b` |
| `eq (add 1 2) 3`; `eq .Values.num 3` YAML / `--set` | 0; 1 / 0 | `true`; `incompatible types … float64 and int` / `true` |
| `range (add 1 1)`; `range .Values.num` YAML / `--set` | 0; 1 / 0 | `[0,1,]`; `range can't iterate over 2` / `[0,1,]` |
| `typeOf`: `add`/`len`/`int`/`1`; `.Values.num`/`int`/`add` | 0 | `int64/int/int/int`; `float64/int/int64` |

### 3.2 Q2. `typeIs "string" (omit (dict "a" 1) "b")` is false

`omit` returns `map[string]interface {}`. `typeIs "string"` is false for both literal and values maps.

Narrowed claim (astra P2-12): this probe proves only that the `typeIs` test is false. In bitnami
`common.tplvalues.render`, `ternary` is eager: both arms execute, and only the selection depends on the test. Whether
Fable's red `tplvalues_render_of_omitted_probe_keeps_fragment_shape` reflects a consumer defect is unproven. That test
joins the R2 admission set with a recorded Helm trace before any expectation changes.

### 3.3 Q3. Piped `required` then `default`

Template: `{{ .Values.name | required "name is required" | default "fallback" }}`.

| `name` | absent / `null` / `""` | `false` / `0` / `{}` / `[]` | `app` / `5` / `{a: 1}` |
| --- | --- | --- | --- |
| Helm | abort | `fallback` | the value |

The fallback is reachable only through non-nil Helm-empty values. This is decision D1 (§7).

### 3.4 Q4. The 15 `selected-*-4..8` cells are closed-root policy (selected.sh)

The cells are `selected-{0,2,3}-{4..8}`. Each override adds `condition`, a key that no template reads.
- The schemas from the main CLI have a root with `additionalProperties: false` and properties
  `enabled`/`global`/`unknown`.
- Without a schema, Helm exits 0 for 15/15.
- With the closed-root schema, Helm exits 1 for 15/15, with `additional properties 'condition' not allowed`. `null` is
  kept in the coalesced document: null deletion applies only to keys that have defaults.
- With the root closure removed, Helm exits 0 for 15/15.
- They are exactly main's 15 ternary127 false rejections, in both sealed baselines.

These are F73-class POLICY-EXCEPTION rows under ADR 0042. W1 ternary falls from 53 to 38.

### 3.5 Review probes (review.sh, 46 probes, all reproduce the reviewer's prediction)

| Id | Template (abridged) | Helm result | Model consequence |
| --- | --- | --- | --- |
| a01 | alias `set $a "c" true`, then `if not (get $d "c")` guards `ternary … (dict)` | exit 0 `ok` | a fresh-operand fact under a stale guard is unsound (§5.3) |
| a02 | `sortAlpha (slice $xs 0 2)` with `$xs := splitList …`; `index $xs 0` | `a` | `[]string` sorts in place through a view |
| a02b | `sortAlpha` over `list "b" "a"` | `b` | `[]interface{}` is copied, not sorted in place |
| a02c | `append $xs 2` | `1/2` | append copies the slots |
| a03 | `set (split "," "a,b") "_0" "x"` | exit 1 `expected map[string]interface {}; got map[string]string` | concrete types, not kinds |
| a03b | `typeOf (index "A" 0)` | `uint8` | the type set includes `uint8` |
| a03c | `set .Values "YAML" true`; `typeOf .Values.YAML` / `index` | `string/bool` | `common.Values` methods win over keys |
| a04 | `$m := fromJson "null"`; `required "r" $m`; `eq $m nil` | `true/map[string]interface {}` | typed nil map: passes `required`, equals nil |
| a04b | `set $m "x" 1` on that map | exit 1 `assignment to entry in nil map` | writes to typed nil abort |
| a04c | `.Values.missing.x` | exit 1 `nil pointer evaluating interface {}.x` | chained field on absent aborts |
| a04d | `(.Values.missing).x \| default "ok"` | `ok` | grouping changes the intermediate |
| a04e | `get .Values "missing"` / `index …` / `typeOf (index …)` | `""/<nil>/<nil>` | `get` gives `""`; `index` gives nil interface |
| a05 | `coalesce $d (unset $d "x") "fallback"` | `fallback` | emptiness is tested after eager args mutate |
| a05b | `required "r" (fromYaml "true")`; type and value | `map[string]interface {}/map[Error:…]` | fixed map return even on error |
| a05c | `typeOf (mustFromJson "true")` | `bool` | Sprig `mustFromJson` is not map-fixed |
| a06 | `and false (fail "x")` | `false` | short-circuit of explicit args |
| a06b | `fail "x" \| and false` | exit 1 `x` | the piped input already ran |
| a06c | `required 0 (fail "later")` | exit 1 `expected string; found 0` | validation per parameter, before later args |
| a06d / a06e | `round 1 0 1` / `round 1 0 (1)` | `1` / exit 1 `expected float64; got int` | direct literals resolve against the parameter type |
| a07 | `set .Values "x" (int .Values.x)`; `trunc .Values.x` | `abc` | D4 must exclude written paths |
| a07b | `slice (list "a" "b") .Values.x`, `x: 1` | `[b]` | coercing interface parameters accept float64 |
| a07c / a07d / a07e | `toYaml\|fromYaml` round trip; `--set-json`; `--set-string` | `float64` / `float64` / `string` | loading kinds |
| a08 | `tpl` defining `h` then including it; outer `include "h"` | `inner/outer` | `tpl` clones the namespace |
| a08b | `len (include "n")` / `len (tpl "<no value>")` | `10/0` | include keeps `<no value>`; tpl strips it |
| a08c | helper `$ \| typeOf` called with `"arg"` | `string` | helper `$` is the argument |
| a09 | `deepCopy (dict "a" $m "b" $m)`; `set $c.a …`; `$c.b.x` | `0` | copies per occurrence |
| a09b | `pick (dict "a" $m) "a"`; `set $p.a …`; `$m.x` | `1` | pick keeps nested references |
| a09c | `range` over `$d` with `unset $d "b"` inside | `ab` | sorted entry snapshot |
| a10 | random helper compared with itself, 30 runs | 16 exit 0, 14 exit 1 | no memoization of nondeterminism |
| a11 | `set .Subcharts.child.Values "x" true`; `.Values.child.x` | `true` | subchart view shares the child map |
| b01 | `tpl "{{ $_ := set .Values \"c\" true }}" .`; `.Values.c` | `true` | literal `tpl` programs write |
| b02 | `$x := index $s 0`; `sortAlpha $s`; `$x` | `a` | `index` binds a live slot, not a frozen scalar |
| b02b | `range $i, $v := $s` with `sortAlpha $s` at `i = 0` | `ab` | slice range variables are live slots, even the current one |
| b03 / b03b | `set .Release "Name" "changed"` / `set .Chart …`, then read | `changed` / `changed` | `.Release` and `.Chart` are mutable maps |
| b04 / b04b | `round 1 0 .Values.cutoff` (`0.5`) / `round 1 .Values.p` (`0`) | `1` / exit 1 `expected int; got float64` | values numbers satisfy float params, not round's int precision |
| b05 | `eq (unset (fromJson "null") "x") nil` | `true` | `unset` on a typed nil map succeeds |
| b05b | `typeOf (index (split "," "a") "missing")` | `"string"` | absent `index` yields the element type's zero value |
| b05c | `.Values.Table` with a `Table: x` key | exit 1 `wrong number of args for Table: want 1 got 0` | every method precedes keys |
| b06 | `shuffle` helper compared with itself, 30 runs | 18 `true`, 12 `false` | determinism must be audited |
| b07 / b07b | `0 \| lower` / `1 \| printf "%v-%v" 2` | exit 1 `expected string; got int` / `2-1` | the pipeline value is validated as the final (variadic) argument |

## 4. Target contracts (concrete types and temporal boundaries)

These contracts refine `plan/value-algebra-interpreter.md` §3. They live in one module, `value/`.

### 4.1 Types

- `GoType` is one of:
  - `Nil` (nil interface);
  - `Bool`, `String`;
  - `Int`, `Int64`, `Uint8`, `Uint64`, `Float64`, `Complex128`;
  - `Map{key: String, elem: Interface | String}`;
  - `Slice{elem: Interface | String}`;
  - `Values` (`common.Values`);
  - `Unknown(KindSet)`.

  Kind is derived from the type. `Unknown(KindSet)` keeps explicit coverage: a type the model does not support is an
  unknown alternative with its possible kinds, never a guessed type.
- Assignability to a parameter is decided on `GoType` (a03: `map[string]string` is not assignable to
  `map[string]interface{}`). Kind consumers (`required`, truth) read the kind.
- Field selection on `Values` resolves *every* method before keys. A zero-argument method returns (a03c: `YAML`). A
  method with parameters aborts on arity even if a same-named key exists (b05c: `Table`). Nested maps are plain maps.
- Typed nil: a `Map` or `Slice` leaf carries `nil: bool`. A typed-nil map:
  - passes `required` and `eq … nil` (a04);
  - aborts on `set` (a04b), while `unset` succeeds and returns it (b05);
  - is falsy.

### 4.2 Presence and lookup

- A field is `Absent`, `Present(value)` (which includes present nil) or `Unknown`.
- Lookup outcomes by mode:
  - `missingkey=zero` (render): an absent key yields the zero value;
  - `missingkey=error` (strict): it aborts;
  - lint mode: `required`/`fail` do not abort. The analysis already keys its cache by mode; every query takes the
    mode.
- Chained field access through an absent or nil intermediate aborts (a04c). After grouping, the intermediate is a
  pipeline value and field access on nil yields nil (a04d).
- `get` on an absent key yields `""`. `index` on an absent key yields the *element type's* zero value: a nil
  interface for `map[string]interface{}` (a04e), `""` for `map[string]string` (b05b). It bypasses
  `missingkey=error`.
- Coalesced input: null overrides delete keys that have defaults; nulls of undeclared keys stay present (Q4).

### 4.3 Values and snapshots (temporal boundaries)

- Leaves (`ScalarValue` successors) are immutable. A leaf carries `GoType`, optional content, and provenance (values
  path plus transform program, ADR 0023). Provenance never substitutes for type or identity.
- References point to heap objects (§4.4). Guarded alternatives reuse `BindingDecision`/`BindingNode`
  (eval_env.rs:183-200) as the single decision structure for expression values, bindings and heap fields.
- Snapshot instants, where a fact is frozen and never re-queried against later state:
  - a guard test, frozen as a predicate over the original coalesced inputs;
  - an output emission (bytes);
  - a serialization (`toYaml`/`toJson`/`toString`): the payload is captured in the term, and payload truth is a query
    on that capture, never on the later object;
  - `include` and `tpl` returns (strings);
  - map range entry: the sorted entry list (a09c). Slice range reads live slots (b02b);
  - capturing a *non-addressable* scalar into a variable.

  `index` on a slice returns an addressable slot. A variable bound to it, and a slice-range variable, is a live
  `SlotRef(backing, i)`, not a frozen scalar (b02).

  Iterable identity and member identity stay distinct facts (value_path_context/mod.rs:29-42).

### 4.4 Heap objects and sequences

- Heap objects are maps and `[]string` backings. A map has known fields (`Present` or a deletion tombstone) plus a
  remainder (`Unknown`, or `Closed`). A fresh allocation is `Closed` only when every key and the whole content are
  statically known. **R3b obligation (astra round 3):** `fromYaml .Values.payload` and `dict .Values.key 1` allocate
  fresh maps with unknown keys (c03, c04 both reach `fail`). An unknown shape never becomes an empty closed map. Setting a key proves non-emptiness. Deleting a key from an open
  map does not prove emptiness.
- `[]interface{}` sequences are immutable values whose elements may reference maps (a02b, a02c).
- `[]string` values (from `splitList`, `regexSplit` and similar) carry a backing identity:
  - `slice` returns a view of that backing, and `index` and range return slot references into it (§4.3);
  - `sortAlpha` on a `[]string` writes through it (a02);
  - any unmodelled function receiving a `[]string` invalidates its backing.
- Allocation identity is `(site, helper-call context, loop-iteration class)`. Allocations inside a loop with unknown
  iteration count are summary objects with weak updates.

### 4.5 Aliasing per operation

| Operation | Effect |
| --- | --- |
| `dict`, `list`, `fromJson`, `fromYaml` | fresh allocation |
| `deepCopy` | fresh allocation per occurrence, breaking internal sharing (a09) |
| `pick`, `omit` | fresh outer map, nested references shared (a09b) |
| `pluck` | fresh outer slice of shared references |
| `append`, `prepend` | copy slots, share elements (a02c) |
| `set`, `unset` | mutate the target |
| `merge*` | mutate the destination; source references may be installed into it |
| `.Subcharts.<c>.Values` | the same object as the parent's `.Values.<c>` (a11) |

An unmodelled writer invalidates every reachable mutable object, including objects inside sequences.

### 4.6 Function catalog outcomes (ADR 0021)

Each catalog entry declares three outcomes separately:
1. Success: either a *fixed* return type, or a *selected/projected* one.
   - Fixed: `include` returns `String`; `add` returns `Int64`; `len`/`int`/`atoi` return `Int`; Helm
     `fromYaml`/`fromJson` return `Map`, even when they return an error map (a05b).
   - Selected/projected: `default`, `coalesce`, `first`, `get`, `index`, `ternary`, `and`/`or`, `pluck`'s elements.
   - A fixed type implies neither success nor fresh content. Sprig `mustFromJson` is selected, not fixed (a05c).
2. Abort conditions: the parameter types (`GoType`) and value conditions such as `required` on nil or `""`.
3. A mutation facet: `set`, `unset`, `merge*` and `sortAlpha` on `[]string` write. A literal `tpl` program is
   scanned recursively (b01). Non-literal programs and unresolved helpers are unknown writers. R2's write-free
   certificate (§5.3) reads this facet.
4. A determinism facet, audited per function. It defaults to *nondeterministic*. Known nondeterministic functions
   include `rand*`, `shuffle` (b06), `uuidv4`, `now`, `date*`/`ago`, `bcrypt`, `htpasswd`, `encryptAES`,
   `gen*`/`derive*`, `lookup`, `getHostByName`, and unsorted `keys`/`values`. A helper is deterministic only if every
   callee transitively is.

Selectors test emptiness *after* every eager argument has run (a05).

### 4.7 Invocation protocol (R3b; R3a keeps legacy order)

1. Execute the pipeline input first.
2. Check arity.
3. For each explicit argument, left to right: evaluate, then validate against the parameter `GoType`. A direct
   literal resolves against the parameter type (a06d); a grouped literal already has its type (a06e). A validation
   failure aborts before later arguments run (a06c).
4. Then validate the already-evaluated pipeline value against the final *argument position*. That position is
   determined by the explicit argument count, not by the signature's last parameter:
   - with enough explicit arguments, it is the variadic element (b07b: `1 | printf "%v-%v" 2` accepts it);
   - otherwise it fills the next fixed parameter, even for a variadic function (c01: `int 1 | round 1` fills `round`'s
     `int` precision; c02: `1 | printf` fills the `string` format and aborts);
   - for a non-variadic function it is the last parameter (b07: `0 | lower` aborts).

   **R2 obligation (astra round 3):** type consumers apply this positional rule.
5. `and`/`or` are special forms: they short-circuit explicit arguments only. The piped input has already run (a06,
   a06b).
6. State threads through each argument.

### 4.8 Frames, namespaces and output boundaries

- A helper call opens a new frame: `$` and `.` are the argument (a08c), and no caller locals are inherited (Go parse
  scope).
- `include` executes in the current namespace and returns its own buffer as a `String` that keeps `<no value>`
  (a08b).
- `tpl` clones the namespace, so definitions made inside it do not escape (a08). Its output and top-level output
  strip `<no value>`, including literal occurrences (a08b).
- Top-level files: all definitions are parsed first, then files render deeper paths first with reverse-lexical ties,
  and `.Template` is replaced per file.
- Output stays *bounded symbolic output* (ADR 0012's renderer non-goal): text terms with unknown spans, accumulated
  after trim handling.

### 4.9 Caching contract (amends ADR 0016)

- A helper result is memoized only if all of these hold:
  1. the helper is deterministic by the audited, transitively propagated catalog facet (§4.6). Unproved means
     ineligible (a10, b06);
  2. it is transitively read-only;
  3. its execution was not truncated by a bound (ADR 0019);
  4. no allocation escapes.
- Key: namespace, mode, evaluated argument identities, the heap snapshot id of reachable objects, and the recursion
  `seen` set (analysis_db.rs:1340). On recursive re-entry (analysis_db.rs:1004) the empty cut summary is not cached
  outside its cycle head.
- A cache entry is an optimization, never an oracle (ADR 0028).

### 4.10 Heap initialization

- The initial graph follows Helm's construction:
  - coalesced chart values, with dependency values nested under `.Values.<child>`;
  - `global` copied at Helm's levels (shallow copies, not universal sharing);
  - `.Subcharts.<c>.Values` aliasing `.Values.<c>` (a11);
  - `.Release` and `.Chart` as *mutable* `map[string]interface{}` heap objects (b03, b03b). The release map is
    shared across chart contexts.
  - `Capabilities` and `Files` are not `map[string]interface{}`, so `set` cannot target them; they are read-only;
  - `.Template` replaced per file (`recAllTpls`).
- Unknown inputs are open objects.
- Objects are interned by Helm's graph identity, not by textual path or structural equality.

### 4.11 Joins and bounds

- A branch forks the heap, sharing identities. The join merges changed fields under the frozen branch predicates.
- Bounds (ADR 0019) widen a field to `Unknown` and an object remainder to `Unknown`; they never drop an alternative.
- Unknown loops widen every touched object and do not keep facts from the first iteration.

## 5. Stages

Each stage lists:
- representations added;
- the inventory rows deleted (`inventory.tsv`, `stage` column);
- the invariant that keeps it safe;
- the admission cells;
- region budgets as tokei code lines. Region sizes are `wc -l` at 65a6d6c7.

Where the deleted region is uncertain, the budget is a ceiling on additions, not a net promise.

### 5.1 R1a — transpose binding and root containers (byte-identical)

- **Adds.**
  - `LocalSlot`: today's `VariableLocalState`, made the stored record, with an `Option` per former map plus the two
    flags.
  - `RootSlot { value, truth, dispatch }`.
- **Deletes.** 58 F/S declarations whose delete stage is R1a, plus 4 K and 1 V:
  - 12 of the 13 `SymbolicLocalState` per-name maps (`fragment_values` becomes the slot's binding);
  - the `VariableLocalState` mirrors and `LocalScopeFrame` copies;
  - `BoundValueContext`'s clones, which become a view of the slots;
  - `RootSetState`'s six maps;
  - the four EvalEnv local maps;
  - the root triples in EvalEnv, Interpreter, `*_observed`, Effects, FragmentSummary and BoundHelperCallResolution;
  - the `HelperArgBindings` dispatch map;
  - the cache-key triple.

  Their facts survive in the new slot rows: `LocalSlot` fields and `RootSlot` maps, classified F or S, each with
  `replaces` naming the deleted rows. `producers.tsv` maps every deleted row to its slot field, for example the
  three `EvalEnv.root_*` maps to `EvalEnv.root: RootSlot` map. Consolidation is demonstrated: about 27 F
  declarations become about 14 slot fields, so the producer count falls.
- **Invariant.** A struct-of-maps and a map-of-`Option`-structs are isomorphic. Per-field join and merge code moves
  unchanged:
  - branch_join.rs;
  - `conjoin_changed_truthy_reductions` (symbolic_local_state/mod.rs:303), including its range-exit rule (:315);
  - `Effects::merge`'s remove-then-extend order (eval_effect.rs:528-536).

  The cache key hashes equal maps equally. Expected result: byte-identical. Any diff is a transcription bug.
- **Admission.** Refactor contract (ADR 0055): fixtures and IR dump byte-identical, schema-only battery 0 flips,
  admission suite verdicts identical. Rule 2's inventory gate lands here.
- **Budget.**

  | Region | Size | Budget |
  | --- | --- | --- |
  | symbolic_local_state/mod.rs | 848 | −150…−300 |
  | branch_join.rs | 402 | −50…−120 |
  | control.rs RootSetState | — | −30…−60 |
  | eval_env.rs root/local fields | 566 | −20…−60 |
  | slot types | — | +80…+120 |
  | **Net** | | −460…−130 |

### 5.2 R1b — binding facts owned by the leaf, per fact (byte-identical or residual)

For each slot field, R1b states a derivation contract. The field moves only when the contract is proven
byte-identical. Otherwise it stays a slot field: its `delete` stage is re-tagged R3b (classes are frozen; stages
may move later, never earlier), and it does **not** count as an R1b deletion.

| Slot field | Derivation contract | Expected |
| --- | --- | --- |
| `default_paths`, `output_meta` | union of `BindingValueMetadata` over the leaves of the `LocalBinding` tree. Today eval_env.rs:24 duplicates them | moves |
| `scalar_dispatch` | per-leaf scalar under each leaf's selection predicate (`BindingNode::Select`) | moves if per-branch arms equal today's join |
| `truthy_reduction` | not a leaf fold. It is an accumulator over control exits: monotone under range (∃), last-write under `truthiness_clears` (mod.rs:315), abstaining under `truthiness_abstentions`. `joined_truthy_reduction_arms` (branch_join.rs:210) joins existing reductions; it does not fold leaves | likely residual → R3b, as heap fields plus a frozen loop summary |
| `typeof_source`, `int_cast_source` | leaf provenance (transform program) | moves if byte-identical |
| `range_domain`, `get_binding`, `range_member_value`, `definite_range_member_value` | per-leaf V provenance for literal domains and member identity | residual → R3b (needs iterable/member identity, §4.3) |

- **Admission.** Refactor contract.
- **Budget.** −50…−200 (deletes `EvalEnv.local_default_paths`/`local_output_meta` plumbing).

### 5.3 R2 — sealed dispatch and the kind leaf (semantic, closes cells)

**Adds.**
- `GoType` with `KindSet` (§4.1, R2 subset: `Nil`, `Bool`, `String`, `Int`, `Int64`, `Float64`, `Map{…}`, `Slice{…}`,
  `Unknown`).
- `ScalarValue::Typed(GoType)`: a known-type, unknown-content leaf.
- Catalog success, abort, mutation and determinism facets (§4.6), stamped once at the invocation return.
- `Literal::Rune` as an `Int` code point (helm-schema-ast).
- `value/kinds.rs` with `require_type` (ternary condition, Go `string`/`int`/`float64` parameters, `required` message)
  and `required_subject`.

**Seal (privacy transitions, not deletions).** Four rows carry `seal = R2`:
`ScalarValueDispatch.arms`, `EvalResult.scalar_dispatch`, `FragmentSummary.scalar_dispatch` and
`HelperArgBindings.scalar_dispatches`. The gate checks that none of them is `pub` after R2. Their declarations are
deleted in R3b.
- `ScalarValueDispatch.arms` (scalar_value.rs:62) becomes private. The type exposes:
  - `content()`, which drops `Typed` arms and sets `complete = false`;
  - `kinds()`.
- Every carrier exposes only these views: `EvalResult`, `HoleEval`, `FragmentSummary::scalar_dispatch()`
  (summary.rs:108), the slots and `HelperArgBindings`. The compiler lists every consumer (37 `EvalResult` reads plus
  the others), and each chooses a view.
- A composed selection (ternary, default, coalesce, and/or) keeps unresolved alternatives as `Unknown` arms, never as
  a narrower complete set.

**Write-free soundness rule (astra P1-1, a01).** A new rejection from a kind fact is published only if:
1. the operand is *identity-stable*: a literal, a fresh construction, or a fixed success type (§4.6); and
2. the chart has a *write-free certificate*, a finite and conservative syntactic proof. Every template, helper and
   library dependency of the chart is parsed. The certificate fails when it finds any of:
   - a call whose catalog mutation facet writes;
   - a `tpl` whose program is not a literal;
   - a literal `tpl` program that fails the same scan when parsed recursively (b01);
   - an unresolved or dynamic `include`/`template`;
   - an exhausted parse or recursion bound.

   Exact semantic write-freedom is not attempted.

   **R2 obligation (astra round 3):** the certificate excludes heap mutation only. It does not prove that legacy
   guard or binding computations are correct. So the per-consumer-family no-regression requirement below stays
   binding: each family is attributed separately and moves whole to R3b if it cannot hold 0 new failures.

Write-freedom makes every guard and binding resolution fresh, so a stale projection cannot gate the consumer. Without
it, the consumer keeps main's verdict exactly:
- main's state-blind raw-path `required` presence (ingress-nginx, okteto, trivy-operator and kyverno are kept);
- main's ternary strict-kind contract on raw paths;
- no message contract for document reads.

Content facts do not change: no coalesce nil fold, no helper-output bytes.

The 7 Codex counterexamples and a01 all contain writes, so main's verdict holds. This is checked as admission cells.

**Deletes.** The 7 E/R2 rows:
- the AST `required` lane: 5 functions and their call sites;
- the negated-truth arm (expr_call_eval/mod.rs:860);
- the syntactic ternary list (comparisons.rs:139).

Seals are not counted as deletions.

**Admission.** 0 new failures on the admission suite (landing 10e), including the 7 Codex counterexamples, a01,
Astra's 69 cells and Fable's controls64. Expected closures are write-free, identity-stable cells:
- required dict/list ×7;
- ternary condition-10/11/17/18;
- `extra-rune`;
- literal non-string messages;
- non-string literals at string parameters;
- `Int64` producers at Go `int` parameters.

Each closure is attributed per family. A family that cannot hold 0 new failures moves whole to R3b.

**Budget.**

| Region | Size | Budget |
| --- | --- | --- |
| hole_effects.rs AST lane | ≈ 330 of 796 | −250…−320 |
| comparisons.rs list + negated-truth arm | — | −50…−80 |
| `value/kinds.rs` + `GoType` | — | +200…+300 |
| catalog facets | — | +80…+150 |
| seal views + consumer edits | — | +40…+100 |
| write-free certificate | — | +40…+80 |
| **Net** | | −40…+330 |

### 5.4 R3a — remove proven-equivalent re-execution and carrier copies (byte-identical)

The invariant is narrowed (sol P1). R3a preserves today's evaluation *demand* (which operands run), effect
*composition order* (`Effects::merge` removes then extends, eval_effect.rs:528-536; ternary merges condition effects
before arm effects, comparisons.rs:39), the *resolver and env* each evaluation sees (including the short-circuit
`constrained_env` refinement, expr_call_eval/mod.rs:1357), and today's operand order. Go order and threaded state are
R3b.

**Deletes.** 20 rows: 13 E and 7 S.
- `static_semver_numeric_selector` (expr_eval.rs:386), which can evaluate, return `None` and force re-evaluation at
  :309.
- 10 condition re-evaluating functions in condition_predicate.rs. Each consumes the hole's existing `EvalResult` only
  where the env and resolver are identical; otherwise the row is re-tagged R3b.
- The double header execution (hole_effects.rs:339).
- `root_values_default_source`'s second `merge` evaluation (root_mutation.rs:301).
- `HoleEval`'s seven copied fields (holes.rs:35-41): a hole holds one `EvalResult`.

**Adds.** The `State` facade over R1's slot maps: `bind`, `read_local`, `read_root`, `write_root`, `write_field`,
`fork`, `join`, `snapshot_id`. All slot access goes through it. This is the seam for R3b, and it changes no semantics.

**Admission.** Refactor contract. **Budget:** −150…−400.

### 5.5 R3b — cut-over: heap, threaded state, frozen guards, flow, in-walk output (semantic)

**Adds.**
- The heap behind `State` (§4.4-§4.5, §4.10).
- The invocation protocol (§4.7).
- Frozen snapshots (§4.3).
- `Flow {Normal, Abort, Break, Continue}`.
- Frames and namespaces (§4.8).
- The caching contract (§4.9).
- Truth, selection, payload truth and dispatch as queries.

**Deletes.** Every remaining row whose delete stage is R3b. At 65a6d6c7 that is 95 F/S/E rows, 32 R, 4 K and 1 D,
plus the slot rows added in R1a and the R1b residuals. These include:
- the 19 `AbstractValue` variants;
- `proven_operands`;
- `DeferredScalarProjection`;
- the fresh helper interpreter;
- the replay functions;
- `RangeSubject`;
- the second helper-output interpreters (`helper_literal_dispatch`, `HelperOutputEvaluator`).

The helper-shape recognizers (analysis_db.rs:364-953) are deleted only where the executed body reproduces their
verdicts. A recognizer that cannot be reproduced stays an explicit, bounded E row with its own obligation entry.

**Admission.** The full admission suite at 0 new failures. Required closures:
- the stateful lanes: store24, guarded-writes6, header10, reviewer42, piped3, supplement3;
- Astra's coalesce-state and nullary cells;
- Fable's 11 B/argument controls;
- the Codex merge, helper-boundary, alias and incidental-write controls;
- review probes a01-a11 as cells;
- the Airflow, comparison, dependency-null, header-fallthrough and output-snapshot controls;
- the coalesce nil fold, `helper-maybe-dict`, `tpl-maybe-text`.

**Budget (additions).**

| Region | Budget |
| --- | --- |
| heap + objects + allocation/summary objects | +900…+1,400 |
| invocation protocol + operand migration (122 `eval_expr*` sites) | +300…+600 |
| snapshots/frozen guards | +200…+350 |
| flow | +150…+250 |
| frames/namespaces | +100…+200 |
| heap init graph | +150…+250 |
| caching contract | +80…+150 |
| bounded loops/widening | +150…+300 |
| provenance preservation | +100…+200 |
| **Total additions** | **+2,130…+3,700** |

**Budget (deletions).**

| Region | Size | Deletion |
| --- | --- | --- |
| abstract_value.rs runtime variants | 1,857 | −700…−1,100 |
| symbolic_local_state + branch_join remainder | ≈ 1,000 | −600…−900 |
| summary.rs projection, keeping `collect_summary_rows` :1026 | 1,260 | −400…−650 |
| root_mutation.rs | 326 | −200…−280 |
| replay functions in assignments/hole_effects | — | −200…−350 |
| `proven_operands` users | — | −250…−400 |
| condition_predicate.rs re-derivation | 3,684 | −300…−800 |
| second interpreters (helper_literal_dispatch 204, HelperOutputEvaluator ≈ 430) | — | −300…−600 |
| **Total deletions** | | **−2,950…−5,080** |

**Net:** +750…−2,950. A positive result needs ADR 0066 clause 3 justification by closed classes. The +5,346 history
is why the receipt reports both totals.

### 5.6 Swapping the store under a running interpreter

**In-binary dual run: rejected.**
- It lands a second interpreter on main, which is the §1 failure.
- It costs CPU in production, or needs a cargo-fc row.
- It adds nothing over a two-binary differential (main CLI against the candidate on fixtures, `IR_DUMP` and witness
  verdicts).

**Chosen design.**
- R3a provides the `State` seam.
- R3b is built on a branch. Its construction commits are explicitly *non-landable*: each must pass the differential
  gate, in which every changed fact is identical, a Helm-adjudicated closure, or a regression, and the regression set
  shrinks monotonically. Only the cut-over landing applies ADR 0066.

**What must land together.** Newly precise state reads, and deletion of every effectful replay that reads pre-write
state:
- `DeferredScalarProjection`;
- the fresh helper interpreter (summary.rs:240-258);
- the replay functions;
- the nested file interpreter without caller state.

The evidence is Astra's +57, the nil-fold false rejections, and Codex turn 0 P1-2.

**What may land earlier.**
- Carrier cleanups that preserve answers (HoleEval in R3a, RootSetState in R1a).
- *Semantic* flow fixes that pass the normal ratchet on their own, for example Flow/abort separation or `and`/`or`
  special forms (§4.7), provided they publish no state-dependent fact.

**Split points examined.**
- (a) By object class: unsafe. Constructed maps carry `$` into helpers (`dict "context" $`), across the replay
  boundary.
- (b) In-walk output before the heap: unsafe. It exposes the stale facts that replay hides.
- (c) Variant deletion after cut-over: rejected (Rule 3, adapters).

## 6. D3: two landing sequences (user decides)

**Sequence A: ADR 0066 unchanged (default, sol P1).** Refactors fold into semantic landings.
1. L1 = R1a + R1b + R2, as one landing with internal commits. Its clause 2 progress is R2's closures.
2. Optional: small semantic flow fixes, each ratcheted on its own (§5.6).
3. L2 = R3a + R3b, the cut-over.

Cost: L1 and L2 are large reviews, and R1's byte-identity is checked inside a semantic landing, which needs a
byte-identity checkpoint commit before R2's first semantic commit.

**Sequence B: amend ADR 0066 with clause 2b, "consolidation-only landing".** Every existing ADR 0066 gate still
applies: no new failure, lists only shrink, clause 3 LOC justification, and the fail-closed chain with its sealed
receipt. Clause 2 is additionally satisfied, without closures, when all of these hold:
- (i) **Frozen coverage.** It uses the same artifact set, admission-suite cell set and `ROSTER_BASELINE` as main. Every
  fixture and the IR dump are byte-identical to main's clean dump, the schema-only battery has 0 flips, and every
  admission verdict equals main's.
- (ii) **Frozen classification.** The inventory gate passes against `baseline-65a6d6c7.tsv`: no class changes, and no
  relabeling of an old F/S/E fact as a new V/D/O row.
- (iii) **Reviewed consolidation.** Every baseline F/S/E row it deletes has a reviewed `producers.tsv` mapping to a
  surviving key. Every new F/S/E row names the rows it `replaces`. The number of distinct surviving producers is
  strictly smaller than the number of deleted rows, or the deleted rows map to producers that already existed.
  R1a's slot maps qualify: each new slot field replaces two or more container fields.
- (iv) **LOC.** `task tokei:core` net production LOC is strictly negative, and it comes from the consolidated region
  named in (iii).

The sequence is then L1 = R1a, L2 = R1b, L3 = R2, L4 = R3a, L5 = R3b.

Recommendation: **B**, with the frozen-classification and producer-mapping conditions. Each landing then has one
kind of evidence. A, which sol prefers as the default, is safe but makes L1 and L2 large reviews.

## 7. Decisions for the user (product questions)

- **D1.** Should a `default` after a piped `required` emit a fallback type hint? Recommendation: no. This is main's
  behavior, and in the algebra it is derived: nil cannot reach the `default`.
- **D2.** Should the 15 closed-root cells be confirmed as ADR 0042 POLICY-EXCEPTION rows (W1 ternary 53 → 38)?
  Recommendation: yes.
- **D3.** Sequence A or B (§6). Recommendation: B.
- **D4 (rescoped per astra P1-7).** Values from CLI-loaded documents (`-f`, `--set`, `--set-json`, `--set-string`)
  never have Go type `int`, so every number aborts at an actual Go `int` parameter. Should the schema reject a number
  there? Scope:
  - only *proven unchanged* paths: the chart is write-free (R2), or the object is unmodified (R3b), because a07's
    `set .Values "x" (int .Values.x)` makes it work;
  - only actual Go `int` parameters: `trunc`, `repeat`, `substr`, `indent`, `nindent`, `abbrev`, `until`, `splitn`,
    and `round`'s integer *precision* parameter (b04b). Never `float64` parameters, which values numbers satisfy
    (b04: `round 1 0 .Values.cutoff` renders), and never coercing `interface{}` parameters (a07b `slice`).

  Product question: values passed natively through the Helm Go SDK can be Go `int` and would render. Are SDK-typed
  values out of scope for ADR 0002's "working configuration"? Recommendation: yes, emit. A values *document* cannot
  carry a Go `int`.

## 8. Risks

- **R3b is still large.** Mitigations: the R3a seam, the differential construction gate, region budgets, and the
  semantic freeze on main (plan §6). Residual risk: rebasing a long branch over landing 10e and later.
- **The write-free rule trades closures for safety.** Cells in charts that write anywhere close only in R3b. The rule
  must be scanned over every template, helper and subchart. Unknown callees count as writers.
- **R1b residuals may dominate**, especially `truthy_reduction`. That is acceptable, because each residual moves
  explicitly to R3b.
- **Helper-shape recognizers** may not be reproducible by execution. Their retention is a bounded, listed exception
  and needs reviewer sign-off at R3b.
- **Cache-key change** (snapshot id plus `seen`) may cost bitnami-style fan-out. Measure under ADR 0064 before the
  cut-over.
- **The inventory scope is declarations matching semantic types.** A fact smuggled through an unrelated type is
  caught only by Rule 1 (privacy) and Rule 4 (review).
