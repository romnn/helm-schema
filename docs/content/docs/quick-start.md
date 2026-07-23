---
title: Quick start
weight: 3
---

# Quick start

This walks through a first run and how to read the output. It assumes `helm-schema` is [installed]({{< relref "installation.md" >}}).

## 1. Point it at a chart

The only required argument is the chart — a directory or a packaged `.tgz`/`.tar.gz`:

```bash
helm-schema ./mychart
```

The generated schema is written to **standard output** by default, so you can redirect it or inspect it directly. To write it where Helm will find it, use `--output`:

```bash
helm-schema ./mychart --output mychart/values.schema.json
```

Helm automatically validates values against a `values.schema.json` next to `values.yaml` on `install`, `upgrade`, `lint`, and `template`.

## 2. A worked example

Take this small chart. It sets a handful of values and uses them across a Deployment and a Service:

{{< chartfile "quickstart/values.yaml" >}}

{{< chartfile "quickstart/templates/deployment.yaml" >}}

{{< chartfile "quickstart/templates/service.yaml" >}}

Running `helm-schema ./quickstart` produces:

{{< schema "quickstart" >}}

## 3. How to read it

Nothing in the chart states a *type* for any of these values — yet the schema types all of them, because it followed each one into the resource field it renders into.

- **`replicaCount`** flows into the Deployment's `spec.replicas`, so it is an `int32`. Its `description` is Kubernetes' own text for that field. The second `anyOf` arm (the `pattern` string) accepts Helm's quoted-integer form — `replicaCount: "2"` renders and validates the same as `2`, so the schema allows both.

- **`service.port`** lands in a Service port and is typed the same way, with the Service field's description.

- **`resources`** is poured into the container's `resources` field with `{{ toYaml .Values.resources | nindent 12 }}`. Because the whole subtree lands in a typed Kubernetes field, the schema expands to the full `ResourceRequirements` shape — `limits`, `requests`, and `claims` — each with its upstream description, even though `values.yaml` only sets `limits.cpu` and `limits.memory`.

- **`image`** is an object with `repository` and `tag`. It is used in a string position (`{{ .Values.image.repository }}:{{ .Values.image.tag }}`), so the `allOf` guard records that `image.repository` must **not** be an array — the precise constraint the template implies, without over-claiming a full string type.

- **`additionalProperties: false`** at the root means the schema lists exactly the values the chart uses. Keys the chart never reads are rejected, which is what catches typos like `replicaCont`.

> [!NOTE]
> The schemas shown throughout these docs are **fully inlined** for readability. By default the tool interns subtrees that repeat across a large schema into a shared `$defs` section to keep the file small — pass `--no-minimize` to inline them, as shown here. See [Output]({{< relref "reference/output.md" >}}).

## 4. Common variations

```bash
# Compact (single-line) JSON instead of pretty-printed
helm-schema ./mychart --compact --output mychart/values.schema.json

# Template analysis only — skip all Kubernetes/CRD schema lookups (fully offline)
helm-schema ./mychart --no-k8s-schemas

# Pin the Kubernetes version whose schemas are consulted
helm-schema ./mychart --k8s-version v1.34.0

# Mark unconditionally-guarded values as required on their parent object
helm-schema ./mychart --infer-required
```

## 5. Where to go next

- [How it works]({{< relref "how-it-works.md" >}}) — the analysis pipeline, phase by phase.
- [Template analysis]({{< relref "guide/template-analysis.md" >}}) — how guards and control flow shape the schema.
- [Kubernetes schemas]({{< relref "guide/kubernetes-schemas.md" >}}) — versions, offline use, and mirrors.
- [CLI reference]({{< relref "reference/cli.md" >}}) — every flag.
