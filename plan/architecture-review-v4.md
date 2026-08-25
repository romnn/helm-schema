# Architecture review v4 — enforcement, single owners, and the performance law

Reviewed tree: `0cbbf9fe` (post-v3-campaign, version 0.0.5). Method: multi-model adversarial
review — four independent read-only reviewers (gpt-5.6-sol via codex; one fable-5 and two opus-5
subagents) with distinct lenses (pipeline/phases, from-scratch design, soundness, seams/data-flow),
every high-severity claim re-verified against source by the orchestrator, disagreements adjudicated
on source evidence, plus direct measurement (release-binary profiling). A second adversarial round
then attacked this plan's own steps and returned nine amendments — two proposed fixes were
themselves unsound as first written (A4, A6), several enforcement moves needed strengthening, and
one whole category (public API / wire compatibility) had been missed by every round-1 lens. All
nine were re-verified against source and are incorporated; the text below is the post-rework state.
A third round (four fresh reviewers, cross-vendor, blue-sky designs explicitly invited under a
four-part bar) produced Part S — the simplification backlog — and the definitive negative result
on alternative architectures recorded there.

## Verdict (unanimous, attacked, upheld)

**Sound shape, needs hygiene. This is the right hill.** The six-phase ladder — load →
abstract interpretation (`fragment_eval`) → contract-signal reduction (`contract_signal_builder`)
→ provider resolution → overlay lowering → canonical emission — matches what a strong compiler
engineer would design from the domain today. All four reviewers independently attacked the
post-Step-8 checkpoint conclusion (builder and overlay lowering are distinct phases) and it held:
the builder must stay provider-free and values-document-free; lowering needs both. **Do not
re-decompose the pipeline. No rewrite. No phase merge.**

What the v3 campaign did not finish is *enforcement*: the codebase invented the right protection
idiom — exhaustive destructuring so a new field/variant refuses to compile until every consumer
decides (`Effects::merge`, `ObservedFacts::absorb`, `ContractIr::finalize`) — and applied it
unevenly. The single most safety-critical fact in the pipeline ("was this value structurally
transformed on the way to its sink?") is spelled by hand five times over a 24-field struct, and
the five spellings **already disagree**. Meanwhile the performance law (<1s typical) is missed by
two orders of magnitude on the worst chart for representation reasons, not essential complexity.

**The local-vs-global answer has two levels.** At the *phase* level: this is the global
maximum — no simpler pipeline shape holds the same semantics. At the *vocabulary* level it is
not yet: the pipeline speaks six-plus overlapping condition/requirement/provenance vocabularies
where three suffice, and that cross-product (vocabulary size × interpreters per vocabulary) is
where production LOC actually lives — and why v3's carrier unifications never deleted what the
estimates promised. Part E carries that trajectory with measure-then-commit spike gates.

Three root causes explain nearly every finding:

1. **Compiler-enforceable invariants left on convention** — hand-listed field walks, if-let
   chains over own enums, hand-mirrored twin functions.
2. **The same rule owned twice** — gen re-deriving judgements IR already made, three
   interpreters of one requirement vocabulary, two spellings of one faithfulness oracle.
3. **Untyped currencies** — values paths as escaped strings manipulated with raw `split('.')`/
   `join(".")`; transform facts as parallel Booleans and path sets; predicates as owned trees
   cloned and re-minimized everywhere.

## Measured performance baseline (orchestrator, release binary, idle host)

Airflow: **112.7s CPU / 2:10 wall**. Symbolicated profile is dominated by
`Predicate::eq`/`partial_cmp`, `Predicate::clone`/drop glue, `Guard`/`String` clones, and
`guard_dnf`'s `minimize_disjunction_by` (a fixpoint with an O(n²) deep-compare pair scan and
sort+clone inside the loop, worst-case ~cubic) running inside `Vec::retain`. DNF minimization is
invoked at ~16 sites *including inside fragment evaluation* — predicates are re-minimized per
merge point, not once per phase boundary. `Predicate`/`Guard` own their `String`s, so every
clone deep-copies and every comparison memcmps. Secondary (gen-side): whole-tree
`SchemaNode ↔ Value` round-trips per materialized path and per-pass hand-rolled raw-JSON walks.

## Part A — confirmed live defects (fix first, each its own adjudicated round)

Ordered by silent-wrongness. Each is Confirmed against source with `file:line` unless marked.

**A1. Provider-schema memo is under-keyed: `stringified` missing from the cache key.**
`ProviderSchemaLookupKey` (`gen/src/path_resolver.rs:40-52`) omits `stringified`, but the cached
candidate embeds `ResolvePolicy::provider_schema_for_value_use`, which branches on
`use_.stringified` for `ValueKind::Scalar` (`gen/src/resolve_policy.rs:227-232`) — the key's own
doc comment states the invariant being violated. Two uses at one slot differing only in
stringification share one preimage: first-resolved pins the schema for both (tighter or wider
depending on order). Fix: add `stringified` to the key; structurally, pass the key type itself to
the policy function so a field read outside the key cannot compile — or hoist provider
resolution into a phase artifact (see C4). *(Adjudication note: one reviewer had listed this key
as verified-clean; overruled by source.)* Effort S/M. Behavior-bearing.

