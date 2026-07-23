---
title: Documentation
bookToc: false
bookFlatSection: false
---

# Documentation

`helm-schema` generates a JSON Schema for a Helm chart's `values.yaml` by statically analyzing the chart's templates. This documentation takes you from installation to the details of how types are inferred, how Kubernetes and CRD schemas are consulted, and how to shape the result.

## Start here

- **[Introduction]({{< relref "introduction.md" >}})** — what the tool does and why template analysis beats reading `values.yaml`.
- **[Installation]({{< relref "installation.md" >}})** — install the `helm-schema` binary.
- **[Quick start]({{< relref "quick-start.md" >}})** — your first run and how to read the output.
- **[How it works]({{< relref "how-it-works.md" >}})** — the analysis pipeline, phase by phase.

## Guide

- **[Template analysis]({{< relref "guide/template-analysis.md" >}})** — value extraction and control-flow-aware guards.
- **[Values & defaults]({{< relref "guide/values-and-defaults.md" >}})** — composed defaults, `default`-literal type inference, and required fields.
- **[Kubernetes schemas]({{< relref "guide/kubernetes-schemas.md" >}})** — versions, fallback, mirrors, offline use, and apiVersion inference.
- **[CRD schemas]({{< relref "guide/crd-schemas.md" >}})** — the CRD catalog, version lookup, and local overrides.
- **[Subcharts & dependencies]({{< relref "guide/subcharts.md" >}})** — vendored dependencies, globals, and library-chart scoping.
- **[Schema overrides]({{< relref "guide/overrides.md" >}})** — post-process the generated schema.

## Reference

- **[CLI reference]({{< relref "reference/cli.md" >}})** — every flag and argument.
- **[Output]({{< relref "reference/output.md" >}})** — the shape of the generated schema, `$ref` handling, and minimization.
- **[Diagnostics]({{< relref "reference/diagnostics.md" >}})** — the warnings and hints the tool emits.
- **[Caching]({{< relref "reference/caching.md" >}})** — cache layout and the compatibility policy.

## Also

- **[Continuous integration]({{< relref "ci.md" >}})** — generate and verify a schema in CI.
- **[FAQ]({{< relref "faq.md" >}})** — status, limitations, and common questions.

> [!NOTE]
> **helm-schema is alpha.** It is useful and works for many charts, but Helm templating has many edge cases; expect some charts to need [manual overrides]({{< relref "guide/overrides.md" >}}) or occasional incorrect inference.
