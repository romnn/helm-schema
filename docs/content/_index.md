---
title: helm-schema
type: docs
bookToc: false
---

<div class="hs-hero">
  <div class="hs-hero__text">
    <h1>helm&#8209;schema</h1>
    <p class="hs-hero__lead">Generate a <strong>JSON Schema</strong> for a Helm chart's <code>values.yaml</code> by <strong>analyzing the chart's templates</strong> — not by guessing from the defaults file. Open source, MIT-licensed.</p>
    <div class="hs-hero__cmd">helm-schema ./mychart</div>
    <div class="hs-hero__actions">
      <a class="hs-btn hs-btn--primary" href="{{< relref "/docs/introduction.md" >}}">Read the docs</a>
      <a class="hs-btn" href="https://github.com/romnn/helm-schema">Source on GitHub</a>
    </div>
  </div>
</div>

<div class="hs-badges">

[![build status](https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/build.yaml?branch=main&label=build)](https://github.com/romnn/helm-schema/actions/workflows/build.yaml)
[![test status](https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/test.yaml?branch=main&label=test)](https://github.com/romnn/helm-schema/actions/workflows/test.yaml)
[![crates.io](https://img.shields.io/crates/v/helm-schema)](https://crates.io/crates/helm-schema)
[![docs.rs](https://img.shields.io/docsrs/helm-schema/latest?label=docs.rs)](https://docs.rs/helm-schema)

</div>

A chart's `values.yaml` is a documented default, not a contract. It says what a value *is set to*, never what it *must be* — a port that happens to be `80` could be typed as a string, an object, or anything else. `helm-schema` recovers the real contract by reading the templates: it follows each `.Values.*` into the resource field it renders, and types it from what Kubernetes expects there.

{{< io in="hero/templates/deployment.yaml" schema="hero" intitle="templates/deployment.yaml" >}}

`replicaCount` lands in a Deployment's `spec.replicas`, so it is typed as an `int32` and annotated with Kubernetes' own field description — recovered from the template, never stated in `values.yaml`. (The second arm covers Helm's quoted-number form, e.g. `"1"`.)

## What it does

<div class="hs-cards">
  <div class="hs-card">
    <h3>Template-aware</h3>
    <p>Parses Helm templates and statically extracts every <code>.Values.*</code> path from the actual render logic — not from the defaults file.</p>
  </div>
  <div class="hs-card">
    <h3>Control-flow aware</h3>
    <p>Understands <code>if</code>, <code>with</code>, <code>range</code>, and patterns like <code>eq</code>, <code>not</code>, <code>or</code>, and <code>default</code>, so the schema mirrors the template's real logic.</p>
  </div>
  <div class="hs-card">
    <h3>Resource-aware</h3>
    <p>Tracks which Kubernetes resource and field a value flows into, so it can type the value against the upstream API.</p>
  </div>
  <div class="hs-card">
    <h3>Schema-backed</h3>
    <p>Types values from upstream Kubernetes JSON schemas and CRD catalogs, and merges everything into one Draft-07 schema.</p>
  </div>
</div>

## Get started

```bash
# Install the CLI (or: brew install --cask romnn/tap/helm-schema)
cargo install --locked helm-schema-cli

# Generate a schema for a chart directory
helm-schema ./mychart --output mychart/values.schema.json
```

Helm picks up a `values.schema.json` next to `values.yaml` automatically and validates user-supplied values against it on `install`, `upgrade`, `lint`, and `template`.

## Documentation

- [Introduction]({{< relref "/docs/introduction.md" >}}) and [Installation]({{< relref "/docs/installation.md" >}}).
- [Quick start]({{< relref "/docs/quick-start.md" >}}) — a first run and how to read the output.
- [How it works]({{< relref "/docs/how-it-works.md" >}}) — the analysis pipeline, phase by phase.
- [Guide]({{< relref "/docs/guide/_index.md" >}}) — template analysis, defaults, Kubernetes & CRD schemas, subcharts, overrides.
- [Reference]({{< relref "/docs/reference/_index.md" >}}) — every CLI flag, the output shape, diagnostics, and caching.
