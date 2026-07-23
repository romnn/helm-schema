---
title: Installation
weight: 2
---

# Installation

`helm-schema` is a single self-contained binary. There is nothing to configure to get started — the Kubernetes and CRD schemas it consults are fetched on demand and cached.

## Homebrew

```bash
brew install --cask romnn/tap/helm-schema
```

This installs a prebuilt binary from the [`romnn/homebrew-tap`](https://github.com/romnn/homebrew-tap) tap. Prebuilt binaries are published for Linux (`x86_64`, `aarch64`), macOS (Intel, Apple Silicon), and Windows (`x86_64`, `aarch64`).

## From crates.io

The CLI ships from the `helm-schema-cli` crate. The binary it installs is named `helm-schema`:

```bash
cargo install --locked helm-schema-cli
```

`--locked` builds against the crate's committed `Cargo.lock` for a reproducible build.

## Prebuilt binaries

Every release attaches prebuilt archives to the [GitHub releases page](https://github.com/romnn/helm-schema/releases). Download the archive for your platform, extract the `helm-schema` binary, and put it on your `PATH`.

## From source

```bash
git clone https://github.com/romnn/helm-schema
cd helm-schema
cargo build --release --package helm-schema-cli
# binary at target/release/helm-schema
```

## Verify

```bash
helm-schema --help
```

You should see the usage summary. If you build from source, `cargo run -- --help` works too.

## What it needs at runtime

- **Network access** (by default) to fetch upstream Kubernetes JSON schemas and CRD schemas the first time a given resource/version is needed. Results are cached, so subsequent runs are fast and can run offline.
- **No Helm installation.** `helm-schema` parses templates itself; it does not shell out to `helm`.
- **No cluster access.** It never contacts a Kubernetes API server; "resource-aware" means it consults published JSON schemas, not a live cluster.

To run without any network — for example in a sealed CI environment — pre-populate a cache and pass `--offline`. See [Kubernetes schemas]({{< relref "guide/kubernetes-schemas.md" >}}) and [Caching]({{< relref "reference/caching.md" >}}).

Next: the [Quick start]({{< relref "quick-start.md" >}}).
