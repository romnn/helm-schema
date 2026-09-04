# openebs — deep bug hunt

**Chart**: `/Volumes/T7/dev/helm-schema/testdata/charts/openebs` (v4.6.0, 28 charts, 620 template
files). **Shipped schema**: `testdata/chart-corpus-schemas/openebs.schema.json`, 34.9 MB
pretty-printed (13.5 MB compact) — the largest in the corpus.

**Scratch dir**: `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-oe/`
**Adjudicator**: `helm` 4.2.4.

---

## 0. Method — and two corrections that everything below depends on

The prompt's premise held: the shipped schema rejects the chart's own defaults (4 errors, the known
D3 collision), so **no false-acceptance evidence can be gathered against it**. Two rebuilds were
made, both verified to render byte-identically to the original under `helm template` (modulo the
chart's own nondeterministic `genPrivateKey`/`randAlphaNum` secrets, which also differ between two
runs of the *unmodified* chart):

| build | what was changed | schema |
|---|---|---|
| `openebs-nd` | all **209** colliding relative template keys renamed, `$.Template.BasePath` include literals rewritten to match | `nd.schema.json` |
| `openebs-md` | **only the 7 `configmap.yaml` files** renamed | `md.schema.json` |

`nd.schema.json` and `md.schema.json` are **byte-identical**. That causally confirms `configmap.yaml`
is the *only* live D3 collision in openebs, and simultaneously proves the broad rename introduced no
artifacts. `nd.schema.json` **accepts the chart's coalesced defaults** (0 errors), so it is a clean
baseline. Everything below is adjudicated against it. The regenerated *unmodified* build equals the
shipped fixture exactly (modulo the stripped `x-helm-schema-*` keys), so the toolchain is pinned
correctly.

### Correction 1 — the prober does not model Helm's null handling

`corpus-prober` (and the repo harness it mirrors,
`crates/helm-schema-cli/tests/common/values_validation.rs`) applies `drop_nulls` to the instance
before validating: *every* null-valued map key is deleted. **Helm 4.2.4 does not do that.** Proven
with a minimal two-chart fixture (`scratch-oe/nulltest/`) validated by Helm's own schema validator:

| user overlay | key declared in | Helm's validated document |
|---|---|---|
| `top: null` | root chart's `values.yaml` | key **deleted** |
| `sub.a: null` | subchart's own `values.yaml` | key **deleted** |
| `sub.b: null` | **parent's** `values.yaml` only | **survives as null** → `at '/sub/b': 'not' failed` |
| `sub.zzz: null` | nowhere | **survives as null** |

The rule, then re-confirmed on 13 independent openebs paths (`scratch-oe/batchprobe.py`, `probe.sh`):

> A user-supplied `null` at path *P* is deleted **iff** *P* exists in the values.yaml of the
> **innermost owning chart** for *P*'s scope. Otherwise it survives, present-and-null, into the
> document Helm validates.

So an arm of the form `required:[K] ∧ enum:[null]` is satisfiable exactly when the *innermost* chart
does **not** declare `K`. This is the fact finding **OE-1** shows helm-schema has backwards, and it
is also why the prober cannot adjudicate that finding. All witnesses below were therefore
re-adjudicated with `scratch-oe/d7.py`, a Draft-07 evaluator covering exactly the keyword set
helm-schema emits, run on the **raw** coalesced document. It was cross-checked against the prober on
30 instances (5 hand-built + 24 randomly mutated + defaults) with **0 disagreements** on null-free
instances.

### Correction 2 — `coalesce2.sh`

Used as instructed (chart templates stripped before dumping `.Values`), so the coalesced document is
pre-mutation and can be produced for values documents Helm refuses to render.

---

## OE-1 — subchart nil-guard emitted inverted: `present-and-null` where only `absent` is reachable

- **Class**: false acceptance (systemic)
- **Status**: PROVEN — 331 independent witnesses
- **Known mechanism**: **NEW root cause**. The *symptom* was already flagged as the corpus's largest
  defect; the inverted branch below has not been named before.

### Root cause

`crates/helm-schema-gen/src/condition_encoding.rs:256-269`:

```rust
let explicit_null = build_required_condition_fragment(
    arm_segments, SchemaNode::enum_values(vec![Value::Null]))?;
let subchart_default_fills = matches!(
    yaml_value_at_path(subchart_defaults_doc, path),
    Some(value) if !matches!(value, YamlValue::Null));
let deleted = if subchart_default_fills {
    explicit_null                                   // <-- only "present AND null"
} else {
    let missing = SchemaNode::not(build_required_condition_fragment(
        arm_segments, SchemaNode::empty())?);
    SchemaNode::any_of(vec![missing, explicit_null]) // <-- correct three-way
};
```

