---
title: Diagnostics
weight: 3
---

# Diagnostics

Anything `helm-schema` can't resolve is reported as a **diagnostic** on standard error, never silently guessed. Diagnostics never appear in the schema (which goes to stdout), so they don't interfere with piping the output to a file.

## Format

```bash
helm-schema ./mychart --diag-format text   # default
helm-schema ./mychart --diag-format json
```

- **`text`** (default): human-readable lines prefixed with `warning:` or `info:`.
- **`json`**: one JSON object per line. Each is a `Diagnostic` — a discriminated union tagged on a `"type"` field. After a successful CLI parse, *every* stderr line is such an object.

CLI **parse** errors are not part of the JSON contract: clap writes a plain-text usage error before the runtime starts. JSON consumers distinguish the two by exit code — a non-zero exit with no JSON line emitted means a parse error.

## Variants

| Variant | When it fires |
|---|---|
| `MissingSchema` | No provider in the chain owns the resource. Carries the Kubernetes versions tried, filenames tried, and (when available) other cache versions that *do* hold the resource. |
| `ResolvedFromFallbackVersion` | A non-primary Kubernetes version answered — the [auto-fallback chain]({{< relref "/docs/guide/kubernetes-schemas.md" >}}#automatic-version-fallback) resolved it. |
| `InferredApiVersion` | An `apiVersion` was inferred for a kind with no static `apiVersion` in the template (requires `--api-version-guess`). |
| `AmbiguousApiVersion` | Multiple plausible `apiVersion`s exist; the analyzer abstains rather than guess. |
| `CrdVersionNotFound` | The chart's exact CRD version wasn't found in any probed location. |
| `CrdVersionAvailableAtOtherVersions` | The exact `(group, kind)` exists at *other* versions in the local cache (loose mode). Informational only — the chain never substitutes. |
| `LocalOverrideUnreadable` | A hand-maintained override claimed a resource but its file is unreadable. A hard error: the chain does **not** fall through. |
| `CacheLayoutInvalidated` | A managed cache root's layout predated the binary; it was wiped and will be repopulated. See [Caching]({{< relref "caching.md" >}}). |
| `CacheLayoutForwardIncompatible` | A managed cache root carries a marker *newer* than the binary; the binary refuses to mutate it. |

## Reading them

Most diagnostics are about *type resolution*, not correctness of your chart — a `MissingSchema` for a rarely-used CRD just means that field stays permissively typed. Common responses:

- **`MissingSchema` / `CrdVersionNotFound`** → add a [`--crd-catalog-mirror`]({{< relref "/docs/guide/crd-schemas.md" >}}), warm the cache online, or supply the schema via [`--crd-override-dir`]({{< relref "/docs/guide/crd-schemas.md" >}}#local-overrides).
- **`AmbiguousApiVersion`** → pin the `apiVersion` in the template if you can, or accept the abstention; the field stays untyped rather than mistyped.
- **`ResolvedFromFallbackVersion`** → expected when a chart targets a deprecated API; informational.
- **`CacheLayout*`** → see the [caching]({{< relref "caching.md" >}}) compatibility policy.

## In CI

Because diagnostics are on stderr and the schema is on stdout, you can capture both independently:

```bash
helm-schema ./mychart --diag-format json \
  --output values.schema.json \
  2> diagnostics.jsonl
```

Then fail the build on specific variants (for example, treat `LocalOverrideUnreadable` as fatal) by scanning `diagnostics.jsonl` for their `"type"`.
