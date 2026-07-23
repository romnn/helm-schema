---
title: FAQ
weight: 8
---

# FAQ

## Is it production-ready?

`helm-schema` is **alpha**. It is useful and works for many charts, but Helm templating and YAML composition have many edge cases — whitespace trimming, dynamic keys, helper indirection, runtime-only behavior — and some charts will need [manual overrides]({{< relref "guide/overrides.md" >}}) or will surface an occasional incorrect or missing inference. Treat the output as a strong starting point you can review and tighten, not an infallible contract.

## Does it need Helm, or a cluster?

No to both. It parses templates itself (it does not shell out to `helm`) and never contacts a Kubernetes API server. "Resource-aware" means it consults **published JSON schemas** for the resources a chart renders, not a live cluster.

## Does it read my chart's existing `values.schema.json`?

No — deliberately. A shipped `values.schema.json` is an external author's assertion that may be stale, incomplete, or written for a different purpose. Treating it as input would silently replace static analysis with trust in another tool. The schema is always recovered from what the chart actually does. If you want to inject external assertions, that's what [`--override-schema`]({{< relref "guide/overrides.md" >}}) is for — applied explicitly, by you.

## Why is an integer typed as an `anyOf` of an integer and a string?

Because Helm accepts a **quoted** number wherever a number is expected — `replicas: "2"` renders and validates the same as `replicas: 2`. The extra `anyOf` arm (a numeric `pattern` string) accepts that quoted form, so the schema doesn't reject inputs Helm itself would happily render. See [Output]({{< relref "reference/output.md" >}}#recurring-building-blocks).

## Why does an `if .Values.x` guard not make `x` a boolean?

Helm's "truthy" is broader than a boolean: `false`, `0`, `""`, an empty list, and an empty map are all falsy. `helm-schema` encodes exactly that (as an `anyOf` of the non-empty forms) so the schema stays as permissive as the template actually is. A value becomes a strict `boolean` only when something types it as one. See [Template analysis]({{< relref "guide/template-analysis.md" >}}#truthiness).

## Nothing is marked `required`. Is that a bug?

No — that's the default. A value with a default is optional, so the schema stays permissive. Opt into required-field inference with [`--infer-required`]({{< relref "guide/values-and-defaults.md" >}}#required-fields), which promotes only paths the chart checks unconditionally.

## A field I care about is typed loosely / not at all. What can I do?

- Check stderr for a [diagnostic]({{< relref "reference/diagnostics.md" >}}) explaining why (often `MissingSchema` or `AmbiguousApiVersion`).
- If it's a CRD, add a [`--crd-catalog-mirror`]({{< relref "guide/crd-schemas.md" >}}) or supply the schema via [`--crd-override-dir`]({{< relref "guide/crd-schemas.md" >}}#local-overrides).
- If the `apiVersion` couldn't be pinned, try [`--api-version-guess`]({{< relref "guide/kubernetes-schemas.md" >}}#apiversion-inference).
- As a last resort, tighten it with an [`--override-schema`]({{< relref "guide/overrides.md" >}}).

## How do I run it fully offline?

Warm a cache online once, then pass `--offline` with the cache dirs. See [Working offline]({{< relref "guide/kubernetes-schemas.md" >}}#working-offline) and [Caching]({{< relref "reference/caching.md" >}}).

## Which Kubernetes version does it use?

`v1.35.0` by default. Override with `--k8s-version`, and use `--k8s-version-fallback` for charts that target APIs removed from recent Kubernetes. See [Kubernetes schemas]({{< relref "guide/kubernetes-schemas.md" >}}).

## Is the output stable enough to commit?

Yes. Given the same chart, options, and upstream schemas, the output is byte-identical run to run, so it diffs cleanly. That's what the [CI verification pattern]({{< relref "ci.md" >}}) relies on.

## Where do I report a bad inference?

Open an issue at [github.com/romnn/helm-schema](https://github.com/romnn/helm-schema/issues) with the chart (or a minimal reproduction) and the schema you got versus what you expected.
