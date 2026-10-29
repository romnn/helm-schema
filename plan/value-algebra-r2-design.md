# R2 design — sealed dispatch, the Go-type leaf, and kind consumers under a write-free certificate (rework 1)

Orchestrator, session 7, 2026-10-24. Base: main 747afaba (R1a landed 67ce9e9a; landed.txt = R1a). Value-algebra stage L3
(`plan/value-algebra-r0.md` §5.3; ADR 0069). R2 is SEMANTIC: ADR 0066 clauses 1–3; clause 2b does not apply. IR paths are
relative to `crates/helm-schema-ir/src/`; line numbers are main 747afaba.

Rework 1 merges two design reviews: sol architecture REWORK (agentmux 20261024T140822-f0451cf6) and astra Helm semantics
ACCEPT WITH CHANGES (20261024T140837-4ed81217). Every finding is accepted; the disposition table is §6.

## 0. Decided inputs (do not reopen)

- User D1 no fallback hint after piped `required`; D2 15 `ternary127.selected-*` cells are ADR 0042 policy; D3 sequence B;
  D4 reject raw values numbers at genuine Go `int` parameters on proven-unchanged paths (never float64, never coercing
  `interface{}` parameters; SDK-typed values out of scope). The parameter list is the complete set of genuine `int`
  parameters in Sprig v3.3.0 as pinned by Helm 4.2.3 (§3 F5), which is D4's principle applied exactly.
- Content facts do not change in R2: no coalesce nil fold, no helper-output bytes, no heap, no nilness model.
- Landing 10e ablation: arity checks created NEW failures only through stale guards after writes → gated like kind rejections.

## 1. Scope after review

**In R2:** seal (S), Go-type leaf (T), one catalog owner for return types and parameter signatures (C), write-free
certificate (W), and families F1 ternary condition, F3 required message, F4 string parameters, F5 int parameters (D4),
F6 ternary/required arity.

**Moved whole to R3b (re-tag, stages only move later):** F2, the `required` subject. That covers the 5 AST-lane E rows
(`required_call_subjects` hole_effects.rs:51, `subject_is_statically_helm_empty` :144, `collect_required_subjects` :164,
`record_required_subjects` :362, `record_required_condition` eval.rs:1914) and the negated-truth arm (expr_call_eval/mod.rs:860).
Reasons: (a) the AST lane collects subjects syntactically, independently of execution demand (`set (dict) …` then
`or true (required "m" .Values.x)` keeps the requirement; an evaluator consumer would follow the skipped operand); (b) Helm
aborts on a nil interface or the exact string `""`, so a kind alone cannot decide it (no map/slice kind fails, typed nil passes);
(c) negated truth is unsound. The 7 required dict/list closures leave R2's expectation.

**Also moved to R3b:** determinism facet and memoization eligibility, value-level nilness (typed-nil maps/slices), full
argument-order/state threading (R0 §4.7 items 1, 5, 6). R2 keeps only the positional rule (item 4) and direct-literal
resolution (item 3) that its consumers need.

## 2. Invariants

I1. **Uncertified charts are byte-identical to main.** That covers schema bytes, IR bytes and admission verdicts on every
    chart without a certificate. A diff there is a defect, never an adoption.
I2. **One producer per touched fact (Rule 4).** After R2 there is one leaf type/kind query, one catalog declaration of a fixed
    success type, one owner of parameter types per position, one ternary-condition mismatch query and one certificate
    producer. Legacy publication eligibility on uncertified charts may stay as *policy* over the one query; it is never a
    second classifier.
I3. **Typed arms refine and never replace.** Adding type information never changes existing content arms, truth, selection,
    effects, guards or arm bounds.
I4. **Sound reachability for every new capture**, separate from certification: `and`/`or` short-circuit, `with false`,
    pipeline-first eagerness (`ternary | and false` aborts). The certificate does not repair binding or guard errors, so the
    per-family no-regression requirement is binding.

## 3. Build order (one commit per step; S, T, C, W are byte-identical checkpoints)

