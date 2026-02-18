
# helm-schema

[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/build.yaml?branch=main&label=build">](https://github.com/romnn/helm-schema/actions/workflows/build.yaml)
[<img alt="test status" src="https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/test.yaml?branch=main&label=test">](https://github.com/romnn/helm-schema/actions/workflows/test.yaml)
[![dependency status](https://deps.rs/repo/github/romnn/helm-schema/status.svg)](https://deps.rs/repo/github/romnn/helm-schema)
[<img alt="docs.rs" src="https://img.shields.io/docsrs/helm-schema/latest?label=docs.rs">](https://docs.rs/helm-schema)
[<img alt="crates.io" src="https://img.shields.io/crates/v/helm-schema">](https://crates.io/crates/helm-schema)

Generate a **JSON Schema** for a Helm chart's `values.yaml` by **analyzing the chart's templates**:

- Discovers a chart (and its vendored dependencies under `charts/`, including `.tgz`/`.tar.gz` archives).
- Parses Helm templates and statically extracts `.Values.*` usages.
- Tracks template control flow (guards like `if`, `with`, `range`, plus patterns such as `eq`, `not`, and `or`).
- Tracks Kubernetes resource context (best-effort `apiVersion`/`kind`) so a value use can be mapped to a Kubernetes schema path.
- Infers value types using upstream Kubernetes JSON schemas and CRD schemas.
- Merges these signals into a single Draft-07 JSON schema (commonly saved as `values.schema.json`).

## Why this is different

Many Helm schema generation approaches primarily rely on:

- The chart's `values.yaml` as the source-of-truth for types, and/or
- Manual annotations/comments embedded in `values.yaml`.

Those approaches can be useful, but they don't see how values are *actually used* in templates.

`helm-schema` goes a step further:

- **Template-aware**: it inspects the templates and extracts `.Values.*` paths from actual render logic.
- **Resource-aware**: it tries to understand *which* Kubernetes resource a value is used in and *where* in that resource.
- **Schema-backed**: it uses upstream Kubernetes JSON schemas (and CRD schemas) to infer realistic object/field types.

In other words: instead of documenting values in isolation, it tries to infer the contract from:

- How values are consumed by templates.
- What Kubernetes expects at the target field.

The result is often a more informative schema than one derived from `values.yaml` alone.

## Installation

```bash
# via brew
brew install --cask romnn/tap/helm-schema

# or from source
cargo install --locked helm-schema-cli
```

## Usage

### Basic

Generate schema for a chart directory:

```bash
helm-schema ./path/to/chart > values.schema.json
```

Write to a file:

```bash
helm-schema ./path/to/chart --output values.schema.json
```

Compact output:

```bash
helm-schema ./path/to/chart --output values.schema.json --compact
```

### Helpful workflows

Generate a schema fully offline (no network access) using a pre-populated schema cache:

```bash
helm-schema ./path/to/chart \
  --offline \
  --k8s-schema-cache-dir ./k8s-schema-cache \
  --output values.schema.json
```

Generate without Kubernetes schemas (template-only extraction + defaults):

```bash
helm-schema ./path/to/chart --no-k8s-schemas --output values.schema.json
```

### Kubernetes schema configuration

- `--k8s-version <VERSION>`
  - Kubernetes schema version (default: `v1.35.0`).
- `--k8s-schema-cache-dir <DIR>`
  - Cache directory for downloaded Kubernetes schemas.
- `--offline`
  - Disable all network access; use only local caches.
- `--no-k8s-schemas`
  - Disable Kubernetes JSON schema lookup entirely.

### CRD schemas

- `--crd-catalog-dir <DIR>`
  - Directory used for CRD schemas and/or caching.

If schema sources are missing or incomplete, the CLI may emit warnings to stderr (the CLI collects warnings during provider lookup and prints them after schema generation).

### Chart traversal options

- `--exclude-tests`
  - Do not scan `templates/tests/**`.
- `--no-subchart-values`
  - Do not include vendored subchart values under `charts/` in the composed values.

### Applying a schema override

You can post-process the generated schema with an override schema:

```bash
helm-schema ./path/to/chart \
  --override-schema ./schema-override.json \
  --output values.schema.json
```

Overrides are applied as a recursive merge (with special handling to union `required` lists), which is useful for tightening types and filling inference gaps.

## How it works (high level)

- Chart discovery reads `Chart.yaml` (and supports `Chart.template.yaml`) and walks vendored dependencies under `charts/`.
- It composes an effective values document by merging the root `values.yaml` with subchart defaults under their dependency keys (and merging subchart `global` into top-level `global`).
- Templates are parsed and an index of named `define` blocks is built so helper templates can be analyzed.
- A symbolic extractor collects value-uses (`.Values.*`) and guards from template actions, and tries to track the YAML path where a value is emitted.
- The chart's `files/` directory is also scanned for YAML/TPL fragments; when templates load YAML fragments from `.Files.Get` using literal paths, those fragments can be analyzed too.
- A Kubernetes/CRD schema provider chain is consulted to infer types for values used in specific resource paths.
- Inference signals are merged into a single JSON schema rooted at the Helm values object.

## Status / disclaimer

This project is **useful and works for many charts**, but it is **not yet fully production-ready** or 100% reliable.

Helm templating and YAML composition have many edge cases (whitespace trimming, dynamic keys, helper indirection, runtime-only behavior, etc.).
Expect some charts to require manual overrides and expect occasional incorrect or missing inference.

