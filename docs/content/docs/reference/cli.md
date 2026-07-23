---
title: CLI reference
weight: 1
---

# CLI reference

`helm-schema` is a single command: one positional argument (the chart) plus flags. There are no subcommands. The generated schema goes to standard output unless `--output` is given; diagnostics go to standard error.

```
helm-schema [OPTIONS] <CHART_DIR>
```

Run `helm-schema --help` for the authoritative, version-specific summary.

## Argument

| Argument | Description |
|---|---|
| `<CHART_DIR>` | Chart directory or packaged chart archive (`.tgz`/`.tar.gz`) to analyze. Required. |

## Output

| Flag | Description |
|---|---|
| `-o`, `--output <FILE>` | Write the schema to a file; standard output is used when absent. |
| `--compact` | Compact JSON instead of the default pretty-printed output. |
| `--strip-descriptions` | Remove JSON Schema `description` annotations. Schema-aware: a property literally named `description` is kept. |
| `--keep-refs` | Leave file/URL `$ref` strings as-is. By default external refs are resolved into root-level `$defs` so the output is self-contained. Conflicts with `--inline-refs`. |
| `--inline-refs` | Fully inline resolved file/URL `$ref`s instead of writing `$defs`. |
| `--no-minimize` | Keep repeated subtrees inline instead of interning them into root-level `$defs`. Interning is on by default. |

See [Output]({{< relref "output.md" >}}) for what these produce.

## Kubernetes schemas

| Flag | Description |
|---|---|
| `--k8s-version <VERSION>` | Kubernetes minor version dir(s) to consult, in priority order; first is primary. Repeatable. Default: **`v1.35.0`**. |
| `--k8s-version-fallback <auto\|N>` | Auto-extend a single `--k8s-version` with older minors. `auto` uses the default window; `<N>` sets the window size. Conflicts with `--strict-k8s-version`. |
| `--strict-k8s-version` | Suppress the auto-fallback chain. |
| `--k8s-schema-mirror <URL>` | Additional upstream Kubernetes schema mirror. Repeatable. Available in strict and loose modes. |
| `--k8s-schema-cache-dir <DIR>` | Managed cache root for Kubernetes schemas. Subject to the cache invalidation contract. |
| `--no-cache` | Bypass cache **reads** and re-check upstream directly. Successful responses and authoritative 404s still refresh the cache. |
| `--offline` | Force offline; use only local caches. Equivalent to `HELM_SCHEMA_ALLOW_NET=0`. |
| `--no-k8s-schemas` | Skip upstream Kubernetes schemas entirely (template analysis only). |

See [Kubernetes schemas]({{< relref "/docs/guide/kubernetes-schemas.md" >}}).

## CRD schemas

| Flag | Description |
|---|---|
| `--crd-version-lookup <strict\|loose>` | CRD version lookup mode. Default `strict` (only the exact `(group, kind, version)`); `loose` adds a local cross-scan and informational hints. Never substitutes a version. |
| `--strict-crd-version` | Short alias for `--crd-version-lookup=strict`. |
| `--crd-catalog-mirror <URL>` | Additional upstream CRD catalog mirror. Repeatable. Available in both modes. |
| `--crd-catalog-cache-dir <DIR>` | Managed cache root for CRD schemas. Subject to the cache invalidation contract. |
| `--crd-override-dir <DIR>` | Hand-maintained schema overrides at the top of the lookup chain. Never wiped; not a managed cache. Keyed by `(group, version, kind)`. |
| `--crd-cache-record-source` | Write a `<schema>.json.meta` sidecar next to each CRD cache entry recording the fetch URL and timestamp. |

See [CRD schemas]({{< relref "/docs/guide/crd-schemas.md" >}}).

## apiVersion inference

| Flag | Description |
|---|---|
| `--api-version-guess` | Enable bounded apiVersion inference for kinds whose `apiVersion` the analyzer couldn't pin. Conflicts with `--strict-api-versions`. |
| `--strict-api-versions` | Disable apiVersion inference entirely. |

See [apiVersion inference]({{< relref "/docs/guide/kubernetes-schemas.md" >}}#apiversion-inference).

## Chart traversal

| Flag | Description |
|---|---|
| `--exclude-tests` | Skip `templates/tests/**`. |
| `--no-subchart-values` | Omit vendored subchart defaults under `charts/` from the composed values. |
| `-f`, `--values <FILE>` | Additional values files whose *comments* layer into schema descriptions. Documentation metadata only — no type hints or accepted paths. Repeatable. |
| `--infer-required` | Mark unconditionally-guarded paths as `required` on their parent. Paths with a `default <expr>` fallback are excluded. |

## Overrides

| Flag | Description |
|---|---|
| `--override-schema <FILE>` | Schema files merged on top of the inferred output, in the order given. Repeatable. |

See [Schema overrides]({{< relref "/docs/guide/overrides.md" >}}).

## Diagnostics & tracing

| Flag | Description |
|---|---|
| `--diag-format <text\|json>` | Format for diagnostics on stderr. Default `text`. `json` emits one structured object per line. |
| `--trace-output <FILE>` | Write a Perfetto-readable trace of the run. |

See [Diagnostics]({{< relref "diagnostics.md" >}}).

## Environment variables

| Variable | Effect |
|---|---|
| `HELM_SCHEMA_ALLOW_NET=0` | Disable all network access (same as `--offline`). |

## Mutually exclusive flags

- `--keep-refs` and `--inline-refs`
- `--strict-k8s-version` and `--k8s-version-fallback`
- `--api-version-guess` and `--strict-api-versions`
- `--k8s-version-fallback` is also rejected alongside multiple explicit `--k8s-version` values.
