---
title: Subcharts & dependencies
weight: 5
---

# Subcharts & dependencies

`helm-schema` analyzes a chart together with its vendored dependencies, so the schema covers values consumed by subcharts too.

## Discovery

Chart discovery reads `Chart.yaml` (and `Chart.template.yaml`) and walks dependencies under `charts/` — both unpacked directories and packaged `.tgz`/`.tar.gz` archives. Each dependency's templates are analyzed, and the values it consumes appear in the schema under its dependency key.

## Composed values and `global`

The effective values document merges:

- the parent chart's `values.yaml`,
- each subchart's defaults, nested under its dependency key,
- each subchart's `global` values, merged into the top-level `global`.

That mirrors how Helm itself composes values, so a value a subchart reads from `.Values.global.*` and one the parent sets under `subchartName.*` both land where the schema expects them.

To analyze only the parent chart's own values and skip vendored subchart defaults:

```bash
helm-schema ./mychart --no-subchart-values
```

## Library charts

Library charts (`type: library`) are the subtle case. A library has **no value scope of its own** — its helpers run in the *caller's* scope, so a `.Values.X` inside a library helper resolves against whichever chart `include`s that helper. `helm-schema` follows those helper calls, so values a library reads on your chart's behalf show up under **your** chart's values, correctly.

Two things follow from this, both of which keep the schema tight:

- **Only helpers you actually use contribute.** If a library defines ten helpers and your chart includes one, only that one's values appear in your schema. A shared library doesn't pollute a consuming chart with values it never reaches.
- **It follows the full chain.** If your chart includes a helper that itself includes another, the values read deep in that chain still land under your chart's values.

This applies to the type hints and `default`-fallback paths that feed [`--infer-required`]({{< relref "values-and-defaults.md" >}}) too, so requiredness inferred through a library helper is attributed to the right chart.
