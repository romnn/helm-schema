---
title: Kubernetes schemas
weight: 3
---

# Kubernetes schemas

When a value flows into a built-in Kubernetes resource field, `helm-schema` types it from the upstream Kubernetes JSON schema for that resource. This is what turns `spec.replicas` into an `int32` with the API's own description, or a `resources` block into the full `ResourceRequirements` shape.

Schemas are fetched on demand from the upstream catalog and cached locally. The upstream source is authoritative; the [cache]({{< relref "/docs/reference/caching.md" >}}) only makes repeat lookups fast.

## Choosing the version

```bash
helm-schema ./mychart --k8s-version v1.34.0
```

`--k8s-version` selects the Kubernetes minor version whose schemas are consulted. The default is **`v1.35.0`**.

The flag is **repeatable** and order matters: the first value is the primary version, and any further values are explicit fallbacks consulted in order when the primary doesn't hold a resource.

```bash
# Try 1.34 first, then fall back to 1.28 for anything 1.34 lacks
helm-schema ./mychart --k8s-version v1.34.0 --k8s-version v1.28.0
```

## Automatic version fallback

Charts in the wild still ship resources on APIs that were removed from recent Kubernetes — for example `policy/v1beta1` (`PodDisruptionBudget`) or `networking.k8s.io/v1beta1` (`Ingress`), both removed in v1.25. `--k8s-version-fallback` auto-extends a single `--k8s-version` with older minors so those lookups still resolve:

```bash
# Extend the primary version with a default-sized window of older minors
helm-schema ./mychart --k8s-version-fallback auto

# Or an explicit number of older minors
helm-schema ./mychart --k8s-version-fallback 5
```

`auto` uses a default window (15 minors), sized to cover the realistic deprecation horizon. The lookup then falls back through `v1.34.0 → v1.33.0 → …` until it finds the schema.

- It is **mutually exclusive** with `--strict-k8s-version`, and is rejected if combined with multiple explicit `--k8s-version` values (spell the list out explicitly in that case).
- Auto-fallback versions are an escape valve for the **schema-lookup layer only**. They do not participate in apiVersion inference, so a chart missing an `apiVersion` won't silently pick up a fallback's deprecated API and become ambiguous.

`--strict-k8s-version` disables the auto-fallback chain entirely:

```bash
helm-schema ./mychart --strict-k8s-version
```

## Mirrors

Add alternate upstream sources with `--k8s-schema-mirror` (repeatable). Mirrors are alternate exact-version sources, not heuristics, so they work in both strict and loose modes. Per-source cache namespacing keeps a mirror's entries from masking the default catalog — the default catalog always wins at lookup time.

```bash
helm-schema ./mychart --k8s-schema-mirror https://my.mirror/schemas
```

## Working offline

The cache makes fully offline runs possible once it is warm. To guarantee no network access:

```bash
helm-schema ./mychart \
  --offline \
  --k8s-schema-cache-dir ./k8s-schema-cache \
  --output values.schema.json
```

- `--offline` disables all network access and uses only local caches. (Equivalent to setting `HELM_SCHEMA_ALLOW_NET=0`.)
- `--k8s-schema-cache-dir` points at a managed cache root you control — warm it once online, then reuse it in a sealed environment. See [Caching]({{< relref "/docs/reference/caching.md" >}}).
- `--no-cache` does the opposite: bypass cache **reads** and re-check upstream directly (successful responses and authoritative 404s still refresh the cache, so this repairs stale entries).

To skip Kubernetes schemas altogether and rely purely on template analysis:

```bash
helm-schema ./mychart --no-k8s-schemas
```

This is fully offline and deterministic, and is the right choice when you only care about the template-implied shape.

## apiVersion inference

To type a resource field, the analyzer needs the resource's `apiVersion` and `kind`. It recovers these structurally from the manifest whenever possible — including across `if`/`else` branches and from helpers that resolve to a finite set of literals.

When the `apiVersion` genuinely can't be pinned (it's templated or absent), the lookup normally fails with `apiVersion unknown`. `--api-version-guess` enables a bounded, three-tier inference:

1. a hardcoded canonical `kind → apiVersion` shortlist for unambiguous Kubernetes and Prometheus-operator kinds,
2. a scan across the configured local caches,
3. a kind-scoped probe against the CRD catalog mirrors (only for kinds in the extended shortlist — no blind group sweeps).

```bash
helm-schema ./mychart --api-version-guess
```

When exactly one `apiVersion` is implied, it emits an [`InferredApiVersion`]({{< relref "/docs/reference/diagnostics.md" >}}) diagnostic. When several are plausible (e.g. `Ingress` in both `networking.k8s.io/v1` and `extensions/v1beta1`), it **abstains** and emits `AmbiguousApiVersion` rather than guessing wrong.

`--strict-api-versions` disables inference entirely (mutually exclusive with `--api-version-guess`), so an unpinnable `apiVersion` is always left unresolved.
