---
title: Template analysis
weight: 1
---

# Template analysis

The core of `helm-schema` is reading the chart's templates. This page explains what it extracts and how the control flow around a value shapes the schema — which is what lets you predict the output.

## Value extraction

Every `.Values.*` path that appears in render logic is collected — from manifests, from `_helpers.tpl` helpers, and from YAML fragments loaded via `.Files.Get` with a literal path. It reads the template's real structure, so quoting, pipelines, nested calls, and whitespace trimming (`{{-`/`-}}`) are all handled correctly rather than approximated.

A value the chart reads becomes a property in the schema even if it has **no default** in `values.yaml`. And the root is closed (`additionalProperties: false`): a key the chart never consumes is rejected, which turns a typo into a validation error instead of a silently-ignored value.

## Control flow becomes structure

A value's meaning depends on the guards around it, and `helm-schema` reflects that in the schema. Consider this chart, which only renders a HorizontalPodAutoscaler when `autoscaling.enabled` is set:

{{< chartfile "guards/templates/hpa.yaml" >}}

With `helm-schema ./guards --no-k8s-schemas`, the guard becomes conditional structure:

{{< schema "guards" >}}

Reading it:

- The `{{- if .Values.autoscaling.enabled }}` guard becomes an **`if`/`then`**: *if* `autoscaling.enabled` is truthy, *then* the fields rendered inside apply — here, `minReplicas` is constrained to an integer. When the guard is false, nothing about the guarded fields is asserted, exactly as the template behaves (they're never rendered).
- `autoscaling.minReplicas` is therefore only typed **inside** the `then` branch. At the top level it's left open, because a chart running with `autoscaling.enabled: false` never reads it.

## Truthiness

Notice `autoscaling.enabled` isn't simply `type: boolean`. Helm's notion of "truthy" is broader than a boolean: `false`, `0`, `""`, an empty list, and an empty map are all falsy; everything else is truthy. A guard like `{{ if .Values.x }}` therefore doesn't prove `x` is a boolean — any non-empty value satisfies it.

That's the `anyOf` you see: `const: true`, or a non-zero number, or a non-empty string / array / object. `helm-schema` encodes exactly what the guard requires, so the schema stays as permissive as the template really is. A value only becomes a strict `boolean` when something types it as one — for example, when it also lands in a boolean Kubernetes field.

## Recognized patterns

Beyond block guards, common function patterns are understood as control flow:

| Pattern | Effect on the schema |
|---|---|
| `if .Values.x` / `with .Values.x` | `x` guarded by truthiness; guarded fields become conditional |
| `range .Values.x` | `x` is an array or map; loop-body fields attach to its elements |
| `eq .Values.x "a"` | `x` compared against a literal — contributes an expected value |
| `default <literal> .Values.x` | `x` gains the literal's type; see [Values & defaults]({{< relref "values-and-defaults.md" >}}) |
| `not` / `or` / `and` | combine guards; reflected in the emitted `anyOf` / `allOf` / `not` |

## Where analysis stops

Some constructs can't be resolved from the templates alone — a value used only through a dynamically-computed key, a helper whose output depends on runtime data, or an `apiVersion` that can't be pinned. When that happens, `helm-schema` keeps the value permissive or emits an explicit [diagnostic]({{< relref "/docs/reference/diagnostics.md" >}}) rather than inventing a type. For those residual cases, add a [schema override]({{< relref "overrides.md" >}}).

## Related flags

- `--no-k8s-schemas` — template analysis only; skip all Kubernetes/CRD lookups. Useful to see exactly what the templates imply, and fully offline.
- `--exclude-tests` — don't analyze `templates/tests/**`.
- `--infer-required` — promote unconditionally-guarded paths to `required`; see [Values & defaults]({{< relref "values-and-defaults.md" >}}).