0. **Admission cells first (test infrastructure, measured with main's binary).** Add astra's R2 cells (the `r2-*` list in the
   review transcript, ≈50: writer cells that must be uncertified, F1/F3–F6 positive and negative controls, method-before-key,
   uint8/uint64 guards, direct/grouped literals incl. `3+0i` and `3e0`, D4 additions, arity under `and`/`or`/`with`/pipe, the
   heap-write-free binding control) to `testdata/w1-admission/` per its README. Also add an **uncertified twin** (the same
   cell plus one unrelated `set`) of every pre-registered closure witness. Record verdicts with main 747afaba's binary
   (`cell_matrix` + `admission_refresh`); new wrong cells get owner `R1-R3`. This commit must be green on main's code.
   Before any further step, capture main's schema-bytes and IR-bytes hashes for every admission job (the gate compares
   verdicts only) so I1 can be checked per family.
1. **S — seal (byte-identical).** Declare `ScalarValueDispatch` (with `arms` and `complete` private) in `value/mod.rs`; its
   implementation moves to a child module (child modules see the parent's private fields). The 16 slot fields stay private
   to `value`. Reads go through borrowed views (`content()`, `kinds()`, one borrowed view per slot, not getter/setter pairs).
   Copies move as opaque owned transport, writes go through operations (`bind`/`join`/`merge`) inside `value/`. A content view
   is never the transport format, or a copy would lose type-only alternatives.
   `content()` returns `Option<ContentView>`: a legacy empty dispatch (from `stringified()`/`trimmed()`, scalar_value.rs:120/:179)
   stays `Some(empty)` because lower.rs:732, holes.rs:387 and comparisons.rs:292 distinguish it. A non-empty dispatch with only
   type-only alternatives projects to `None`; a mixed dispatch keeps its content arms with `complete = false`.
   Inventory: add checked one-to-one **relocations** to `inventory.py` (+ self-test case) for moved declarations, keeping
   class and stage. Add the missing `ScalarValueDispatch.complete` seal row, and add `GoType` to the semantic roots.
   No `Typed` arm exists yet → byte-identical (clean dump, schema-only battery 0 flips, admission verdicts and hashes identical).
2. **T — Go-type leaf (byte-identical).** `value/kinds.rs`: `GoType` = {`Nil`, `Bool`, `String`, `Int`, `Int64`, `Uint8`,
   `Uint64`, `Float64`, `Complex128`, `Map{elem: Interface|String}`, `Slice{elem: Interface|String}`, `Unknown(KindSet)`};
   kind is derived. No `nil` flag: nilness is R3b, and R2 consumers never decide it. `ScalarValue::Typed(GoType)`.
   A type is attached **where the value is produced** (a literal syntax node, a catalog fixed return), never inferred later
   from content: `GuardValue::Int` content alone does not mean Go `int` (`index "A" 0` is uint8, semver `.Major` is uint64).
   Raw input identities are `Unknown(all)`, except D4's unchanged-document numbers, which are `Float64|Int64` (YAML/JSON
   float64, `--set` int64, `--set-string` string), and only on certified charts. Unsupported types (`[]byte`, `[]int`,
   `common.Values`, `VersionSet`, semver, time) are `Unknown`, never a guessed supported type. `common.Values`
   methods-before-keys holds for every `Values` receiver incl. `.Values.Table` results (`.Values.YAML` is a String;
   `.Values.AsMap.YAML` reads the key).
   **Direct literals keep their syntax until parameter validation.** A direct number literal resolves against the
   parameter type (`trunc 3.0`, `trunc 3e0`, `trunc 3+0i` render). A grouped, bound or piped literal has its runtime type
   (`trunc (3+0i)` aborts on complex128). Do not erase parentheses with `deparen()` on this path. Unsupported numeric forms
   become Unknown, never a definite mismatch. `Literal::Rune` (helm-schema-ast) is a numeric literal; activating it needs
   explicit preservation evidence, because a rune is not an inert content integer.
   Stamping (I3) is one operation in `value/`, a refinement with a written preservation contract. It must not go through
   `EvalResult::set_scalar_dispatch*` (eval_effect.rs:1350-1362 recomputes truth and selection). Type-only alternatives cover
   only previously unresolved coverage. They never consume the 128-arm bound at value/branch_join.rs:119 and never
   displace retained content.
