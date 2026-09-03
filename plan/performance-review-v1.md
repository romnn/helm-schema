# Performance review v1 — the normalization tax, and how to stop paying it

Reviewed tree: `b3475dec` (post-v4-campaign, version 0.0.7). Method: direct measurement first —
release binary, private cache snapshot, offline runs, perfetto phase traces, windowed symbolized
samples, a throwaway counter build, template-subset scaling, and four candidate-fix spikes each
checked for byte identity and timed on the same host — then a cross-vendor adversarial review
(two gpt-5.6-sol reviewers via codex, one fable-5 and one opus-5 reviewer) with distinct lenses
(IR phase, emission phase, accuracy/determinism/protocol adversary, first-principles complexity).
Every high-risk claim was re-verified against source by the orchestrator; the reviewers'
disagreements with the orchestrator's first-round findings are recorded where they changed the
plan. This plan is frozen; the implementor keeps `plan/performance-review-v1-progress.md`.

## Verdict (four reviewers and the measurements agree)

**The performance law is missed for one reason, and it is a representation problem inside one
phase, not an architectural one.** On the three slow charts 85% (kube-prometheus-stack), 86%
(airflow) and 55% + 20% (datadog: `normalize` + `exact_implies`) of all CPU is spent rebuilding
bounded decision diagrams inside `predicate_bdd::normalize`, and 83–99% of those calls repeat an
input the run has already normalized. Underneath every BDD build, every `Guard`/`Predicate`/
`ValuesPath` comparison allocates two encoded `String`s (152–246 million comparisons per large
chart). The calls are driven by one join in the abstract interpreter which, at every `if` exit,
re-splits every live local variable's scalar dispatch over every arm condition — including
variables no arm touched — so arm counts double per nested join until a cap silently drops them.
That is the super-linear dimension: **live scalar locals × branch outcomes × carried dispatch
arms × normalization cost per nested `if`**, excited by a handful of branch-dense templates. It is
not template count, action count, helper count, CRD bytes, output bytes or output nodes.

Two representation-only changes, spiked and measured byte-identical on all ten reference
charts (schema bytes everywhere; diagnostics and exit codes on the five charts checked), remove
most of the tax: an allocation-free path comparator (A1) and a per-run memo of the two BDD entry
points (A2). Measured CPU on a loaded host: datadog 81 → 24.6 s, airflow 112 → 43 s,
kube-prometheus-stack 154 → 107 s, grafana 5.1 → 3.7 s, cilium 8.7 → 5.4 s. A third byte-exact
lever costs one line: enabling the `mimalloc` allocator the CLI already ships for musl measured
−21 to −33% CPU on grafana, cilium and datadog (B2). A third change (A3, the unchanged-variable shortcut in the join)
removes the exponent itself — kube-prometheus-stack 107 → 24.5 s byte-identical, its 45 s template
→ 0.5 s — but re-spells guard formulas on four other charts, so it is a semantic round with Helm
adjudication, not a free win. Nothing in this plan touches the phase ladder, the semantic
vocabularies, the abstention bounds, or the cache contract. **Do not parallelize in wave 1** (see
anti-findings): the serial hot spot is removable, and the evidence says the residual after A1–A3
is a different, smaller problem that must be re-measured before anyone spends the `Send` surgery.

One correctness defect surfaced in passing and is filed first (C1): the gen-side
`ConditionFragmentCache` is keyed by two of the four inputs it reads, which violates the cache
law today.

## Measurement protocol (binding on the implementor)

Every number in this plan and every number the implementor records must be produced this way.
Lens C attacked the campaign's own protocol; its corrections are folded in.

- **Binary.** `cargo build -p helm-schema-cli --release` of the exact tree under test, copied out
  of `target/` before any other build can replace it. Never compare an installed binary against a
  freshly built one. Build both A and B before timing either.
- **Cache state.** A private copy of `~/.cache/helm-schema/{kubernetes-json-schema,crds-catalog}`
  addressed through `HELM_SCHEMA_K8S_SCHEMA_CACHE` / `HELM_SCHEMA_CRD_SCHEMA_CACHE`, warmed once
  online per chart, every chart verified byte-identical between the online run and an `--offline`
  run (this campaign: 10/10 identical). All timed runs use `--offline --k8s-version v1.35.0
  --compact`. A number produced with network access is not a timing number. For any change that
  touches a cache or memo, additionally compare a first empty-cache online run, a warm online run
  and a warm offline run — schema AND diagnostics — before calling it cache-law clean.