The comment above it (`condition_encoding.rs:233-241`) states the model:

> *"A missing DEPENDENCY-owned key instead reads as the subchart's declared default, which fills at
> the subchart's own coalesce stage — only a non-null default rescues it from absence."*

That is exactly backwards. When the subchart declares a non-null default for the key
(`subchart_default_fills == true`), Helm's coalescing **deletes** the user's `null` — it does *not*
refill it with the default. So in that branch:

- the reachable nil state is **absent**, and
- **present-and-null is unreachable** — the emitted arm can never fire.

The `else` branch (subchart does not declare the key) is the one where a null *does* survive, and
there the code already emits the sound three-way form. The two branches are swapped with respect to
reachability. Emitting `any_of([missing, explicit_null])` unconditionally is both sound and complete
in both cases and would fix every witness below. `subchart_defaults_doc` is `documents.dependency`
(`emission_plan.rs:213-215`) — the dependency-owned defaults — which is precisely the "innermost owning
chart's values.yaml" that governs Helm's deletion, so the predicate is computed from the right
document and then used with the wrong polarity.

### Quantification

`scratch-oe/mustatoms.py` + `reach.py`. Sound under-approximation: an arm is counted dead only when
*every* satisfying assignment of its `if` requires a present-null Helm cannot produce.

| | base (shipped) | nd (D3-fixed) |
|---|---|---|
| `{"if": …, "then": false}` reject arms | 3790 | 3786 |
| arms requiring an unreachable present-null → **dead** | **3495 (92%)** | **3497 (92%)** |
| distinct guard paths made unreachable | — | **961** |
| dead-causing paths in the **root chart's own scope** | — | **0** |

The last row is the control the prompt asked for: openebs's own values scope has **zero** such arms.
All four root-scope nil-deref witnesses are **correctly rejected** — `preUpgradeHook: null`,
`engines: null`, `engines.local: null`, `loki.localpvScConfig: null` (each aborts Helm at
`templates/pre-upgrade-hook.yaml:1` or `templates/loki-storage/storage/localpv-storageclass.yaml:1`).
The defect is exclusively subchart-scoped, matching the `subchart_defaults_doc` lookup above.

### Empirical sweep

`scratch-oe/fuzz.py`, one path per run, overlay `<path>: null`, adjudicated by `helm template` +
`d7.py` against `nd.schema.json`. Paths were drawn from the 961 unreachable-null guard paths,
restricted to charts that actually render in the default configuration.

| outcome | count |
|---|---|
| probed guard paths | **630** |
| Helm **aborts**, schema **accepts** → **false acceptance** | **335** |
| Helm aborts, schema rejects | 5 |
| Helm renders, schema accepts | 290 |
| Helm renders, schema rejects → false rejection | 0 |

**Of the 340 aborting values documents the schema catches 5 — 1.5%.** 4 of the 335 abort only inside
`templates/tests/` (excluded from analysis by `--exclude-tests`) and are discounted, leaving **331**.
Distribution of the abort sites:

```
charts/mayastor              102     charts/loki/charts/minio      26
charts/loki                   50     charts/mayastor/charts/nats   25
charts/mayastor/charts/etcd   40     charts/localpv-provisioner    18
charts/lvm-localpv            27     charts/alloy                  14
charts/zfs-localpv            26     charts/*/charts/crds           6
```

### Canonical witness

`$defs/JL`, referenced at `/properties/alloy/allOf/48`, fully resolved:

```json
{"if": {"allOf": [
   {"properties": {"rbac": {"enum": [null]}}, "required": ["rbac"], "type": "object"},
   {"anyOf": [ …alloy.enabled falsy-or-absent…, …alloy.enabled truthy… ]},
   {"type": "object"}]},
 "then": false}
```

- **Template**: `charts/alloy/templates/rbac.yaml:1` — `{{- if .Values.rbac.create }}`
- **Values document**: `alloy: {rbac: null}`
- **Coalesced** (`coalesce2.sh`): `alloy` present, `rbac` **absent** — `alloy/values.yaml` declares
  `rbac.create: true`, so Helm deletes the key rather than nulling it.
- **`helm template`**: **ABORTS** —
  `openebs/charts/alloy/templates/rbac.yaml:1:14 … at <.Values.rbac.create>: nil pointer evaluating interface {}.create`
- **Schema**: **accept**. The arm needs `rbac` *present*; it is absent, so the `if` is false.
  Isolated check of the arm alone: `corpus-prober inst_alloy.json armJL.json` → `reject` (the guard
  does not match the aborting document).
- **Why they disagree**: `alloy` is a dependency and `rbac` is declared in the dependency's
  values.yaml, so `subchart_default_fills` is true and the emitter chose `explicit_null`. The only
  state Helm can actually reach for that key is `absent`, which the arm does not mention.