3. **C — catalog owner (byte-identical; consolidation, not addition).** `function_semantics` (ADR 0021, one arm per name)
   becomes the single owner of (a) the success type `Fixed(GoType)` | `Selected`, conditional on normal return and never a
   proof of success (Helm `fromJson`/`fromYaml` → `Map` even for error maps; `mustFromJson` selected; `fromToml` can yield
   int64/time → selected), and (b) parameter types per position with a variadic tail (astra's table: `round(any,int,...float64)`,
   `printf(string,...any)`, `quote`/`squote`/`toString`/`urlquery` take `any`). The overlapping classifiers become **derived
   views or are deleted**: `scalar_value_type_is`/`guard_value_schema_type` (comparisons.rs:336), `OutputSemantics` where it
   restates a return type, and `StringOperands` where it restates `string` parameter positions (strict_operands.rs:170).
   Behavior that is genuinely different (stringification, provenance, parser kinds) stays as separate behavior facets.
   Note: `PredicateSemantics::String` is not "returns Bool" (`urlParse`, `mustDateModify`). No determinism facet in R2.
   Fixed types are stamped once by an outer completion wrapper around the invocation dispatcher, so every early-return path
   passes through one stamp. Byte identity is proven, not assumed: the stamp is observable by setters, fallbacks, joins and
   deferred projection.
4. **W — write-free certificate (no consumer yet).** One immutable certificate per **complete render-tree source set**,
   owned by the shared `ParsedDefinesInner` (analysis_db.rs:34-60). Every per-chart `IrAnalysisDb` references it
   (helm-schema/src/analysis/collection.rs:35/:57 build one db per non-library chart). Any source not in the index — e.g.
   `generate_contract_ir(src)` (symbolic/mod.rs:84) — is scanned too or stays uncertified. Template roles are kept apart:
   executable templates and literal `tpl` programs are scanned; `.Files.Get` payloads are not executable.
   Fail closed: the mutation facet is `ReadOnly | MayWrite | Unknown`, default **Unknown**. Only audited functions and methods
   certify, and receiver-qualified calls are included.
   Writers: `set`, `unset`, `merge`, `mustMerge`, `mergeOverwrite`, `mustMergeOverwrite`, every `sortAlpha`, every
   `call`, semver `Scan`/`UnmarshalText`/`UnmarshalJSON`, `time` `AppendFormat`/`AppendText`/`AppendBinary`, `Values.Encode`,
   and unresolved methods.
   Read-only but alias-producing (do not refute; scan their arguments): `pick`, `omit`, `pluck`, `values`, `get`, `index`,
   `dig`, `slice`, `toStrings`, `.Files.GetBytes`/`Glob`, `Values.Table`/`PathValue`/`AsMap`. Copying non-writers: the
   append/prepend/reverse/rest/initial/compact/uniq/without/concat/chunk families and `deepCopy`.
   Namespace rules: every `define` body is scanned (unused and shadowed too). `include` scans its arguments and the resolved
   template (file templates included); a dynamic or unresolved name refutes. `template` scans its pipeline and target.
   `block` = define + invoke. A literal `tpl` is parsed recursively in a cloned namespace (b01, `r2-tpl-define-writer`).
   A non-literal `tpl`, an unsupported construct or an exhausted bound refutes.
   **A parse failure refutes:** `CachedDefineBody::expressions()` (analysis_db.rs:1333) turns a failed parse into an empty
   list today; the certificate must see the failure.
   Open for review (Q1 below): whether variable reassignment (`$x = …`) also refutes.
   Scope: stock Helm CLI; `Engine.CustomTemplateFuncs` (SDK) is out of scope with D4's SDK exclusion. Cache keys: results are
   db-owned (analysis_db.rs:140) and the certificate is immutable per db, so no extra key field. `seen` stays (:1343). The
   certificate is shared by lint and render modes; render-vs-lint result semantics are not claimed by R2.
   Evidence: a per-chart report (chart → certified | first refuting site `file:line` + reason) over the corpus, the luup2 charts
   and the admission charts.
