---
title: Introduction
weight: 1
---

# Introduction

`helm-schema` generates a **JSON Schema** for a Helm chart's `values.yaml` by **statically analyzing the chart's templates**. You point it at a chart directory (or a packaged `.tgz`), and it prints a Draft-07 schema:

```bash
helm-schema ./mychart --output mychart/values.schema.json
```

Helm reads a `values.schema.json` placed next to `values.yaml` automatically and validates user-supplied values against it during `install`, `upgrade`, `lint`, and `template`. A good schema turns a class of "it rendered, but wrong" mistakes into an error at submit time.

## The problem with reading `values.yaml`

The obvious way to describe a chart's values is to look at its `values.yaml`. Most schema generators do exactly that — they read the defaults, and optionally some annotations in comments, and emit types to match.

But `values.yaml` is a set of **defaults**, not a **contract**. It tells you what a value is *set to*, never what it is *allowed to be*:

```yaml
service:
  port: 80          # is this an int? a string? does anything else validate?
replicas: 1         # must it be ≥ 0? an int32? could a quoted "1" work?
ingress:
  enabled: false    # a bool — or is it used as a truthy guard that accepts anything?
```

The defaults file can't answer those questions, because the answer lives in the templates. Reading `values.yaml` alone also can't see values that have **no default at all** but are still consumed by a template, and it can't distinguish a value that must be an object from one that merely happens to be an empty map today.

## The approach

`helm-schema` recovers the contract from the two places that actually define it:

1. **How the templates consume each value** — the `.Values.*` paths that appear in render logic, and the control flow (`if`, `with`, `range`, `default`, `eq`, `not`, `or`) that guards them.
2. **What Kubernetes expects at the target field** — once a value is traced to a resource field (say a Deployment's `spec.replicas`), its type comes from the upstream Kubernetes or CRD schema for that field.

{{< io in="hero/templates/deployment.yaml" schema="hero" intitle="templates/deployment.yaml" >}}

Neither `replicaCount` nor `revisionHistoryLimit` has a stated type anywhere in the chart. `helm-schema` types both as `int32` — and carries over Kubernetes' own field descriptions — purely from where they land in the Deployment.

## What makes it different

| | Reads `values.yaml` | `helm-schema` |
|---|---|---|
| Source of truth | the defaults file | the templates |
| Sees values with no default | no | yes |
| Understands guards (`if`/`with`/`range`/…) | no | yes |
| Types from the Kubernetes/CRD API | no | yes |
| Preserves genuine ambiguity | collapses to the default's type | keeps a union, or abstains |

And it doesn't guess. When a value is genuinely ambiguous, the schema keeps it permissive (a union, or an explicit "unknown") rather than collapsing it to a convenient type that might reject valid input. You get a schema that reflects what your chart really accepts — not a plausible-looking one that's subtly wrong.

## What you can do with it

| Goal | Where |
|---|---|
| Generate a schema for a chart | [Quick start]({{< relref "quick-start.md" >}}) |
| Understand the pipeline | [How it works]({{< relref "how-it-works.md" >}}) |
| Tune template analysis and required fields | [Template analysis]({{< relref "guide/template-analysis.md" >}}), [Values & defaults]({{< relref "guide/values-and-defaults.md" >}}) |
| Control Kubernetes / CRD schema lookup | [Kubernetes schemas]({{< relref "guide/kubernetes-schemas.md" >}}), [CRD schemas]({{< relref "guide/crd-schemas.md" >}}) |
| Fill inference gaps by hand | [Schema overrides]({{< relref "guide/overrides.md" >}}) |
| Wire it into CI | [Continuous integration]({{< relref "ci.md" >}}) |

> [!NOTE]
> **helm-schema is alpha.** It works well for many charts, but Helm templating has many edge cases (whitespace trimming, dynamic keys, helper indirection, runtime-only behavior). Expect some charts to need [overrides]({{< relref "guide/overrides.md" >}}) or to surface an occasional incorrect or missing inference. See the [FAQ]({{< relref "faq.md" >}}).
