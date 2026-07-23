---
title: Continuous integration
weight: 7
---

# Continuous integration

There are two ways to use `helm-schema` in CI: **commit** the generated schema and verify it stays in sync, or **generate** it fresh as part of packaging. Both rely on the output being [deterministic]({{< relref "reference/output.md" >}}#determinism).

## Verify a committed schema

The most common setup commits `values.schema.json` next to `values.yaml` (so Helm validates against it) and has CI fail if the committed file drifts from what the chart now produces.

```yaml
name: helm-schema
on: [push, pull_request]

jobs:
  schema:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@v4

      - name: Install helm-schema
        run: cargo install --locked helm-schema-cli

      - name: Regenerate the schema
        run: helm-schema ./charts/mychart --output ./charts/mychart/values.schema.json

      - name: Fail if it drifted
        run: git diff --exit-code -- ./charts/mychart/values.schema.json
```

If the schema is out of date, `git diff --exit-code` fails and prints the diff. Contributors run the same `helm-schema … --output …` command locally to update it.

## Generate at package time

Alternatively, don't commit the schema — generate it just before `helm package`:

```yaml
      - name: Generate schema
        run: helm-schema ./charts/mychart --output ./charts/mychart/values.schema.json

      - name: Package
        run: helm package ./charts/mychart
```

## Running offline

CI runners that block egress can't fetch upstream schemas on the fly. Warm a cache in a step that has network (or restore it from the workflow cache), then run with `--offline`:

```yaml
      - name: Restore schema cache
        uses: actions/cache@v4
        with:
          path: .helm-schema-cache
          key: helm-schema-k8s-v1.35.0

      - name: Generate (offline once warm)
        run: |
          helm-schema ./charts/mychart \
            --k8s-schema-cache-dir .helm-schema-cache/k8s \
            --crd-catalog-cache-dir .helm-schema-cache/crd \
            --k8s-version v1.35.0 \
            --output ./charts/mychart/values.schema.json
```

Pin `--k8s-version` so the cache key and the generated types are stable. See [Caching]({{< relref "reference/caching.md" >}}).

## Surfacing diagnostics

To gate the build on resolution problems, emit [diagnostics]({{< relref "reference/diagnostics.md" >}}) as JSON and inspect them:

```bash
helm-schema ./charts/mychart --diag-format json \
  --output ./charts/mychart/values.schema.json \
  2> diagnostics.jsonl

# Example: fail if any hand-maintained override was unreadable
if grep -q '"type":"LocalOverrideUnreadable"' diagnostics.jsonl; then
  echo "::error::an override schema was unreadable" && exit 1
fi
```

Most diagnostics are informational and shouldn't fail a build — pick the specific variants that matter for your charts.