### Nine more, each independently reproduced (`<path>: null`; all Helm-abort / schema-accept)

| values path | Helm abort site |
|---|---|
| `alloy.rbac` | `charts/alloy/templates/rbac.yaml:1:14` — nil pointer `.create` |
| `loki.enterprise.enabled` | `charts/loki/templates/_helpers.tpl:13` — `ternary … .Values.enterprise.enabled` (`wrong type for value; expected bool`) |
| `loki.loki.configObjectName` | `charts/loki/templates/single-binary/statefulset.yaml:229:14` |
| `loki.minio.securityContext` | `charts/loki/charts/minio/templates/statefulset.yaml:84:24` — nil pointer `.enabled` |
| `mayastor.etcd.preUpgradeJob` | `charts/mayastor/charts/etcd/templates/preupgrade-hook-job.yaml:6:14` — nil pointer `.enabled` |
| `mayastor.etcd.auth.rbac` | `charts/mayastor/charts/etcd/templates/statefulset.yaml:164:64` — nil pointer `.create` |
| `mayastor.etcd.disasterRecovery` | `charts/mayastor/charts/etcd/templates/statefulset.yaml:156:50` |
| `mayastor.base.logging.format` | `charts/mayastor/templates/mayastor/agents/ha/ha-node-daemonset.yaml:80:26` — `fail "invalid logging format…"` |
| `zfs-localpv.analytics` | `charts/zfs-localpv/templates/zfs-controller.yaml:138:48` — `wrong type for value; expected map[string]interface{}` |
| `localpv-provisioner.hostpathClass.name` | `charts/localpv-provisioner/templates/hostpath-class.yaml:5:23` — `invalid value; expected string` |

`loki.enterprise.enabled` also shows the loss extends past null. `_helpers.tpl:13` is
`{{- $default := ternary "enterprise-logs" "loki" .Values.enterprise.enabled }}`; `ternary` demands a
bool, so `enabled: "yes"` and `enabled: 1` both abort at `_helpers.tpl:13:56`
(`wrong type for value; expected bool`) and are both accepted. The real obligation is
"must be a bool"; the schema encodes only the single value (`null`) Helm can never deliver.
396 reject arms in the schema hang off that one unreachable atom (792 more off
`mayastor.loki.enterprise.enabled`).

- **Severity**: highest in this chart. Every subchart-scoped `{{ if .Values.x.y }}` / nil-deref guard
  the analyzer *did* find is emitted in a form that cannot fire. 92% of openebs's reject arms are
  inert, and the configurations they exist to catch — removing a nested block with
  `--set alloy.rbac=null`, the idiomatic Helm way to do it — are silently accepted by a 34 MB schema
  that is otherwise almost entirely about rejecting them.

---

## OE-2 — the generated schema exceeds Helm's 5 MiB chart-file limit and cannot be installed

- **Class**: unjustified output / usability defect
- **Status**: PROVEN
- **Known mechanism**: NEW

**Witness**: copy the chart, drop the generated schema in as `values.schema.json`, run Helm.

```
$ cp -R testdata/charts/openebs oe-sv
$ cp nd.schema.json oe-sv/values.schema.json          # 14.25 MB
$ helm template oe ./oe-sv
Error: chart file "values.schema.json" is larger than the maximum file size 5242880
```

Helm refuses the file before parsing it, for directory charts as well as archives. The shipped
fixture is 34.9 MB pretty-printed; even the compact form is 13.5 MB — 2.7× the limit. **The primary
artifact helm-schema produces for openebs cannot be used for its stated purpose.**

**Where the bytes go** (`nd.schema.json`, 13,501,477 bytes compact):

| region | bytes | share |
|---|---|---|
| `$defs` | 10,430,698 | 77.3% |
| root `allOf` (the 3786 reject arms) | 2,559,631 | 19.0% |
| root `properties` | 510,580 | 3.8% |
| — within `$defs`: the 108 `providerShared*` entries | 4,805,693 | 35.6% of file |
| `description` strings + `x-kubernetes-*` extensions, verbatim from upstream OpenAPI | 5,716,616 | **42.3%** |
| byte-identical duplicated subtrees ≥2 KB (77 distinct shapes; 5.4 MB is pure *n−1* duplication) | 5,673,088 | **42.0%** |

The `providerShared*` family is the mechanism. Sharing is done at the **values-path wrapper** level
(`{"properties": {"read": {"properties": {"extraVolumes": …}}}}`), so each of the 108 wrappers
re-inlines the whole Kubernetes subtree underneath it. The Container schema (43,429 bytes) is inlined
**44** times = 1.9 MB; the `podAffinityTerm`/`labelSelector` block (5,604 bytes) **149** times; the
probe-handler block (4,650 bytes) **143** times; `SecurityContext` (5,824 bytes) **64** times. All
108 `providerShared*` bodies are pairwise distinct only because of their outer wrapper.