**A2. Dependency values-key modeling: last-write-wins map + the in-flight patch keys on the
wrong thing.** `dependency_metadata_map` computes `values_key = alias.unwrap_or(name)` per entry
but inserts keyed by `dependency.name` (`helm-schema/src/chart/discovery.rs:308-330`); the
vendored-chart lookup (`:140-149`) can produce only one values key per chart name, so with
`{name: redis, alias: cache}` + `{name: redis, alias: queue}` the `cache.*` subtree silently
loses its context/activation today. The **uncommitted working-tree patch**
(`reject_duplicate_dependency_names`) counts duplicates by `name`, which (a) rejects the
one-vendored-chart-N-aliases pattern Helm handles deterministically, and (b) does not catch
distinct names sharing one alias (`{name: redis, alias: cache}` + `{name: valkey, alias: cache}`),
which silently unions two subcharts under `.Values.cache.*`. The Step-11b-measured Helm
nondeterminism concerns multiple *installed* entries sharing one internal name — a third case.
Policy: reject duplicate **values keys** always; for same-name-N-aliases either model one
vendored chart under N keys (real feature) or reject interim with an honest "not yet supported"
diagnostic (not an ambiguity claim); keep rejecting the measured installed-entry ambiguity.
**Round-2 correction:** do NOT re-key the sole discovery map by `values_key` — the installed
vendored chart is identified by internal `sub_name` and looked up by that name
(`discovery.rs:128,140`), so an alias-keyed sole map breaks the ordinary
`{name: redis, alias: cache}` lookup. Keep two indexes: uniqueness is validated through a
values-key index, while name→metadata association remains for installed-chart lookup; full
N-alias support requires `name → Vec<DependencyMetadata>` with one context per alias.
**Adjust the in-flight patch before it lands.** Effort S (validation + policy) / M (N-alias
modeling). Behavior-bearing.

**A3. Values-path currency corrupted at ~15 raw-string sites.** The codec escapes `.` and `\`
inside segments (`core/src/value_path.rs`), but requirement lowering re-encodes parents with raw
`segments.join(".")` (`ir/src/contract_signal_builder/requirements.rs:224,265`;
`ir/src/expr_call_eval/traversal.rs:135-138`), and `collapse_layered_truthy_gates` splits with
raw `split('.')`/`strip_prefix` (`ir/src/contract_signal_builder/conditional_overlays.rs:79-146`);
`contract_signals.rs:861-873` uses the correct helper and the wrong `format!` in one function.
Any dotted key (`podAnnotations."prometheus.io/scrape"`, dig keys like `"config.yaml"`) lands
abort-grade requirements on phantom paths or silently drops overlay-guard collapses. Fix now:
sweep the wrong sites to the codec helpers — **scoped to dot/backslash repair only**. The
sibling defect — the literal segment `"*"` is unescaped and indistinguishable from the
range-member wildcard (`gen/src/schema_tree.rs:1233`) — **cannot** be fixed by codec
substitution (the codec has no `*` spelling); it belongs to B4b's typed `EachMember` segment.
Effort S (sweep). Behavior-bearing (fixes real charts).

**A4. One requirement vocabulary, three disagreeing interpreters.**
`requirements_allow_runtime_kind` demands `required == schema_type` strictly
(`gen/src/path_resolver/fail_requirement.rs:283-290`) while `requirement_admits_runtime_type`
widens `number ⊇ integer` (`gen/src/overlay_lowering/member_projection.rs:655-665`); a third
spelling picks per-member vs whole-value mode via a `bool`. Concrete false rejection: a ranged
collection with `SchemaType("number")` members emits an integer lane of
`{"type":"integer","maximum":0}` though every positive count renders. **Round-2 correction —
one function is NOT enough**, because the two questions differ in quantifier: JSON-kind domain
admission is set reasoning (`number ⊇ integer` is right there), but whole-integer-lane
admission for `Members` targets is ∀-quantified over the members a count produces — `range` over
N yields `0..N-1`, and member `0` is Helm-falsy, so `HelmTruthy`/`HelmFalsy`/`NotEquals`
requirements are value-sensitive and cannot be answered by kind membership (today's
`HelmTruthy => schema_type != "null"` at `fail_requirement.rs` is exactly that unsound
shortcut). Fix: two owned operations — `admitted_json_value_kinds` (disjoint `Integer` /
`NonIntegerNumber` classes, `number` expanding to both) for domain reasoning, and a
value-sensitive `integer_range_constraint` returning `Any` / bounded max / `None` / abstain for
the range lanes. Derive schema spellings from these; never derive both from one kind set.
Effort M. Behavior-bearing.

**A5. Faithfulness oracle asymmetry certifies an approximation as exact.**
`condition_lowering_is_faithful`'s `Field|Selector` arm returns
`!paths_for_expr(expr).is_empty()` (`ir/src/value_path_context/condition_predicate.rs:144-149`)
without the `MergedLayers` check its own `Variable` arm performs (`:156-172`) — whose comment
says the all-paths conjunction "is not faithful for them." Negating an unfaithful conjunction
weakens the abort requirement (accepts documents Helm aborts). Fix now: mirror the check for
**both** unfaithful value shapes — `MergedLayers` *and* `FirstTruthy` (whose all-path fallback
is likewise documented unfaithful at `condition_predicate.rs:1600`); the arm must refuse
whenever the exact decoder abstains, not only for one shape. Structural fix is B3 (one decoder
returning `Exact|Approximate`, deleting the twin table). Effort S. Behavior-bearing
(reachability Suspected; the asymmetry is Confirmed).

**A6. gen recomputes the integer-range lane from a different fact subset, dropping
`has_json_decoded_range_use`.** IR bakes `!destructured && !json_decoded` into
`Members { allow_integer }` (`ir/.../requirements.rs:633`); gen independently derives the
overlay's range domain from three *other* facts and ignores `has_json_decoded_range_use`
(`gen/src/overlay_lowering.rs:538-541`). Where no IR `Members` arm exists to rescue the
conjunction, a JSON-round-tripped ranged path accepts `x: 3` though Helm aborts ranging
`float64(3)`. **Round-2 correction — the "add the missing conjunct" quick fix is itself
unsound:** overlays receive path-global facts (`conditional_overlay_evidence(global_facts, …)`,
`contract_rows.rs:166`), so the conjunct would kill the integer lane in a branch that ranges
the path raw. The structural fix IS the immediate fix: IR publishes a branch-scoped
`RangeDomain` on the overlay evidence (guarded range facts are already recorded per branch at
`contract_rows.rs:1022`) and gen only renders it — single owner, branch-local. Add
complementary-guard coverage proving integers survive only in the raw branch. Effort M.
Behavior-bearing.

**A7. Chart activation clears structurally known facts.** `append_guards_to_all_uses` drops
`values_default_sources` and `values_root_overlay_prefixes` for conditionally-activated
dependencies because the signal vocabulary cannot express them conditionally
(`ir/src/contract/graph.rs:126-130` — the comment admits the gap). Deliberate abstention, but it
loses precision for every `condition:`/`tags:` dependency. **Round-2 correction:** one
`GuardedFact<T>` wrapper is not enough because the two channels have different consumers —
default sources are eagerly applied into ONE composed values document before any guard
evaluation (`gen/src/emission_plan.rs:84`), so a guarded default cannot simply ride the
composition. Split: guarded root-overlay projection conjoins the activation predicate onto the
cloned implications; guarded default sources are excluded from unconditional composition and
handled by branch-aware prepared-values/absence logic. Effort M-L. Behavior-bearing
(precision-adding).

**A8. Abstention bounds that fall the wrong way.** (a) `TruthCondition::from_subsets(_,_,true)`
promotes to `Exact(when_true)` without proof (`ir/src/scalar_value.rs:885-910`). **Round-2
correction:** exactness promises BOTH polarities (`scalar_value.rs:833`), so the check must
establish disjointness (`true ∧ false` unsatisfiable) **and** exhaustiveness (`true ∨ false`
tautological) via the bounded BDD, retaining `Partial` otherwise — disjointness alone is not
enough. Two of five construction sites check today, three do not. (b) The fanout collapse to
`vec![(Predicate::True, parts)]` exists at **three** sites, not two
(`fragment_eval/holes.rs:218`, `fragment_eval/lower.rs:767`,
`fragment_eval/inline_regions.rs:164`) — dropping conditions *tightens* and concatenates
cross-arm text into quote-context claims no render produces. Fix all three: one unconditional
taint carrying the union of influencing paths, claiming nothing. (c) **Confirmed, no longer
Suspected:** past `MAX_STAMPED_GUARDS` the unstamped reduction survives
(`symbolic_local_state/mod.rs:177`) and the branch join *unions* it
(`branch_join.rs:51`, `join_predicate_union`) — the truthiness claim escapes its arm. Fix:
remove the reduction at the cap. Effort S each. Behavior-bearing (register expectations first);
**sequence within Part A, before B1/B2.**

## Part B — structural campaign: make the invariants compile

The v3 lesson stands: do **not** promise LOC deltas. Measure each step in representations
removed, invariants moved into types, and (for B5/C-steps) wall-clock on the corpus.

**B1. Exhaustiveness sweep — apply the house idiom everywhere it is missing.** Mechanical and
compiler-driven, but **split into separate byte-exact rounds** (dispatcher; destructures;
serde/module moves) rather than one omnibus commit:
- `record_fail_conjunction`: convert to an exhaustive early-return dispatcher over
  `match &capture.kind` (no wildcard); only `CaptureKind::Fail` falls through to the
  fail-negation tail (`ir/.../requirements.rs:19-352`). **Round-2 note:** the
  `StringRequirement` arm is behavior-sensitive — its three routes differ (serialized abstains,
  selected can rewrite member scope, direct records unconditional facts), so it gets its own
  nested exhaustive `StringRequirementRoute` match.
- `ContractValuePathFacts`: destructure both merges (`core/src/contract_signals.rs:1076-1114`,
  `ir/.../contract_rows.rs:114-135`); fix the two-zero-values trap by removing
  `#[derive(Default)]` and giving the universally-quantified bits an `AllUses(bool)` wrapper
  whose identity is `true` (today `..default()` can poison `all_render_uses_self_guarded`).
