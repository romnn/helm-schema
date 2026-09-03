# Corpus expansion v1 — findings

A wide survey over real-world Helm charts, run to answer one question the
existing battery structurally cannot: **does helm-schema's precision generalize
beyond the 63 charts it is tested on?**

It does not. 8% of real-world charts receive a schema that rejects the chart's own
defaults, five distinct root causes are confirmed with `file:line` (plus two more
defect families reproduced but not yet root-caused), and two committed fixtures
already have wrong answers frozen into them.

## Method

349 charts from the Artifact Hub popularity ranking, deduplicated against the
existing corpus, fetched at pinned versions. Per chart:

1. `helm template <chart>` on chart defaults (Helm 4.2.3) — does Helm render it?
2. Generate a schema with the corpus-canonical options (`--exclude-tests`,
   `--k8s-version v1.29.0-standalone-strict`, provider caches seeded from
   `testdata/provider-bundle`, network enabled for cache misses).
3. Validate the chart's **coalesced** values document against that schema.

The oracle is the crisp one: **Helm renders the defaults, our schema rejects
them.** That is a false rejection with no interpretation required. Charts where
Helm itself refuses its own defaults (`required`/`fail` on unset values) are
excluded from the defect count — for those, agreement is correct behavior, and
the repo already models that with `KNOWN_VALUES_REJECTIONS`.

Invocation faithfulness was checked first: the CLI with those options reproduces
`testdata/chart-corpus-schemas/coredns.schema.json` byte-for-byte apart from the
two `x-helm-schema-*` metadata keys the CLI adds.

### Methodology correction — the repo's own gate validates the wrong document