Measured remediation headroom:

| | size | under 5 MiB? |
|---|---|---|
| as generated | 13.50 MB | no |
| + `$ref` deduplication of identical subtrees ≥200 B | ~5.47 MB (5.2 MiB) | just over |
| + drop `description` / `x-kubernetes-*` | ~4.29 MB (4.1 MiB) | **yes** |

- **Severity**: total for this chart — no user can ship the artifact. 18 of 156 corpus schemas are
  over the same limit, and the duplication mechanism is generic to provider-schema emission, so one
  fix covers all of them.

---

## OE-3 — D3 collision: exact blast radius, a fourth victim, and a vacuous ambiguity guard

- **Class**: false rejection (chart's own defaults) + unjustified constraints
- **Status**: PROVEN causally, by rename
- **Known mechanism**: **D3** — reported only for the parts that were *not* already settled.

**(a) `mayastor.etcd` is a fourth victim, not previously listed.** The settled write-up named
`loki.minio`, `mayastor.loki.minio` and `mayastor.nats`. `charts/mayastor/charts/etcd/templates/`
also does `include (print $.Template.BasePath "/configmap.yaml")` — `statefulset.yaml:37` and
`preupgrade-hook-job.yaml:28` — so it is hijacked too. Full contamination, by value-path diff of
`base` vs `nd` (`scratch-oe/paths.py`): **34 fabricated paths, 0 legitimate paths lost from the path
set**, and all four victims are contaminated identically:

```
{loki.minio, mayastor.loki.minio, mayastor.nats, mayastor.etcd}
  × {zfs, zfs.bin, zfsNode, zfsNode.componentName, role, enableHelmMetaLabels}
```

`zfs.bin` comes from `charts/zfs-localpv/templates/configmap.yaml:15` (`{{ .Values.zfs.bin }}`); the
other four come from the `zfslocalpv.zfsNode.labels` helper it includes. None of these names occurs
anywhere in the victim charts' own sources.

**(b) The structural screen over-predicts by 209:1.** openebs has 620 template files and **209**
colliding relative keys (`_helpers.tpl` ×25, `NOTES.txt` ×17, `serviceaccount.yaml` ×9,
`service.yaml` ×8, `configmap.yaml` ×7, `rbac.yaml` ×8, …). Only keys reached through
`template_base_path_suffix` (`expr_call_eval/mod.rs:1672-1700`) can ever be observed, and that
requires a literal `$.Template.BasePath` receiver with literal string suffixes. Consequently
`charts/loki/templates/_helpers.tpl:1159` (`include (print .ctx.Template.BasePath .name)`, the
`loki.configMapOrSecretContentHash` helper, 10 call sites in that chart) and
`charts/mayastor/charts/etcd/charts/common/templates/_utils.tpl:75`
(`include (print .context.Template.BasePath .path)`) are **not** matched by `is_template_base_path`
and never resolve at all. Of the remaining literal targets (`configmap.yaml`, `secrets.yaml`,
`token-secrets.yaml`, six `_helper_*` files), `secrets.yaml` and the `_helper_*` files do collide but
their colliding bodies are byte-**identical** (`cmp` on all seven pairs), so only `configmap.yaml` is observable. Confirmed
experimentally: renaming *only* the 7 `configmap.yaml` files produces a schema byte-identical to
renaming all 209.

**(c) The ambiguity guard in `implicit_template_name` is vacuous.** `analysis_db.rs:273-284` is
written to abstain when a suffix matches more than one file:

```rust
let mut matches = …implicit_template_names.iter().filter(|(path,_)| path.as_str()==suffix)…;
let first = matches.next()?;
matches.next().is_none().then_some(first)      // abstain if ambiguous
```

but the map it filters is `BTreeMap<relative_path, name>`, populated by
`implicit_template_names.insert(template_relative_path, name)` at `analysis_db.rs:59`. The insert has
already collapsed the duplicates last-wins, so the filter can never yield two matches and the guard
never fires. Making that a `BTreeMap<String, Vec<String>>` (or keying on the full path) turns D3 into
the abstention the code already intends, which is exactly the
"prefer 'ambiguous' over a wrong deterministic-looking answer" rule in `CLAUDE.md`.

**Side effect of the fix, worth knowing before adopting it.** With the collision removed, the
`mayastor.etcd` node changes from `anyOf: [{…82 properties}, {"type":"null"}]` to a bare 75-property
object, and three *legitimate* typed property listings disappear along with the four fabricated ones:
`common` (nested object types), `kubeVersion` (`type: string`),
`removeMemberOnContainerTermination` (`type: boolean`). `additionalProperties` stays open at that
node in both builds, so nothing Helm renders becomes rejected — it is typing/documentation loss, not
a validation change. It reproduces identically in `md` and `nd`, so it is caused by the D3 fix
itself, not by the renaming.

- **Severity**: the shipped fixture's 4 errors (already known). The 34 fabricated paths are the
  unjustified-constraint half and are invisible to any mechanical check.

---

## OE-4 — `global.imagePullSecrets` rejects both of its legal forms (false rejection)

- **Class**: false rejection
- **Status**: PROVEN — and present in the **shipped** schema, not just the rebuild
- **Known mechanism**: NEW

**Schema says** — root arm `/allOf/2793`, an `if/then` (not a reject arm), resolved:

```
IF   global.imagePullSecrets is present, non-null and truthy
     AND (engines.replicated.mayastor.enabled truthy  OR  that flag is absent/null)
THEN each ITEM of global.imagePullSecrets must satisfy
       IF NOT(type=object) THEN anyOf[
           string ~ "^&[A-Za-z0-9_-]+[ \t]*(#.*)?$"       # a YAML anchor
           string ~ "^[ \t]*(#.*)?$"                       # blank or comment
           string ~ "^(~|null|Null|NULL)([ \t]+#.*)?$"     # a null literal
           type=null  & items[{name: string|null, …}]
           type=array & items[{name: string|null, type: object|null, additionalProperties:false}] ]
     …and the same schema is applied through `additionalProperties` to the members
       of an item that IS an object.
```

The `^(~|null|Null|NULL)…$` and `^&[A-Za-z0-9_-]+…$` patterns are the scalar-preimage vocabulary from
`crates/helm-schema-gen/src/resolve_policy/scalar_preimage.rs:129,316`. The K8s
`imagePullSecrets` provider constraint (an array of `LocalObjectReference`) has been attached **one
nesting level too deep** — to each *element* of `.Values.global.imagePullSecrets` rather than to the
array — with a text-preimage escape hatch that admits only anchors, comments and nulls.

**Template says** — `charts/mayastor/templates/_helpers.tpl:85-93`, consumed as
`imagePullSecrets: {{- include "base_pull_secrets" . }}` from
`mayastor/templates/mayastor/io/io-engine-daemonset.yaml`, `…/csi/csi-node-daemonset.yaml`,
`…/apis/api-rest-deployment.yaml` and others:

```gotmpl
{{- define "base_pull_secrets" -}}
    {{- if (not (empty .Values.global.imagePullSecrets)) }}
        {{- range .Values.global.imagePullSecrets | uniq -}}
            {{- if kindIs "map" . -}}
            {{ nindent 8 "- name:" }} {{ .name }}
            {{- else -}}
            {{ nindent 8 "- name:" }} {{ . }}
            {{- end }}
        {{- end }}
```

The chart explicitly supports **both** item shapes via `kindIs "map"`. openebs's own
`templates/_helpers.tpl:98-120` (`openebs.common.pullSecrets`) does the same, and
`values.yaml:4-6` documents the bare-string form (`# - secret`).

**Why they disagree**: the provider constraint for the *list* was lowered onto the list's *items*
across the `range`, so a `{name: …}` item is validated as if it were the whole
`imagePullSecrets` array, and a bare string item is only allowed if it happens to look like a YAML
anchor, a comment, or a null literal.

**Witnesses** (raw coalesced document, `d7.py` against `nd.schema.json`; the same errors reproduce
against the shipped `base.schema.json`):

| # | values document | `helm template` | schema |
|---|---|---|---|
| 1 | `global: {imagePullSecrets: [{name: mysecret}]}` | **RENDERS** | **reject** — `/global/imagePullSecrets/0/name: anyOf` |
| 2 | `global: {imagePullSecrets: [mysecret]}` | **RENDERS** | **reject** — `/global/imagePullSecrets/0: anyOf` |
| 3 | `mayastor: {image: {pullSecrets: [{name: s1}]}}` | **RENDERS** | **reject** — `/mayastor/image/pullSecrets/0/name: anyOf` |
| 4 | `mayastor: {image: {pullSecrets: [s1]}}` | **RENDERS** | **reject** — `/mayastor/image/pullSecrets/0: anyOf` |

Controls that pin the mechanism: `global: {imagePullSecrets: [""]}` is **accepted** (it matches the
blank/comment pattern); the same document with `engines.replicated.mayastor.enabled: false` is
**accepted** (the arm's `if` no longer holds); the default `imagePullSecrets: []` is accepted (the
`if` requires a truthy, i.e. non-empty, list), which is why the corpus default check never saw this.

A structural scan (`scratch-oe`) finds **17** value paths carrying an item-level scalar-preimage
constraint, including `loki.global.imagePullSecrets`, `mayastor.global.imagePullSecrets`,
`loki.minio.global.imagePullSecrets` and `mayastor.jaeger-operator.image.imagePullSecrets`. Of the
five probed, the two above reject; the others (`loki.minio.service.externalIPs`,
`mayastor.csi.node.topology.segments`, `loki.networkPolicy.externalStorage.ports`,
`mayastor.jaeger-operator.image.imagePullSecrets`) render and are accepted, so the misplacement is
only harmful where the target type is a list of maps.

- **Severity**: high and immediate. `global.imagePullSecrets` is the single most commonly-set global
  in the Helm ecosystem, and the arm is live in the **default** configuration (mayastor enabled).
  Any private-registry install of openebs is locked out, in both documented spellings.

---

## OE-5 — the bitnami `common` validation family is entirely unmodelled under `mayastor.etcd`

- **Class**: false acceptance
- **Status**: PROVEN (3 witnesses)
- **Known mechanism**: NEW (matches the family the prompt flagged)

**Template**: `charts/mayastor/charts/etcd/templates/_helpers.tpl:164-201` — the standard bitnami
message accumulator, invoked from `templates/NOTES.txt:119`:

```gotmpl
{{- define "etcd.validateValues" -}}
{{- $messages := list -}}
{{- $messages := append $messages (include "etcd.validateValues.startFromSnapshot.existingClaim" .) -}}
{{- $messages := append $messages (include "etcd.validateValues.startFromSnapshot.snapshotFilename" .) -}}
{{- $messages := append $messages (include "etcd.validateValues.disasterRecovery" .) -}}
{{- $messages := without $messages "" -}}
{{- if (join "\n" $messages) -}}{{- printf "…%s" … | fail -}}{{- end -}}
```

Three abort conditions. **Zero** reject arms mention `snapshotFilename`; the three sub-helpers'
conditions appear nowhere; `mayastor.etcd.startFromSnapshot`, `disasterRecovery` and `persistence`
are emitted as fully open `{}` objects. The four arms that *do* mention `disasterRecovery` /
`startFromSnapshot` (`/allOf/1494`, `/allOf/2629`, `/allOf/2984`, `/allOf/3491`) are unrelated
nil-deref guards, and all four are dead under OE-1.

| # | values document | `helm template` | schema |
|---|---|---|---|
| 1 | `mayastor.etcd.disasterRecovery.enabled: true`, `mayastor.etcd.persistence.enabled: false` | **ABORTS** — `NOTES.txt:119`, *"Persistence must be enabled when disasterRecovery is enabled!!"* | **accept** |
| 2 | `mayastor.etcd.startFromSnapshot.enabled: true` | **ABORTS** — `NOTES.txt:119`, *"An existing claim must be provided when startFromSnapshot is enabled…"* | **accept** |
| 3 | `mayastor.etcd.resourcesPreset: gigantic` | **ABORTS** — `statefulset.yaml:339`, *"ERROR: Preset key 'gigantic' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge"* | **accept** |

Witness 3 is the `hasKey <dict literal>` gate the prompt flagged:
`charts/mayastor/charts/etcd/charts/common/templates/_resources.tpl:13-48` builds a literal 7-entry
`$presets` dict and does `{{- if hasKey $presets .type -}} … {{- else -}} … | fail -}}`. The allowed
set is fully static and even printed in the error message, yet
`properties.mayastor.properties.etcd.properties.resourcesPreset` is `{}` — **no enum**. The same call
site appears four more times in that chart (`statefulset.yaml:103`, `cronjob-defrag.yaml:140`,
`cronjob-snapshotter.yaml:132`, `preupgrade-hook-job.yaml:147`), so five values paths lose the same
enum.

`common.errors.insecureImages` (`NOTES.txt:122`) was probed with a rewritten `image.repository` and
does **not** abort in this fork, so it is not a finding here.

- **Severity**: `resourcesPreset` is a user-facing knob with a closed 7-value domain and a ready-made
  enum; the two `validateValues` conditions are exactly the cross-field mistakes a values schema
  exists to catch before the cluster round-trip.

---

## OE-6 — numeric-comparison guards (`gt`/`lt` over `int`) produce no constraint

- **Class**: false acceptance
- **Status**: PROVEN (2 witnesses)
- **Known mechanism**: NEW

**Witness A** — `charts/loki/templates/validate.yaml:10-23`:

```gotmpl
{{- $atLeastOneScalableReplica := or (gt (int .Values.backend.replicas) 0) (gt (int .Values.read.replicas) 0) (gt (int .Values.write.replicas) 0) }}
{{- $atLeastOneDistributedReplica := or (gt (int .Values.ingester.replicas) 0) … }}
{{- if and $atLeastOneScalableReplica $atLeastOneDistributedReplica (ne .Values.deploymentMode "SimpleScalable<->Distributed") }}
{{- fail "You have more than zero replicas configured for scalable targets … and distributed targets…" }}
```

- values: `loki: {ingester: {replicas: 2}, backend: {replicas: 2}}`
- `helm template`: **ABORTS** — `validate.yaml:23`, the message above.
- schema: **accept**. No reject arm anywhere encodes this `fail`.

**Witness B** — `charts/mayastor/templates/_helpers.tpl:212-214`:

```gotmpl
{{- if gt 1 (.Values.io_engine.cpuCount | int) -}}
{{- fail ".Values.io_engine.cpuCount must be >= 1" -}}
```

- values: `mayastor: {io_engine: {cpuCount: 0}}` (also reproduced with `"0"`)
- `helm template`: **ABORTS** — `io-engine-daemonset.yaml:128`, *".Values.io_engine.cpuCount must be >= 1"*.
- schema: `properties.mayastor.properties.io_engine.properties.cpuCount` is `{}` → **accept**.

Both are `minimum` / `exclusiveMinimum` obligations Draft-07 expresses directly. The contrast is
informative: the *same* `validate.yaml` file's non-numeric guards are all encoded correctly (see
"What is clean"), so this is specifically the arithmetic comparator, not the file or the chart.