5. **Families (semantic), one commit each, each attributed on the admission suite with I1 checked per family.** Consumers in
   `value/kinds.rs` read a guarded leaf view (type, content where known, input identity, unresolved coverage) plus the
   argument form (direct literal vs evaluated) and the position computed from the explicit argument count. A NEW rejection
   needs (i) an identity-stable operand — literal, fresh construction, fixed-return result, or (F5 only) a D4 unchanged raw
   document identity, never a variable name after reassignment and never through `default`/selection/conversion
   (`trunc (default 3 .Values.num)` renders with `num: 0`); (ii) a certified chart; (iii) sound reachability (I4).
   "Every alternative under an input predicate fails"; unknown alternatives and incomplete coverage never fail.
   - **F1** ternary condition `require_type(Bool)` — the one query; the syntactic list at comparisons.rs:139 is deleted and
     its eligibility (literal / TotalNumericCast / TypeDescriptor) becomes the uncertified-chart publication policy over that
     query. `record_strict_kind_result` (raw-path contract) is unchanged. A helper returning text `true` is a String and fails.
     Closes condition-10/11/17/18 (certified charts).
   - **F3** required message is a Go `string`: stable non-strings fail even when the subject passes; string-typed results pass;
     a skipped `or` operand never rejects. Active in lint mode too (signature validation).
   - **F4** actual `string` parameters from the signature owner (not from `StringOperands`): stable non-strings fail; no
     Stringer/numeric/byte coercion; `.Values.YAML` and helper output are String. Positional rule for piped values (c01/c02,
     `1 | printf "%v"`).
   - **F5** actual `int` parameters (D4), astra's complete table. Evaluated `Int` passes (`index (until 4) 3`); evaluated
     `Int64`/`Float64`/`Uint8`/`Complex128` fail; direct literals resolve per step 2; D4 raw numbers on unchanged
     identities. `derivePassword` is uint32, `slice`/`index` coerce → excluded.
   - **F6** arity: `ternary` exactly 3 and `required` exactly 2 (a pipeline input counts once), only on reachable executions (I4).
   A family that cannot hold 0 NEW on the admission suite and 0 unmatched flips moves whole to R3b (rows re-tagged).
6. **Last commit:** add `R2` to `landed.txt` (the checker then requires the seals and the 1 remaining R2 E row deleted).

## 4. Admission and landing evidence

- Admission: 0 NEW; every closure re-recorded with `admission_refresh` (shrink-only); a per-family closure table
  (pre-registered vs measured); uncertified twins unchanged; per-job schema/IR hashes identical on uncertified jobs.
- Corpus: one clean dump of the final tree compared with main's clean dump (not just the fixtures), joined to the certificate
  report: diff set ⊆ certified charts. Live battery vs `ROSTER_BASELINE` with matched flips allowed, every flip adjudicated.
  Playbook self-attack items 1–10.
- Carries `r1b/leaf-facts` (c4e352e7, cherry-picked as d038e682 on the R2 branch).
- LOC: reviewers estimate +200…+500 before consolidation. R2 must delete the duplicated type and signature classifiers to
  pull this down; report measured production LOC (`task tokei:core`) separately from test/tooling growth, and justify the
  rest per ADR 0066 clause 3 by the closed classes.

## 5. Questions for the re-review

Q1 (both). Binding writes are not heap writes, yet the certificate does not make old binding facts valid (astra's
`r2-binding-control`; the 10e `stale-local-guard.0`). Choose one: (a) any `$x = …` reassignment also refutes the certificate
(simple, conservative, fewer closures); (b) keep reassignment and rely on I4 plus the admission cells. Recommendation: (a)
for R2, (b) in R3b once frames are exact.
Q2 (sol). Is the reduced R2 (S, T, C, W, F1, F3–F6) the right cut, and is the LOC position defensible under clause 3?
Q3 (sol). Is the C consolidation complete enough to pass Rule 4 without making C a large refactor on its own? If not, which
facets may stay as R3b-tagged rows?
Q4 (astra). Are F1/F3–F6's publication predicates and the identity-stable set now exactly sound? Is any writer still missing
from W's list for the stock CLI?

## 6. Review disposition (rework 1)