- Every struct-level `map_value_paths` (`core/src/contract_use.rs:264-280`,
  `ir/src/observed_facts.rs:87-127`, `ir/src/contract/graph.rs:148-174`): exhaustive
  destructure, deliberately-unmapped fields named in place.
- `BoundHelperCallCacheKey::from_resolution` (`ir/src/analysis_db.rs:1187-1224`): destructure
  `BoundHelperCallResolution` — one line; the key is complete today and this keeps it so
  (the quietest possible future corruption: helper summaries served across contexts).
- `contract_guards` / `contract_guards_are_exact` / `negated_contract_guards`
  (`core/src/predicate.rs:319-562`): collapse the hand-synced twins into one flatten returning
  `Option<Vec<Guard>>` (`None` = not exact); delete the second recursion and both wildcards.
- Identity predicates (`helper_meta.rs:159-259`, `fragment_eval/domain.rs:374-392`): full
  destructuring as an interim tooth until B2 lands.
- Delete `WireContractUse` (`core/src/contract_use.rs:133-186`) for `#[derive(Deserialize)]`;
  comment `GuardDnf`'s lossy wire format as never-round-trip.
- Convert gen's five `include!`s to real modules (`overlay_lowering.rs:652`,
  `path_resolver.rs:396`, `resolve_policy.rs:780,1056`); the resulting `pub(super)` list is the
  real interface and the compiler enumerates it.

**B2. The `Transform` vocabulary becomes a type.** The highest-leverage structural change.
Replace the transform Booleans on `HelperOutputMeta` and the parallel `BTreeSet<String>`
channels on `Effects` with one `enum Transform { Encoded, ShapeErased, NilOmitted, Stringified,
YamlSerialized, TemplatedYaml, DerivedText, PartialText, PlainSlotStringFormat, JsonSerialized,
JsonDecoded, NilScrubbed, ParsedMap }` — **`Encoded` included; round 2 caught its omission**
(it is identity-breaking at `domain.rs:309`) — carried as a `TransformSet` through `Effects`,
`HelperOutputMeta`, `SpliceMeta`, **and `RenderedRow`**. "Untransformed" stops being an
enumerated negation and becomes **set emptiness PLUS absence of the typed payload-bearing
facts** (`merge_layers`, omitted keys, lexical escapes, split segments, range-key role,
empty-rescue/fold), which stay as `Option`/collection fields — set emptiness alone is NOT
identity. Legitimate subsets become named constants (`RANGE_SHAPE_BREAKING: &[Transform]`)
reviewable side by side. This dissolves: the five
disagreeing identity spellings (the review found a 9-row disagreement table across
`is_structurally_untransformed`, `is_input_member_identity`,
`output_meta_preserves_range_shape`, `condition_predicate.rs:3240`), the ten
`meta.X || path_is_encoded(...)` OR-pairs at `fragment_eval/lower.rs:96-118`, and the
three-place `clear_plain_slot_string_format_paths`. Gate it with **G2 first** (the generated
transform×position suite) so behavior preservation is pinned before the migration. Keep
channels whose merge/execution semantics genuinely differ (`helper_observed_*`, mutations)
separate — the v3 "identical domains only" rule. Effort L, staged carrier-by-carrier,
byte-exact at every step.