- **Severity**: moderate. `cpuCount` is a headline mayastor tunable; the loki replica matrix is the
  most common way to mis-configure that chart.

---

## OE-7 — root `additionalProperties: false` rejects values documents Helm accepts

- **Class**: unjustified constraint / false rejection
- **Status**: PROVEN, **but almost certainly deliberate policy** — see the caveat
- **Known mechanism**: NEW

- values: `myCustomKey: 1` (or the very common `commonAnnotations: {foo: bar}`)
- `helm template`: **RENDERS** — Helm ignores undeclared values keys.
- schema: **reject** — `/myCustomKey: additionalProperties`.

The root object is closed (14 declared properties) while **every nested level is open**
(`additionalProperties: {}`), so `loki.myCustomKey: 1` is accepted and `myCustomKey: 1` is not. No
template justifies the asymmetry.

**Caveat, stated plainly**: 146 of the 147 corpus schemas small enough to load close the root the same way (the lone
exception is `airflow`), so this is a global emission policy, not something openebs-specific.
Recorded so the next engineer can affirm it deliberately rather than rediscover it; it should not be
counted as a new openebs defect.

- **Severity**: low-moderate but broad — it breaks `--reuse-values` across a chart-version bump and
  any workflow that parks extra keys in the values file.

---

## What is clean