Helm validates `values.schema.json` against the **fully coalesced** values
document: root values, plus every subchart's defaults merged under its key, plus
propagated globals. `CLAUDE.md` already states this ("Schemas validate the
COALESCED values document").

Verified directly, with a control. A two-chart witness whose parent schema is
`{"required": ["sub"], "properties": {"sub": {"required": ["subOnly"]}}}`, where
`subOnly` exists **only** in the subchart's own `values.yaml` and nowhere in the
parent's: `helm template` **accepts**. Control — a schema requiring a key that
exists nowhere is **rejected** (`values don't meet the specifications of the
schema(s)`), proving the schema is genuinely enforced rather than skipped.

`crates/helm-schema-cli/tests/common/values_validation.rs` does not. It reads the
raw root `values.yaml` and validates that. So does `chart_corpus.rs`'s
self-validation gate. This both manufactures phantom errors and hides real ones —
on gitlab it produced 49 phantom errors *and* masked the genuine
`certmanager-issuer.email` rejection.

The survey's first pass had the same flaw. Every number below is from the
corrected oracle: a throwaway `{{ .Values | toYaml }}` template rendered inside a
copy of the chart, which is the only way to extract what Helm actually validates.
Re-probing moved **3 of 27** flagged charts from "reject" to "accept" — the
correction is real and changes verdicts, but it does not explain the findings
away.

The correction was applied in **both** directions. The coalesced document contains
strictly more keys than the raw one, so it can also turn an accept into a reject.
All 271 charts that passed the raw oracle were re-probed: **266 still accept, 4
are artifacts of the dump technique, 1 fails to parse.** So the raw oracle was not
hiding a large second population.

**Known limitation of the dump technique.** `{{ .Values | toYaml }}` observes
`.Values` *after* any render-time mutation that ran before it, whereas Helm
validates the coalesced values *before* rendering. Istio's charts do
`set $.Values "_original" (deepCopy $.Values)` (`base/templates/zzz_profile.yaml:55`),
so `base`, `gateway`, `cni` and `ztunnel` show a spurious "additional property
`_original`". These are the 4 artifacts above, and the limitation only bites
charts that mutate `.Values`. It cannot explain any confirmed finding: of the 158
errors across the 24 confirmed rejections, **zero** are additional-property
errors. Where an exact answer was needed, the coalesced document was rebuilt by
hand instead (parent values + each dependency's defaults + `global` propagation);
on gitea that reproduced the dump's 16 errors exactly.

**This is itself a finding.** The self-validation gate is the cheapest correctness
oracle the project has, and it is currently pointed at the wrong instance.

**But correcting it changes nothing for the existing corpus — measured.** All 63
corpus charts were re-gated against their committed fixtures using the coalesced
document: **50 of 50 renderable charts still accept, zero rejections.** The other
13 are explicable and excluded — 4 are the documented `KNOWN_VALUES_REJECTIONS`
(aws-load-balancer-controller, karpenter, loki,
schema-emission-unconditional-fail), `cert-manager` is vendored from source with
`Chart.template.yaml` and no `Chart.yaml`, `common` is a library chart, and 7
synthetic charts have no corpus fixture.

So the gate's logic error is real but currently *inert*: no existing corpus chart
exercises a shape where raw-vs-coalesced changes the verdict. Fixing it buys
future correctness, not an immediate crop of failing tests. That is itself the
thesis of this document in miniature — **the gap is the sample, not the gate.**

## Headline results

| Measure | Result |
| --- | --- |
| Charts attempted | 349 |
| Analyzed successfully | 338 |
| Helm renders defaults | 298 |
| **Schema rejects what Helm renders** | **24 (8.1% of 298)** |
| Hard failure (analyzer aborts, Helm succeeds) | 1 |
| Timeout at 900 s | 9 |
| Distinct root causes confirmed with `file:line` | 5 (+2 defect families recorded, not root-caused) |
| Committed fixtures found already contaminated | 2 |

Zero panics and zero nondeterminism: every re-run reproduced its schema
byte-for-byte. The analyzer is robust; it is *wrong* more often than the existing
battery can see.

**The defect surface is composition, not single-chart analysis.** 20 of the 24
confirmed rejections are umbrella charts with bundled subcharts; only 4 are
standalone (nginx-ingress, kubeshark, imgproxy, aws-ebs-csi-driver). Three of the
five root causes — D3, D4, and the gitlab leak — exist only across a chart
boundary, and the performance cliff below tracks subchart count rather than chart
size. Single-chart analysis is in good shape; everything that crosses a chart
boundary is not.

## Confirmed defects

D1–D5 were each reduced to a minimal witness chart proving Helm renders it and we
reject it; D6 and D7 are recorded with reproductions but not root-caused.
Verification status is stated per item — *orchestrator-verified* means I read the
cited code and confirmed the claim myself.

### D1 — lost guard via CST eviction (false rejection)

`crates/helm-schema-ir/src/fragment_eval/eval.rs:2281-2285`. The `Node::Control`
arm of `node_belongs_inside` uses `.all(...)` over branch-body nodes, so a
**single** bare output at the container's own column evicts the entire control
region from the container. The evicted branch content is then evaluated with
`active_predicates == []` — its enclosing guard is silently dropped and every
fact inside it is emitted unguarded.

Minimal trigger, needing neither `fail` nor `hasKey`:

```gotemplate
data:
  k: "v"
{{- if .Values.a }}
{{- printf "" }}
  u: {{ .Values.b.c }}
{{- end }}
```

With `a: false` Helm renders and we reject; delete the `printf` line and we
accept. Reproduces with `.a.b.c`, `index`, `dig`, `get`, `toYaml`, and with
provider and range contracts. Does not reproduce with `with` guards, with
assignments or comments in the branch body, or with outputs carrying
`indent`/`nindent`.

*Orchestrator-verified:* the `.all(...)` is there, and the neighbouring
`Node::Output` arm uses `.any(...)` — the asymmetry is real. The two other
functions asking the same question (`eval.rs:1909`, `eval.rs:232`) use `min`/`any`.

`.all → .any` is **not** the fix: `.any` on an empty body regresses cases that are
currently correct.

### D2 — `set` into `.Values` is a no-op (false rejection)

`crates/helm-schema-ir/src/expr_call_eval/root_mutation.rs:107-143`.
`eval_set_call` dispatches on three target shapes — local variable,
`$copy.Values`, and root context `.` — and has **no branch for a `.Values.<path>`
target**. The write falls through recording nothing, so the analyzer keeps
believing `.Values.<path>.KEY` may be absent. When the chart then navigates
through that key, member-access nil-abort analysis emits
`{"if": "<path>.KEY absent or null", "then": false}` for a state the chart
repaired one line earlier.

The proof is devastatingly simple: two witness charts differing by exactly one
line — `{{- $_ := set .Values.config "cache" dict -}}` — generate **byte-identical
schemas**. The `set` has literally zero effect.

Real instance: gitea's `_helpers.tpl:350-387` creates each config section this
way; exactly the 8 sections that are later navigated through get a reject arm,
and the three that are created but never navigated through get none. That
asymmetry is the tell.

*Orchestrator-verified:* the three-branch chain with no `.Values.<path>` arm.

### D3 — cross-chart template-path collision (cross-chart value leakage)

`crates/helm-schema-ir/src/analysis_db.rs:1051-1055` and `:59`.
`template_relative_path` strips everything before the last `templates/`, and the
result is used as a `BTreeMap` key. Sibling charts sharing a template filename —
`configmap.yaml`, `secret.yaml` — therefore collide in one map, and iteration
order makes it **lexicographically last-wins**. `include (print $.Template.BasePath
"/configmap.yaml")` in chart A then resolves to chart Z's body, and chart Z's
values get attributed to chart A's scope and become required.

The intended abstention already exists and is **structurally dead**:
`implicit_template_name` (`:273-284`) ends with
`matches.next().is_none().then_some(first)`, meaning "abstain when ambiguous" —
but the map key *is* the suffix, so there is never a second match. The ambiguity
is destroyed at line 59, before the guard runs. This discards uniqueness that
`crates/helm-schema/src/chart/define_index.rs:47-59` deliberately constructs,
with a comment explaining why.

Causally confirmed on four charts by renaming **only** the colliding file, leaving
`helm template` output unchanged: spinnaker 5→0 errors, weblate 10→0, milvus
33→0, graylog 12→0. On weblate the root `allOf` actually *grows* (465→472) once
the collision is gone — the fix restores analysis rather than deleting it.

*Orchestrator-verified:* the `rfind("templates/")`, the map insert, and the dead
uniqueness guard.

**This is the dominant mechanism.** 17 of the 24 confirmed rejections carry its
structural precondition (duplicate template basenames across charts *and* a
`Template.BasePath` include). That screen is necessary, not sufficient — but of
the 9 charts actually adjudicated, 6 were confirmed D3.

### D4 — `.Subcharts.<name>` is not a values-scope switch (false rejection)

`crates/helm-schema-ir/src/expr_eval.rs:196-213` hard-wires `.Values.…` to the
calling chart's root; only the `Overlay` shape from `set $copy.Values` re-routes
it. With `analysis_db.rs:1124-1131`'s `.or_else(|| params.current_dot.cloned())`
fallback, a helper invoked as `include "postgresql.foo" .Subcharts.postgresql`
has its `.Values.auth`, `.Values.primary.…` facts landed on the **parent** root.
`grep -rn Subcharts crates/` finds only two abstain-list mentions.

Netbox exhibits D3 and D4 simultaneously and separably: rename the colliding file
only → 5 errors left; neutralize `.Subcharts` only → 11 left; both → 0. Leaks via
`.Subcharts.sub`, `$.Subcharts.sub`, `index .Subcharts "sub"`, and
`dict "Values" .Values.sub` alike.

`.Subcharts` appears in 6 of the 24 confirmed rejections.

### D5 — empty block scalar swallows a dedented guard (false rejection)

An empty `|` / `>` / `|-` block body followed by a control region dedented below
the block's content column drops the guard entirely. openldap-stack-ha's
`configmap-replication-acls.yaml:65-83` is the real instance: `acls.ldif: |` with
an empty body, then a column-0 `{{- if .Values.customAcls }}`, yielding
`customAcls absent-or-null → false`. Indenting the region gives byte-identical
`helm template` output and makes the schema accept.

Distinct from D1: it survives all three of D1's negative signatures — still
rejects with an assignment in the branch body, with a comment, and with
`with`/`range` instead of `if` — and needs no bare output at the container column.

**Root cause honestly partial.** Localized to the block-scalar adoption path
(`fragment_eval/eval.rs:2030-2075` → `fragment_eval/holes.rs:1260-1262`), but the
predicate-loss line is not pinned; `activate_inline_if` does push the guard. The
leading hypothesis — `finish_block` (`helm-schema-syntax/src/parse.rs:602-616`)
synthesizing a zero-length body span, changing the `region.span.start <
block.body.end` escape test at `eval.rs:2043` — is **unconfirmed**.

### D6 — scalar type over-narrowing on `toYaml` passthrough (false rejection)

A different family from D1–D5: not a lost guard or a mis-scoped path, but a type
inferred too narrowly. Where a chart passes a structured value straight through
`toYaml`, we conclude `type: "string"` and then reject the chart's own object,
array, or boolean default.

`synapse` is the clearest instance — 25 of its errors are this shape:

```
/homeserver/listeners        [{"bind_addresses":…,"port":9000,…}]   is not of type "string"
/homeserver/report_stats     true                                    is not of type "string"
/logconfig/version           1                                       is not of type "string"
/homeserver/database         {"args":{…},"name":"sqlite3"}           is not of type "string"
```

`okteto` shows the nullable variant on 8 paths: `image` is inferred
`["null","string"]` and rejects `{"repository":"okteto/registry","tag":"1.48.0"}`.

Not root-caused. The likely locus is the rendered-scalar sink treating a `toYaml`
operand as evidence about the *input* type rather than about the rendered output —
`plan/chart-corpus-status.md` records earlier work in exactly this area
("provider boundary as a rendered-value preimage instead of a raw values kind"),
so this may be a regression or an uncovered corner of it. Worth its own
adjudication before any fix.

### D7 — hard abort on duplicate YAML keys

`openldap` fails outright: `Error: Yaml(Error("duplicate entry with key
\"ingress\"", line: 5, column: 1))`. Helm renders the same chart without
complaint — YAML 1.1 last-wins is what Helm's loader does. We abort the whole
analysis. Not root-caused; recorded with its reproduction.

## Two committed fixtures already contain wrong answers

This is the finding that most directly indicts the current gate.

**`testdata/chart-corpus-schemas/signoz-signoz.schema.json`** — the
`clickhouse.zookeeper` node declares `externalSecrets`, `primary`, and
`readReplicas`. Those three strings appear **zero times** anywhere in
`testdata/charts/signoz-signoz/charts/clickhouse/charts/zookeeper` (859-line
`values.yaml`, whole subchart grepped). They are D3 leakage from a sibling chart,
frozen as expected output. *Orchestrator-verified.*

**`testdata/chart-corpus-schemas/kube-prometheus-stack.schema.json`** — confirmed
by construction. Starting from the chart's own coalesced defaults, set:

```yaml
thanosRuler:
  enabled: true
  thanosRulerSpec:
    containers: [{name: sidecar, image: "x"}]
```

`helm template` renders this cleanly (7,020 lines). The **committed fixture
rejects it**: `/thanosRuler/thanosRulerSpec/containers: … is not valid under any
of the schemas listed in the 'anyOf' keyword`.

The discriminator is sharper than expected and makes the wrongness unarguable:

| `containers` | committed fixture |
| --- | --- |
| `[]` | accept |
| `[{name: sidecar}]` | accept |
| `[{name: sidecar, image: "x"}]` | **reject** |

Adding a legal optional field to a valid Kubernetes container flips acceptance to
rejection. *Orchestrator-verified.*

**Mechanism not attributed.** This was reported as a D1 instance at
`templates/thanos-ruler/ruler.yaml:177` — a CST-level detector over all 2,623
corpus templates finds 6 D1 sites across 5 corpus charts — but the witness above
does not obviously exercise that site, and I did not confirm the link. The
contamination is certain; its cause is open. (Separately, note that the chart is
genuinely broken when `containers` *and* `extraEnv` are both set: Helm itself
fails with a YAML parse error at `ruler.yaml` line 53. Do not mistake that for our
defect.)

Both fixtures will therefore flip when their causes are fixed. Those flips are
**corrections**, and must be adjudicated as such rather than treated as
regressions. The kube-prometheus-stack witness above is a ready-made acceptance
test to hold the fix to.

## Why 121,055 probes never caught these

Structural, not incidental:

- **The self-validation gate validates the wrong document** (see above) — the one
  oracle that would have caught D2 and D5 directly.
- **The probe battery is deletion-anchored on defaults.** It composes deletions
  over the chart's own defaults, so it cannot reach states needing a flag flipped
  *on* plus a sibling guard *off* — exactly kube-prometheus-stack's D1 instance.
  Paths the defaults omit entirely are unreachable, which is nginx-ingress's.
- **Differential flip-counting is blind to a stable wrong answer.** D2's bogus arm
  is present in both old and new schemas, so no flip is ever counted.
- **Composition, not count, is the gap.** 19 of 63 corpus charts are umbrellas —
  umbrella coverage is fine. But only 1 of 63 actually mis-resolves a colliding
  template path, and 6 of 63 use `set` on `.Values` with **none** navigating
  through a `set`-created key. The corpus samples the *constructs* and misses the
  *interactions*.
- **Leaked facts land permissively.** D3 leakage arrives as open `{}` properties,
  which no probe rejects. It only becomes visible as a rejection when the leaked
  path is also required.

The corpus is internally consistent — 50 of 50 renderable charts accept their own
coalesced defaults against their committed fixtures. It is not *wrong*; it is
*narrow*. Every defect above needed a chart the corpus does not contain.

## Performance

Independent of correctness, and worse than the earlier single-chart sample
suggested. Warm cache, one chart at a time, on a loaded host — treat as ordering,
not benchmark:

| Percentile | Time |
| --- | ---: |
| median | 1.54 s |
| p75 | 7.6 s |
| p90 | 76.6 s |
| p95 | 356 s |
| max | 901 s+ (timeout) |

142 of 348 charts finish under 1 s. **22 exceed 300 s. 9 never finish within 900
s** (kong, zitadel, redpanda, seaweedfs, langfuse, sumologic, ingress, redash,
console). `CLAUDE.md`'s stated law is "most schemas under a second, very large
charts within a few seconds."

The most diagnostic measurement is not a curve but a **cliff**, measured on
controlled synthetic umbrellas: 0 subcharts 1.5 s → 1: 6.2 s → 2: 23.3 s → 3:
**424 s** → 4: 392 s (flat). The 3→4 step nearly doubles `$defs` (1,763→2,510) and
abort arms (585→1,206) yet costs nothing. Something **saturates**, and the
non-monotonic step is where a profiler should be pointed first. Cost tracks
cross-chart guard composition, not chart size — on gitea, 968 of 1,206 `then:
false` arms come from four bitnami subcharts.

Subchart count is a strong but not sole driver: 7 of the 9 timeout charts are
umbrellas (sumologic bundles 14 charts, redash 5, langfuse and ingress 4), but
`seaweedfs` and `console` are standalone and still exceed 900 s. Any performance
work should explain both.

This materially strengthens the case for the performance campaign, and it
supplies a second, independent reason to fix D3: collision-driven leakage inflates
exactly the cross-chart arm count that dominates the cost.

## Corpus adoption plan

**Do not adopt charts yet.** Adopting now would freeze the current wrong answers
as expected fixtures — precisely what already happened to signoz-signoz. Fix
first, adopt second.

Order:

1. Fix the self-validation gate to validate the coalesced document. Cheapest and
   structurally right, but note the measurement above: it produces **zero** new
   failures on today's corpus. Its value is that every chart adopted afterwards is
   judged against the document Helm actually validates. Do it first precisely
   because it is inert right now — landing it later, alongside adopted charts,
   would tangle a gate change with a fixture change.
2. Fix D3, then D1 — the two with contaminated fixtures. Both fixture flips are
   corrections and need explicit adjudication.
3. Fix D2 and D4. Both are missing branches in an otherwise-complete dispatch,
   and both should be exhaustively destructured so a future target shape cannot
   be silently dropped.
4. Finish D5's root cause; adjudicate D6; fix D7.
5. **Then** adopt, prioritizing minimal witnesses over whole charts. A 9-file
   parent/`a`/`z` collision witness pins D3 better than 7 MB of openebs, and the
   gitlab reduction (`glmin4`, 6-second reproduction from unmodified upstream
   sources) pins its leak better than the 13.5 MB chart.
6. Adopt whole charts only where the chart itself is the phenomenon: a handful of
   the 24 for regression breadth, plus one of the 9 timeout charts as a
   performance fixture.

For charts where Helm itself refuses its own defaults, follow the existing
`KNOWN_VALUES_REJECTIONS` pattern — sonarqube is a clean example, verified to
refuse for *exactly* Helm's two reasons.

## Open, not adjudicated

- **gitlab, 86 of 87 errors survive** on a values document Helm renders: 65 from
  registry-owned paths duplicated onto `gitlab.{toolbox,sidekiq,…}`, 15 forbidding
  arrays on `global.appConfig.sidekiq.routingRules` (default `[]`, template does
  `kindIs "slice"`), 4 requiring `workhorse.keywatcher` falsy where the chart ships
  `true`, 2 on redis sub-instance inheritance. Bisected to `charts/registry`'s
  `templates/configmap.yaml` — D3-shaped but *duplicating* rather than
  *displacing*, and asymmetric. Audit should start at
  `manifest_contract.rs:25-32`, where per-chart `values_prefix` scoping is applied.
- **A dropped `and`-conjunct** in gitea's `valkey-cluster`: the emitted condition
  is `update-cluster.yaml:6`'s inner disjunction verbatim, minus its leading
  `and .Values.cluster.update.addNodes`. Possibly D1; not confirmed.
- **15 of the 24 confirmed rejections were never individually adjudicated** —
  only screened structurally. Their mechanism attribution is a hypothesis.

## Reproduction assets

The tools and raw results are committed under `plan/corpus-expansion-scripts/`
(see its `README.md`): `survey.sh`, `coalesce.sh`,
`self-validation-prober.rs`, plus `worklist.tsv` (the 349 surveyed charts),
`survey.jsonl` (raw per-chart results), `final-verdicts.txt` (the 24 confirmed
rejections) and `corpus-gate.txt` (the existing corpus re-gated).

Bulk artifacts stay out of the repo, under
`/Volumes/T7/dev/helm-schema-corpus-survey/`: the 349 fetched charts, every
generated schema and diagnostic under `results/<chart>/`, the coalesced instances
under `coalesced/`, and `adjudication/*.md` — six detailed reports (2,825 lines)
carrying the minimal witnesses, the resolved firing conditions, and the
rename-only causal experiments for D3.

The technique that makes an opaque `"False schema does not allow {...}"` tractable:
extract each `{"if": …, "then": false}` arm's condition into a standalone schema
with the full `$defs` attached, extract the relevant subtree of the coalesced
defaults, and run the prober to find which arm actually fires.