- **What "identical" means.** Hash schema bytes, stdout, stderr (`--diag-format json`, captured
  separately from the timing wrapper's output) and the exit status. Schema-only identity is not
  identity.
- **Repeats and statistic.** 5 runs for charts under 2 s, 3 otherwise, and for the final decision
  on a large chart at least 5 interleaved A/B pairs in randomized AB/BA order within the same
  minutes on the same cache snapshot. Report the median and min–max of CPU (user+sys) and of wall.
  The process is single-threaded, so CPU ≈ wall on an uncontended core; wall/CPU above 1.10 flags
  contention and the wall number is then decorative. A gain whose paired range crosses zero is not
  a gain.
- **Host load.** No concurrent builds on the host during decision runs. Record the 1-minute load
  average before every run; mark load1 > 4 as *loaded*. This campaign could not satisfy the first
  rule (another agent built continuously; load1 6–26) and says so on every number.
- **Phase attribution.** `--trace-output` perfetto trace; self time = inclusive − child spans.
  Spans are loaded-host wall, not CPU; overhead scales with span count (~10% on grafana, unmeasured
  on the large charts); never mix traced and untraced numbers, use traces for attribution only.
- **Leaf attribution.** `/usr/bin/sample` on an unstripped release twin
  (`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, separate
  `--target-dir`), which is output- and timing-identical. **A single long `sample` window is
  depth-biased**: deep interpreter stacks are walked more slowly and the analysis phase came out
  ~3× under-sampled in a 4 s whole-run window (the emission phase correspondingly ~2.4×
  over-weighted). Use 1 s windows placed inside the phase of interest; take phase proportions from
  the trace only.
- **Byte gate for every spike.** Compare the candidate binary against the baseline binary on all
  ten reference charts (schema, stderr, exit) before timing it; a candidate that changes one byte
  is a semantic change and leaves the performance track for the adjudication track.

Host for this campaign: Apple M3 Pro (11 cores, 18 GB), macOS 26.6, rustc 1.98.0. Output md5 was
identical across every repeat of every chart, and identical between the baseline, A1 and A1+A2
binaries on all ten charts; diagnostics (`--diag-format json`) and exit codes were identical between baseline and A1+A2 on grafana, cilium, argo-cd and datadog, and between baseline and A1+A2+A3 on kube-prometheus-stack. One protocol caveat the implementor must not inherit: the same baseline binary measured 65.1–65.4 s CPU on datadog when load1 fell to 5–7 late in the campaign versus 76–88 s at load 7–8 with concurrent builds, so loaded-host CPU inflation on the large charts is on the order of 20%; only paired, interleaved runs are comparable.

## Baseline (this campaign's numbers)

| Chart | n | CPU s median (min–max) | Wall s median (min–max) | load1 | templates | actions | defines | `.Values` paths | subcharts | output MB | output nodes | µs CPU / action |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| coredns | 5 | 0.36 (0.35–0.38) | 0.38 (0.36–0.39) | 7 | 17 | 707 | 9 | 120 | 0 | 0.36 | 9,552 | 495 |
| metrics-server | 5 | 0.18 (0.17–0.21) | 0.20 (0.18–0.22) | 7 | 20 | 404 | 11 | 82 | 0 | 0.18 | 5,125 | 421 |
| istiod | 5 | 0.67 (0.66–0.67) | 0.68 (0.67–0.69) | 7 | 28 | 948 | 8 | 126 | 0 | 0.29 | 14,018 | 696 |
| cert-manager | 5 | 0.73 (0.71–0.77) | 0.74 (0.73–0.79) | 7 | 48 | 1,521 | 19 | 226 | 0 | 0.70 | 16,249 | 467 |
| argo-cd | 3 | 6.25 (6.07–6.45) | 6.35 (6.16–6.61) | 7 | 166 | 5,536 | 63 | 1,291 | 1 | 2.08 | 77,624 | 1,096 |
| grafana | 3 | 5.12 (5.06–5.47) | 5.18 (5.13–5.64) | 7 | 39 | 2,677 | 29 | 425 | 0 | 1.26 | 49,452 | 1,890 |
| cilium | 3 | 8.65 (8.53–10.22) | 8.75 (8.63–10.89) | 7 | 148 | 5,861 | 49 | 1,126 | 0 | 1.70 | 78,339 | 1,455 |
| datadog | 3 | 81.1 (76.3–88.1) | 87.7 (79.3–121.1) | 7–8 | 143 | 6,534 | 203 | 838 | 4 | 2.10 | 118,295 | 11,670 |
| airflow | 3 | 111.5 (106.6–128.9) | 118.5 (112.4–147.0) | 9–22 | 160 | 8,612 | 223 | 1,333 | 1 | 3.90 | 175,792 | 12,382 |
| kube-prometheus-stack | 3 | 154.2 (151.0–158.6) | 179.2 (170.0–219.8) | 9–15 | 265 | 21,004 | 112 | 1,985 | 5 | 6.99 | 294,793 | 7,189 |

The law holds for the four small charts, is missed 5–9× by the three mid charts and 25–45× by the
three large ones. The right-hand columns do not predict cost: kube-prometheus-stack is 31× coredns
in output nodes and 430× in CPU; datadog and cilium have equal action counts and differ 9×. A
ten-chart linear fit favours output nodes × depth (R² ≈ 0.95), but the subset experiment below
rejects it as causal (lens D): output size is endogenous — the branch-dense files that cost the
most also emit the most schema.

## Where the time goes

### Phase split (perfetto traces, self ms = inclusive − child spans; loaded-host wall)

| Span | kube-prometheus-stack (194.3 s traced) | airflow (129.4 s) | datadog (78.6 s) | grafana (5.2 s) |
|---|---:|---:|---:|---:|
| `collect_manifest_contract_for_template` self (template bodies) | 151,683 | 10,685 | 5,295 | 171 |
| `summarize_bound_helper_call` self (helper bodies) | 7,760 | 52,156 | 46,471 | 2,706 |
| `collect_manifest_contract_for_chart` self (NOTES + static-template evaluation) | 1,727 | 41,095 | 7,554 | 9 |
| `normalize_contract_uses` self | 3,626 | 2,142 | 7,417 | 142 |
| `derive_schema_signals_from_contract_parts` | 5,695 | 2,621 | 4,576 | 295 |
| gen `LoweredEmissionPlan::build` incl. (`collect_conditional_schemas` inside) | 6,661 (4,264) | 4,090 (2,891) | 872 (628) | 549 (389) |
| `append_selected_constraints` | 3,075 | 1,372 | 624 | 135 |
| `extract_repeated_provider_payloads` | 2,450 | 4,041 | 878 | 190 |
| `minimize_schema` (output pipeline) | 3,202 | 3,349 | 494 | 230 |
| `parse_go_template` self (81k–102k spans) | 1,058 | 543 | 562 | 92 |
| everything else (load, provider lookups, write) | < 700 | < 700 | < 400 | < 60 |

Abstract interpretation is 83% of kube-prometheus-stack, 80% of airflow, 76% of datadog, 64% of
grafana. Contract finalization is 5–15% — and its cost is `ValuesPath::cmp`/`encode` (A1 work),
not an algorithmic defect (lens B re-derived this from the samples: 174/255 and 163/255 of the
`derive_schema_signals` samples). Generation plus output is 6–15% on the large charts and scales
as ≈ S^1.2 in pre-minimization document bytes (lens B measured 539–1,918 ms per pre-minify MB
across five charts) — a fat constant, not a runaway term.

### Which templates

Template-subset scaling (CPU s; the sorted top-level manifest list cut at 25/50/75/100%; helpers,
subcharts and CRDs always kept): kube-prometheus-stack 19.6 / 29.1 / 34.4 / 159.7; airflow 42.6 /
45.9 / 57.3 / 116.5; datadog 6.9 / 67.8 / 66.7 / 74.6. The last quarter of kube-prometheus-stack
(the `prometheus/rules-1.14/` alert files) adds 125 s while output nodes rise 50%; datadog's
second quarter (`cluster-agent-*`, `confd-configmap`, `daemonset.yaml`) adds 61 s and its third
quarter adds nothing. Four kube-prometheus-stack templates cost 45.0 + 21.8 + 18.8 + 15.6 s
(`kubernetes-apps.yaml` — 17 alerts, 105 `if`s, four long-lived scalar locals read by every alert —
`kubernetes-system-kubelet.yaml`, `prometheus.yaml`, `node-exporter.yaml`). Airflow's cost is in
the pseudo-helper `@file:templates/configmaps/configmap.yaml`, evaluated 7 times for 33.9 s, and
`standard_airflow_environment`, 28 calls for 11.2 s. Datadog's is spread over helpers called from
many contexts (`get-agent-version` 326 calls / 6.6 s, `resolved-discovery-enabled` 40 / 4.4 s,
`system-probe-feature` 30 / 2.9 s, `should-enable-system-probe` 31 / 2.9 s, `image-path` 38 /
2.5 s).

### What the time is spent on (throwaway counter build; counts exact, seconds inclusive and inflated by the counters)

| Chart | `normalize` calls / distinct inputs | s in `normalize` (of run) | `exact_implies` calls / distinct / s | helper calls / misses / call-chain-only misses | second scalar pass: count / s | `ValuesPath::cmp` calls |
|---|---|---:|---|---|---|---:|
| grafana | 285,423 / 15,491 | 2.8 of 5.9 | 76,756 / 4,693 / 0.2 | 349 / 93 / 45 | 84 / 0.2 | 8.1 M |
| argo-cd | 457,190 / 19,424 | 1.8 of 8.0 | 111,703 / 4,880 / 0.2 | 1,435 / 389 / 71 | 319 / 2.3 | 6.6 M |
| cilium | 492,292 / 24,489 | 5.8 of 11.0 | 118,350 / 4,757 / 0.2 | 411 / 138 / 29 | 120 / 1.5 | 15.3 M |
| datadog | 4,377,882 / 33,463 | 45.4 of 81.9 | 1,342,879 / 16,183 / 16.0 | 3,100 / 1,111 / 762 | 1,090 / 23.8 | 152.5 M |
| airflow | 1,464,518 / 65,814 | 100.2 of 116.9 | 404,736 / 12,182 / 1.0 | 2,268 / 986 / 226 | 926 / 2.6 | 204.3 M |
| kube-prometheus-stack | 1,289,209 / 218,896 | 137.3 of 162.3 | 302,517 / 33,819 / 1.7 | 1,925 / 288 / 82 | 255 / 0.7 | 246.3 M |

### The mechanism (windowed symbolized samples, verified in source)

`kubernetes-apps.yaml` alone (3 × 1 s windows, 2,071 samples): 97% inside
`predicate_bdd::normalize`, 92% via `scalar_value::conjoin_predicates`, 69% via
`SymbolicLocalState::join_scalar_dispatch_arms`, 29% via `ScalarValueDispatch::truth_condition`
(reached from `EvalResult::set_scalar_dispatch` on every local read), 66% inside
`ValuesPath::cmp`. Airflow (10 × 1 s windows over the run, 7,001 samples): 96% in `normalize`, 91%
via `conjoin_predicates`, 81% via `join_scalar_dispatch_arms` — inside helper bodies and the
static-template evaluation, same mechanism. `append_guards_to_all_uses` (chart activation) appears
in no window; the v4 "Activation-DNF" thread is not a performance thread.

The chain:

1. `Interpreter::eval_node_list` closes an `if` region and calls
   `SymbolicLocalState::join_scalar_dispatch_arms`
   (`crates/helm-schema-ir/src/fragment_eval/control.rs:357-368`; also `:269` at range exits and
   `fragment_eval/inline_regions.rs:164`).
2. `joined_scalar_dispatch_arms` (`crates/helm-schema-ir/src/symbolic_local_state/branch_join.rs:94-154`)
   builds the outcome list — the arms plus the implicit fall-through, which duplicates the entry
   state (`:105-111`) — takes the union of every variable in any arm or the entry (`:113-117`), and
   for each variable × outcome × inner dispatch arm calls `conjoin_predicates` (`:124-141`); a
   variable whose product exceeds `MAX_JOINED_SCALAR_ARMS = 128` (`:12`) is dropped (`:142-144`)
   after all the work was done, and the result REPLACES the map (`symbolic_local_state/mod.rs:118-127`).
   There is no "unchanged in every arm" shortcut; the sibling `joined_truthy_reduction_arms` has
   one (`:181-191`). For a variable no arm touched, the join equals the entry dispatch under an
   exhaustive outcome set but is spelled with |outcomes|× as many arms, so arm counts multiply at
   every nested `if` until the cap drops the variable — which is also a precision loss today.
3. Every conjoin is `conjoin_predicates` → `Predicate::And(..).normalize_boolean()`
   (`crates/helm-schema-ir/src/scalar_value.rs:819-825`) → `predicate_bdd::normalize`
   (`crates/helm-schema-core/src/predicate_bdd.rs:33-55`), which builds a fresh BDD per call
   (`for_predicate` `:143-160`: `BTreeSet<Guard>` atoms + `BTreeMap<Guard, usize>` index; `build`
   `:163-199`; `apply` with per-BDD `BTreeMap` caches), enumerates true and false paths, builds a
   `GuardDnf` (`from_disjunction` → `minimize_disjunction_by`) and picks the smallest candidate by
   `(size, structural order)` (`:51-54`). `exact_implies` (`:128-140`) builds another fresh BDD per
   call; `TruthCondition::from_subsets` (`scalar_value.rs:897-926`) does two of those plus two
   normalizations per call. Nothing is memoized across calls.
4. Every `Guard` comparison in those `BTreeMap`s, every `Predicate::cmp`
   (`predicate.rs:252-282`), every `BTreeSet<Predicate>` insert and every sort reaches
   `ValuesPath::cmp` (`crates/helm-schema-core/src/value_path.rs:244-248`) —
   `self.encode().cmp(&other.encode())`, two `String` allocations per comparison — and
   `Segment::cmp` (`:30-34`) does the same with `encode_component` (`:54-60`), which is a
   DIFFERENT encoding (it escapes `\` and the literal `*` but not `.`). `Guard` derives `Ord`
   (`guard.rs:104`) with the path as the first field of most variants. Also: `PredicateNode::new`
   (`predicate.rs:86-95`) computes a structural hash by recursing through children instead of
   folding their cached `structural_hash`, and `Predicate::hash` (`:284-307`) recurses likewise, so
   node construction and any hash lookup are O(subtree).

Helper evaluation adds a second multiplier: the bound-helper memo key
(`crates/helm-schema-ir/src/analysis_db.rs:1276-1315`) carries the whole active call chain
`seen: BTreeSet<String>`, whose only effect on a summary is the cycle cut at `:1007-1012` (a
helper already on the stack returns an empty summary; the same set is consulted again at
`holes.rs:604` and propagated to static template programs at `files.rs:152-156`). Datadog misses
762 of 1,111 times on the chain alone. The airflow configmap misses seven times for a different
reason: the callers' `root_truthy_predicates`/`root_value_dispatches` differ and the key carries
those maps whole (`:1175-1182`, `:1305-1312`) although the body consults a subset.

### Candidate-fix spikes (throwaway worktree builds, never committed; loaded host)

| Candidate | grafana | cilium | argo-cd | datadog | airflow | kube-prometheus-stack |
|---|---:|---:|---:|---:|---:|---:|
| baseline CPU s (median) | 5.12 | 8.65 | 6.25 | 81.1 | 111.5 | 154.2 |
| L1 (= A1): allocation-free `ValuesPath`/`Segment` order | paired 6.99/6.97 → 6.15/5.28 (−12/−24%) | paired 15.5/14.1 → 12.6/10.6 (−19/−25%) | paired 10.4/10.1 → 10.3/9.0 (−1/−11%) | 75.9 (−6%) | 89.9 (−20%) | 118.5 (−23%) |
| L1+L2 (= A1+A2): plus per-run memo of `normalize`/`exact_implies` | 3.65 (−29%) | 5.35 (−38%) | 6.60 (load 14; flat) | 24.6 (−70%) | 43.4 (−61%) | 107.2 (−30%) |
| L1+L2+L3 (= +A3): plus unchanged-variable shortcut in the scalar join | 3.35 (identical bytes) | 5.9 (bytes differ) | 7.7 (bytes differ) | 27.3 (bytes differ) | 17.7 (bytes differ) | 24.5 (identical; the 45 s template alone 0.54 s) |
| fat LTO + codegen-units = 1 on the clean tree (= B1) | paired 6.28/5.61 → 5.98/6.31 (noise) | paired 13.8/10.1 → 12.1/7.8 (−13/−23%) | — | 65.9 (single run at load 7; the base binary measured 65.3 at the same load, i.e. flat) | — | — |
| `mimalloc` as global allocator (= B2) | paired 5.02/4.52 → 3.41/3.49 (−32/−23%) | paired 9.60/8.35 → 6.42/6.38 (−33/−24%) | — | paired 65.3 → 51.4 (−21%, load 5–7) | — | — |

Small charts with L1+L2: coredns 0.30 (0.36), metrics-server 0.22 (0.18), istiod 0.53 (0.67),
cert-manager 0.73 (0.73). L1 and L1+L2 are byte-identical on all ten charts (schema md5; lens A
and lens B re-verified independently from the artifacts). L1+L2+L3 is identical on six charts;
on cilium the unminimized documents differ in 95 places, all inside conditional carriers' guard
formulas (e.g. one carrier's `if` goes from a 2-disjunct to a 3-disjunct `anyOf` over the same
`ipam.mode`/`preflight.enabled`/`aksbyocni.enabled` atoms), plus the `$defs` renumbering that
causes; on argo-cd, datadog and airflow the sizes move by +35 B, +62 KB and −30 KB. Those are
re-spellings of (probably) equivalent predicates and precision changes where the 128-cap drop no
longer fires; none was adjudicated in this campaign.

## Target and definition of done

The law: most charts under 1 s, very large charts within a few seconds.

- **Wave-1 (byte-exact items C1, A1, A2, A2b, B2, E1, E3, and B1 if it measures):** no chart
  slower; mid charts under 4 s; datadog under 30 s, airflow under 50 s, kube-prometheus-stack
  under 110 s (all already demonstrated by the A1+A2 spike on a loaded host, so these are floors
  to beat, not hopes).
- **Wave-1 stretch (A3 adjudicated and adopted, A6):** all three large charts under 30 s; mid charts
  under 3 s.
- The law itself is not promised by this plan. After the byte-exact items land, R1 re-traces the
  three large charts and replaces this plan's residual estimates for A4, A5, A6, A7 and E2 with
  measured ones before any of them is started.

"Done" means: every item either landed with its gates green and its curve row filled, or is
recorded as rejected with the measured outcome that triggered its rejection criterion; zero
corpus acceptance flips; the 84 schema and 18 IR artifacts byte-exact for every representation
item (A3's flips individually adjudicated against Helm 4.2.3 with zero candidate-accepts/
Helm-aborts cells); diagnostics identical; luup2 `check:local` green; one fresh perfetto phase
table per large chart in the ledger.

## Items

Ordered by expected gain divided by risk, with the prerequisites that force the order. "Byte gate"
= 84 schema + 18 IR artifacts byte-identical, zero corpus flips, ten-chart schema/stderr/exit
identical to the pre-change binary, and for cache-touching items the empty-online / warm-online /
warm-offline triple. Each rejection criterion is the measured outcome at which the implementor
records the item as rejected and moves on; forcing past it is the v3 failure mode.

### C1. `ConditionFragmentCache` is under-keyed — fix before adding any other memo (S, correctness)

- Evidence (lens B, orchestrator-verified): the memo `BTreeMap<(Vec<String>, ConditionalGuard),
  Option<SchemaNode>>` (`crates/helm-schema-gen/src/condition_encoding.rs:58-59`) is filled by
  `build_condition_clauses_cached` (`:61-85`) calling `build_single_condition_fragment(guard,
  ancestor_segments, values_yaml_doc, absence, …)`, which reads `values_yaml_doc`
  (`yaml_value_at_path(values_yaml_doc, path)`) and `absence.deeper_stage`/`dependency_roots`. One
  cache is created at `overlay_lowering/conditional_constraints.rs:557` and shared by the two call
  sites `:591-600` and `:672-679`, each of which first selects a guard-set-specific document via
  `documents.condition_context(&guards, …)` (`emission_plan.rs:57-97`, populated from
  `guarded_values_default_sources` at `:109-155`). Two guard sets that select different guarded
  documents but share `(ancestor, guard)` get the first one's fragment.
- Fix: add the identity of the selected guarded document (its index in `RootValuesDocuments.guarded`,
  or `None` for the base documents) to the key. Exposure is latent unless a chart has guarded
  values-default sources; the gate decides.
- Gate: byte gate first; any flip is a bug being fixed and goes through the adjudicated battery.
- Effort S to fix, M if flips appear. No prerequisites; land it first so the campaign does not add
  memos beside a known under-keyed one.
- Reject if: never — if nothing flips, it lands as insurance; if something flips, that flip was a
  defect.

### A1. Allocation-free `ValuesPath` and `Segment` ordering — representation-only, S

- Evidence: `value_path.rs:244-248`, `:30-34`; 8–246 M comparisons per chart; `ValuesPath::cmp`
  39–66% inclusive inside the hot windows; measured as L1 above; byte-identical on all ten charts.
- Design: compare the bytes `encode()` would emit, produced lazily — separator `.`, `EachMember` as
  `*`, literal `*` as `\*`, `\` inserted before `.` and `\` inside literals — exactly the order
  `String::cmp` gives today. `.` (0x2E) and `\` (0x5C) never occur inside a UTF-8 continuation
  sequence, so the byte stream is identical to the encoded string's. `Segment::Ord` needs its own
  encoder (`encode_component` does not escape `.`). Segment-wise lexicographic order is NOT
  equivalent (`["a","b"]` vs `["a-x"]`, `["a.b"]` vs `["a/b"]` sort differently) and hash- or
  pointer-based order is forbidden (BTree iteration order is emitted). `Eq` stays derived on
  segments and agrees with the new `Ord` because encoding is injective (no empty segment is
  constructible, `:145,:192,:219-222`); `Hash` (`:250-254`) is unaffected.
- Variant worth one A/B (lens A): store the encoded spelling once per path (`Box<str>` maintained
  by the constructors at `:108-132,:136-148,:182-200`) so `cmp` is a `memcmp` and `encode()` a
  clone. The lazy-iterator spike gained 12–25% against a sampled 62–66% share, which suggests the
  nested `flat_map`/`chain` iterators are themselves slow. Adopt the cached variant only if it
  beats the lazy one by ≥5% on the `kubernetes-apps.yaml` single-template chart.
- Gate: extend `crates/helm-schema-core/tests/value_path.rs::ordering_matches_legacy_encoded_strings`
  (currently six paths) into a property test asserting `new_cmp(a,b) == a.encode().cmp(&b.encode())`
  and `(new_cmp == Equal) == (a == b)` over generated segment lists drawn from ASCII including
  `.`, `\`, `*`, `-`, `/`, space; multi-byte UTF-8; the literal `"*"` segment; `EachMember`; the
  root path; prefixes — plus the same for `Segment` against `encode_component`; then the byte gate.
- Expected gain: 6–25% CPU everywhere (measured), shrinking to a smaller share after A2 but
  speeding every `BTreeMap<ValuesPath,…>` in finalization and gen. High confidence.
- Risk: none if the property test is exhaustive over the escape cases. Effort S. No prerequisites.
- Reject if: any property-test or fixture byte mismatch, or paired gain under 5% on grafana AND
  cilium.

### A2. Per-run memo for `predicate_bdd::normalize` and `exact_implies` — representation-only, S

- Evidence: `predicate_bdd.rs:33-55, 128-140`; 83–99% of `normalize` calls and 94–99% of
  `exact_implies` calls repeat an input; measured as L1+L2 above (datadog −70%, airflow −61%,
  kube-prometheus-stack −30%), byte-identical on all ten charts.
- Purity (lens A verified; lens C could not break it): no thread-locals, statics or atomics exist
  in `helm-schema-core`; the caps are compile-time constants (`:7-9`); atom order is a
  `BTreeSet<Guard>` under derived structural `Ord`; `simplify_structure` rebuilds And/Or from a
  `BTreeSet`, so the result is independent of child order; the final pick ties by structural
  order; `PartialEq` uses `Arc::ptr_eq` only as a shortcut consistent with structural equality
  (`predicate.rs:225-238`). Both functions are pure functions of their arguments, so a memo keyed
  by the full predicate (pair) satisfies the cache law and determinism; a hit returns exactly the
  (possibly abstaining) result a miss would compute.
- Design: `HashMap<Predicate, Predicate>` and `HashMap<(Predicate, Predicate), bool>` owned by the
  analysis (per `IrAnalysisDb`/session) or, since `normalize` is called from core without a
  context, a thread-local cleared at session start. Implementation hazards named by the reviewers
  and binding here: key by the FULL predicate, never by the 64-bit hash alone; no eviction (an
  evicted-then-recomputed entry is identical, but bounded eviction adds nothing but risk); release
  the `RefCell` borrow before computing, because `normalize` recursively calls `exact_implies`
  (`:68-99`); memory is bounded by the distinct-input count (219k on kube-prometheus-stack) and
  must be measured.
- Gate: byte gate; a differential test that memoized and unmemoized results agree over generated
  predicates including approximations and cap-abstaining formulas; two sessions in one process
  produce identical output; peak RSS on kube-prometheus-stack recorded before/after
  (`/usr/bin/time -l` outside the sandbox).
- Expected gain: −55 to −70% CPU on datadog/airflow, −10 to −25% on kube-prometheus-stack on top of
  A1 (its distinct set is the join product, which the memo cannot absorb — that is A3's job).
  High confidence on direction; the attribution between A1 and A2 on the large charts was not
  isolated by paired runs and the implementor records it.
- Effort S. Prerequisite: A1 (so the memo's own hashing and comparisons are not allocating).
- Reject if: any fixture byte differs; peak RSS on kube-prometheus-stack grows more than 1.5×; or
  the paired A/B gain on datadog is under 20% and on kube-prometheus-stack under 10%.

### A2b. Fold cached child hashes; stop deep-cloning guards inside the BDD — representation-only, S

- Evidence (lens A): `PredicateNode::new` (`predicate.rs:86-95`) hashes `kind` recursively;
  `Predicate::hash` (`:284-307`) recurses instead of writing the child's cached `structural_hash`,
  so every node construction and every memo lookup is O(subtree) (SipHash `write` 12–14% of
  samples in the hot windows); `canonical_guard` (`predicate_bdd.rs:348-366`) deep-clones the
  guard's path for every atom in `collect_atoms` and every guard node in `build`;
  `contains_approximation` (`predicate.rs:636-645`) is an O(size) walk repeated at `:35,:83,:88-91,
  :129`.
- Design: hash a node as (rank, cached child hashes); keep `Hash` consistent with structural `Eq`
  (no non-test `HashMap<Predicate,…>` exists today; A2 adds one and needs exactly this); cache the
  approximation flag on the node; index atoms by reference.
- Gate: byte gate (hashes are consumed only by `HashMap`s with `RandomState` and by
  `PredicateNode::new`; order never derives from them).
- Expected gain: 10–20% of the post-A1/A2 normalization time (suspected). Effort S. After A2.
- Reject if: under 3% paired gain on the single-template chart.

### A3. Unchanged-variable shortcut in `joined_scalar_dispatch_arms` — S to write, L to adjudicate; SEMANTIC

- Evidence: `branch_join.rs:94-154` vs `:181-191`; the doubling mechanism above; measured as
  L1+L2+L3: kube-prometheus-stack 107 → 24.5 s byte-identical, its 45 s template → 0.54 s; airflow
  43 → 17.7 s and datadog 24.6 → 27.3 s (flat, higher load) with byte changes; cilium and argo-cd
  byte changes. All four reviewers independently name this join as the exponent and independently
  warn the shortcut is semantic: with a `Partial` arm, `when_true()` is a sound subset so the
  product is strictly smaller and `complete` is forced false (`:121-133`); the 128-cap drop
  discards variables the shortcut retains; and `normalize`'s min-size candidate choice can pick a
  different equivalent spelling.
- Design: when the entry has a dispatch for the variable and every outcome's dispatch equals it,
  keep the entry dispatch. Precondition to writing it: a throwaway counter reporting, per chart,
  the number of joined variables, the number unchanged in every outcome, and the number of joins
  that reached the cap — the implementor records those three numbers in the ledger before the
  adjudication run.
- Byte-exact companion (A3a, lens C): stop generating arms after the 129th feasible one, since the
  result is unconditionally discarded past the cap (`:142-144`). This changes no result and is a
  separate, byte-gated commit that lands first.
- Gate for A3 proper: pre-registered expectation "bytes change on cilium, argo-cd, datadog,
  airflow"; the full-depth battery with Helm 4.2.3 adjudication of every flip; adopted only if
  every flip is a strict precision gain or a pure re-spelling (candidate accepts a document Helm
  renders, or rejects one Helm aborts) with zero candidate-accepts/Helm-aborts cells; old/new
  `ScalarValueDispatch` differential tests over nested reassignment, `Partial` arms, and cap cases.
- Expected gain: removes the exponent — kube-prometheus-stack ×4.4 and airflow ×2.4 on top of
  A1+A2 (measured). High confidence on magnitude, medium on adjudication outcome.
- Effort S code, L adjudication. Prerequisites: A1, A2 (so the battery runs are cheap), A3a.
- Reject if: any flip is not a strict precision gain or equivalent re-spelling; any
  candidate-accepts/Helm-aborts cell; or the counter shows unchanged variables under 20% of joined
  variables AND the paired gain on kube-prometheus-stack under 15%.

### A6. Lazy `truth_condition` on `EvalResult` — representation-only, M

- Evidence (lens A, orchestrator-verified): `EvalResult::set_scalar_dispatch`
  (`crates/helm-schema-ir/src/eval_effect.rs:1217-1224`) eagerly computes
  `dispatch.truth_condition()` — two conjoins per arm plus `from_subsets` (two normalizations and
  two implications) — on every read of a `$var`, whether or not truth is consumed (16 non-test
  readers of `.truth`); 29% of the single-template samples sit under this path.
- Design: keep the dispatch, compute `truth` in a `OnceCell` behind an accessor. Pure; only memo
  insertion order changes.
- Gate: byte gate + identical stderr. Expected gain: 5–15% of the single-template chart after A2
  (suspected). Effort M (accessor refactor over 16 sites). After A2.
- Reject if: under 5% paired gain on the single-template chart after A1/A2.

### A4. Helper memo key: replace the whole call chain by its consulted footprint — M–L, datadog-only

- Evidence: `analysis_db.rs:1023-1042, 1276-1315`; cycle cut `:1007-1012`, `holes.rs:604`,
  `files.rs:152-156`; datadog 762/1,111 call-chain-only misses, 63.6 s inclusive pre-A2; airflow
  226 (1.3 s), kube-prometheus-stack 82 (0.8 s).
- **Design correction (all four reviewers; orchestrator-verified):** the orchestrator's first
  design — key on `seen ∩ static_include_closure(helper)` using `unconditional_include_closure` —
  is UNSOUND: `unconditional_include_names` (`crates/helm-schema-ast/src/expr.rs:750-786`) skips
  `if`/`range`/`with`/`define`/`block` bodies and non-literal names by design, so the closure
  under-approximates reachability; if A conditionally calls B and B calls A, evaluating A inside B
  must suppress B and the closure would omit it. Two sound designs remain: (i) record, during each
  miss, the set of names whose `seen`-membership was tested (transitively, including nested hits'
  footprints) and accept a later lookup among same-`(name, resolution)` entries iff
  `seen ∩ footprint == stored_seen ∩ footprint` — exact by the trace-reproduction argument, provided
  every membership test site is recorded (debug-assert at each); or (ii) a conservative all-call
  transitive closure over every literal `include`/`template` in every branch, falling back to the
  whole `seen` when any name is dynamic. (i) is exact; (ii) is simpler and still sound.
- Gate: byte gate; tests for direct, conditional, mutual and dynamic-name recursion asserting the
  cut still fires; a two-chain test asserting one evaluation.
- Expected gain: up to ~40% of datadog's post-A2 helper time; ≤1% elsewhere. Medium confidence.
- Effort M (ii) / L (i). Prerequisites: A2, R1 (re-measure seen-only miss share first).
- Reject if: after A2 the seen-only share of helper-miss time on datadog is under 10%, or any
  fixture byte differs, or any recursion test changes.

### A5. Helper memo key: caller-context footprint — study only, L

- Evidence: airflow's `@file:templates/configmaps/configmap.yaml` misses seven times on
  `bindings`/`dot`/`root_truthy_predicates` (`analysis_db.rs:1175-1182, 1305-1312`), not on `seen`.
- Design to study: as A4(i) but for the root-fact maps — record which keys the body consulted and
  key later lookups on the projection. Exact only if every consultation goes through recorded
  accessors; the study enumerates the read sites of `root_truthy_predicates` and
  `root_value_dispatches` with `file:line` and either proves totality or rejects.
- Reject if: the enumeration cannot be shown total, or the post-A2/A3 cost of those seven
  evaluations is under 2 s.

### A7. Second scalar-projection pass: make it lazy, never skip it — M, measured-after

- Evidence: `fragment_eval/summary.rs:132-161` runs a second interpreter with
  `scalar_output_projection = true` whenever the structural dispatch is incomplete; it is a
  different projection with its own fan-out semantics (`inline_regions.rs:502-550`), not a replay,
  so skipping it by first-pass shape is semantic (lens A, lens C agree). Its sole consumer is
  `bound_helper_resolver.rs:128`; entire-hole splices never read it. Cost: ≤2.6 s inclusive
  everywhere except datadog (23.8 s inclusive, containing nested misses).
- Design: compute in a `OnceCell` on first read, using the ORIGINAL `seen` snapshot (cycle cuts)
  and releasing the `bound_helper_calls` borrow. Gate: byte gate + identical stderr.
- Prerequisite: R1 (re-measure the pass's share and its consumed fraction after A2/A4).
- Reject if: consumed fraction above 80%, or under 5% paired gain on datadog.

### E1. Linear-time repeated-subtree detection in the minimizer and provider-definition extraction — representation-only, M

- Evidence (orchestrator; lens B measured the constants): `minify/src/lib.rs:76-84`
  (`candidate_fingerprint` = `contains_unsafe_reference_scope_keyword`, a non-short-circuiting
  full walk, then `canonical_json_string`, a clone+sort+serialize of the whole subtree, for EVERY
  subschema), repeated in `rewrite_schema` `:159-179`; `normalize_logical_schema` `:276-317`
  re-digests nested junctors per level; `:33` deep-clones the 15.9 MB pre-minify document to strip
  `$defs`. `provider_definitions.rs:301-310` → `core_hash_len` `:266-286` → `canonical_hash_len`
  `:224-262` recomputes each subtree's hash at every ancestor and again in passes two and three
  (`:322`, `:346`); the comments at `:181-185` and `:220-223` claiming one linear pass are false
  about the traversal. Measured amplification: 6.4× (kube-prometheus-stack, 256,297 schema
  positions), 6.9× (airflow), 6.6× (grafana) — a chart-independent constant, so these are fat
  linear passes, not the super-linear term. Kube-prometheus-stack: 3.2 + 2.5 s; airflow 3.3 + 4.0 s.
  Only 29,751 distinct fingerprints and 6,765 repeated groups exist on kube-prometheus-stack; 95%
  of the string work is discarded.
- Design: one post-order pass per document computing `(structural digest, exact canonical byte
  length, unsafe-scope flag)` per node from its children's values (one primitive in
  `helm-schema-json-schema-walk`, replacing the two implementations that exist today —
  `canonical_hash_len` and `logical_sort_digest`); materialize `canonical_json_string` only for
  digest-selected candidates; confirm every replacement by exact canonical-string comparison
  (hashes prefilter, never decide). Naming order is untouched: `plan_definitions:86-119` sorts by
  `(length desc, occurrences desc, canonical asc)` and `compact_definition_names:180-217` by
  reference count — both need strings only for repeated candidates.
- Gate: byte gate (the `$defs` naming is the fixture-sensitive part; the v4 C2 round showed the
  gate catches a permutation); forced-collision test.
- Expected gain: kube-prometheus-stack ≈ 3–4 s, airflow ≈ 4–5 s of CPU (2–4% of baseline, 10–15%
  of the post-A1/A2 residual). High confidence. Effort M. Independent of the IR items.
- Reject if: any fixture byte differs, or the two spans do not drop by at least 50% each.

### E2. Validator compilation in `conditional_target_schema` — S, measure-then-commit

- Evidence, corrected by lens B: `schema_accepts_json_value`
  (`crates/helm-schema-gen/src/resolve_policy/declared_default.rs:207-219`) compiles a `jsonschema`
  validator per call. The orchestrator's first-round attribution to `preserve_declared_defaults`
  was wrong (that span is 16 ms on grafana, 207 ms on kube-prometheus-stack); the samples put 171
  of 180 calls under `resolve_policy::conditional_target_schema` (`resolve_policy.rs:900-904`,
  the `rejects_declared_default` closure) inside `collect_conditional_schemas` — 42.5% of that
  span's grafana samples, two-thirds of it regex compilation for `pattern` keywords. The whole-run
  "8%" was inflated by the sample's depth bias; the span is 4.3 s on kube-prometheus-stack, 2.9 s
  on airflow.
- Design: first a throwaway counter for calls, distinct `(wrapped document, instance)` keys and
  compile time; then, if hits ≥ 50%, a per-generation memo keyed by the complete wrapped document
  (including the injected Helm-truthy `$defs`) plus the instance — never by address (schemas are
  `mem::take`n and replaced during default preservation, `:24-43,:95-107`), never by digest alone.
- Gate: byte gate; debug-assert memo == recompute. Expected gain: 1–2 s on kube-prometheus-stack,
  ~0.3 s on grafana (suspected). Effort S after the counter.
- Reject if: hit rate under 50%, or the span does not drop by 30%.

### E3. Small emission-side cleanups (each S, byte-exact by construction)

- `contract_normalization.rs:476-484` `contract_predicates` clones an owned `BTreeSet<Predicate>`
  per row per pass although `disjuncts()` lends a borrow; `:223` clones it again per index inside
  the bucket loop purely to compare against borrowed neighbours; `has_self_default_guard:469-474`
  clones `source_expr` per call; `drop_default_guard_subsumed_duplicates:171-184` builds the
  four-clone `RenderSite` twice per row. Borrow instead. (`normalize_contract_uses` self: 3.6 /
  2.1 / 7.4 s on the large charts; the bucketed quadratic itself is fine.)
- `merge.rs:33,44,181` `sort_by_key(canonical_json_string)` recomputes the key per comparison (and
  insertion sort on small slices is O(n²) key calls); decorate–sort–undecorate keeps the stable
  order identical. ~2% of grafana.
- `output_pipeline/format.rs:44-59`: the pretty (default) path serializes fully, then serializes
  again compact when the pretty form exceeds Helm's 5 MiB limit (kube-prometheus-stack: 15.5 MB
  pretty, 7.0 MB compact, 22.5 MB written to emit 7.0 MB); `final_output_metrics` walks the whole
  document canonicalizing every `if`/`then` payload (2.85 MB of serialization on
  kube-prometheus-stack) and the CLI discards the result (`cli/src/lib.rs:139,148`). Use a counting
  writer for the size probe; make the metrics opt-in for the tests that read them. Note the
  campaign protocol timed only `--compact`; the default invocation path was never timed.
- Reject if: (individually) no measurable movement — these are dead-work removals and land on
  byte identity alone.

### B1. Release profile: `lto`/`codegen-units = 1` — S, measure-then-commit

- Evidence: `Cargo.toml:22-26` (both commented out). Measured on the clean tree with fat LTO and
  `codegen-units = 1`, byte-identical on grafana, cilium and datadog: grafana within noise (paired 6.28/5.61 vs 5.98/6.31 s), cilium −13/−23% (paired 13.8/10.1 vs 12.1/7.8 s), datadog flat (65.9 vs 65.3 s at equal load); binary 13.06 MB vs 16.06 MB. Loaded-host pairs; the cilium gain may be real, the others are noise. Lens C could not construct a
  semantic or cache-law failure (the only `unsafe` is the fixed tree-sitter symbol wrapper).
- Gate: byte gate; three interleaved pairs per large chart; measure `lto` and `codegen-units`
  separately; record build-time cost.
- Reject if: paired CPU gain under 5% on the large-chart median, or release build time more than
  doubles.

### B2. Global allocator (`mimalloc`, already a musl-only dependency) — S, measure-then-commit

- Evidence: malloc/free/realloc were 20–30% of leaf samples in every window; `mimalloc` is
  already the global allocator on musl (`crates/helm-schema-cli/src/main.rs:5-7`, `Cargo.toml`
  target section) and off everywhere else. Measured with it enabled on macOS, byte-identical: grafana −32/−23% (paired 5.02/4.52 vs 3.41/3.49 s), cilium −33/−24% (paired 9.60/8.35 vs 6.42/6.38 s), datadog −21% (paired 65.3 vs 51.4 s at load 5–7). This is the cheapest byte-exact item in the plan and composes with A1/A2 (which reduce allocation counts rather than allocation cost); the implementor measures it on top of A1+A2 before adopting.
- Gate: byte gate. Effort S (drop the `cfg`, move the dependency).
- Reject if: paired CPU gain under 5% on grafana and datadog after A1–A2, or any fixture byte
  differs.

### R1. Re-trace after the byte-exact items and rewrite the residual

After C1, A1, A2, A2b, E1 land (and A3 if adopted), re-run the perfetto traces and the counter
questions (distinct inputs, misses, seen-only misses, second-pass share, validator hit rate) on
the three large charts and replace this plan's estimates for A4–A7 and E2 with measured ones in
the ledger before starting any of them. Expected residual hot spots, in order:
`TruthCondition::from_subsets`/`truth_condition` (A6), helper evaluation itself (A4/A5/A7),
`normalize_contract_uses`/`derive_schema_signals` (shrunk by A1; E3), the emission walks (E1/E2).

## Anti-findings (considered and rejected for wave 1, with reasons)

- **Per-template parallelism.** Every reviewer and the orchestrator reject it for wave 1, with
  different emphases worth keeping: the mechanism is intra-template (one file is 45 s), so the wall
  floor would be the hottest template; the shared `IrAnalysisDb` is `Rc`/`RefCell`/`OnceCell`
  (`analysis_db.rs:136-151, 1245-1261`) and per-thread DBs multiply the helper misses the residual
  depends on; a shared memo cannot hold its lock across the recursive helper evaluation
  (`bound_helper_resolver.rs:52-63`); byte-exactness would need ordinal-tagged results and
  lowest-path-order error selection (the append order itself is deterministic today,
  `graph.rs:72-82`, and the diagnostic sink sorts by key, `diagnostic/sink.rs:6-40`, but
  first-writer diagnostic payloads can differ, `diagnostic.rs:127-135,233-242`). "Single-threaded by
  construction" (the orchestrator's first phrasing) describes current ownership, not a law of the
  pipeline; the correct statement is *deferred until R1 shows a serial floor above the law*, with
  the design conditions above as the entry ticket.
- **Skipping the second scalar-projection pass by first-pass shape.** Semantic (A7 explains why);
  only laziness is representation-only.
- **Template re-parsing** (`parse_go_template`, 81k–102k spans, ~30× the action count): 0.5–1.1 s
  self on the largest charts. Not worth a cache.
- **The `$defs` ordering/grouping owner as a performance item.** Separable (lens B, orchestrator
  agree): the cost is the fingerprint strategy (E1), fixable while keeping the exact naming
  order; redesigning the owner changes bytes and belongs to a semantic wave. Coupling them would
  destroy the byte gate that makes E1 cheap. Drop the synergy thread; the only shared asset is the
  bottom-up digest primitive.
- **Moving DNF minimization to phase boundaries** (v4 B5c's optional half): the profile no longer
  shows `minimize_disjunction_by` as dominant; A2 makes it moot.
- **Product-size prechecks in the scalar join** (rejecting a join from an estimated arm count):
  contradictions make many products infeasible; only actual conjoins decide the cap. A3a is the
  byte-exact version of this idea.
- **Lowering any cap** (`MAX_JOINED_SCALAR_ARMS`, `MAX_SCALAR_DISPATCH_STATES`, `MAX_BDD_NODES`,
  `MAX_NORMAL_FORM_*`, `MAX_STAMPED_GUARDS`): semantic; flips guaranteed.
- **Sharing one BDD across an arm product or a join:** the variable order is the per-pair sorted
  atom set; a union order changes the cube decomposition and hence the chosen normal form → bytes.
- **Hash-only deduplication or hash-based `$defs` naming:** exact canonical strings protect both
  collision correctness and stable naming; digests may only prefilter.
- **A global `ValuesPath` interner:** hidden shared state with no ordering benefit over A1's cached
  key.
- **Trace instrumentation overhead**: ~10% only when tracing is on; not a production cost.
- **Provider lookups, chart loading, values composition, CRD universe, output writing**: all under
  0.7 s on the largest chart.
- **Any of the seven rejected architectures** (`plan/architecture-review-v4.md` Part S): nothing in
  this profile needs incrementality, saturation, or a different lattice; it needs the existing
  algorithms to stop repeating themselves.

## Corrections to the orchestrator's first-round findings (recorded so the ledger does not inherit them)

- F4's key design (`seen ∩ static_include_closure`) was unsound — replaced by A4's footprint or
  all-call-closure designs.
- F6's third bullet mis-located the validator cost under `preserve_declared_defaults`; it is under
  `conditional_target_schema` (E2), and the "8%" whole-run figure was depth-bias-inflated.
- F6's "superlinear walks" overstated the emission side: the amplification is a chart-independent
  ~6.5× constant (E1); emission is 6–15% and not the answer to Q1.
- F8's "single-threaded by construction" is an ownership fact, not a constraint (anti-findings).
- The chart-level self time on airflow (41 s) is static-template/NOTES evaluation, not the
  activation-guard pass, which appeared in no sample window.
- F5 ("helper bodies evaluated twice") is measured now: 89–98% of misses run the second pass, at
  0.4–29% of the run depending on chart; it is a different projection, not a replay (A7).

## Verified clean (do not re-open)

- Cold vs warm cache output identity (10/10 charts online vs offline); the capability oracle and
  provider chain are not on any hot path.
- `normalize`/`exact_implies` purity: no thread-locals, statics or atomics in core; structural
  `Ord`/`Eq`/`Hash`; constant caps; result independent of And/Or child order; a memo hit returns
  the abstaining result a miss would.
- `ValuesPath` `Eq` ⇔ `Ord` consistency under any comparator that reproduces the encoded-byte
  order exactly (injective encoding; no empty segments constructible).
- Helper summaries depend on the caller chain only through the cycle cut and the memo key itself
  (`summary.rs:6-10,121`; `splice_summary` `:410-423`); provenance, site and `inline_files` are
  reset per evaluation.
- `apply_chart_activation_guard_sets` is not a hot spot on any large chart; the v4 Activation-DNF
  spike stays closed.
- `contract_normalization.rs:63-82` (distinct-condition ranking) and `:191-210` (render-site
  buckets) are correct and effective; only the clones in E3 remain.
- `overlay_lowering.rs:208-227` indexes resolved paths once; `program_wrapper.rs:43-93` is bounded
  and early-returning (0.001 ms); `prune_unreachable_provider_definitions` is a worklist closure;
  `provider_resolution.rs:118-128` already memoizes its validator behind a `OnceCell`.
- Determinism of every timed run: identical md5 across repeats for every chart and every spike.

## Execution order

C1 → A1 → A2 → A2b → B2 → B1 (both are one-line measure-then-commit experiments; do them here so
later A/B runs use the shipped allocator and profile) → E1 (independent; may interleave) → A3a → A3 (counter first,
then adjudication) → R1 re-trace → A6 → A4 → E2 → E3 → A7 → A5 study. Every item ships alone with
its own ledger dossier: pre-registered expectation (byte-exact or adjudicated), the three-way
cache comparison for memo items, paired A/B numbers with load, and the full gate list from
`CLAUDE.md`. Abandoning mid-wave leaves the tree strictly faster.

## Appendix — reproduction

```
# private cache snapshot, warmed once online, then offline only
export HELM_SCHEMA_K8S_SCHEMA_CACHE=$SNAP/k8s HELM_SCHEMA_CRD_SCHEMA_CACHE=$SNAP/crd
/usr/bin/time -p $BIN $CHART --compact --offline --k8s-version v1.35.0 \
  --diag-format json --output $OUT 2> $ERR      # hash $OUT, $ERR (minus the time lines), exit
# phase trace
$BIN $CHART --compact --offline --k8s-version v1.35.0 --trace-output $T.pftrace --output $OUT
# symbolized twin for windowed sampling
CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
  cargo build -p helm-schema-cli --release --target-dir $SYMDIR
$SYMBIN $CHART ... & sleep N; /usr/bin/sample $! 1 1 -mayDie -file win.txt   # 1 s windows only
# single-template chart for the join mechanism: kube-prometheus-stack with every top-level
# manifest except prometheus/rules-1.14/kubernetes-apps.yaml deleted and charts/ removed
```
