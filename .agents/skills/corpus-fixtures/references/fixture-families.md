# Fixture families

Source of truth: `ArtifactId::spec` and `FIXTURE_COUNTS` in
`crates/helm-schema-test-support/src/registry.rs`. The producer writes each file into its output
directory under the name given below. The manifest entry's `file` and `fixture` fields give the
same mapping for any single artifact.

## Produced families

| Family | Key (`--only`) | Count | Produced file | Committed fixture | Checked by | Reads producer artifacts | Trailing newline |
|---|---|---|---|---|---|---|---|
| Chart corpus (full profile) | `chart/<dir>` | 156 | `helm-schema.cli.chart-corpus.<dir>.schema.json` | `testdata/chart-corpus-schemas/<dir>.schema.json` | `crates/helm-schema-cli/tests/chart_corpus.rs` (one test per chart, named by the `corpus_charts!` snake name) plus the `chart_*.rs` semantic tests | yes, via `consume` | yes |
| Internal nested chart | `chart/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql` | 1 | `internal/helm-schema.cli.chart-internal.signoz-postgresql.schema.json` | none | `crates/helm-schema-cli/tests/chart_signoz_postgresql.rs` | yes | yes |
| Template-level generator schemas | `template/<dump_stem>` | 20 | `helm-schema.<dump_stem>.schema.json` | `crates/helm-schema-gen/tests/fixtures/<fixture>` (`TemplateId::case`) | `crates/helm-schema-gen/tests/corpus.rs` `schema_fixtures_match` | no, generates locally | no |
| Lean profile | `lean/<chart>` | 4 | `helm-schema.emission-profile.lean.<chart>.schema.json` | `testdata/emission-profile-schemas/lean/<chart>.schema.json` | `crates/helm-schema/tests/schema_emission_profiles.rs` `lean_profile_schemas_match_their_separate_fixture_lane` | no | yes |
| Final-output policy | `final-policy/<name>` (`full`, `lean`, `caller-overwrite`, `boolean-false`) | 4 | `helm-schema.final-output.<name>.schema.json` | `testdata/final-output-schemas/<name>.schema.json` | `crates/helm-schema/tests/final_output_policy.rs` | no | yes |
| Contract IR | `ir/<template path>` | 18 | `helm-schema-ir.<template path, every non-alphanumeric byte replaced by '-'>.ir.json` | `crates/helm-schema-ir/tests/fixtures/<fixture>` (`IrId::case`) | `crates/helm-schema-ir/tests/corpus.rs` `ir_corpus_fixtures_match` | no | no |

Recipe details:

- Chart and lean recipes use `ChartRecipe::corpus`: tests excluded, subchart values included, no
  extra values files, `infer_required` off, Kubernetes `v1.29.0-standalone-strict` from the
  vendored bundle, downloads off, `$defs` minimized with source-path names.
- Final-output recipes emit `schema-emission-controls` through the output pipeline, some with a
  caller override file.
- Template recipes pin a Kubernetes version per case (`v1.35.0` from the bundle), and some use
  the CRD catalog chain. `SurveyorConfigmap` uses inline values instead of the chart's
  `values.yaml`. The template artifact is always generated over the registered values document,
  never over a test's ad-hoc document.
- The lean and final-output lanes use the non-corpus charts `schema-emission-controls`,
  `schema-emission-local-kind` and `schema-emission-temporal-wrapper`.

`crates/helm-schema-test-support/tests/registry.rs`
(`fixture_directories_hold_exactly_the_registered_fixtures`) requires each of the five fixture
directories to hold exactly the registered fixtures. A stray or missing file fails it.

## Static gates over committed fixtures

These gates read the committed files, not the producer's output. After adoption they check the
fixtures you copied in.

| Test | Scope | Checks |
|---|---|---|
| `crates/helm-schema-cli/tests/schema_dialect_hygiene.rs` | `*.schema.json` in `testdata/chart-corpus-schemas`, `crates/helm-schema-gen/tests/fixtures`, `crates/helm-schema-cli/tests/fixtures` | valid against its metaschema; every `pattern` and `patternProperties` key compiles under ECMA-262 (`regress`) |
| `crates/helm-schema-cli/tests/defs_names.rs` | `testdata/chart-corpus-schemas`, `final-output-schemas`, `emission-profile-schemas/lean` | readable `$defs` names; short keys only through explicit shortening |

## Hand-maintained fixtures (not produced)

Update these from the test's own diff output, and adjudicate them like any other fixture change.

| Fixture | Test | Notes |
|---|---|---|
| `crates/helm-schema-cli/tests/fixtures/full_fixture.disable_k8s.schema.json` | `cli.rs` `generates_schema_for_fixture_chart_without_k8s_provider` (integration) | chart `testdata/fixture-charts/full-fixture`, cold provider caches; `SCHEMA_DUMP=1` writes `<target>/schema-dump/helm-schema.cli.full-fixture.disable-k8s.schema.json` (no trailing newline, same as the fixture) |
| `crates/helm-schema-gen/src/tests/fixtures/root_values_merge_source_presence.schema.json` | `crates/helm-schema-gen/src/tests/fail_validators.rs` (unit, default profile) | `include_str!` |
| `crates/helm-schema-syntax/tests/fixtures/*.cst.txt` | `crates/helm-schema-syntax/tests/corpus.rs` | `TemplatedDocument::dump()` of corpus templates, `include_str!` |
| `crates/helm-schema-k8s/tests/fixtures/networkpolicy_v1_35_*.json` | `crates/helm-schema-k8s/tests/kubernetes_json_schema_networkpolicy.rs` | provider materialization |

## Test data that is not a fixture

- `testdata/charts/` — vendored corpus charts, plus test-only charts without corpus fixtures
  (`schema-emission-*` apart from `schema-emission-unconditional-fail`,
  `grouped-argument-evaluation`, `round-58-review`, `structural-helper-widening`).
- `testdata/fixture-charts/` — small synthetic charts for CLI tests.
- `testdata/helm-values/` — values-composition cases (aliases, globals, nulls, …).
- `testdata/provider-bundle/` — the only provider source generation reads:
  `kubernetes-json-schema-cache/` and `crds-catalog-cache/`, each with `CACHE_LAYOUT_VERSION` and a
  `default/` tree. `*.not-found` files are recorded upstream 404s. It is a recipe input of every
  chart and template artifact.
