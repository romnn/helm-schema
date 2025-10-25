Short answer: this is **feasible** (with caveats). There isn’t, as of today, a tool that *fully* derives a chart’s **complete** `values.schema.json` by analyzing *templates themselves* across all control-flow paths. Existing generators either rely on comments/annotations and the `values.yaml` (e.g. `helm-schema`) or work from rendered YAML (single instances), which is exactly the limitation you called out. ([GitHub][1])

Below is a concrete, implementable plan for a Rust CLI that does what you want: parse Helm (Go) templates, statically analyze control flow (`if`, `range`, `with`, `include`, `tpl`), map `.Values.*` paths to *output YAML* paths, then **pull types/enums** from the right Kubernetes/CRD JSON Schemas to synthesize a precise `values.schema.json`. I’ve included references to reusable grammars/libs and where we’ll need new work.

---

# Feasibility & prior art (what exists today)

* **Annotation/values-driven**: `dadav/helm-schema` generates schemas from `values.yaml` (+ inline `@schema` comments). It explicitly reads values files, can even “uncomment” commented keys, but does **not** analyze templates to discover additional value paths or conditionals. Good to learn from, not what we need. ([GitHub][1])
* **Editor tooling**: `helm-ls` parses templates using Tree-sitter, and (beta) can *generate* schemas for `values*.yaml` to drive editor completion — again values-file centric, not exhaustive template analysis. Useful for parser choices. ([GitHub][2])
* **Template semantics**: official Helm docs describe functions (`include`, `tpl`, `default`, `required`, `lookup`, Sprig set, etc.). We’ll encode a subset semantically for static analysis. ([Helm][3])
* **Kubernetes & CRD Schemas**: mature registries for native objects and popular CRDs exist; we can target yannh’s fork (kept current) and Datree’s CRD catalog, plus conversion guidance for OpenAPI→JSON Schema. ([GitHub][4])
* **Bitnami “common” helpers**: a large share of charts use these helpers (`common.tplvalues.render`, `common.capabilities.ingress.apiVersion`, etc.). We can special-case those to raise precision. ([GitHub][5])

**Conclusion**: There’s no “ultimate” template-to-schema generator publicly available; building one is hard but doable with a static analysis + schema back-propagation approach.

---

# High-level architecture

1. **Chart loader & dependency resolver**

   * Discover `Chart.yaml`, `templates/**`, `crds/**`, `_helpers.tpl`, subcharts in `charts/`.
   * Optionally untar chart dependencies (`helm dep build`) to analyze library templates (esp. Bitnami common). (We can shell out or document as a prerequisite.) ([GitHub][2])

2. **Dual parser (YAML ⨂ Go-template)**

   * Parse YAML structure and embedded Go-template *together* so we can map template nodes to YAML keys/values.
   * **Reuse**: Tree-sitter grammars

     * YAML: `tree-sitter-yaml` (Rust crate exists). ([Docs.rs][6])
     * Go-template w/ Helm dialect: `tree-sitter-go-template` (use as git submodule and bind via build.rs). ([GitHub][7])
   * Strategy: create an interleaved CST that tracks (a) YAML keys/anchors and (b) template nodes (`{{ ... }}`, `{{- ... -}}`, `{{ define }}`, `{{ include }}`, pipelines). This mirrors how `helm-ls` does injections, so their repo is a good reference for node types. ([GitHub][2])

3. **Template IR (intermediate representation)**
   Build an IR that captures:

   * **Emission points**: “this YAML field (path) is produced here”
   * **Value origins**: expressions that read `.Values.*` (including `index .Values "foo" "bar"`)
   * **Guards**: boolean conditions over values/built-ins (from `if/with`, `required`, `default`, etc.)
   * **Transforms**: `toYaml`, `quote`, `tpl`, `include`, `nindent`, `merge`, etc. (as abstract ops with partial semantics)
   * **Context**: dot (`.`) rebinding via `with`/`range`, captured variables (`$`, loop vars).

4. **K8s/CRD schema provider**

   * Local cache of **kubernetes-json-schema** (yannh fork), version selected via a flag (`--k8s 1.30.0`) or inferred from `.Capabilities.KubeVersion` if specified. ([GitHub][4])
   * **CRDs**: load from chart’s `crds/**`; convert embedded OpenAPI v3 validation to JSON Schema (follow **kubeconform** conversion notes). Fallback: Datree CRD catalog for popular CRDs. ([kubeconform.mandragor.org][8])

