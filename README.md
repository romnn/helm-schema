
# helm-schema

[<img alt="build status" src="https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/build.yaml?branch=main&label=build">](https://github.com/romnn/helm-schema/actions/workflows/build.yaml)
[<img alt="test status" src="https://img.shields.io/github/actions/workflow/status/romnn/helm-schema/test.yaml?branch=main&label=test">](https://github.com/romnn/helm-schema/actions/workflows/test.yaml)
[![dependency status](https://deps.rs/repo/github/romnn/helm-schema/status.svg)](https://deps.rs/repo/github/romnn/helm-schema)
[<img alt="docs.rs" src="https://img.shields.io/docsrs/helm-schema/latest?label=docs.rs">](https://docs.rs/helm-schema)
[<img alt="crates.io" src="https://img.shields.io/crates/v/helm-schema">](https://crates.io/crates/helm-schema)

Generate a **JSON Schema** for a Helm chart's `values.yaml` by **analyzing the chart's templates**:

📖 **[Documentation](https://romnn.github.io/helm-schema/)** — introduction, quick start, guide, and full CLI reference.

- Discovers a chart (and its vendored dependencies under `charts/`, including `.tgz`/`.tar.gz` archives).
- Reads the Helm templates and finds every `.Values.*` the chart actually uses.
- Follows the control flow around each value (`if`, `with`, `range`, plus patterns such as `default`, `eq`, `not`, and `or`), so the schema mirrors the template's real logic.
- Tracks which Kubernetes resource and field a value flows into, so it can type that value from the upstream API.
- Types values using upstream Kubernetes JSON schemas and CRD schemas.
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

- `--k8s-version <VERSION>` (repeatable)
  - Kubernetes minor version directory(s) to consult, in user-supplied priority order.
    The first value is the primary; any further values are explicit fallbacks.
    Default: `v1.35.0`.
- `--k8s-version-fallback=auto|<N>`
  - Auto-extend the (single explicit) `--k8s-version` with `N` older minors. `auto`
    uses the default window (`15`, sized to cover the realistic K8s deprecation
    horizon — charts in the wild still ship `policy/v1beta1` and
    `networking.k8s.io/v1beta1`, both removed in v1.25). Useful for charts that
    target a deprecated API (e.g. `PodSecurityPolicy (policy/v1beta1)`) — the
    lookup falls back through `v1.34.0 → v1.33.0 → …` until it finds the schema.
    Mutually exclusive with `--strict-k8s-version`; rejected when combined with
    multiple explicit `--k8s-version` values (the right knob in that case is the
    explicit list).
  - Auto-fallback versions are escape valves for the schema-lookup layer only;
    they do NOT participate in apiVersion inference cache scans (so a chart
    that's missing an `apiVersion` for a `PodDisruptionBudget` won't pick up the
    fallback's `policy/v1beta1` and become ambiguous against `policy/v1`).
- `--k8s-schema-mirror <URL>` (repeatable)
  - Additional upstream K8s schema mirror URL. Per-source cache namespacing keeps
    mirror entries from masking the default catalog. **Available in both strict
    and loose modes** — mirrors are alternate exact-version sources, not heuristics.
- `--k8s-schema-cache-dir <DIR>`
  - Managed cache root for K8s schemas. Subject to the [cache compatibility
    contract](#cache-compatibility-policy-alpha) below.
- `--strict-k8s-version`
  - Disable Feature B's auto-fallback chain. Conflicts only with
    `--k8s-version-fallback`; orthogonal to `--k8s-schema-mirror`.
- `--offline`
  - Disable all network access; use only local caches.
- `--no-k8s-schemas`
  - Disable Kubernetes JSON schema lookup entirely.

### CRD schemas

- `--crd-version-lookup=strict|loose` (default: `strict`)
  - `strict`: only the exact `(group, kind, version)` the chart pinned is consulted.
  - `loose`: same as strict for the actual schema resolution (CRD version is never
    substituted), but additionally enables a local-cache cross-scan that lets the
    tool emit a `CrdVersionAvailableAtOtherVersions` hint when the requested
    version is missing but other versions of the same `(group, kind)` are present
    in the cache.
- `--strict-crd-version`
  - Short alias for `--crd-version-lookup=strict`. Kept for symmetry with the other
    strict flags and to keep CI opt-out flags short.
- `--crd-catalog-mirror <URL>` (repeatable)
  - Additional upstream CRD catalog mirror URL. Per-source cache namespacing keeps
    mirror entries from masking the default catalog. **Available in both strict
    and loose modes**.
- `--crd-catalog-cache-dir <DIR>`
  - Managed cache root for CRD schemas. Subject to the [cache compatibility
    contract](#cache-compatibility-policy-alpha) below.
- `--crd-override-dir <DIR>`
  - Hand-maintained local schema override layer. **Never wiped**, never subject
    to the cache invalidation contract. Despite the historical `crd-` prefix in
    the flag name (kept for compatibility), this layer accepts schemas for
    **any** grouped resource — CRDs you've patched locally, built-in K8s
    resources whose upstream schema you want to override, anything keyed by
    `(group, version, kind)`. It sits at the top of the lookup chain ahead of
    both the CRD catalog and the K8s OpenAPI provider, so anything placed here
    is authoritative.
  - Authoritative shadowing of built-in schemas is deliberate (power-user
    feature for adding custom constraints, locking a chart to a historical
    schema, or working around an upstream bug). The safety implication: don't
    point this at a directory you don't control; whatever JSON is at
    `<group>/<kind>_<version>.json` will silently replace the upstream answer.
  - If the override file is unreadable, the chain emits
    `LocalOverrideUnreadable` and **does not fall through** to the catalog or
    upstream — silently substituting a different schema for one the user pinned
    would be wrong.
- `--crd-cache-record-source`
  - Write a `<schema>.json.meta` sidecar alongside every CRD cache entry recording
    the fetch URL and timestamp. Useful when debugging which mirror answered.

Note: the previous `--crd-catalog-dir` flag is **removed**. Use
`--crd-override-dir` (hand-maintained schemas) and/or `--crd-catalog-cache-dir`
(managed cache root) instead. Passing the old flag fails CLI validation with a
hint pointing to the replacements.

### apiVersion guessing for unknown kinds

When the IR walker can't statically pin a manifest's `apiVersion` (because it's
templated or absent), the lookup normally fails with `apiVersion unknown`. Two
flags control the optional inference:

- `--api-version-guess`
  - Enable a three-tier inference path: (1) a hardcoded canonical
    `kind → apiVersion` shortlist for unambiguous K8s + Prometheus operator kinds,
    (2) a local cache scan across every configured K8s + CRD cache namespace,
    (3) a kind-scoped HTTP probe against the CRD catalog mirrors (only for
    kinds in the extended shortlist — no blind group sweeps). When exactly one
    apiVersion is implied, the chain emits `InferredApiVersion`. When multiple
    plausible candidates exist (e.g. `Ingress` in both `networking.k8s.io/v1`
    and `extensions/v1beta1`), the chain abstains and emits `AmbiguousApiVersion`.
- `--strict-api-versions`
  - Disable the inference path entirely. Mutually exclusive with
    `--api-version-guess`.

### Diagnostics

- `--diag-format=text|json` (default: `text`)
  - In `text` mode, diagnostics print to stderr prefixed with `warning:` or `info:`.
  - In `json` mode, each diagnostic is emitted as a single JSON object per line on
    stderr. After successful CLI parse, every stderr line is a `Diagnostic` JSON
    object (a discriminated union tagged on the `"type"` field). CLI parse errors
    are not subject to this contract — clap writes its plain-text usage error to
    stderr before our runtime starts; JSON consumers detect "parse vs runtime" by
    exit code: non-zero exit before any JSON line means parse-error.

Diagnostic variants the tool emits:

| Variant | When |
| --- | --- |
| `MissingSchema` | No provider in the chain owns the resource. Carries the K8s versions tried, filenames tried, and (when available) other cache versions that *do* hold the resource. |
| `ResolvedFromFallbackVersion` | A non-primary K8s version answered (Feature B). |
| `InferredApiVersion` | The apiVersion was inferred for a kind with no static apiVersion in the template (Feature D). |
| `AmbiguousApiVersion` | Multiple plausible apiVersions; the chain abstains. |
| `CrdVersionNotFound` | The chart's exact CRD version was not found in any probed location. |
| `CrdVersionAvailableAtOtherVersions` | Same `(group, kind)` exists at other versions in local cache; informational only — the chain never substitutes. |
| `LocalOverrideUnreadable` | The hand-maintained override claimed a resource but its file is unreadable. Hard error: the chain does not fall through. |
| `CacheLayoutInvalidated` | A managed cache root's layout was older than the compiled binary; the root was wiped and will be repopulated. |
| `CacheLayoutForwardIncompatible` | A managed cache root carries a marker newer than the binary; the binary refuses to mutate the root. |

### Cache compatibility policy (alpha)

helm-schema cache layout is **not a stable compatibility surface during alpha**.
Each managed cache root (`--k8s-schema-cache-dir`, `--crd-catalog-cache-dir`)
carries an on-disk `CACHE_LAYOUT_VERSION` marker. At startup:

- Marker matches the binary's compiled-in constant → root is used as-is.
- Marker missing and root is populated (legacy layout) → managed subtree is
  wiped and repopulated. One `CacheLayoutInvalidated` diagnostic is emitted.
- Marker missing and root is empty → first-populate, marker is written. No
  diagnostic.
- Marker older than the binary's constant → wipe and repopulate, same as above.
- Marker newer than the binary's constant → the binary refuses to mutate the
  root (forward-incompat). One `CacheLayoutForwardIncompatible` diagnostic is
  emitted; the root is left untouched. The user is expected to upgrade or point
  the flag at a different path.

K8s and CRD roots are versioned and invalidated **independently**. A forward-
incompat K8s cache does not block CRD resolution and vice versa.

`--crd-override-dir` is a **different concept** — it is hand-maintained content,
never wiped, no marker, not subject to this contract. Mixing the two roles in
a single directory is prevented at CLI parse time.

### Cache layout (per-source namespacing)

Both managed cache roots use a per-source layout so a `--k8s-schema-mirror` /
`--crd-catalog-mirror` URL never silently masks the default catalog's content:

```
<k8s-cache-root>/
├── CACHE_LAYOUT_VERSION
├── default/                                  # built-in yannh catalog
│   ├── v1.35.0/service-v1.json
│   └── …
└── <hash-of-mirror-url>/                     # per-mirror namespace
    └── v1.35.0/service-v1.json

<crd-cache-root>/
├── CACHE_LAYOUT_VERSION
├── default/                                  # built-in datreeio catalog
│   ├── monitoring.coreos.com/servicemonitor_v1.json
│   └── …
└── <hash-of-mirror-url>/                     # per-mirror namespace
    └── monitoring.coreos.com/servicemonitor_v1.json
```

Precedence at lookup time: default catalog wins over mirrors. The mirror's cache
entry stays in its own namespace for inspection / debugging but is not returned.

### Chart traversal options

- `--exclude-tests`
  - Do not scan `templates/tests/**`.
- `--no-subchart-values`
  - Do not include vendored subchart values under `charts/` in the composed values.
- `--infer-required`
  - Mark paths that the chart checks unconditionally at the top of a
    template (`{{- if .Values.X }}` / `{{- eq .Values.X "..." }}` with no
    enclosing guard) as `required` on their parent object. Paths reachable
    via any `default <expr> .Values.X` fallback (literal or non-literal,
    e.g. `default .Chart.Name .Values.nameOverride`) are excluded because
    the chart explicitly handles them being unset. Off by default — the
    generated schema stays as permissive as the template logic allows.

### Default-value type inference

When a template uses `default <literal> .Values.X` (or the pipeline form
`.Values.X | default <literal>`), `helm-schema` infers `X`'s type from the
literal — string, integer, number, or boolean. Combined with a `null` (or
absent) value in `values.yaml`, the inferred schema becomes a nullable
union: `anyOf: [{type: null}, {type: <literal-type>}]`. This catches the
common Helm pattern of "ship a `null` placeholder, fall back to a literal
at render time" without surfacing it as a schema validation error.

Non-literal fallbacks (`default .Chart.Name .Values.X`, `default (printf
"%s" .Y) .Values.X`) don't get a type hint — we can't statically infer the
type — but they still suppress `--infer-required` for `X`.

#### Scoping for library (`type: library`) subcharts

Library charts have no value scope of their own — their helpers run in
the caller's scope, with `.Values.X` resolving against the chart that
`include`s the helper. `helm-schema` builds a per-helper call graph
across every chart's templates: nodes are individual `{{ define
"name" }}` blocks plus per-chart "chart-direct" pseudo-nodes for text
outside any define, edges are `{{ include "callee" ... }}` /
`{{ template "callee" ... }}` references.

When a non-library chart's schema is generated, the helpers reachable
from that chart's chart-direct includes — followed transitively through
the graph — are the helpers whose signals (type hints and
`default`-fallback paths used by `--infer-required`) apply at that
chart's value prefix. The resolution is helper-granular, not library-
name-granular: a library that defines both a helper the app includes
and another helper the app never references will only contribute the
included helper's signals. Transitive chains (`app → libA.X → libB.Y`)
carry signals from the deepest helper back to the app.

### Applying a schema override

You can post-process the generated schema with one or more override schemas:

```bash
helm-schema ./path/to/chart \
  --override-schema ../schemas/injected-keys.override.json \
  --override-schema ./schema-override.json \
  --output values.schema.json
```

`--override-schema` is repeatable. Files are applied in the order given, each merged on top of the previous result. The intended split is a shared cross-chart override (e.g. top-level keys an outer pipeline injects at render time and helm-schema can't see in `values.yaml`) followed by a chart-specific override.

Overrides are applied as a recursive merge (with special handling to union `required` lists), which is useful for tightening types and filling inference gaps. One exception: an override subtree that contains `$ref` replaces the corresponding base subtree entirely rather than merging — JSON Schema draft-07 ignores siblings of `$ref`, and merging would otherwise leave inferred constraints from the base alongside the refed schema's constraints, producing shapes no input can satisfy.

## What it looks at

For every value, `helm-schema` combines three signals, and the schema is recovered from what the chart *does* — not from what its defaults happen to be:

- **How your templates use it** — which resource field the value renders into, and the control flow around it. A value used only behind `{{ if .Values.x.enabled }}` is only constrained when that guard is on; a value with no default still appears if the chart reads it.
- **Your composed defaults** — the root `values.yaml` merged with each subchart's defaults and `global`, the same way Helm composes them.
- **What Kubernetes expects** — once a value is traced to a resource field, its type (and description) comes from the upstream Kubernetes or CRD schema for that field. A port that happens to be `80` is an integer because the Service field *is* one, not because `80` looks numeric.

When a value is genuinely ambiguous it stays a union rather than a wrong guess, and anything that can't be resolved is reported as a diagnostic instead of being invented. See the [documentation](https://romnn.github.io/helm-schema/docs/how-it-works/) for the details.

## Status / disclaimer

This project is **useful and works for many charts**, but it is **not yet fully production-ready** or 100% reliable.

Helm templating and YAML composition have many edge cases (whitespace trimming, dynamic keys, helper indirection, runtime-only behavior, etc.).
Expect some charts to require manual overrides and expect occasional incorrect or missing inference.

