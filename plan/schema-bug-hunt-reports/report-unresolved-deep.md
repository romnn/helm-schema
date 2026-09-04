# Bug hunt — unresolved-deep batch (synapse, netbox, stacks-blockchain-api)

Recovered from the agent's completion message: the harness blocked its file write.
Reproduction assets are in `scratch-unresolved/`.

## F1 — synapse type over-narrowing: root-caused, and it is a general idiom

A `range $k, $v` over a derived map projects the **sink's member type onto every
values path that merely influenced the iterable**.

Responsible code:

| file:line | what |
|---|---|
| `crates/helm-schema-ir/src/fragment_eval/control.rs:979-983` | `source_paths = range_subject.influence_paths` |
| `.../control.rs:1120-1143` | `if renders_mapping_entries { for path in &source_paths { splice_arm(path.encode(), ValueKind::Fragment, …) } }` — the wrong-path splice |
| `.../control.rs:1001-1002` | `renders_mapping_entries = destructured && !emits_sequence_items && has_dynamic_entries` |
| `.../value_path_context/path_resolution.rs:107-120` | where `influence_paths` is filled |
| `.../fragment_eval/inline_regions.rs:251` | the mirrored inline-range case |

The `RangeSubject` doc comment (`value_path_context/mod.rs:26-32`) states exactly
why `influence_paths`, `input_identity` and `member_identity` are kept separate —
"prevents a transformation such as `splitList` from turning its string input into
a collection contract". The mapping-entry splice ignores that separation. The
correct source is `member_identity`/`input_identity`, already `None` for a derived
dict.

**It is not synapse-specific.** A 20-line chart with no synapse code reproduces it
using the single most common idiom in Helm — a `checksum/config` pod annotation:

```gotemplate
annotations:
  {{- $d := merge .Values.podAnnotations (dict "checksum/config" .Values.checksum) }}
  {{- range $key, $value := $d }}
  {{ $key }}: {{ $value | quote }}
  {{- end }}
```

`checksum` is emitted as `anyOf[{const: "abc123"}, {type: object, additionalProperties: {type: string}}]`
— pinned to its literal default or forced to be an object of strings. Helm renders
`checksum: "different"`; the schema rejects it.

Bisection: `merge` not required, `sha256sum` not required, a named helper works as
well as a file include, and a *fixed* key inside the range removes it — confirming
the templated-key path is the trigger. The sink type propagates elementwise: with
a `containerPort:` sink the same shape yields an integer constraint instead.

## F2 — `hasKey <literal dict>` guarding a `fail` produces no constraint at all

Proven against the **shipped, otherwise-clean** `bitnami-postgresql` schema (its
defaults validate with 0 errors): `primary.resourcesPreset: huge` aborts Helm
(`ERROR: Preset key 'huge' invalid…`) and the schema accepts.

Isolation, both branches inline against `.Values` directly:

- `has .Values.mode <literal list>` → **is** lowered to an enum, correctly rejects.
- `hasKey <literal dict> .Values.preset` → **no arm mentions `preset` at all**.

Membership over a literal *list* is recovered; over a literal *dict* it is
dropped. 51 template files call `common.resources.preset`; 22 corpus charts expose
`resourcesPreset` at the top level of `values.yaml` alone.

## F3 — the validateValues accumulator loses the guard at `append` or `without`

Proven against the shipped, clean `bitnami-redis` schema: `architecture: bogus`
aborts Helm via `NOTES.txt:202` and the schema accepts.

A one-validator-per-chart matrix isolates the exact step:

| shape | schema |
|---|---|
| `list` → `append` → `without` → `join` → `if` → `fail` | **accept** |
| `list` → `append` → `join` → `printf \| fail` | **accept** |
| `without (list (include …)) ""` → `join` → `fail` | **accept** |
| `$msg := include …` → `if $msg` → `fail` | reject |
| `join "\n" (list (include …))` → `if` → `fail` | reject |
| `printf "%s" (include …)` → `if` → `fail` | reject |

The guard survives `list`, `join` and `printf`, and is lost the moment the
fragment passes through **`append`** or **`without`**. Both appear in every
Bitnami `validateValues`.

`NOTES.txt` *is* analysed and a `fail` reached only through it *is* encoded
(verified with a minimal chart), so this is an accumulator gap, not a NOTES gap.

**Methodological note:** the bug interferes across helpers within one chart — a
working `fail` capture at a shared site appears to rescue the others. Only the
one-validator-per-chart matrix is trustworthy.

## F4 — a builtin's Go `string` parameter aborts on nil; the requirement is unmodelled

synapse's `deployment.yaml:1,4,7` call `required` with **swapped arguments**
(`required <val> <msg>` instead of `required <msg> <val>`), so the secret lands in
the `warn string` slot. Null-deleting the key makes Go abort before `required`
runs (`wrong type for value; expected string; got interface {}`), and the schema
accepts.

Minimal isolation: `required "msg" .Values.good` emits the arm;
`required .Values.swapped "msg"` emits nothing. Also live in netbox's
`contains "NodePort" .Values.service.type`.

## F5 — `hasKey` lowered as present-and-non-null (sibling finding, now proven)

`cfg: {}` in defaults, `{{- if hasKey .Values.cfg "k" }}` then `.Values.cfg.k.sub`.
The emitted arm carries a `cfg.k == null` disjunct that should not be there —
`hasKey` is presence-only, and `cfg: {k: null}` is *not* deleted by coalescing
because `k` is absent from the chart defaults. Helm aborts on the nil pointer; the
schema accepts.

## stacks-blockchain-api — 100% D4, zero D3, no third class

Repair matrix on chart defaults: baseline 10 errors; neutralizing
`include … .Subcharts.postgresql` (7 sites) → 2; neutralizing
`include … (index .Subcharts "stacks-blockchain")` (6 sites) → 8; both → **0**.

The chart has **no** `$.Template.BasePath` include anywhere, so its 64 duplicate
basenames are never exercised — D3 is not involved despite the structural screen.

**`(index .Subcharts "name")` is a second spelling of D4** that the documented root
cause (`expr_eval.rs:196-213`) does not cover. Charts whose subchart name contains
a `-` are *forced* into this spelling, so it is not exotic.

Looking past the repair: a top-level and full second-level null-deletion probe
(24 + 273 targets, each helm-adjudicated) found no further disagreement.

## netbox — D3 also mis-resolves *self*-includes

`include (print $.Template.BasePath "/configmap.yaml")` is looked up by the same
"path after the last `templates/`" key, so a subchart asking for **its own**
configmap can be handed the parent's, and vice versa. That is why a one-sided
rename does not repair netbox — it only flips which chart wins:

- rename valkey's `configmap.yaml` → valkey's self-include lands on netbox's, so
  netbox facts appear under `/valkey` (11 errors).
- rename netbox's → netbox's self-include lands on valkey's, so valkey facts
  appear at the netbox root (5 errors).

**A fix for D3 must key the include target by chart identity, not by trailing
path.** Complete D3 repair plus D4 gives 0 errors.

## Negative results

- The sibling finding "`else if .Values.X` in a chain with a total `else` makes `X`
  mandatory" **did not reproduce** here: a minimal chart emits exactly
  `if not(mode=="a") and not(mode=="b") → false`, which is correct.
- `service.port` required in synapse and netbox is a **legitimate** provider fact —
  the rendered Service would be rejected by the API server.
- netbox `allowedHosts: []` and `extraConfig: ["x"]` are correctly rejected.
- synapse scalar/shape probes all behave correctly.