5. **Resource/type detector**

   * For each templated document (split on `---`), try to determine `(apiVersion, kind)`.

     * If `apiVersion` is templated via helpers (Bitnami capabilities), resolve using the selected K8s version (e.g., `Ingress` → `networking.k8s.io/v1`). Provide pluggable “capability resolvers” for known helpers. ([GitHub][9])
     * If entire doc is behind conditionals, treat as *conditionally present* emission.

6. **Static data-flow & path typing (the heart of it)**

   * Walk the IR to build a **Value Access Graph** mapping **`.Values` paths ⇒ YAML paths**, carrying **guards** and **transforms**.
   * For each YAML path, look up the **target JSON Schema** node from the resource schema, then **propagate types/enums/patterns** *back* to the `.Values` node(s).
   * For arrays (`range .Values.ingress.extraHosts`), derive **item schemas**; for `default`/`coalesce`, compute unions; for maps (annotations/labels), use `patternProperties` from K8s schema (string→string).
   * For `include`/`toYaml`/`common.tplvalues.render`, detect arguments named `value`/`values` and treat them as **YAML-injection** of the provided value at the current path. (We will ship first with Bitnami helper semantics modeled.) ([GitHub][5])
   * For `tpl`, if the target is expected to be YAML (by schema or context), treat as “evaluate string then parse YAML” ⇒ **type equals expected schema**; otherwise as plain string. ([Austin Dewey][10])

7. **Schema synthesis (for `values.schema.json`)**

   * Merge all inferred constraints on each `.Values` path.
   * Where guards differ (e.g., `if .Values.ingress.enabled`), emit **conditional JSON Schema** using `if/then/else` or group with `anyOf` to represent structural toggles.
   * Preserve **descriptions** from comments (optionally), but **do not rely** on them.
   * Emit references for subcharts under their namespace in `.Values` (e.g., `postgresql.*`) by analyzing subchart templates too.

8. **Soundness vs. completeness**

   * **Sound** for common patterns; **not complete** where keys are built dynamically from other values (`printf`, `tpl` that produces keys) or driven by external files (`.Files.Get`) or cluster `lookup`. For these, degrade to **looser schemas** (e.g., `additionalProperties: true` with best-effort constraints) and record a warning. ([Helm][11])

---

# Detailed implementation plan (step-by-step)

## Phase 0 — Repo scaffold & crates

* Workspace layout: `cli/`, `core/`, `parser/`, `analyzer/`, `schemas/`, `emit/`, `tests/`.
* **Crates to use**

  * Parsing: `tree-sitter`, `tree-sitter-yaml`; bring `tree-sitter-go-template` via git submodule + `build.rs`. ([Docs.rs][6])
  * YAML value handling: `serde_yaml` or `serde_yml` + `serde_json` for schema building. ([Serde YML][12])
  * JSON Schema utils: we’ll *construct* schemas as serde JSON; use `jsonschema` crate for validating *our* output in tests. ([Docs.rs][13])

## Phase 1 — Chart discovery

* Walk dirs, find `Chart.yaml`. Load `values.yaml` **only** for doc strings (optional) and default shapes (not required for inference).
* Untar `charts/*.tgz` if present (mirror `helm-ls` suggestion). ([GitHub][2])

## Phase 2 — Parser: YAML ⨂ gotmpl

* Build a tokenizer that splits files into YAML segments and `{{ ... }}` template nodes (Tree-sitter can parse both; follow the “injection” idea from `helm-ls`).
* Produce a CST where each YAML scalar/indent node can contain an ordered list of template nodes interleaved with text.

## Phase 3 — Expression & control-flow AST

* For Go-template fragments, build a typed AST for:

  * **Selectors**: `.Values.foo.bar`, `index .Values "foo" "bar"`.
  * **Pipelines**: `expr | func1 | func2 ...` (preserve func names & args).
  * **Blocks**: `if/else`, `with`, `range`, `define`/`include`, variable assignments (`$x := ...`).
* Maintain **scope** (dot rebinding) to resolve what `.`, `$`, and loop vars refer to.

## Phase 4 — IR construction

* Visit the combined CST and emit IR tuples like:

  ```
  Emit {
    yaml_path: /spec/rules/0/http/paths/0/pathType,
    source: Selector(.Values.ingress.pathType) | default("ImplementationSpecific"),
    guards: [Selector(.Values.ingress.enabled) == true, Has(.Values.ingress.hostname) OR Range(.Values.ingress.extraHosts)]
  }
  ```
* Special-case “YAML-injection” calls:

  * `toYaml X | nindent N` ⇒ injection of the **value** `X` at current YAML path.
  * `include "common.tplvalues.render" (dict "value" V "context" .)` ⇒ injection of **V**. ([GitHub][5])