Read against their templates and adjudicated; these are **correct**:

- **Root-chart-scope guards — the OE-1 control.** 0 of 961 unreachable-null guard paths lie in
  openebs's own values scope, and all four root-scope nil-deref witnesses are correctly rejected:
  `preUpgradeHook: null`, `engines: null`, `engines.local: null`, `loki.localpvScConfig: null`.
- **`templates/_validation.tpl` (openebs's own `fail`s)** — both conditions correctly encoded, at
  `/allOf/1057` and `/allOf/3947`. `engines.local.hostpath.enabled: false` with loki's localpv
  storage class enabled aborts and is rejected; so does the mayastor-etcd variant.
- **`templates/loki-storage/storage/localpv-storageclass.yaml:12,29`** — `required "StorageClass name
  … cannot be empty"`; empty names for both `loki` and `minio` abort and are rejected.
- **`charts/loki/templates/validate.yaml`** — 5 of the 6 reachable `fail`s are correctly rejected:
  top-level `config`, canary-vs-test, single-binary-without-object-storage, `useTestSchema` with
  `schemaConfig`, and missing `schema_config`. Only the replica-count one (OE-6) is missed.
- **`charts/mayastor/templates/_helpers.tpl:191` (`logFormat`)** — encoded as a `pattern`;
  `mayastor.base.logging.format: bogus` is rejected at `/mayastor/base/logging/format`.
- **`charts/mayastor/templates/_helpers.tpl:360` (`etcd.externalUrl must be set`)** —
  `mayastor.etcd.enabled: false` aborts and is rejected.
- **No false rejections in ordinary configurations.** Across 630 single-path mutation probes plus 13
  hand-built configurations — everything-enabled; mayastor off; loki+alloy off; all engines off;
  external-S3 loki with no localpv; rawfile enabled; loki `Distributed` mode with a full replica
  matrix; loki gateway with basicAuth; alloy autoscaling; mayastor cert-manager TLS;
  zfs-localpv nodeSelector; localpv-provisioner default hostpath class; preUpgradeHook tolerations —
  `helm template` and the schema agreed in the accept direction every time. Only the
  `imagePullSecrets` family (OE-4) breaks that.
- **The reference build reproduces the shipped fixture** exactly, modulo the stripped
  `x-helm-schema-generated` / `x-helm-schema-policy` keys.

## Harness note (not a chart finding, but it blocks reproducing OE-1)

`corpus-prober` / `crates/helm-schema-cli/tests/common/values_validation.rs` `drop_nulls` deletes
*all* null map keys before validating. Helm keeps a null whenever the innermost owning chart does not
declare the key (§0). Two consequences worth acting on:

1. **The corpus self-validation gate validates a document Helm never validates.** On present-null
   paths it is *stricter* than Helm — it converts them into absences — so it can manufacture
   rejections. Concretely: `mayastor.etcd.localpvScConfig.enabled: null` with
   `engines.local.hostpath.enabled: false` is **rejected** by the prober (arm `/allOf/3947` fires on
   the synthesised absence) and **accepted** by a Helm-faithful evaluation of the same schema, while
   `helm template` renders it. A candidate false-rejection finding was retracted for exactly this
   reason; the next engineer should expect the same trap.
2. **It is the same fact the analyzer has backwards** in `condition_encoding.rs`. Fixing the Helm
   null model once, in a place both the emitter and the test harness consume, fixes both.

## Summary

| finding | class | status |
|---|---|---|
| OE-1 subchart nil-guard emitted inverted (`condition_encoding.rs:256-269`) | false acceptance | PROVEN, 331 witnesses |
| OE-2 schema exceeds Helm's 5 MiB chart-file limit; 42% descriptions, 42% duplicated subtrees | unjustified output | PROVEN |
| OE-3 D3: 4th victim `mayastor.etcd`, 34 fabricated paths, vacuous ambiguity guard | false rejection + unjustified | PROVEN (D3) |
| OE-4 `global.imagePullSecrets` / `mayastor.image.pullSecrets` reject both legal forms | **false rejection** | PROVEN, 4 witnesses |
| OE-5 bitnami `etcd.validateValues` + `common.resources.preset` unmodelled | false acceptance | PROVEN, 3 witnesses |
| OE-6 numeric `gt`/`lt` guards produce no constraint | false acceptance | PROVEN, 2 witnesses |
| OE-7 root `additionalProperties: false` | unjustified constraint | PROVEN (corpus-wide policy) |

Counts by class: **false acceptance 3** (OE-1, OE-5, OE-6), **false rejection 2** (OE-3, OE-4),
**unjustified constraint 2** (OE-2, OE-7).

**Charts examined and found clean**: none. openebs was the only chart in scope and it is not clean.
Within it, these scopes were read and produced no finding of their own: `openebs-crds`,
`zfs-localpv/charts/crds`, `lvm-localpv/charts/crds`, `rawfile-localpv/charts/crds`,
`mayastor/charts/crds`, `mayastor/charts/jaeger-operator` — all CRD/object-only, with no `.Values`
control flow beyond an `enabled` flag that is correctly encoded — and openebs's own `templates/`
(5 files, all correct; see "What is clean").

## Reproduction

```sh
cd /Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-oe
python3 neutralize.py /Volumes/T7/dev/helm-schema/testdata/charts/openebs ./openebs-nd
python3 minimal.py                          # the surgical 7-file variant -> ./openebs-md
helm template oe ./openebs-nd > nd.render.yaml && python3 cmp2.py orig.render.yaml nd.render.yaml
helm-schema ./openebs-nd --exclude-tests --k8s-version v1.29.0-standalone-strict \
  --k8s-schema-cache-dir …/kubernetes-json-schema-cache \
  --crd-catalog-cache-dir …/crds-catalog-cache --offline -o nd.schema.json
./adj2.sh <overlay>.yaml nd.schema.json     # helm verdict + Helm-faithful schema verdict
python3 fuzz.py nd.schema.json deadpaths.json '[null]'
```

Key scratch artifacts: `neutralize.py`, `minimal.py`, `cmp2.py` (render equivalence), `d7.py`
(Draft-07 evaluator), `reach.py` (Helm null-deletion model), `mustatoms.py` (dead-arm analysis),
`tools.py`/`paths.py` (schema walkers), `fuzz.py`, `adj2.sh`, `nulltest/` (the Helm null-semantics
fixture), `fuzz1.out` + `fuzz2.out` (630 raw sweep results),
`base.schema.json` / `nd.schema.json` / `md.schema.json`.
