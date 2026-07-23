---
title: Schema overrides
weight: 6
---

# Schema overrides

Static analysis can't recover everything — a value assembled through a dynamically-computed key, a field an outer pipeline injects at render time, or a type the templates simply never pin down. For those residual cases, post-process the generated schema with one or more **override schemas**.

```bash
helm-schema ./mychart \
  --override-schema ../schemas/injected-keys.override.json \
  --override-schema ./schema-override.json \
  --output values.schema.json
```

`--override-schema` is **repeatable**. Files are applied in the order given, each merged on top of the previous result. A common split is a shared cross-chart override (for example, top-level keys an outer templating pipeline injects and `helm-schema` can't see in `values.yaml`) followed by a chart-specific override.

## How merging works

Overrides are applied as a **recursive merge**, which is what makes them good for tightening types and filling inference gaps rather than replacing the whole schema. Two behaviors are worth knowing:

- **`required` lists are unioned**, not replaced, so an override can add required properties without dropping the ones inference already found.
- **A subtree that contains `$ref` replaces** the corresponding base subtree entirely, rather than merging into it. JSON Schema draft-07 ignores siblings of `$ref`, so merging would otherwise leave inferred constraints stranded next to the ref'd schema and produce a shape no input can satisfy.

## Example

Suppose an outer pipeline injects a `global.tenant` string that never appears in the chart's own `values.yaml`, and you want to require a `region`:

```json
{
  "properties": {
    "global": {
      "type": "object",
      "properties": {
        "tenant": { "type": "string" }
      }
    },
    "region": { "type": "string", "enum": ["us", "eu"] }
  },
  "required": ["region"]
}
```

Merged on top of the inferred schema, this adds `global.tenant`, constrains `region`, and unions `region` into the root `required` list — leaving everything the analyzer inferred intact.

> [!NOTE]
> An override schema is a **caller policy input** applied *after* inference — it is the one sanctioned way to inject external assertions. This is deliberately different from a chart's shipped `values.schema.json`, which `helm-schema` never reads as inference evidence: that file is another author's assertion and may be stale or written for a different purpose. Overrides are yours, and you apply them explicitly.