* Recognize Bitnami helpers for capability lookups, ingress backends, etc., but treat outputs as *opaque strings* unless they determine `apiVersion/kind`. ([GitHub][9])

## Phase 5 — Resource detection

* For each document:

  * Try to read literal `kind:` and `apiVersion:`; if templated, resolve via helper tables keyed by the selected K8s version (e.g., Ingress uses `networking.k8s.io/v1` on ≥1.19). Provide a user override flag. ([GitHub][9])
* Load the right **JSON Schema** (native or CRD). ([GitHub][4])

## Phase 6 — Back-propagation typing

* For each **Emit**, find the **target** schema node (via JSON Pointer built from `yaml_path`; careful with arrays `[*]` and maps).
* Derive the **source** schema for the `.Values` path:

  * If the YAML node is a scalar with `enum`/`pattern`, propagate that to the values path.
  * If it’s an object (e.g., annotations), use schema’s `patternProperties`.
  * If it’s an array of structured items (e.g., `rules`), and we inject `toYaml .Values.ingress.extraRules` under it, set `.Values.ingress.extraRules`’s **items** to the resource’s **item schema**.
  * Track **guards**: build `if/then/else` rules where appropriate (e.g., when enabling `.Values.ingress.enabled` adds a subtree of required fields).
* **Merges**: `merge`/`list`/`dict` produce unions; compute `anyOf` of candidate schemas or `allOf` when all must hold.

## Phase 7 — Synthesis & emission

* Compose a single **Draft-07** `values.schema.json` (Helm validates against draft-7). ([GitHub][1])
* Include per-path descriptions (optional), defaults only if we can reliably read them from `default` pipes.
* Generate **warnings** for dynamic keys/opaque includes (`tpl` that emits keys), `.Files.*`, or `lookup`.

## Phase 8 — CLI, performance & DX

* `helm-valueschema generate ./charts/mychart --k8s 1.30.0 --bitnami-helpers on --crd-catalog on`
* Output to `values.schema.json` by default, `--stdout` supported.
* Cache schemas by version; parallelize per file; deterministic output (stable sorting).

## Phase 9 — Validation suite

* Golden tests: run on ~20 popular charts (Bitnami, ingress-nginx, cert-manager, Argo CD).
* **Property-based** check: sample values from the **generated** schema, render `helm template` with those values, then validate the rendered YAML with **kubeconform**. This gives a precision/recall signal. ([GitHub][14])

---

# Handling your ingress example (key points we’ll cover)

* `if .Values.ingress.enabled`: schema wraps **conditional presence** of the entire Ingress document under an `if/then` block that gates other fields.
* `host: {{ tpl .Values.ingress.hostname . }}`: if hostname present ⇒ `string` w/ Kubernetes DNS rules (we can’t easily infer regex; we’ll keep `type: string` unless we choose to import common patterns). ([Austin Dewey][10])
* `extraHosts` `range`: infer `.Values.ingress.extraHosts` is an **array of objects** with `name`, `path`, `pathType`, mapped to the `IngressRule/HTTPIngressPath` schemas (so we inherit enum for `pathType`).
* `annotations` via `common.tplvalues.render` merge: infer `.Values.ingress.annotations`/`.Values.commonAnnotations` are **string→string** maps (from metadata schema). ([GitHub][5])
* `tls` gated by complex condition: produce **anyOf** branches where TLS entries exist or not, and type `.Values.ingress.tls` and `.Values.ingress.selfSigned` accordingly.

---

# Tricky cases & mitigations

* **Dynamic keys** (e.g., `{{ printf "annotations.%s" .Values.suffix }}`) → fall back to `additionalProperties: true` with documented warning.
* **`tpl` that emits keys** → treat as opaque YAML injection; if parent schema is `object` with `additionalProperties: {…}`, adopt that; otherwise relax but warn. ([Austin Dewey][10])
* **External includes** (library charts): ship adapters for Bitnami Common first (`tplvalues.render`, capabilities, ingress helpers). Pluggable adapters let users add semantics for their org’s libraries. ([GitHub][5])
* **`lookup`** (cluster-dependent) → ignore for typing (document as non-deterministic). ([Helm][11])
* **CRDs inside chart** → convert OpenAPI v3 to JSON Schema (kubeconform method), then type against that. ([kubeconform.mandragor.org][8])

---

# Reusable building blocks (Rust & refs)