| Finding | Disposition |
| --- | --- |
| sol P1-1 typed arms change consumers (set_scalar_dispatch recomputes truth; arm bound) | Accepted: I3, step 2 stamping contract |
| sol P1-2 catalog second producers | Accepted: step 3 consolidation; determinism deferred |
| sol P1-3 F2 changes demand | Accepted: F2 whole to R3b (6 rows re-tagged) |
| sol P1-4 consumer inputs (content, literal form, D4 identity, nilness) | Accepted: leaf view + argument form; D4 identity explicit; nilness out of GoType |
| sol P1-5 certificate program set, parse failures | Accepted: step 4 |
| sol P2-6 seal mechanics + inventory relocation + `complete` + GoType root | Accepted: step 1 |
| sol P2-7 LOC | Accepted: §4 |
| sol P2-8 admission demonstrates verdicts, not bytes | Accepted: step 0 hashes, uncertified twins, dump-vs-dump |
| astra P1-1 method and byte-slice writers | Accepted: fail-closed facet, writer list |
| astra P1-2 `.Files`/`Values` methods, namespace rules | Accepted: step 4 table; R0 §4.10 "Files read-only" corrected in plan at landing |
| astra P1-3 contextual literals | Accepted: step 2 |
| astra P1-4 per-family predicates, reachability | Accepted: I4, step 5 |
| astra P2-5 D4 list incomplete | Accepted: F5 uses the complete table |
| astra P2-6 widening (uint8/uint64, unsupported types, decoders) | Accepted: step 2 |
| astra admission cells | Accepted: step 0 |

## 7. Rework 2 (re-review merge; OVERRIDES §1–§5 where they differ)

Re-reviews: sol turn 2 REWORK (4 findings), astra turn 2 ACCEPT WITH CHANGES (3 findings + Q1/Q4). All accepted.

R2-1 (sol P1-1) **Leaf = type + optional content.** Replace the separate `ScalarValue::Typed(GoType)` arm with one sealed leaf
  `ScalarValue { go_type: GoType, content: Option<ScalarContent> }`; `ScalarContent` is today's `ScalarValue` variants unchanged.
  Type refinement changes the same leaf; it never adds an overlapping alternative. Type-only alternatives (content `None`) have
  their own bound; on exhaustion the type widens to `Unknown` and retained content is kept. `content()` = the arms with content
  (typed-only arms dropped, `complete = false`); the §3 step 1 `Some(empty)`/`None` rule stands. Semver components etc. that main
  carries as `GuardValue::Int` get their real type (`Uint64`) at the production site (expr_eval.rs:401).
R2-2 (sol P1-2, astra P1-1) **C keeps named compatibility views.** The legacy schema-kind comparison (comparisons.rs:336-378,
  JSON-class equality: `typeIs "int64" (len "x")` true on main) stays as an explicitly named compatibility view derived from the
  canonical type owner and is used for every existing truth/guard answer; it is NOT exact Go-type testing and the two are never
  interchangeable. Legacy supplied-operand selection (function_semantics.rs:189/:328, `lower .Values.a .Values.b` selects the last)
  stays as compatibility policy without declaring parameter types. The fallback at comparisons.rs:378 delegates to the owner.
  Controls with uncertified twins before C is called byte-identical.
R2-3 (astra P1-1) **The R2 certificate is write-free, reassignment-free AND type-test-free.** Main's type tests disagree with Go
  (`kindIs "int64" 1` is true on main, false in Helm), and the guard folds to a constant, so a new capture under it is
  undetectably unsound. In R2, any call of `typeIs`, `typeIsLike`, `kindIs`, `typeOf`, `kindOf` anywhere in the render-tree source
  set refutes the certificate (reason `type-test`). Exact type tests come with R3b's canonical query. Report how many expected
  closures this costs. Probes reach-f1..reach-f6, reach-raw (astra turn 2) become admission cells (Helm renders; R2 must not reject).
R2-4 (Q1, both) **Every explicit reassignment refutes:** `TemplateExpr::Assignment` (`$x = …`) anywhere — action bodies, `if`/`with`
  headers, `range $x = …`, unused/shadowed defines, literal `tpl` programs. `:=` declarations and range declarations do not refute
  (their facts must stay scoped correctly; I4 still applies). astra's assign-body/assign-header/assign-range (uncertified) and
  shadow-control/range-control (may certify) become admission cells.
R2-5 (sol P1-4, astra P1-2) **F1 uncertified eligibility is frozen to main exactly:** source forms `String`, `RawString`, `Int`,
  `Float`, `Nil` literals and the main-era `TotalNumericCast`/`TypeDescriptor` membership (freeze the name list in the policy),
  with main's source-expression handling and guard attachment. Runes, complex, `len` etc. never become eligible on uncertified
  charts. astra's uncert-rune/uncert-length become admission cells with byte comparison. Rune activation is a semantic family
  commit (with F1), not part of T. D4's raw-identity typing moves from T to F5 (W must exist first).
