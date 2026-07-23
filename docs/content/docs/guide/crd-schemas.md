---
title: CRD schemas
weight: 4
---

# CRD schemas

Charts that deploy custom resources — a `ServiceMonitor`, a `Certificate`, an operator's CRD — need schemas that aren't part of the built-in Kubernetes API. `helm-schema` resolves those against a **CRD catalog**, using the same lookup order as built-in resources, with your local overrides at the top.

The lookup order for any grouped resource is:

1. **local overrides** (`--crd-override-dir`) — authoritative,
2. **the CRD catalog** (fetched and cached, per version),
3. **upstream Kubernetes JSON schemas** (for built-in kinds).

## Version lookup: strict vs loose

```bash
helm-schema ./mychart --crd-version-lookup strict   # default
helm-schema ./mychart --crd-version-lookup loose
```

- **`strict`** (default) consults only the exact `(group, kind, version)` the chart pinned. A CRD version is never substituted — pinning `v1` and getting a `v1alpha1` schema would be wrong.
- **`loose`** resolves identically (still no substitution), but additionally scans the local cache for *other* versions of the same `(group, kind)`. When the requested version is missing but others are present, it emits a [`CrdVersionAvailableAtOtherVersions`]({{< relref "/docs/reference/diagnostics.md" >}}) hint — informational only.

`--strict-crd-version` is a short alias for `--crd-version-lookup=strict`, kept for symmetry with the other strict flags.

## Mirrors and caching

```bash
helm-schema ./mychart --crd-catalog-mirror https://my.mirror/crds
```

`--crd-catalog-mirror` (repeatable) adds alternate upstream catalog sources, available in both modes. As with Kubernetes mirrors, per-source cache namespacing keeps mirror entries from masking the default catalog.

`--crd-catalog-cache-dir` sets the managed cache root for CRD schemas — a separate root from the Kubernetes cache, versioned and invalidated independently. See [Caching]({{< relref "/docs/reference/caching.md" >}}).

`--crd-cache-record-source` writes a `<schema>.json.meta` sidecar next to each cache entry recording the fetch URL and timestamp — useful when debugging which mirror answered.

## Local overrides

`--crd-override-dir` is a hand-maintained layer that sits at the **top** of the lookup chain, ahead of both the catalog and the Kubernetes provider. Anything placed here is authoritative.

```bash
helm-schema ./mychart --crd-override-dir ./schema-overrides
```

Files are keyed by `(group, version, kind)` at `<group>/<kind>_<version>.json`. Despite the historical `crd-` prefix, this layer accepts a schema for **any** grouped resource — a CRD you patched locally, or even a built-in Kubernetes resource whose upstream schema you deliberately want to shadow (to add constraints, lock to a historical schema, or work around an upstream bug).

Two things to know:

- This directory is **never wiped** and is not subject to the cache invalidation contract — it is your content, not a managed cache. Do not point it at a directory you don't control.
- If an override file *claims* a resource but is unreadable, the chain emits [`LocalOverrideUnreadable`]({{< relref "/docs/reference/diagnostics.md" >}}) and **does not fall through** to the catalog or upstream. Silently substituting a different schema for one you pinned would be wrong.

> [!NOTE]
> The old `--crd-catalog-dir` flag has been **removed**. Use `--crd-override-dir` for hand-maintained schemas and/or `--crd-catalog-cache-dir` for the managed cache root. Passing the old flag fails CLI validation with a hint pointing to the replacements.
