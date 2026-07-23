---
title: Values & defaults
weight: 2
---

# Values & defaults

Beyond where a value is *used*, `helm-schema` looks at how the chart *defaults* it — both the composed `values.yaml` document and `default` expressions in templates.

## The composed values document

Before analysis, the tool builds one effective values document:

- the root `values.yaml`,
- each subchart's defaults merged under its dependency key,
- subchart `global` merged into the top-level `global`.

This composed document seeds the schema's shape and supplies the default value for each path. Use `--no-subchart-values` to omit vendored subchart defaults under `charts/` (see [Subcharts]({{< relref "subcharts.md" >}})).

Additional files passed with `-f`/`--values` are treated as **documentation metadata only**: their *comments* can layer into schema `description`s, but they never contribute types or accepted paths.

## `default`-literal type inference

A very common Helm idiom is to ship a `null` (or absent) placeholder in `values.yaml` and fall back to a literal at render time:

{{< chartfile "defaults/values.yaml" >}}

{{< chartfile "defaults/templates/deployment.yaml" >}}

When a template uses `default <literal> .Values.X` — or the pipeline form `.Values.X | default <literal>` — the literal reveals `X`'s type: string, integer, number, or boolean. Combined with a `null`/absent default, the result is a **nullable union**:

{{< schema "defaults" >}}

Both `replicaCount` and `nameOverride` become "the literal's type, *or* null, *or* a falsy value". That first `not: { … }` arm is the falsy placeholder — `null`, `""`, `0`, `false`, or an empty collection — the value the chart's `default` is there to catch. Admitting it means the schema accepts exactly the inputs the template is built to handle, without flagging the intended `null` as an error.

Non-literal fallbacks don't get a type hint — the tool can't statically know the type of `default .Chart.Name .Values.X` or `default (printf "%s" .Y) .Values.X` — but they are still recognized as fallbacks (which matters for required-field inference below).

## Required fields

By default the schema marks **nothing** as `required`: the generated schema stays as permissive as the template logic allows, because a value with a default is, by definition, optional.

`--infer-required` opts into inferring requiredness from the templates. A path is marked `required` on its parent object when the chart checks it **unconditionally** — at the top of a template, with no enclosing guard:

```gotmpl
{{- if .Values.provider }}          {{/* provider is checked unconditionally */}}
...
{{- end }}
```

```bash
helm-schema ./mychart --infer-required
```

Paths reachable through **any** `default <expr> .Values.X` fallback are excluded from requiredness — literal or not — because the chart explicitly handles them being unset:

```gotmpl
{{ default "myapp" .Values.nameOverride }}   {{/* never required: has a fallback */}}
```

This makes `--infer-required` safe to turn on: it only tightens paths the chart itself treats as mandatory, and never a path the chart is prepared to see absent.

> [!NOTE]
> Requiredness is inferred from **unconditional guards**, not from "has no default". A value with no default that is only used inside another guard is not promoted, because the template doesn't actually require it in every code path.