**B3. One owner per rule.** Three consolidations, each deleting a twin:
- Faithfulness: decoder returns `Decoded::{Exact(Predicate), Approximate(Predicate)}`;
  `condition_lowering_is_faithful` becomes a projection of the same function — the
  `_ => false` / `_ => truthy_predicate` divergence can then not exist
  (`condition_predicate.rs:137-234` vs `:440-497`). Effort L (~65 methods, compiler-driven).
- Self-guard classification: the identical `Not(Absent{path == value_path})` matching lives in
  both `ir/.../final_signals.rs:313-329` and `gen/.../member_projection.rs:529-601`. Move to
  `ConditionalGuard::is_self_truthy_for/is_self_presence_for` in core, or stamp
  `SelfScope::{None,Truthy,Presence}` on the overlay/implication at the builder. Effort S/M.
- Requirement kinds: A4's `admitted_kinds` single function; `JsonKind` becomes an enum,
  killing the stringly `"integer"`/`"number"` comparisons. Effort M.

**B4. `ValuesPath` newtype — split B4a/B4b, sequenced BEFORE B2 and B5** (round-2 correction:
B2's `BTreeMap<ValuesPath, …>` needs the type to exist, and B5's ordering story depends on it).
- **B4a (representation-only, byte-preserving):** segmented `ValuesPath` in core: private
  representation, `parse`/`from_segments` constructors, `segments()`, `parent()`, `push()`,
  `is_descendant_of()`, `item_parent()`; **no `Deref<Target=str>`, `AsRef<str>`, or `Display`**
  — explicit encoding only at diagnostics/wire boundaries, so `split('.')` and
  `format!("{a}.{b}")` stop compiling. Custom serde and a manual `Ord` that preserve the legacy
  encoded-string order exactly (fixture bytes and BTree iteration depend on it). Migrate every
  semantic path carrier, **including the internal ones round 2 flagged**:
  `AbstractValue::ValuesPath(String)` (`ir/src/abstract_value.rs:14`), `Splice.values_path`
  (`ir/src/fragment_eval/domain.rs:271`), `RangeModes` keys — plus the phase-crossing carriers
  (`Guard`/`Predicate` paths, `ContractUse`, `CaptureKind` payloads, `Effects`/`ObservedFacts`
  keys, `ContractSchemaSignals` map keys, removing the dual map-key/embedded-`value_path`
  identity). Do not newtype unrelated strings. Bonus: `map_value_paths` walks become total.
- **B4b (behavior-bearing):** the typed `Segment::{Literal(String), EachMember}` distinction,
  making the literal-`*` collision (A3) unrepresentable — adjudicated flips expected only where
  a chart really uses a literal `"*"` key.
Effort L, crate-by-crate, byte-exact steps in B4a.

**B5. Predicate canonicalization (the measured perf item) — split B5a/B5b/B5c** (round-2
correction: hashes must not change ordering, and delayed minimization can explode DNF size).
- **B5a (representation-only):** `Conjunction` newtype (sorted, deduped, `And`-flattened,
  `True`-elided — replaces `Vec<Predicate>` in `FailCapture` and the implication types,
  deleting the manual sort+dedup at ≥7 sites); private `Arc`-backed predicate nodes with a
  **manual `Ord` preserving current structural order** — the cached hash is an equality
  fast-reject only, never an ordering key (`Predicate` ordering is serialized into
  `GuardDnf`'s nested `BTreeSet`s and the fixtures).
- **B5b:** re-profile Airflow on the B5a tree; record wall-clock in the ledger.
- **B5c:** optimize `minimize_disjunction_by` in place (no call-site moves). Boundary-only
  minimization is **optional** and gated: it requires an inventory of intermediate DNF
  observers plus a recorded gate on maximum disjunct count, peak RSS, wall time, semantic
  equivalence, and exact serialized guard order — `GuardDnf::conjoined` currently normalizes a
  Cartesian product immediately, and deferring that can grow instead of shrink.
`MergeLayersUse` validation (`Vec<MergeLayer{path, transform}>` + checked position, replacing
the silent clamp/Identity fallbacks at `core/src/contract_use.rs:32-66`) moves to **its own
round**. Acceptance for B5a/B5c: byte-exact fixtures with **ordering preservation as an
explicit invariant, not an expected fixture rewrite**; no wall-clock number promised. Effort L.

## Part E — the deletion thesis: collapse vocabularies, not carriers (spike-gated)

**Why v3's LOC estimates failed, mechanically.** v3 unified *carriers* (one `ObservedFacts`, one
selection carrier, one invocation model) — but production LOC in this codebase is dominated by
the **cross-product of semantic vocabularies × the interpreters that must understand them**, and
carrier unification leaves that product intact. Measured on the current tree: four overlapping
condition/requirement vocabularies — `Guard` **26** variants, `FailValueRequirement` **26**,
`CaptureKind` **19**, `ConditionalGuard` **18** — plus the transform vocabulary spelled as ~12
Booleans and ~8 path-set channels. Each vocabulary needs producers, mergers, path-mappers, and
2–3 interpreters; every new Helm idiom adds a variant to several enums and an arm to several
interpreters. That product is why "removable wrappers" kept turning out to be live semantics:
the wrappers *are* the product.

**The global-maximum question, re-posed at the right level.** Round 1 answered local-vs-global
at the *phase* level (verdict: right hill — confirmed). The user-level question is at the
*vocabulary* level, and there the answer is different: the globally simpler design is the same
six-phase pipeline speaking **three vocabularies instead of six-plus**:

1. **Conditions** = `Predicate` (Boolean) + `ContextMark` (with/range/default scope) —
   currently: `Predicate` + markers-as-pseudo-guards + `Guard` + `ConditionalGuard`.
