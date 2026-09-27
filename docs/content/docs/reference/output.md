---
title: Output
weight: 2
---

# Output

`helm-schema` emits a single **JSON Schema, Draft-07**, rooted at the Helm values object and written to standard output (or `--output <FILE>`). This page describes the shape of that output and the flags that control it.

## Anatomy

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$defs": { "…": "…" },
  "additionalProperties": false,
  "properties": { "…": "…" },
  "allOf": [ "…" ],
  "type": "object",
  "x-helm-schema-generated": true
}
```

- **`$schema`** — always Draft-07. This is the dialect Helm validates against.
- **`type: object`** and **`additionalProperties: false`** — the root is the values object, and only keys the chart actually consumes are allowed. Unrecognized keys are rejected, which is what catches typos.
- **`properties`** — one entry per value path the chart reads, typed from template use and resolved resource fields.
- **`allOf`** — conditional structure produced from template control flow (`if`/`then`, guard-scoped constraints). See [Template analysis]({{< relref "/docs/guide/template-analysis.md" >}}).
- **`$defs`** — interned subtrees shared by `$ref` (see [Minimization](#minimization)), plus a few reusable building blocks such as `helm-truthy`.
- **`x-helm-schema-generated: true`** — a marker identifying the file as machine-generated.

## Recurring building blocks

Some `$defs` entries appear across many charts:

| Definition | Meaning |
|---|---|
| `helm-truthy` | Helm's notion of "truthy" — anything other than `false`, `0`, `""`, an empty list, or an empty map. Used to model `if`/`with` guards precisely rather than assuming a boolean. |
| An int-or-string pattern (e.g. `values/replicas@anyOf(1)`) | Accepts Helm's quoted-integer form (`"3"`) alongside a numeric field, because a quoted number renders and validates the same as the bare number. |

Every `$defs` entry `helm-schema` creates is named by a schema path. By
default (`--defs-names source`) the path says where the definition's content
comes from:

- `k8s/io.k8s.api.core.v1.SecurityContext`,
  `k8s/deployment-apps-v1/spec.template.spec.containers@items`,
  `crd/monitoring.coreos.com/prometheus_v1/spec.containers@items`: a Kubernetes
  or CRD schema, named by its type or document and the pointer inside it;
- `values/agents.containers.agent.securityContext`: a position in the values
  schema. When the same content occurs in several places, the smallest path
  wins, so a name changes only when a smaller occurrence is added or removed;
- `when/agents.enabled:t+agents.mode=fast/then/agents.image`: a conditional
  fragment without a stable position (it sits below an `allOf`/`anyOf` arm,
  whose index shifts when an unrelated arm is added), named by the values
  paths its `if` tests and its `then`/`else` constrain;
- `constrains/image.pullPolicy+image.tag`: any other such fragment, named by
  the values paths it constrains;
- `values/agents/string:uri;image-repository-to-pull`: such a fragment that
  constrains no values path (a leaf), named by what it is below the nearest
  stable path above it: its `type` with `:format`, `=value` and the first
  words of its description (or `;pattern-…`), `not/<inner>`,
  `anyOf/<member>+<member>`, `:name` for a named definition, or
  `keywords/<its keywords>`.

A path in a `when/` or `constrains/` name carries the operator its schema
states: `=v` for `const`/`enum`, `!=v` for a negated one, `:name` for a
reference to a named definition (`:t` is Helm truthiness), `?` for a key that
is only required; `@items` and `@*` step into array items and map values.
Paths are sorted and joined by `+`; a long list ends in `+Nmore`, long values
and names are cut and marked `...`. Fragments with equal names are numbered
`@2`, `@3`, … in the order of their content, so an unrelated fragment never
renames them.

Property steps are written `.name`; every other step carries an `@` marker,
such as `@items`, `@additionalProperties`, `@anyOf(1)`, `@then`, or
`@patternProperties('(5e)a')`. A property name other than letters, digits,
`_` and `-` is quoted, and characters a URI fragment cannot carry are written
as `(hex)`. A reference at the root of a definition adds `@ref`, and a name
that is already taken receives an `@2`, `@3`, … suffix.

`--defs-names destination` names each definition after its first reference
instead, visiting object keys in sorted order and the definitions in the
order they are first reached. Named building blocks such as `helm-truthy`
keep their names.

`--defs-names destination` names are positions and do cross `allOf`/`anyOf`
indexes, so adding or removing a conditional can rename the definitions below
later arms.

## `$ref` handling

By default the output is **self-contained**: external file/URL `$ref`s discovered during analysis are resolved and re-homed into root-level `$defs`, so the schema stands alone while still sharing referenced subschemas.

| Flag | Effect |
|---|---|
| *(default)* | Resolve external refs into root-level `$defs`. |
| `--keep-refs` | Leave external `$ref` strings exactly as-is. |
| `--inline-refs` | Fully inline resolved refs instead of writing `$defs`. |

`--keep-refs` and `--inline-refs` are mutually exclusive.

## Minimization

Branch-scoped guard arms tend to repeat large fragments. By default, `helm-schema` interns repeated subtrees into root-level `$defs` and references them with `$ref`. This is an **output-only** transform — it never affects inference — and it matters in practice: it substantially reduces large schemas and speeds up downstream validators.

```bash
# Keep everything inline (larger, but no $ref indirection)
helm-schema ./mychart --no-minimize
```

Minimization is lossless, so it cannot guarantee that every schema fits
Helm's 5 MiB chart-file limit. Disabling it generally makes both the file and
downstream validator compilation larger.

## Size limit

Helm refuses chart files larger than 5 MiB (5,242,880 bytes). `helm-schema`
never changes the format or the names of its output to fit: when the written
schema is larger, it warns and names the fix. Ship Helm a shortened copy,
with every `$defs` entry renamed to a short base-62 key (the most referenced
definition getting the shortest one), usually as compact JSON:

```bash
# generate and shorten in one step
helm-schema ./mychart --compact --shorten-defs --defs-map defs-map.json -o values.schema.json
# or shorten a reviewed readable schema
helm-schema shorten values.readable.json values.schema.json --map defs-map.json --compact
```

Shortening is a one-to-one rename of the same schema. The map file lists the
readable name of every short key, so a Helm or validator error that mentions
`#/$defs/2b` can be translated back
(`helm_schema::output::expand_short_definition_names`).

## Emission profiles

The default `full` profile emits every supported constraint. For exceptionally
conditional charts, the `lean` profile omits document-level conditional
validation while preserving base path, type, values-default, and provider
constraints:

```bash
helm-schema ./mychart --profile lean --compact
```

Lean is an output policy over the same analysis result, not a separate
inference mode. Removing conditional refinements can only widen acceptance:
the lean schema may admit a value that the full schema rejects, but it must
not reject one that full accepts. This trade-off reduces schema size and
Helm validator compilation cost on arm-heavy charts.

## Formatting

| Flag | Effect |
|---|---|
| *(default)* | Pretty-printed, human-readable JSON. |
| `--compact` | Single-line JSON — smaller, for committing or piping. |
| `--strip-descriptions` | Drop `description` annotations (schema-aware: a property named `description` is preserved). Useful when the upstream Kubernetes descriptions make the file larger than you want. |

## Determinism

Given the same chart, options, and upstream schemas, repeated runs produce **byte-identical** output. Object keys are ordered and any iteration that could affect the emitted JSON is sorted, so the file is safe to commit and diff in CI. See [Continuous integration]({{< relref "ci.md" >}}).
