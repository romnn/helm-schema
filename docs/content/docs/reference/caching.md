---
title: Caching
weight: 4
---

# Caching

`helm-schema` caches the Kubernetes and CRD schemas it fetches so repeat runs are fast and can run [offline]({{< relref "/docs/guide/kubernetes-schemas.md" >}}#working-offline). The cache is purely a speed optimization — **never** a source of truth.

> [!NOTE]
> The upstream schema source is always authoritative; the cache is fetch-on-miss. A run against a cold cache and a run against a warm cache produce the **same schema**. If a code path could answer differently depending on what happens to be cached, that is a bug — the tool is designed so it can't.

## Cache roots

Two managed cache roots, set independently and versioned/invalidated independently:

| Flag | Holds |
|---|---|
| `--k8s-schema-cache-dir <DIR>` | Upstream Kubernetes JSON schemas. |
| `--crd-catalog-cache-dir <DIR>` | CRD catalog schemas. |

A forward-incompatible Kubernetes cache does not block CRD resolution, and vice versa.

`--crd-override-dir` is a **different concept** — hand-maintained content, never wiped, no marker, not subject to the policy below. Mixing the two roles in one directory is prevented at CLI parse time.

## Per-source layout

Each managed root uses a per-source layout so a mirror URL never silently masks the default catalog:

```
<k8s-cache-root>/
├── CACHE_LAYOUT_VERSION
├── default/                    # built-in catalog
│   └── v1.35.0/service-v1.json
└── <hash-of-mirror-url>/       # per-mirror namespace
    └── v1.35.0/service-v1.json

<crd-cache-root>/
├── CACHE_LAYOUT_VERSION
├── default/
│   └── monitoring.coreos.com/servicemonitor_v1.json
└── <hash-of-mirror-url>/
    └── monitoring.coreos.com/servicemonitor_v1.json
```

At lookup time the **default catalog wins** over mirrors. A mirror's entry stays in its own namespace for inspection, but is not returned in preference to the default.

## Compatibility policy (alpha)

The cache layout is **not a stable compatibility surface during alpha**. Each managed root carries a `CACHE_LAYOUT_VERSION` marker, checked at startup against the binary's compiled-in constant:

| Marker state | Behavior | Diagnostic |
|---|---|---|
| Matches the binary | Used as-is | — |
| Missing, root populated (legacy) | Managed subtree wiped and repopulated | `CacheLayoutInvalidated` |
| Missing, root empty | First-populate; marker written | — |
| Older than the binary | Wiped and repopulated | `CacheLayoutInvalidated` |
| Newer than the binary | Binary refuses to mutate the root; left untouched | `CacheLayoutForwardIncompatible` |

On a forward-incompatible marker, upgrade `helm-schema` or point the flag at a different path.

## Refreshing

- `--no-cache` bypasses cache **reads** and re-checks upstream directly. Successful responses and authoritative 404s still refresh cache state, so this repairs stale or partial entries.
- `--offline` (or `HELM_SCHEMA_ALLOW_NET=0`) uses only what is on disk. On a miss with no authoritative record, the tool returns an explicit unknown rather than inventing an answer — it never promotes "not in the cache" to "does not exist upstream".

## Warming a cache for offline CI

```bash
# Once, online: populate the cache for the versions/resources you need
helm-schema ./mychart \
  --k8s-schema-cache-dir ./cache/k8s \
  --crd-catalog-cache-dir ./cache/crd \
  --output /dev/null

# Later, sealed environment: reuse it with no network
helm-schema ./mychart \
  --offline \
  --k8s-schema-cache-dir ./cache/k8s \
  --crd-catalog-cache-dir ./cache/crd \
  --output values.schema.json
```

Commit or archive the cache directories as a build artifact so the offline run is reproducible.