* **Tree-sitter** core & YAML grammar: crates available. ([Docs.rs][6])
* **Go-template grammar**: `ngalaiko/tree-sitter-go-template` (includes Helm dialect) — pull in via submodule and bind from Rust. ([GitHub][7])
* **Helm template semantics**: official docs for functions and control flow (use as ground truth). ([Helm][3])
* **K8s/CRD schemas**: yannh’s kubernetes-json-schema, Datree CRDs catalog; kubeconform docs for conversions & strict/standalone flavors. ([GitHub][4])

---

# Milestones (you can implement in this order)

1. **M1 (2–3 weeks):** Parse templates & list all `.Values.*` paths (with guards); print a “values map” report.

   * Stretch: resolve `index` and `with`/`range` scopes.
2. **M2:** Resource detection + schema loading; map a subset of fields (scalars under direct selectors) and generate a tiny `values.schema.json`.
3. **M3:** Add YAML-injection semantics for `toYaml` and Bitnami `tplvalues.render`; support arrays/maps; generate enums like `Ingress.pathType`. ([GitHub][5])
4. **M4:** Conditional schema (`if/then/else`, `anyOf`) and merging across branches; subchart namespaces.
5. **M5:** CRD support (OpenAPI→JSON Schema); adapters for Bitnami capabilities (Ingress apiVersion, etc.). ([GitHub][9])
6. **M6:** Robustness: warnings, coverage metrics, property-based tests using `kubeconform` as an oracle on rendered samples. ([GitHub][14])

---

# Why this should work in practice

* Helm templates are **Go templates**; we can parse them accurately with **Tree-sitter** (already used by `helm-ls`). ([GitHub][2])
* Kubernetes & many CRDs have **maintained JSON Schemas** we can type against. ([GitHub][4])
* Common helper libraries (Bitnami) are **documented code** we can model to resolve frequent patterns (`tplvalues.render`, capabilities). ([GitHub][5])

**Limitations** will remain around truly dynamic constructs (`tpl`/external files/cluster `lookup`), but the plan produces *precise* schemas for the vast majority of real-world charts, and degrades gracefully (with explicit warnings) where complete static inference isn’t possible.

If you’d like, I can turn this into an issue-sized checklist with module skeletons (`mod parser`, `mod analyzer`, etc.) and a first pass at the IR data types.

[1]: https://github.com/dadav/helm-schema "GitHub - dadav/helm-schema: Generate jsonschemas from helm charts."
[2]: https://github.com/mrjosh/helm-ls "GitHub - mrjosh/helm-ls: ⎈ Language server for Helm ⎈"
[3]: https://helm.sh/docs/chart_template_guide/function_list/?utm_source=chatgpt.com "Helm | Template Function List"
[4]: https://github.com/yannh/kubernetes-json-schema?utm_source=chatgpt.com "GitHub - yannh/kubernetes-json-schema: JSON Schemas for every version ..."
[5]: https://github.com/bitnami/charts/blob/main/bitnami/common/templates/_tplvalues.tpl?utm_source=chatgpt.com "charts/bitnami/common/templates/_tplvalues.tpl at main - GitHub"
[6]: https://docs.rs/tree-sitter-yaml?utm_source=chatgpt.com "tree_sitter_yaml - Rust - Docs.rs"
[7]: https://github.com/ngalaiko/tree-sitter-go-template?utm_source=chatgpt.com "Golang template grammar for tree-sitter - GitHub"
[8]: https://kubeconform.mandragor.org/docs/json-schema-conversion/?utm_source=chatgpt.com "Kubeconform - Fast Kubernetes manifests validation! | OpenAPI to JSON ..."
[9]: https://github.com/bitnami/charts/blob/main/bitnami/common/templates/_capabilities.tpl?utm_source=chatgpt.com "charts/bitnami/common/templates/_capabilities.tpl at main · bitnami ..."
[10]: https://austindewey.com/2021/02/22/using-the-helm-tpl-function-to-refer-values-in-values-files/?utm_source=chatgpt.com "Using the Helm Tpl Function to Refer Values in Values Files"
[11]: https://helm.sh/docs/chart_template_guide/functions_and_pipelines/?utm_source=chatgpt.com "Template Functions and Pipelines - Helm"
[12]: https://doc.serdeyml.com/serde_yml/?utm_source=chatgpt.com "Crate - serde_yml - Rust"
[13]: https://docs.rs/jsonschema/latest/jsonschema/?utm_source=chatgpt.com "jsonschema - Rust - Docs.rs"
[14]: https://github.com/yannh/kubeconform?utm_source=chatgpt.com "GitHub - yannh/kubeconform: A FAST Kubernetes manifests validator, with ..."

