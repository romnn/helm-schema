---
title: Guide
weight: 5
bookCollapseSection: true
---

# Guide

How `helm-schema` turns a chart into a schema, and how to steer it when a chart needs a nudge.

- **[Template analysis]({{< relref "template-analysis.md" >}})** — how values are extracted from templates and how control flow (`if`, `with`, `range`, `eq`, `not`, `or`) becomes schema structure.
- **[Values & defaults]({{< relref "values-and-defaults.md" >}})** — the composed values document, `default`-literal type inference, nullable unions, and `--infer-required`.
- **[Kubernetes schemas]({{< relref "kubernetes-schemas.md" >}})** — pinning versions, the fallback chain, mirrors, offline use, and apiVersion inference.
- **[CRD schemas]({{< relref "crd-schemas.md" >}})** — the CRD catalog, strict vs loose version lookup, and hand-maintained overrides.
- **[Subcharts & dependencies]({{< relref "subcharts.md" >}})** — vendored dependencies, `global`, and how library-chart helpers are scoped.
- **[Schema overrides]({{< relref "overrides.md" >}})** — merge hand-written schemas on top of the inferred output.