2. **Requirements** = `Subject` (value / members / keys / field-at-path) × a small `Req`
   algebra — currently: `CaptureKind` (site) translated to `FailValueRequirement` (document)
   by a 2,321-line reducer. (Round-2 correction to this framing: the reducer's *translation*
   share is thin — most ladder arms are a one-line `Req` mapping — and its bulk is scope
   classification and negation algebra, much of which legitimately survives; see E3's bound.)
3. **Provenance** = `Transform` set (Part B2) — currently three parallel flag carriers.

One dimension the three-vocabulary phrasing must carry as **data, not docs**: per-atom
*usable polarity* (exact vs sound-subset-positive-only). Several condition atoms are documented
one-sided (`Guard::IntGt`/`IntLt`, `AtMostOneMember`, `RangeKeyEquals` — `guard.rs:255-289,
171-180`), and the whole fail-lowering discipline rests on "may hold LESS often, never more"
(`requirements.rs:950-959`). Today that contract lives in doc comments and in *which* enum an
atom appears in; a unified vocabulary needs a typed `UsablePolarity`/`Exactness` attribute
(generalizing what `Predicate::Approximate{sound_subset}` already does for formulas), or the
collapse silently erases a soundness contract.

**What is genuinely irreducible (do not attack):** the Helm idiom catalog —
`condition_predicate.rs` (3,431), `function_semantics`, the `expr_call_eval` families. That is
Helm's surface area; ~8–10k LOC of essential complexity where deletion means losing precision.
The reviews confirmed these are coherent catalogs, not entangled strata.

**The rule that prevents another miss:** every E-step starts with a **spike on a branch** that
implements enough to measure the real diff, and is adopted only if the spike shows (a) at least
one representation deleted, (b) **non-positive whole-tree production LOC by `task tokei:core`**
— never "on the touched surface", which is gameable by shrinking the reducer while producers
grow, (c) for E1–E3: **byte-exact fixtures only** (they are representation-only by intent;
offering an adjudicated-flip path here invites semantic drift under deletion pressure — flips
belong to Part A), (d) flat-or-better corpus wall-clock. **E3 has a fifth gate:** the corpus is
too thin to catch producer-side abstention (most `CaptureKind` lanes are pinned by ONE chart
each — minio, loki, traefik, oauth2-proxy per the variant docs), and gates (a)/(b)/(d) actively
*reward* a producer that silently emits nothing; E3 therefore requires a generated
requirement-lane microchart suite (one chart per CaptureKind-lane × subject shape, asserting
the emitted implication set, not just final schema bytes) green before and after the spike.
A spike that misses any gate is *recorded and abandoned* — the v3 failure mode was forcing
estimates through implementation; the v4 rule is measure-then-commit.

- **E1 = B2 (Transform enum).** The clearest mechanism-backed deletion: ~10 OR-pair
  reconciliations (`fragment_eval/lower.rs:96-118`), five hand-enumerated identity predicates,
  the triple-cleared formatter channel, and the parallel `Effects` channel declarations+merge
  arms collapse into one enum + set operations. Spike first on `HelperOutputMeta` alone.
- **E2 = B-item (condition/context split on `FailCapture`).** Separating `condition: Predicate`
  from `context: Vec<ContextMark>` deletes the marker-strip dances
  (`requirements.rs:156-189, 377-406`), the marker halves of the conjunct-classification loop
  (`:442-461`) and of `capture_outer_guards` (`:908-934`), and the producer-side marker
  spelling workarounds ("may spell those markers as `Truthy` instead of `With`"). The
  Predicate→ConditionalGuard lowering itself (`fail_outer_guard`, `:950-1028`) is polarity-
  disciplined condition lowering, NOT marker classification — it survives until/unless E4.
  Prerequisite to E3 and independently valuable.
- **E3 (one requirement vocabulary, producer-lowered).** The genuine global-maximum candidate,
  with an honest bound: producers in `expr_call_eval` emit `(context, condition, Subject, Req)`
  directly; **the per-kind payload translations delete** (mostly one `Req` constructor per
  ladder arm, `requirements.rs:68-352`) along with ~12 `CaptureKind` variants and their
  `map_value_paths`/`sole_value_path` walks (`eval_effect.rs:295-368`) — but **the scope/target
  machinery stays**: `record_value_requirement_capture`, member scoping (which consults
  document-global `range_modes`), the negation algebra, and cross-capture folds are global
  reduction, and two claims are irreducibly global — `Fail` (test-vs-outer classification needs
  the whole conjunction) and `MemberAccess` (its `complete_domain` is a property of the union
  of all access sites; no single producer can know it). So: "`CaptureKind` shrinks to subject
  shapes plus the two global claims; Subject finalization remains a reducer job."
  Dependency the spike must respect: `StringRequirement`'s `Serialized` route factors into a
  **`Transform` provenance mark on the claim itself** — three consumers key on it
  (`requirements.rs:92-94`, `input_channels.rs:139-189`, `contract/graph.rs:301-424`) — so B2's
  `TransformSet` must be attachable to requirement claims, not only to splices. Do not attempt
  before E2 and B1 land.
- **E4 (Guard/ConditionalGuard unification study).** 26- and 18-variant condition vocabularies
  with a hand-maintained partial map (`conditional_overlays.rs:497-660`). Post-E2 the residual
  atom sets converge more than round 1 credited (even `NotMatchesPattern` round-trips as
  `AllOf[TypeIs(string), Not(Matches)]`, `:568-583`) — but the map is not a variant table: its
  entries are *instance-level* policies (`MatchesPattern{templated:true}` drops; `MinMembers`/
  `TypeIs` translate differently when self-targeted; `With` only without a target), so
  unification deletes the translation while the self-guard policy survives as a same-vocabulary
  rewrite, and it requires the `UsablePolarity` attribute above. External-visibility carve-out:
  `ContractUse.condition` serializes the `Guard` vocabulary into the 18 IR fixtures
  (`core/src/contract_use.rs:69-131`), so E4 needs a versioned-document story; byte-exactness
  is scoped to the 84 schema artifacts. Spike-gated; a recorded "genuinely distinct" verdict is
  an acceptable outcome (the v3 checkpoint pattern).

## Part C — gen/session hygiene (representation-only, independent of Part B)

**C1. Typed tree to the edge.** Port `schema_runtime_types` and
`relax_required_members_supplied_by_default` (raw-JSON reach-ins,
`member_projection.rs:713-815`) onto `SchemaNode`; make `LoweredConjunct.schema` and
`ValuePathSchemaInputs`' seven positional `Value` fields typed (named-channel enum); confine
`Value` to provider ingestion and final emission. Kills the "unknown keyword shape → silently
all types" hazard (`member_projection.rs:734`) and the per-pass keyword-list drift
(descriptions/wrappers/defaults walkers each owning a different applicator subset).

**C2. One materialization.** Replace per-ranged-path whole-tree `into_value()`→mutate→reparse
(`gen/src/emission_plan.rs:527`, `schema_tree.rs:112-129`) with direct `SchemaNode` edits, and
the per-description full-branch walks with one path-trie traversal. Superlinear output-size
multipliers, confirmed by inspection.

**C3. Chart-load snapshot.** `files_with_role` re-scans the chart tree per caller; template
sources are read into `DefineIndex`, re-read for manifests, copied per `IrAnalysisDb`
(`helm-schema/src/chart/file_roles.rs:40`, `analysis/collection.rs:40`,
`ir/src/analysis_db.rs:151`). One immutable `LoadedChartCorpus` owned by session preparation.

**C4. Provider resolution as a phase artifact.** Build one
`BTreeMap<ProviderSchemaUse, Option<Fragment>>` from `evidence.provider_schema_uses` before
lowering; the four provider-requirement synthesis passes and the path resolver consume it
instead of five private caches (`gen/src/provider_requirement_synthesis.rs:37-436`). Also the
structural close of A1 (the memo disappears).

## Part D — gates and test infrastructure (high value, do early, cheap)

**G1. Battery cap de-biasing.** The 50k cap is a prefix `take(n)` over a DFS-ordered probe list
(`tests/common/emission_profile_harness.rs:394-411`): capped charts lose whole late-alphabet
top-level subtrees. Replace with deterministic round-robin over `(top_level_key,
replacement_kind)` buckets — same count, same accounting; and treat `base_emitted == 0` (guard
probes consuming the whole budget) as a coverage failure, not a disclosed drop. Effort S.

**G2. Generated transform×position suite.** Synthetic microcharts: each `Transform` fact ×
each consuming position (identity projection, member identity, range subject, `hasKey`
decoding, splice lowering). Pins the exact invariant class the corpus only samples (a wrong
schema for one transform in one position). **Round-2 staging:** land in two stages — behavior
cases (asserting the *produced fact* first, then full schema equality) before B2, then
exhaustive `Transform::ALL` coverage during B2, including mixed-branch transforms,
removal/clear semantics, scalar dispatch, quoted/plain-token capture, and the payload-bearing
states (merge layers, omitted keys, lexical escapes, splits) that set-emptiness alone does not
witness. Effort M.

**G3. Capability-probe row validation.** The table's only failure mode left is a *wrong* row
(silently kills a live `.Capabilities` branch — the permissive direction). Add one corpus-gated
test resolving every `(api_version, kind)` row against the pinned `testdata/provider-bundle`.
Keep the table itself: all four reviewers concur removal is a bad trade (eager bundle fetch
reintroduces the cache-as-oracle antipattern). Effort S.

**G4. Duplicate-alias policy completion** = A2. Listed here because the diagnostic and tests
are the deliverable; the in-flight working-tree patch must be re-keyed to `values_key` before
commit.

## Part S — simplification backlog (round 3: four fresh reviewers, cross-vendor, anti-over-engineering bar)

A third round hunted concrete simplifications under a strict bar: every finding must make the
code easier to reason about (named compiler pattern, mandatory anti-findings section proving
rejected abstractions), LOC as evidence only. Convergence was strong and on *different ground*
than rounds 1–2 — real convergence, not churn. Items are Confirmed with file:line in the round-3
review outputs (`target/arch-v4-review/`); orchestrator-verified where marked ✓.

**Blue-sky verdict (the definitive answer to "is there a genius simpler design"):** seven
candidate architectures were examined from first principles — Datalog-style fact derivation,
Salsa-style query incrementality, e-graph canonicalization, one unified constraint lattice,
bidirectional expected-type checking, fusing the reducer away, and oracle-guided learning —
and **each dies on a named hard case** (respectively: negation over positive-polarity-only
approximations; no incrementality consumer + the cache law favoring five coarse artifacts;
saturation limits vs determinism and polarity-typed rewrites; quantifier/marker distinctions a
single lattice erases; guarded-expected-type doubling + phase re-fusion; the irreducibly global
claims; the <1s law and abstention). The current core is a faithful instantiation of classical
abstract interpretation (one interpreter, communicating domains, deliberate widening/abstention);
the real simplification frontier is vocabulary count (Part E) plus this backlog.

**S-A. Correctness-adjacent (behavior-bearing, adjudicate like Part A):**
- **Junctor expansion early-return** ✓: `expand_schema_node_at` expands only the FIRST
  `allOf|anyOf|oneOf` and returns, leaving sibling keywords' `$ref`s unresolved in provider
  fragments (`k8s/src/kubernetes_openapi/resolve_ctx.rs:252-259`); the generic loop below
  already handles those keywords. Deleting the 8-line special case makes expansion total.
  Found independently by two reviewers.
- **`LookupTrace` is production-dead — both halves** ✓ (fused from two half-findings): the
  resource half is built then discarded (production always passes
  `commit_miss_diagnostics=false`, then emits from an EMPTY default trace,
  `lookup/chain.rs:181`); the API-presence half is dropped by its only consumer
  (`CapabilityOracle for Chain` returns `.answer` only, `chain.rs:335-340`). Delete the trace
  document, its duplicate tri-state (`SourceProbeTraceOutcome`), both recorders; carry
  local-override-unreadable on the chain outcome at the decision point; re-pin the offline
  contract tests on answers + recorded fetcher calls. The unreadable-override diagnostic starts
  actually firing — pre-register it.
- **Required-in-parent double descent**: the parent walk re-descends from the root and can pick
  a DIFFERENT junctor branch than the leaf walk (`resolve_ctx.rs:332-389`). One descent
  returning (parent, leaf) is simpler, faster, and removes a must-agree invariant. Two
  reviewers independently.
- **Inference orderings**: two hand-written rank tables re-spell the enums' derived order, and
  the diagnostic sink sorts the same enums by `Debug` STRING — a third, different order
  (`k8s/src/inference/aggregator.rs:100-125`, `diagnostic/canonicalise.rs:12-20`). Derive
  `Ord`, pin declaration-order-is-priority with one test; the diagnostic candidate order flip
  is one adjudicated fixture.

**S-B. Hot-path work reduction (attacks the measured Airflow profile; representation-only):**
- **Normalize once at the phase boundary** ✓: `finalize` runs the whole-vector deep sort
  (`expand_condition_disjuncts`, over the profile's hottest comparator) ~8× because every
  normalization entry defensively re-canonicalizes (`contract_normalization.rs:30,134,151`;
  `contract/graph.rs:281-290`). One owner function with stated form transitions; keep rows
  expanded (single-conjunction) through subsumption/append/rebasing and compact to DNF once at
  the end; store `ContractDocument` in `FinalizedContract` instead of rebuilding per request.
  (Two reviewers converged; sequence with/before B5.)
- **`resolve_all` clone-per-path**: deep-clones every `ContractPathSchemaEvidence` plus a
  staging `Vec<String>` purely for the borrow checker; field-splitting `self` makes it one pass
  over one map with zero clones (`gen/src/path_resolver.rs:110-134`).
- **Values composed three times** ✓(shape): `compose_subchart_values` runs 3× per session (two
  runs bit-identical: `dependency = refill − root_declared`), plus a fourth per-chart re-parse
  for global ownership (`helm-schema/src/chart/values.rs:44-67,73-93`; `session.rs:77-101`).
  Parse each `values.yaml` once into the C3 snapshot; derive `dependency` from `refill`. Two
  reviewers independently. Also: each subchart `Chart.yaml` is parsed twice during discovery
  (`discovery.rs:128` + recursion) — thread the parsed value; sequence after A2.
- **`K8sVersionChain::ordered()`** rebuilds its version list per lookup with `format!` allocs;
  private fields + materialize once in `new` (`k8s/src/kubernetes_openapi/version_chain.rs`).
- **One parsed define program**: helper bodies are copied into every per-chart `IrAnalysisDb`
  and re-parsed by recognizers; a shared lazy `ParsedDefines` (OnceCell tree + expressions,
  policy-dependent summaries staying per-context) extends C3 to syntax
  (`ir/src/analysis_db.rs:142,257,439`). Effort L; measure eager-work risk.
- **`ValuePathContext` embeds `EvalEnv`** instead of carrying parallel fields projected+cloned
  into an env per decode (`ir/src/value_path_context/mod.rs:43`, `path_resolution.rs:53`).
- **Helper summary double walk**: rendered rows and suppressed reads collected by two full
  fragment-tree traversals with the same condition stack; one synthesized-attribute fold
  emitting both lanes (`ir/src/fragment_eval/summary.rs:181-207,859-1034`).

**S-C. Scope discipline and canonical forms (representation-only):**
- **`ScopeMark`** (two reviewers independently): replace the four hand-restored interpreter
  stacks' eleven per-site save/truncate clusters with one `mark_scope()`/`rewind(mark)` pair
  (+ `loop_depth`); `locals` stays explicit (its exits are semantic joins). Kills the A8(c)
  bug class at the root.
- **`thread_local!` dispatch depth → context-owned `Cell<u8>`** on `ValuePathContext` — the
  recursion re-enters only through `&self` methods; the cell is the simple answer, parameter
  threading through ~65 decoders would be the over-engineered one
  (`condition_predicate.rs:21-23`).
- **Smart constructors for `ConditionalGuard` conjunctions**: 30 hand-written
  `guards.sort(); guards.dedup();` sites (receipts in the round-3 output) collapse into the
  three constructors that own the vectors (`GuardScopes::new`,
  `ContractRequirementImplication::new`, `ConditionalPathOverlay::new`). Complements B5a
  (which covers `Vec<Predicate>` only). Sites that today forget to sort begin to — any fixture
  diff is an ordering bug being fixed.
- **`merged_layers` canonical constructor**: recursively flatten nested `MergedLayers` at
  construction; delete the two duplicated flatten helpers (`abstract_value.rs:78-87` =
  `fragment_eval/lower.rs:173-182`). Two reviewers independently.
- **Duplicate helper bodies**: `guard_value_is_truthy` (private copy in `scalar_value.rs:821`
  beside the shared `pub(crate)` one), `bool_predicate` (×2), `render_site` key + comparator
  alias in `contract_normalization.rs` — one owner each.
- **Activation-DNF (spike-gated, adjudicated tension):** one reviewer proposes replacing the
  N-way whole-`ContractIr` clone per activation arm with `GuardDnf` conjunction
  (`manifest_contract.rs:104-125`); another's anti-findings warn a single disjunctive guard may
  not be byte-preserving because overlay branch structure keys on rows. Treat as an E-style
  spike: byte-exact or abandon-and-record.

**S-D. Dead surface (delete; Part F applies where `pub`):**
- **The IR fragment domain is public for two golden tests** ✓: ~30 `pub` items
  (`AbstractFragment`, `SpliceMeta`, `Guarded`, …) have zero external consumers; move the two
  golden tests into `src/tests/`, make the domain `pub(crate)`, drop the `Rc`-wrapped
  one-field `SymbolicIrContextInner` + unused constructors. **Strategic: removes the Part-F
  compatibility obligation from B2/B4 entirely.**
- `K8sSchemaProvider::has_resource`: required trait method, four impls, zero production
  callers (the residue of the pre-typed-result protocol). Delete, with its stale doc mention.
- k8s `record_source`/`with_record_source` (never wired), `use_cache: true`/
  `use_not_found_marker: false` literal knobs; shortlist consulted by three providers → hoist
  to one aggregator-side consult; local-override `Shortlist` restamp deletes.
- **`CompletionPass`** ✓: 8-variant knob with one production value and seven dead early
  returns; restructure `complete` as named step functions the profile test composes directly.
  Plus the one-field `FactAccounting` wrapper and the `generate_values_schema_through`
  forwarder.
- `EmissionReport`'s origin map dimension serves one accessor with zero callers; collapse to
  `BTreeMap<EmissionClassKind, FactCounts>`.
- `OwnedDefinitions::capture` duplicates `root_definitions` verbatim in the same file.

**S-E. Measure-then-commit:** `&mut impl HelperCallValueResolver` → `&mut dyn` at 56
signatures (two production impls, one a null object; deletes a monomorphized ~6k-LOC copy).
Indirect-call cost fires only at helper-call nodes, but this one is adopt-only-if-flat on the
corpus wall-clock.

Round-3 anti-findings worth keeping on record (rejected as over-engineering; do not re-open):
carrier-generic global-projection walker, generic `AbstractValue` fold/visitor, arm-chain
driver unifying `eval_control`/inline, unifying the two JSON-schema bundlers (different
preserve policies by design), `DiagnosticKey`/`Diagnostic` collapse, MemoCache/SourceDocCache
merge, tree-sitter parser threading, `EvalEnv` borrow/Cow framework, `join_map`'s closures
(eleven instantiations of one rule — earns its keep), `NoHelperCallResolver` null object.

## Part F — public API and wire compatibility (the category every round-1 lens missed)

`helm-schema-core` publicly exports `ContractUse`, `Guard`, and `Predicate`
(`core/src/lib.rs:31,39`); `Guard` has string-shaped serde (`guard.rs:101`), `ContractUse` has
a compatibility-aware deserializer (`contract_use.rs:133`), and `Predicate::And/Or(Vec<_>)` is
publicly constructible (`predicate.rs:45`). B4 and B5 change public construction, ordering, and
potentially serialized artifacts; E4 changes what the IR fixtures serialize. Standing gate for
every such step: preserve decoding of previously serialized guard/contract documents and their
byte order, and either preserve the public Rust API or record an intentional breaking release
with a semver/API check in the round's dossier. Compatibility is a *decision*, never a side
effect.

## Verified-clean — do not re-open (attacked by ≥1 reviewer, confirmed by orchestrator spot-checks)

- The phase ladder and the builder/lowering boundary (attacked from both directions, holds).
- BDD soundness: free-atom conservativity, bound-abstention (`None` on every cap), approximate
  refusal in `exact_implies`; `predicates_are_contradictory`'s exactness per arm.
- The four ambiguity carriers (`Predicate::Approximate`, `TruthCondition`, `SelectionState`,
  `ScalarValueDispatch::complete`) hold genuinely different information — **do not unify**.
- `function_semantics`' `_ => UNKNOWN` default abstains permissively on every facet — the model
  catalog design.
- Capability oracle tri-state and offline contract; cache-scan determinism (all `read_dir`
  consumers sort before use); session per-stage memoization (no cross-run persistence).
- `EmissionClass` smart constructor (`NonEmptyGuardScopes`); `guard_encodes_fully` defined by
  calling the encoder (the pattern B3 spreads); the output pipeline's no-feedback rule;
  gen's provider backprojection is legitimately gen-phase work.
- `condition_predicate.rs` (3,431 lines) and `fragment_eval/eval.rs` are each one coherent
  responsibility — size is the idiom catalog, not entanglement. Do not split for size alone.
- `Effects::merge`/`execution_only`, `ObservedFacts::absorb`, `ContractIr::finalize`,
  `CaptureKind::map_value_paths`, `Guard::map_value_paths` — the enforcement idiom, correctly
  applied; B1 copies it, not fixes it.

## Execution protocol

Reuse the v3 verification machinery unchanged: per-round pre-registered acceptance
expectations; one clean dump per final tree from an immutable archive; the 60-chart full-depth
battery with live Helm adjudication of every flip and zero-drop mandatory coverage; luup2
downstream gate; per-round ledger dossiers (progress file: `architecture-review-v4-progress.md`).
Representation-only rounds require byte-exact fixtures; Part A rounds are behavior-bearing with
individually adjudicated flips. Never fold infra/tooling changes into a semantic commit.

Suggested order (dependencies, then value density; round-2 corrected):
G1, G3 → A1, A2/G4, A3-sweep, A4, A5, A6, A8, S-A (small adjudicated correctness rounds, all
before Part B) → **S-D IR-privacy item** (removes Part-F cost from B2/B4) → B1 (split:
dispatcher round, destructure round, serde/module round) → G2 stage 1 → **B4a** (path carrier,
byte-preserving — before B2 and B5) → B2/E1 (+ G2 stage 2) → B4b → A7 → E2 → B3 →
MergeLayersUse round → **S-B normalization-once** (with B5's profiling) → B5a → B5b
(re-profile) → B5c → E3 spike (adopt or record) → E4 study → C1–C4 and the remaining S-B/S-C/
S-D items (independent, interleave freely; S-E adopt-only-if-flat). Every step ships alone;
abandoning mid-campaign leaves the tree strictly better.

**Success metrics:** count of hand-synced producer/consumer pairs (currently ≥9, target 0);
count of rules with two owners (≥4 → 0); **count of semantic vocabularies in the
condition/requirement/provenance space (6+ → 3, per Part E)**; raw-string path operations on
the values currency (≥15 → 0, then unrepresentable); Airflow generation wall-clock (112.7s →
recorded per B5/C2 round with the ambition of the <1s law); battery coverage guarantee
("prefix of the tree" → "every top-level root × replacement kind before any repeat").
Production LOC is **evidence, not a target**: each E-spike's adoption gate requires
non-positive LOC on the touched surface, but no step promises a band — the v3 campaign proved
banded LOC promises select for forcing, and forcing selects for deleted semantics.