R2-6 (sol P2-3) **inventory.py:** the aggregate "surviving producers < deleted rows" check applies to consolidation-only landings
  (clause 2b) only; semantic stages keep declaration, mapping, survival and seal checks. Self-test: one semantic deletion mapped
  to its sole surviving producer passes; self-mapping and missing targets still fail.
R2-7 (astra P2-3, Q4) Catalog: `fromToml` → `Fixed(Map{Interface})` (members may be int64/time); Go `index` does not coerce a float64
  index, Sprig `slice` does — both outside D4. F5 also rejects evaluated `Uint64`. D4 raw numbers are `Float64|Int64` only under a
  numeric-input predicate; `--set-string` is the string alternative; loader-dependent alternatives are never collapsed into one
  JSON guard. Unknown receivers leave method resolution unknown.
R2-8 (sol Q2) LOC: recalculate by region for the reduced scope (F2's deletion credit is gone; the R1b carry gives zero); credit
  only real classifier deletions; justify growth per closed class. Do not pull frames/heap/selection work into R2 to recover LOC.
R2-9 (sol turn 3 P1) **Legacy type-test compatibility is derived from preserved content/fragment projections, not GoType.**
  Main answers `kindIs "string" (len (list "a"))` true (collection length is `AbstractValue::StringSet`, expr_call_eval/mod.rs:617;
  fallback classifies it string, comparisons.rs:406) and `kindIs "string" (len "a")` false (`ScalarValue::Length`, comparisons.rs:351),
  although both are Go `int`. The compatibility view keeps main's dispatch-versus-fallback precedence over the preserved
  content; it is isolated from the canonical Go-type query and never supplies types or evidence to new R2 captures. Both
  expressions are byte-comparison controls (uncertified twins).
R2-10 (sol turn 3 P2) **Legacy content joins use content equality.** The unchanged-entry shortcut (value/branch_join.rs:91) and every
  other legacy join/merge decision compare preserved content only; where equal content meets conflicting types, keep the old
  content arm and widen its type (no new content guards). Full typed equality is used for caches and genuinely typed
  comparisons. Control on an uncertified chart: a local holding literal `1`, then conditionally assigned semver `.Major` (`1`).
R2-11 (orchestrator, 2026-10-24 21:4x, on builder witnesses) **Unconditional-site rule.** The builder found legacy guard errors that
  the certificate cannot exclude: a helper's shadow-declaration snapshot keeps a skipped branch live (float-shadow witness), and
  `fromJson "[]"` (a non-empty error map in Helm) folds to a falsy empty list. A new capture under such a guard is a false
  rejection on a certified chart. Rule: a NEW R2 rejection publishes only at a call site reached with an empty ambient guard set —
  no enclosing if/else/with/range at any level, not inside a skippable and/or operand, and every helper call on the path from file
  top level unconditional. Both witnesses (+ twins) become admission cells owned by R3b. Families with no remaining closures move
  whole to R3b. Supersedes the builder's bounded scope-snapshot proposal (not adopted).

## 8. Scope decision (orchestrator, 2026-10-25 01:2x; user delegated scope calls to "more correct and/or simpler")

R2 STOPS at S + T + C. W (certificate, built measurement-first on r2/write-free-certificate, +693 LOC) certifies 10/165 corpus
charts and 0/44 luup2 charts (873/1,382 admission charts, all synthetic). Under R2-11 a new rejection can only fire at sites that
abort for every input, which real charts do not ship, so F1/F3–F6 would close synthetic cells only, for ~+1,000 temporary LOC
(W + consumers + publication policy) that R3b deletes. Not simpler, not materially more correct. Therefore: no family consumers,
W parked unlanded (branch kept for reference), R2's syntactic-ternary-list delete row and the family obligations re-tagged R3b.
S/T/C (sealed type+content leaf, single catalog owner, compatibility views) plus R3a form the base of the R3b construction branch;
neither R2 nor R3a lands alone (no closures for R2; clause 2b(iv) fails for S+T+C+R3a combined). The next interpreter landing
is the R3b cut-over, which closes these cells with exact heap tracking. Deviation from D3's L3/L4 sequence, recorded in the ledger.
