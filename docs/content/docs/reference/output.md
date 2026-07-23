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
| An int-or-string pattern (e.g. `schema1`) | Accepts Helm's quoted-integer form (`"3"`) alongside a numeric field, because a quoted number renders and validates the same as the bare number. |

The exact names (`schema1`, `schema2`, …) are assigned deterministically during minimization; their **contents** are what carry meaning.

## `$ref` handling

By default the output is **self-contained**: external file/URL `$ref`s discovered during analysis are resolved and re-homed into root-level `$defs`, so the schema stands alone while still sharing referenced subschemas.

| Flag | Effect |
|---|---|
| *(default)* | Resolve external refs into root-level `$defs`. |
| `--keep-refs` | Leave external `$ref` strings exactly as-is. |
| `--inline-refs` | Fully inline resolved refs instead of writing `$defs`. |

`--keep-refs` and `--inline-refs` are mutually exclusive.

## Minimization

Branch-scoped guard arms tend to repeat large fragments. By default, `helm-schema` interns repeated subtrees into root-level `$defs` and references them with `$ref`. This is an **output-only** transform — it never affects inference — and it matters in practice: it keeps big charts well under Helm's 5 MiB chart-file limit and speeds up every downstream validator.

```bash
# Keep everything inline (larger, but no $ref indirection)
helm-schema ./mychart --no-minimize
```

## Formatting

| Flag | Effect |
|---|---|
| *(default)* | Pretty-printed, human-readable JSON. |
| `--compact` | Single-line JSON — smaller, for committing or piping. |
| `--strip-descriptions` | Drop `description` annotations (schema-aware: a property named `description` is preserved). Useful when the upstream Kubernetes descriptions make the file larger than you want. |

## Determinism

Given the same chart, options, and upstream schemas, repeated runs produce **byte-identical** output. Object keys are ordered and any iteration that could affect the emitted JSON is sorted, so the file is safe to commit and diff in CI. See [Continuous integration]({{< relref "ci.md" >}}).
