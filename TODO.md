- that weird nats template should be parseable by our parsers i think right? control directives just "disappear"
- DONE: use VFS for readonly without writing temp files
- DONE: add some of the hard nats patch applying test cases.


I’ll compare your two schemas quantitatively (what keys/paths are missing, what types differ), then cross-check against the actual SigNoz chart templates/values in `test-data/charts/signoz-signoz` to identify *why* our pipeline missed them. From that, I’ll propose a concrete “next implementation steps” plan that targets the biggest completeness gaps first (e.g. missed `.Values` usages in `tpl`/`include`, dynamic keys, `fromYaml`/`toYaml` fragments, subchart values, etc.). I’m going to start by loading both JSON files and computing a “missing JSON-pointer paths” report.




















I’ve now compared the **actual** schema generated from [testdata/charts/signoz-signoz](cci:7://file:///home/roman/dev/helm-schema/crates/helm-schema-mapper/testdata/charts/signoz-signoz:0:0-0:0) against your large reference schema. Next I’ll normalize away representational differences (your ref uses `item_0`, ours uses `items/*`) to avoid overcounting “missing” paths, and I’ll attribute the remaining gaps to fundamental pipeline limitations (subchart recursion, values.yaml baseline ingestion, dynamic value accesses). Then I’ll give you a concrete, step-by-step implementation plan to close the gap for SigNoz (and by extension many charts).









# Findings: why SigNoz is “not even close” today

## 1) You compared two different “universes” at first
Your [./debug/signoz.schema.json](cci:7://file:///home/roman/dev/helm-schema/debug/signoz.schema.json:0:0-0:0) is **2.6KB** and only has ~**28** property paths. That output corresponds to a *different chart analysis run* (it looks like the classic Bitnami-style chart where top-level keys include `mariadb`, `mongodb`, etc.).

When I re-ran our CLI against **the actual vendored SigNoz chart** at:

[crates/helm-schema-mapper/testdata/charts/signoz-signoz](cci:7://file:///home/roman/dev/helm-schema/crates/helm-schema-mapper/testdata/charts/signoz-signoz:0:0-0:0)

we produced a much larger schema (**~1,719 property paths**), with correct top-level keys like:

- `alertmanager`
- `clickhouse`
- `externalClickhouse`
- `otelCollector`
- `schemaMigrator`
- `signoz`
- etc.

So: the “2.6KB schema” is essentially not the SigNoz chart; it’s the wrong input or wrong debug artifact.

## 2) Even for the correct SigNoz chart, we’re missing a lot vs your reference
Your reference schema is ~**468KB** and has a very broad surface area. When comparing our SigNoz testdata schema to your reference (normalizing the ref’s `item_0`/`item_1` into our `*` array wildcard), we’re still missing **~1,400** normalized paths.

The missing paths cluster heavily into:

- **`signoz-otel-gateway` (~657 missing)**  
  We don’t even have `signoz-otel-gateway` at the top-level in our output.
- **`clickhouse` (~552 missing)**  
  Tons of ClickHouse subchart surface is absent.
- **`otelCollector` (~176 missing)**
- Smaller misses under:
  - `signoz` (~25)
  - `alertmanager` (~16)
  - `schemaMigrator` (~9)
  - plus `clusterName` (top-level) is missing even though it exists in `values.yaml`.

This pattern strongly suggests the biggest gap is **not “type inference”** but **coverage**: we are not collecting value paths comprehensively from all the sources your reference includes.

## 3) We also have “extra” junk paths that shouldn’t be in a values schema
In the normalized comparison we also had a lot of “extra” paths (not in the reference) like:

- `otelCollector.Values`
- `otelCollector.Release`
- `...delay`, `...endpoint`, etc. showing up as if they were values keys
- large swaths like `clickhouse.nameOverride.spec.template.spec...` (K8s manifest-ish paths) appearing under values keys

This implies **we are sometimes mis-attributing non-values identifiers / YAML keys / Helm objects** as `.Values` paths (or incorrectly binding `.` / `$` inside `with`/`range`/`include` contexts).

That’s a fundamental correctness issue for completeness too, because “noise” makes it harder to see what’s missing, and also causes wrong merges.

---

# Root causes (fundamental gaps)

## A) We are not incorporating `values.yaml` as a baseline schema source
Your reference almost certainly includes the entire `values.yaml` structure (and nested structures from dependency charts), even if some values aren’t referenced directly in templates.

Our current pipeline mostly synthesizes from **template usage** (VYT `.Values.*` discovery) + some fallbacks.

**Consequence:** anything not referenced in templates is likely missing, e.g. top-level `clusterName` (present in values.yaml, apparently not used in templates).

## B) We are not properly integrating dependency/subchart values into the parent schema
SigNoz has dependencies:

- `clickhouse` (condition `clickhouse.enabled`)
- `signoz-otel-gateway` (condition `signoz-otel-gateway.enabled`)

Your reference schema clearly contains full surfaces for both.

Even though the CLI loads subcharts, our **values schema generation does not “compose” subchart values** into the top-level schema under the dependency name in a systematic way. Right now we only see what templates happen to touch.

**Consequence:** huge missing surfaces under `clickhouse.*` and especially `signoz-otel-gateway.*`.

## C) VYT extraction currently prioritizes “where values are used”, but misses hard cases
Even within parent chart templates, we’ll miss or degrade in cases like:

- `tpl` + `fromYaml` + `toYaml` pipelines where structure is indirect
- dynamically computed value paths (e.g. `index .Values $k`, [get](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-mapper/src/vyt.rs:123:4-125:5), `pluck`)
- values passed through helper templates where binding isn’t attributed cleanly
- `with`/`range` scoping where `.` changes and we mis-bind it (which also causes the **junk** paths above)

**Consequence:** missing “deep fields” under otherwise-present roots (e.g. under `signoz.*`, `otelCollector.*`).

## D) Representation mismatch (ref’s `item_0` vs our `*`) is not the real issue
Even after normalizing, we’re still far off. So the difference is truly completeness/coverage, not formatting.

---

# Concrete follow-up implementation plan (step-by-step)

This is intentionally broken into incremental steps you can implement and validate one by one.

## Step 1: Add `values.yaml` → schema ingestion (baseline completeness)
**Goal:** At minimum, any key present in `values.yaml` should exist in the schema, with correct object/array/scalar structure.

- **Implement:** `schema_from_values_yaml(chart.values_files)`:
  - Parse YAML into `serde_yaml::Value`
  - Convert to a JSON Schema:
    - mapping -> `type: object`, `properties` for keys
    - seq -> `type: array`, `items` as merged schema of elements (or first element for strictness)
    - scalar -> `type` by YAML scalar kind (`string`/`integer`/`number`/`boolean`/`null`)
  - Strictness:
    - For objects coming from values.yaml, decide whether to set `additionalProperties: false` (strict) vs allow unknown.
    - For charts like SigNoz, values.yaml is curated, so strict is usually good, but beware of “user-extensible maps” like `podAnnotations`, `extraEnv`, `extraLabels`: we can recognize those by name and use map schemas.

- **Merge strategy:** final schema = `merge(schema_from_values_yaml, schema_from_templates)`
  - Values.yaml provides *surface* and default types
  - Templates + K8s provider refine and strengthen types (e.g. booleans/ints, or K8s shapes)

- **Validation:** re-run SigNoz and confirm:
  - `clusterName` appears (it’s missing now)
  - many missing `signoz.*` leaves appear even if unused in templates

## Step 2: Compose dependency chart values under parent keys
**Goal:** `clickhouse.*` and `signoz-otel-gateway.*` become complete like the reference.

- **Implement:** when loading a chart with subcharts:
  - Load each subchart’s `values.yaml` (and any `values.*.yaml`) and produce `schema_from_values_yaml(subchart)`.
  - Insert under top-level property `{subchart.name}` in the parent schema.
  - Respect dependency `condition` when possible:
    - e.g. if dependency has condition `clickhouse.enabled`, then:
      - ensure `clickhouse.enabled` exists and is boolean
      - (optional later) gate requiredness of clickhouse subtree based on enabled, but for strictness-first we can just include it.

- **Important:** handle names like `signoz-otel-gateway` (contains `-`):
  - Our value-path splitting uses `.`; hyphens should be treated as normal key characters. No problem for insertion, but ensure no normalization accidentally breaks it.

- **Validation:** re-run SigNoz and confirm:
  - top-level `signoz-otel-gateway` exists
  - missing roots shrink massively

## Step 3: Fix VYT false positives (“Release”, “Values”, manifest paths under values keys)
**Goal:** reduce junk paths so merges don’t get polluted and strictness remains meaningful.

- **Investigate/adjust in VYT:**
  - Ensure we only record value paths that actually originate from:
    - `.Values.<ident...>`
    - `$.<something>.Values.<ident...>`
    - `Values.<ident...>` if you intentionally support that
  - Ensure dot-binding in `with`/`range` only extends **values-derived** bases.
  - Ensure `.Release`, `.Chart`, `.Capabilities` etc. do not become “bases”.
  - In other words: tighten [first_values_base](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-mapper/src/vyt.rs:466:0-481:1) / [collect_values_anywhere](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-mapper/src/vyt.rs:1357:4-1415:5) so it won’t treat arbitrary identifiers as values.

- **Validation:** re-run SigNoz and ensure those `otelCollector.Release` / `otelCollector.Values` etc. disappear.

## Step 4: Add support for dynamic access patterns (index/get) with conservative schemas
**Goal:** capture more values that are referenced dynamically (common in complex charts).

- **Implement conservative extraction:** detect patterns like:
  - `index .Values "foo" "bar"`
  - `get .Values "foo"`
  - `pluck "foo" .Values.something`
- Because keys might be dynamic, emit:
  - either a concrete path if string literals are used (`index .Values "foo" "bar"` -> `foo.bar`)
  - or a map-like schema if key is unknown (`index .Values $k` -> treat parent as `additionalProperties: {}` or `additionalProperties: <scalar schema>` depending on context)

- **Validation:** compare again; missing “map style” config areas should shrink.

## Step 5: Improve `tpl` / `include` / fragment structural inference
**Goal:** for things like `fromYaml (tpl ...)` and `toYaml .Values.someConfig`, infer object structure better.

- **Approach:** treat these patterns as “structure-carrying”:
  - if a fragment is used under a known YAML key path, and that key path is known from K8s schema provider, attempt to “project” structure back onto the value path.
  - This is more advanced, but it’s how you get deep schema fields for config blobs.

- **Validation:** this is how you chase the last ~10–20% of completeness in deeply dynamic charts.

## Step 6: Add a meaningful SigNoz regression test suite
Your criticism is correct: the current SigNoz test asserts too little.

Once Steps 1–3 land, add assertions like:
- **Presence checks**:
  - `clusterName`, `signoz`, `clickhouse`, `signoz-otel-gateway`
- **Depth checks**:
  - existence of representative deep fields from the reference, e.g.
    - `alertmanager.ingress.hosts.*.paths.*.pathType`
    - `clickhouse.clickhouseOperator.image.repository`
    - `signoz.smtpVars.existingSecret.usernameKey`
- **Type checks**:
  - `enabled` booleans
  - `replicaCount` integers
  - string-map keys like annotations

---

# Summary (what’s fundamentally needed for “reference-level completeness”)
To match the completeness of your SigNoz reference schema, **template-only extraction is insufficient**. We need:

1. **Values-file-driven baseline schema** (for breadth)
2. **Subchart/dependency composition** (for breadth across dependencies)
3. **Stricter/more accurate VYT binding** (reduce noise + avoid wrong merges)
4. **Optional advanced inference** for dynamic patterns and YAML fragments (for depth)

If you confirm you want to start with Step 1 (values.yaml ingestion) and Step 2 (dependency composition), I can propose the exact APIs/files to touch and we can implement them incrementally with SigNoz as the main acceptance test.
