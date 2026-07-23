---
title: How it works
weight: 4
---

# How it works

You don't need to know any of this to use `helm-schema` — but it explains *why* the schema is more accurate than one derived from `values.yaml`, and what to expect from the output.

The short version: it reads your chart the way Helm does, follows each value to where it is actually used, and types it from what's expected there.

## It combines three signals

For every value, `helm-schema` looks at:

1. **How your templates use it.** Which resource field the value renders into, and the control flow around it (`if`, `with`, `range`, `default`, `eq`, `not`, `or`).
2. **Your composed defaults.** The root `values.yaml` merged with each subchart's defaults and `global` — the same composition Helm does.
3. **What Kubernetes expects there.** Once a value is traced to a resource field, its type comes from the upstream Kubernetes JSON schema (or the CRD schema) for that field.

These are merged into one Draft-07 schema rooted at the values object. That's the whole idea — the contract is recovered from what the chart *does*, not from what its defaults happen to be.

## What that buys you

Compared with reading `values.yaml`, following the templates catches things a defaults-only tool can't:

- **Values with no default.** A value the chart reads but doesn't ship a default for still appears in the schema, correctly typed.
- **Real types, not sample types.** A port that happens to be `80` isn't "an integer because 80 looks numeric" — it's an integer because the Service field it lands in *is* one. The schema even carries Kubernetes' own description for that field.
- **Control flow.** A value only used behind `{{ if .Values.x.enabled }}` is only constrained when that guard is on — exactly as the template behaves.
- **Whole subtrees.** A block poured into a typed field with `{{ toYaml .Values.resources | nindent … }}` expands to the full shape Kubernetes expects there (all of `limits`, `requests`, `claims`), not just the keys your defaults set.
- **Typos.** The root is closed (`additionalProperties: false`), so a key the chart never reads — `replicaCont` instead of `replicaCount` — is rejected instead of silently ignored.

## When it can't be sure, it doesn't guess

Accuracy also means not inventing answers:

- A genuinely ambiguous value stays a **union** (or an explicit "unknown") rather than being collapsed to a convenient type.
- Anything that can't be resolved — a value behind a computed key, an `apiVersion` that can't be pinned — is surfaced as a [diagnostic]({{< relref "reference/diagnostics.md" >}}) on stderr, never quietly guessed. For those residual cases you can add a [schema override]({{< relref "guide/overrides.md" >}}).

## Two guarantees worth knowing

- **It ignores any shipped `values.schema.json`.** A schema a chart or dependency already ships is another author's assertion — possibly stale, incomplete, or written for a different purpose. `helm-schema` never reads it as input; the schema is always recovered from the chart itself. (If you *want* to inject assertions, that's what [`--override-schema`]({{< relref "guide/overrides.md" >}}) is for — applied explicitly, by you.)
- **The output is deterministic.** The same chart, options, and upstream schemas always produce byte-identical output, so it diffs cleanly and is safe to commit and check in CI.
