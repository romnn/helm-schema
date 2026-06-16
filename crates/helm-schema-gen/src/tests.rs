use std::collections::BTreeMap;

use indoc::indoc;
use serde_json::Value;

use crate::{
    ValuesSchemaInput, generate_values_schema,
    resolve_policy::{ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs},
};
use helm_schema_ast::{DefineIndex, TreeSitterParser};
use helm_schema_ir::{
    ContractIr, ContractSchemaSignals, ContractUse, Guard, ProviderSchemaUse, ResourceRef,
    SymbolicIrContext, ValueKind, YamlPath, extract_default_type_hints,
};
use helm_schema_k8s::{
    Chain, K8sSchemaProvider, KubernetesJsonSchemaProvider, ProviderOrigin, ProviderSchemaFragment,
};

fn provider() -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true)
}

fn production_chain_provider() -> Chain {
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
        .with_api_version_guess(true);
    Chain::new(vec![Box::new(k8s_provider)]).with_inference_enabled(true)
}

fn parse_ir(src: &str) -> Vec<ContractUse> {
    let idx = DefineIndex::new();
    SymbolicIrContext::new(&idx)
        .generate_contract_ir(src, &idx)
        .project()
        .uses()
        .to_vec()
}

fn parse_ir_with_helpers(src: &str, helpers: &str) -> Vec<ContractUse> {
    let mut idx = DefineIndex::new();
    if !helpers.trim().is_empty() {
        idx.add_file_source("helpers.tpl", helpers);
        idx.add_source(&TreeSitterParser, helpers)
            .expect("helpers parse");
    }
    SymbolicIrContext::new(&idx)
        .generate_contract_ir(src, &idx)
        .project()
        .uses()
        .to_vec()
}

fn collect_hints(src: &str) -> BTreeMap<String, Vec<Value>> {
    let mut hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (path, schema) in extract_default_type_hints(src) {
        hints.entry(path).or_default().push(schema);
    }
    hints
}

fn schema_signals_for(uses: Vec<ContractUse>) -> ContractSchemaSignals {
    ContractIr::from_contract_uses(uses).into_schema_signals()
}

fn schema_for(uses: &[ContractUse]) -> Value {
    let schema_signals = schema_signals_for(uses.to_vec());
    generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()))
}

fn schema_for_values_yaml(uses: &[ContractUse], values_yaml: Option<&str>) -> Value {
    let schema_signals = schema_signals_for(uses.to_vec());
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(values_yaml),
    )
}

fn schema_for_values_yaml_and_hints(
    uses: &[ContractUse],
    values_yaml: Option<&str>,
    type_hints: &BTreeMap<String, Vec<Value>>,
) -> Value {
    let schema_signals = schema_signals_for(uses.to_vec());
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider())
            .with_values_yaml(values_yaml)
            .with_type_hints(type_hints),
    )
}

#[test]
fn type_hint_only_descendant_preserves_object_input_branch() {
    let uses = vec![ContractUse {
        source_expr: "image".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        guards: Vec::new(),
        resource: Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
    }];
    let mut type_hints = BTreeMap::new();
    type_hints.insert(
        "image.tag".to_string(),
        vec![serde_json::json!({ "type": "string" })],
    );

    let schema = schema_for_values_yaml_and_hints(&uses, Some("image: {}\n"), &type_hints);
    let variants = schema
        .pointer("/properties/image/anyOf")
        .and_then(Value::as_array)
        .expect("image schema should preserve object and scalar branches");

    assert!(
        variants.iter().any(|variant| {
            variant
                .pointer("/properties/tag/type")
                .and_then(Value::as_str)
                == Some("string")
        }),
        "type-hint descendant should preserve an object input branch with the hinted leaf: {schema:#}",
    );
    assert!(
        variants
            .iter()
            .any(|variant| variant.get("type").and_then(Value::as_str) == Some("string")),
        "rendered scalar sink should still preserve the scalar branch: {schema:#}",
    );
}

fn schema_contains_open_string_map(schema: &Value) -> bool {
    if schema
        .pointer("/additionalProperties/type")
        .and_then(Value::as_str)
        == Some("string")
    {
        return true;
    }

    ["anyOf", "oneOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(schema_contains_open_string_map)
}

fn schema_contains_type(schema: &Value, schema_type: &str) -> bool {
    if schema.get("type").and_then(Value::as_str) == Some(schema_type) {
        return true;
    }
    if schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| {
            types
                .iter()
                .any(|value| value.as_str() == Some(schema_type))
        })
    {
        return true;
    }

    ["anyOf", "oneOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(|variant| schema_contains_type(variant, schema_type))
}

fn schema_property_contains_type(schema: &Value, property: &str, schema_type: &str) -> bool {
    if let Some(property_schema) = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(property))
        && schema_contains_type(property_schema, schema_type)
    {
        return true;
    }

    ["anyOf", "oneOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(|variant| schema_property_contains_type(variant, property, schema_type))
}

fn assert_open_string_map_or_templated_string(schema: &Value, label: &str) {
    assert!(
        schema_contains_open_string_map(schema),
        "{label} should include an open string-map branch, got {schema}"
    );
    assert!(
        schema_contains_type(schema, "string"),
        "{label} should include a templated string branch, got {schema}"
    );
}

#[derive(Debug)]
struct DescriptionProvider;

impl K8sSchemaProvider for DescriptionProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "description": "provider description",
            "type": "string",
        })))
    }

    fn schema_fragment_for_resource_path(
        &self,
        _resource: &ResourceRef,
        _path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        None
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    fn has_resource(&self, _resource: &ResourceRef) -> bool {
        true
    }
}

#[derive(Debug)]
struct SharedObjectProvider;

impl K8sSchemaProvider for SharedObjectProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        })))
    }

    fn schema_fragment_for_resource_path(
        &self,
        _resource: &ResourceRef,
        _path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        None
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    fn has_resource(&self, _resource: &ResourceRef) -> bool {
        true
    }
}

#[test]
fn repeated_exact_provider_subtrees_emit_shared_definitions() {
    let resource = ResourceRef {
        api_version: "example.io/v1".to_string(),
        kind: "Example".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let uses = vec![
        ContractUse {
            source_expr: "first".to_string(),
            path: YamlPath(vec!["spec".to_string(), "first".to_string()]),
            kind: ValueKind::Fragment,
            guards: Vec::new(),
            resource: Some(resource.clone()),
        },
        ContractUse {
            source_expr: "second".to_string(),
            path: YamlPath(vec!["spec".to_string(), "second".to_string()]),
            kind: ValueKind::Fragment,
            guards: Vec::new(),
            resource: Some(resource),
        },
    ];
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(ValuesSchemaInput::new(
        &schema_signals,
        &SharedObjectProvider,
    ));

    let expected_definition = serde_json::json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    assert_eq!(
        schema.pointer("/properties/first"),
        Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    assert_eq!(
        schema.pointer("/properties/second"),
        Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    assert_eq!(
        schema.pointer("/$defs/providerSchema1"),
        Some(&expected_definition)
    );
}

#[test]
fn values_yaml_comments_override_provider_descriptions() {
    let uses = vec![ContractUse {
        source_expr: "name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        guards: Vec::new(),
        resource: Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
    }];
    let descriptions = BTreeMap::from([("name".to_string(), "chart description".to_string())]);
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &DescriptionProvider)
            .with_values_yaml(Some("name: example\n"))
            .with_values_descriptions(&descriptions),
    );

    assert_eq!(
        schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        Some("chart description")
    );
}

#[test]
fn values_yaml_comments_do_not_create_schema_paths() {
    let uses = parse_ir(
        r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Values.name }}
        "#,
    );
    let descriptions = BTreeMap::from([
        ("name".to_string(), "name description".to_string()),
        (
            "commentedOut.enabled".to_string(),
            "comment-only path".to_string(),
        ),
    ]);
    let provider = Chain::new(Vec::new());
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider)
            .with_values_yaml(Some("name: example\n"))
            .with_values_descriptions(&descriptions),
    );

    assert_eq!(
        schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        Some("name description")
    );
    assert!(
        schema.pointer("/properties/commentedOut").is_none(),
        "description metadata must not create schema paths: {schema}"
    );
}

fn bitnami_tplvalues_helpers() -> &'static str {
    indoc! {r#"
        {{- define "common.tplvalues.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
        {{- if contains "{{" (toJson .value) }}
          {{- if .scope }}
              {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
          {{- else }}
            {{- tpl $value .context }}
          {{- end }}
        {{- else }}
            {{- $value }}
        {{- end }}
        {{- end -}}

        {{- define "common.tplvalues.merge" -}}
        {{- $dst := dict -}}
        {{- range .values -}}
        {{- $dst = include "common.tplvalues.render" (dict "value" . "context" $.context "scope" $.scope) | fromYaml | merge $dst -}}
        {{- end -}}
        {{ $dst | toYaml }}
        {{- end -}}
    "#}
}

fn bitnami_labels_helpers() -> String {
    format!(
        "{}\n{}",
        bitnami_tplvalues_helpers(),
        indoc! {r#"
            {{- define "common.names.name" -}}minio{{- end -}}
            {{- define "common.names.chart" -}}minio{{- end -}}

            {{- define "common.labels.standard" -}}
            {{- if and (hasKey . "customLabels") (hasKey . "context") -}}
            {{- $default := dict "app.kubernetes.io/name" (include "common.names.name" .context) "helm.sh/chart" (include "common.names.chart" .context) "app.kubernetes.io/instance" .context.Release.Name "app.kubernetes.io/managed-by" .context.Release.Service -}}
            {{- with .context.Chart.AppVersion -}}
            {{- $_ := set $default "app.kubernetes.io/version" . -}}
            {{- end -}}
            {{ template "common.tplvalues.merge" (dict "values" (list .customLabels $default) "context" .context) }}
            {{- else -}}
            app.kubernetes.io/name: {{ include "common.names.name" . }}
            helm.sh/chart: {{ include "common.names.chart" . }}
            app.kubernetes.io/instance: {{ .Release.Name }}
            app.kubernetes.io/managed-by: {{ .Release.Service }}
            {{- with .Chart.AppVersion }}
            app.kubernetes.io/version: {{ . | quote }}
            {{- end -}}
            {{- end -}}
            {{- end -}}
        "#}
    )
}

/// True if the schema permits a `null` value — either directly via
/// `{"type": "null"}` or as one branch of an `anyOf` union.
fn permits_null(schema: &Value) -> bool {
    if schema.get("type").and_then(Value::as_str) == Some("null") {
        return true;
    }
    if schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|v| v.as_str() == Some("null")))
    {
        return true;
    }
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|variants| variants.iter().any(permits_null))
}

fn any_of_variant_matching<'a, F: Fn(&'a Value) -> bool>(
    schema: &'a Value,
    predicate: F,
) -> Option<&'a Value> {
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .and_then(|variants| variants.iter().find(|variant| predicate(variant)))
}

fn object_variant_with_property<'a>(schema: &'a Value, property: &str) -> Option<&'a Value> {
    if schema.pointer(&format!("/properties/{property}")).is_some() {
        return Some(schema);
    }
    any_of_variant_matching(schema, |variant| {
        variant
            .pointer(&format!("/properties/{property}"))
            .is_some()
    })
}

fn permits_type(schema: &Value, ty: &str) -> bool {
    if schema.get("type").and_then(Value::as_str) == Some(ty) {
        return true;
    }
    if schema
        .get("type")
        .and_then(Value::as_array)
        .is_some_and(|types| types.iter().any(|value| value.as_str() == Some(ty)))
    {
        return true;
    }
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|variants| variants.iter().any(|variant| permits_type(variant, ty)))
}

fn permits_empty_string(schema: &Value) -> bool {
    if let Some(variants) = schema.get("anyOf").and_then(Value::as_array) {
        return variants.iter().any(permits_empty_string);
    }
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        return variants.iter().any(permits_empty_string);
    }
    if !permits_type(schema, "string") {
        return false;
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values.iter().any(|value| value.as_str() == Some(""));
    }
    schema
        .get("minLength")
        .and_then(Value::as_u64)
        .is_none_or(|min_length| min_length == 0)
}

/// Simple template produces correct schema structure.
#[test]
fn simple_template_schema() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        foo: {{ .Values.name }}
        replicas: {{ .Values.replicas }}
        {{- end }}
    "};
    let schema = schema_for(&parse_ir(src));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "enabled": {},
            "name": {},
            "replicas": {}
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn self_guarded_empty_string_preserves_empty_fallback_branch() {
    let provider_schema = serde_json::json!({
        "type": "string",
        "minLength": 1
    });
    let values_yaml_schema = serde_json::json!({
        "type": "string"
    });

    let schema = ResolvePolicy::default().resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts {
            path_has_render_use: true,
            path_all_render_uses_self_guarded: true,
            contract_path_is_nullable: true,
            values_yaml_is_empty_string: true,
            ..ValuePathSchemaFacts::default()
        },
        provider_schema,
        values_yaml_schema,
        guard_constraint_schema: serde_json::json!({}),
        type_hint_schema: serde_json::json!({}),
    });

    assert!(
        permits_empty_string(&schema),
        "self-guarded empty-string default should stay valid, got {schema}"
    );
    assert!(
        any_of_variant_matching(&schema, |variant| {
            variant.get("minLength").and_then(Value::as_u64) == Some(1)
        })
        .is_some(),
        "non-empty rendered values should keep provider constraints, got {schema}"
    );
    assert!(
        permits_null(&schema),
        "nullable wrapping should preserve the empty-string branch, got {schema}"
    );
}

/// A truthy guard is a control-flow fact, not a type assertion.
#[test]
fn guard_only_values_without_type_evidence_stay_unconstrained() {
    let src = indoc! {r"
        {{- if .Values.feature.enabled }}
        key: {{ .Values.feature.name }}
        {{- end }}
    "};
    let schema = schema_for(&parse_ir(src));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "feature": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {},
                    "name": {}
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

/// A `with`-guarded fragment accepts null for object inputs too: Helm skips
/// the body when the guarded value is nil, so the chart input contract includes
/// both the rendered object shape and null.
#[test]
fn step1_with_fragment_null_default_is_nullable() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          {{- with .Values.extraAnnotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        extraAnnotations:
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    assert!(
        permits_type(extra, "object"),
        "extraAnnotations should keep the K8s annotations object shape, got {extra}"
    );
    assert!(
        permits_null(extra),
        "with-guarded fragment object should allow null, got {extra}"
    );
}

/// Step 1 negative: a path with no `with`-fragment use does not get widened
/// to include null on the strength of Step 1 alone. (When the same fixture
/// is run through Step 2, the type hint adds the nullable-string union.)
#[test]
fn step1_no_with_fragment_does_not_widen_to_null() {
    // No `with`, no `default` — just a plain reference. Step 1's predicate
    // requires a Fragment use, which doesn't exist here.
    let src = indoc! {r"
        name: {{ .Values.nameOverride }}
    "};
    let values_yaml = indoc! {"
        nameOverride:
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    // nameOverride should remain `{}` — no signal points to a specific type.
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    similar_asserts::assert_eq!(name, &serde_json::json!({}));
}

/// Step 2 (prefix form): `default <literal> .Values.X` with null default in
/// values.yaml produces a nullable-typed union for X.
#[test]
fn step2_default_prefix_string_literal_is_nullable_string() {
    let src = indoc! {r#"
        name: {{ default "fallback" .Values.name }}
    "#};
    let values_yaml = indoc! {"
        name:
    "};
    let schema =
        schema_for_values_yaml_and_hints(&parse_ir(src), Some(values_yaml), &collect_hints(src));

    let name = schema.pointer("/properties/name").expect("name present");
    let variants = name
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected anyOf union for nullable-string");
    assert!(permits_null(name));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string"))
    );
}

/// Step 2 (pipeline form): `.Values.X | default <literal>` is recognised
/// equivalently to the prefix form.
#[test]
fn step2_default_pipeline_string_literal_is_nullable_string() {
    let src = indoc! {r#"
        name: {{ .Values.name | default "fallback" }}
    "#};
    let values_yaml = indoc! {"
        name:
    "};
    let schema =
        schema_for_values_yaml_and_hints(&parse_ir(src), Some(values_yaml), &collect_hints(src));

    let name = schema.pointer("/properties/name").expect("name present");
    let variants = name
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected anyOf union for nullable-string");
    assert!(permits_null(name));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string"))
    );
}

/// Step 2 negative: `default $someVar .Values.x` with a non-literal first
/// argument emits no type hint. Schema is unchanged.
#[test]
fn step2_default_non_literal_first_arg_no_hint() {
    // The first arg is a variable, not a literal. Recognizer must skip.
    let src = indoc! {r#"
        {{- $fallback := "x" -}}
        name: {{ default $fallback .Values.name }}
    "#};
    let hints = collect_hints(src);
    assert!(hints.is_empty(), "expected no hints, got {hints:?}");
}

/// Step 2: integer literal → integer type hint (not string).
#[test]
fn step2_default_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default 5 .Values.replicas }}
    "};
    let hints = collect_hints(src);
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "expected integer hint, got {schemas:?}"
    );
}
/// `with or .Values.A .Values.B` now tags both A and B with `Guard::With`
/// (instead of keeping them as `Guard::Or`), so a downstream Fragment use of
/// either path qualifies for Step 1 null preservation. The body's `.` is not
/// rewritten in `with or` (dot-binding requires a single header path), so
/// this test references the path explicitly to drive a Fragment use.
#[test]
fn step1_with_or_per_path_guards_enable_null_preservation() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with or .Values.primary .Values.fallback }}
          config: |
            {{- toYaml .Values.primary | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        primary:
        fallback:
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let primary = schema
        .pointer("/properties/primary")
        .expect("primary property present");
    assert!(
        permits_null(primary),
        "primary should permit null after `with or` + explicit Fragment use, got {primary}"
    );
}

/// Explicit null defaults are preserved for object fragments, but a non-null
/// object default remains the source of truth unless values.yaml says the path
/// is nullable.
#[test]
fn step1_with_fragment_non_null_default_not_widened() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          {{- with .Values.extraAnnotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        extraAnnotations:
          foo: bar
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    assert!(
        !permits_null(extra),
        "non-null default must not be widened to nullable, got {extra}"
    );
}

/// Explicit `null` defaults stay valid when a scalar is rendered only from a
/// `with` body that skips on nil. This is the common `priorityClassName`
/// pattern across many charts.
#[test]
fn nullable_scalar_preserved_for_with_guarded_render_use() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              {{- with .Values.priorityClassName }}
              priorityClassName: {{ . }}
              {{- end }}
    "};
    let values_yaml = indoc! {"
        priorityClassName:
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let priority = schema
        .pointer("/properties/priorityClassName")
        .expect("priorityClassName present");
    let variants = priority
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected nullable priorityClassName union");
    assert!(permits_null(priority));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string")),
        "priorityClassName should also accept the provider string type, got {priority}"
    );
}

/// Explicit `null` defaults also stay valid when a scalar is rendered only
/// from a truthy self-guard inside a larger condition, such as optional
/// Service nodePorts gated by `not (empty ...)`.
#[test]
fn nullable_scalar_preserved_for_truthy_guarded_render_use() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        spec:
          type: {{ .Values.service.type }}
          ports:
            {{- with .Values.service }}
            - port: 25
              {{- if (and (eq .type "NodePort") (not (empty .ports.smtp.nodePort))) }}
              nodePort: {{ .ports.smtp.nodePort }}
              {{- end }}
            {{- end }}
    "#};
    let values_yaml = indoc! {"
        service:
          type: ClusterIP
          ports:
            smtp:
              nodePort:
    "};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let node_port = schema
        .pointer("/properties/service/properties/ports/properties/smtp/properties/nodePort")
        .expect("service.ports.smtp.nodePort present");
    let variants = node_port
        .get("anyOf")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected nullable nodePort union, got {node_port}"));
    assert!(permits_null(node_port));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "nodePort should also accept the provider integer type, got {node_port}"
    );
}

/// Explicit `null` defaults stay valid for range-only collection values.
/// Helm treats a nil range source as empty, so a chart that ships `snapshots:`
/// and later ranges over it accepts both null and concrete arrays.
#[test]
fn nullable_array_preserved_for_range_only_collection_use() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          initialize.sh: |
            exec ./entrypoint.sh {{ range .Values.snapshots }} --snapshot {{ . }} {{ end }}
    "#};
    let values_yaml = indoc! {"
        snapshots:
    "};
    let ir = parse_ir(src);
    let nullable_paths = schema_signals_for(ir.clone()).nullable_value_paths;
    assert!(
        nullable_paths.contains("snapshots"),
        "range-only collection should be classified nullable; nullable_paths={nullable_paths:?}; ir={ir:#?}"
    );
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let snapshots = schema
        .pointer("/properties/snapshots")
        .expect("snapshots present");
    assert!(
        permits_null(snapshots),
        "snapshots should allow null, got {snapshots}"
    );
    assert!(
        permits_type(snapshots, "array"),
        "snapshots should also allow concrete arrays, got {snapshots}"
    );
}

/// Truthy-guarded optional scalars should accept null even when values.yaml
/// chooses an empty-string default instead of an explicit YAML null.
#[test]
fn truthy_guarded_scalar_allows_null_without_explicit_null_default() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          {{- if .Values.fullnameOverride }}
          name: {{ .Values.fullnameOverride }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        fullnameOverride: \"\"
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-guarded fullnameOverride should allow null, got {fullname}"
    );
}

#[test]
fn common_fullname_helper_keeps_fullname_override_nullable() {
    let helpers = indoc! {r#"
        {{- define "common.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}

        {{- define "common.fullname" -}}
        {{- if .Values.fullnameOverride }}
        {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
        {{- else }}
        {{- $name := default .Chart.Name .Values.nameOverride }}
        {{- if contains $name .Release.Name }}
        {{- .Release.Name | trunc 63 | trimSuffix "-" }}
        {{- else }}
        {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: {{ include "common.fullname" . }}
    "#};
    let values_yaml = indoc! {"
        nameOverride:
        fullnameOverride:
    "};

    let mut define_index = DefineIndex::new();
    define_index
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src, &define_index)
        .project();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "common.fullname should keep fullnameOverride nullable, got {fullname}"
    );
    assert!(
        permits_type(fullname, "string"),
        "common.fullname should keep fullnameOverride string-like, got {fullname}"
    );
}

#[test]
fn nested_label_helpers_keep_common_name_override_nullable_string() {
    let helpers = indoc! {r#"
        {{- define "common.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}

        {{- define "common.selectorLabels" -}}
        app.kubernetes.io/name: {{ include "common.name" . }}
        app.kubernetes.io/instance: {{ .Release.Name }}
        {{- end }}

        {{- define "common.labels" -}}
        helm.sh/chart: test-0.1.0
        {{ include "common.selectorLabels" . }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          labels:
            {{- include "common.labels" . | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        nameOverride:
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let name_override = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        permits_null(name_override),
        "nested label helper should keep nameOverride nullable, got {name_override}; ir={ir:?}"
    );
    assert!(
        permits_type(name_override, "string"),
        "nested label helper should keep nameOverride string-like, got {name_override}; ir={ir:?}"
    );
    assert!(
        !permits_type(name_override, "object"),
        "scalar helper output should not inherit the parent labels-map object schema, got {name_override}; ir={ir:?}"
    );
}

#[test]
fn assignment_inside_inline_label_helper_does_not_project_to_parent_map() {
    let helpers = indoc! {r#"
        {{- define "common.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}

        {{- define "common.labels" -}}
        {{- $default := dict "app.kubernetes.io/name" (include "common.name" .) -}}
        app.kubernetes.io/name: {{ include "common.name" . }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: test
          labels: {{- include "common.labels" . | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        nameOverride:
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let name_override = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        permits_null(name_override),
        "assigned helper input should keep nameOverride nullable, got {name_override}; ir={ir:?}"
    );
    assert!(
        permits_type(name_override, "string"),
        "assigned helper input should keep nameOverride string-like, got {name_override}; ir={ir:?}"
    );
    assert!(
        !permits_type(name_override, "object"),
        "assignment inputs should not inherit the parent labels-map object schema, got {name_override}; ir={ir:?}"
    );
}

#[test]
fn helper_local_assignments_render_through_printf_scalar_slot() {
    let helpers = indoc! {r#"
        {{- define "common.image" -}}
        {{- $registryName := .imageRoot.registry -}}
        {{- $repositoryName := .imageRoot.repository -}}
        {{- $termination := .imageRoot.tag | toString -}}
        {{- if .global }}
          {{- if .global.imageRegistry }}
            {{- $registryName = .global.imageRegistry -}}
          {{- end -}}
        {{- end -}}
        {{- if $registryName }}
          {{- printf "%s/%s:%s" $registryName $repositoryName $termination -}}
        {{- else -}}
          {{- printf "%s:%s" $repositoryName $termination -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: app
                  image: {{ include "common.image" (dict "imageRoot" .Values.image "global" .Values.global) }}
    "#};
    let values_yaml = indoc! {"
        image:
          registry: docker.io
          repository: example/app
          tag: latest
        global:
          imageRegistry:
    "};

    let mut define_index = DefineIndex::new();
    define_index
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src, &define_index)
        .project();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    let image = schema.pointer("/properties/image").expect("image present");
    for property in ["registry", "repository", "tag"] {
        assert!(
            object_variant_with_property(image, property).is_some(),
            "image.{property} should be attributed through helper-local assignments, got {image}; ir={ir:?}"
        );
    }
}

#[test]
fn wrapper_helper_preserves_nested_local_assignment_outputs() {
    let helpers = indoc! {r#"
        {{- define "common.images.image" -}}
        {{- $registryName := .imageRoot.registry -}}
        {{- $repositoryName := .imageRoot.repository -}}
        {{- $separator := ":" -}}
        {{- $termination := .imageRoot.tag | toString -}}
        {{- if .global }}
          {{- if .global.imageRegistry }}
            {{- $registryName = .global.imageRegistry -}}
          {{- end -}}
        {{- end -}}
        {{- if .imageRoot.digest }}
          {{- $separator = "@" -}}
          {{- $termination = .imageRoot.digest | toString -}}
        {{- end -}}
        {{- if $registryName }}
          {{- printf "%s/%s%s%s" $registryName $repositoryName $separator $termination -}}
        {{- else -}}
          {{- printf "%s%s%s" $repositoryName $separator $termination -}}
        {{- end -}}
        {{- end -}}

        {{- define "app.image" -}}
        {{ include "common.images.image" (dict "imageRoot" .Values.image "global" .Values.global) }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: app
                  image: {{ template "app.image" . }}
    "#};
    let values_yaml = indoc! {"
        image:
          registry: docker.io
          repository: example/app
          tag: latest
        global: {}
    "};

    let mut define_index = DefineIndex::new();
    define_index
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src, &define_index)
        .project();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    let image = schema.pointer("/properties/image").expect("image present");
    for property in ["registry", "repository", "tag"] {
        assert!(
            object_variant_with_property(image, property).is_some(),
            "wrapper helper should preserve image.{property} output, got {image}; ir={ir:?}"
        );
    }
}

/// Fragment inputs that flow into K8s label/annotation maps should keep the
/// provider's open string-map shape instead of being closed to whatever keys
/// `values.yaml` happened to default.
#[test]
fn step_fragment_open_string_map_stays_open() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- with .Values.podLabels }}
          labels:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        podLabels:
          app: inbucket
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    assert_eq!(
        pod_labels
            .get("additionalProperties")
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("type"))
            .and_then(Value::as_str),
        Some("string"),
        "podLabels should stay an open string map, got {pod_labels}"
    );
    assert_ne!(
        pod_labels.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "podLabels should not be closed to values.yaml keys, got {pod_labels}"
    );
}

/// An empty-map placeholder in `values.yaml` (`annotations: {}`) still carries
/// less information than the provider's label/annotation map schema. Fragment
/// inputs should keep the provider's richer contract in that case too.
#[test]
fn step_fragment_empty_map_default_keeps_open_string_map() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- with .Values.annotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        annotations: {}
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let annotations = schema
        .pointer("/properties/annotations")
        .expect("annotations present");
    assert_eq!(
        annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "annotations should stay an open string map, got {annotations}"
    );
}

/// Destructured map ranges should keep the chart input as a map, even when the
/// rendered output lands in a K8s array field like `env:`.
#[test]
fn destructured_range_map_input_does_not_become_output_array() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: test
              image: busybox
              env:
                {{- range $key, $value := .Values.environment }}
                - name: {{ $key }}
                  value: {{ $value | quote }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        environment:
          INBUCKET_LOGLEVEL: debug
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    assert_eq!(
        environment.get("type").and_then(Value::as_str),
        Some("object"),
        "environment should stay an object-valued input, got {environment}"
    );
    assert_eq!(
        environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "environment should generalize to an open string map when the chart ranges over its entries, got {environment}"
    );
    assert!(
        environment.get("anyOf").is_none(),
        "environment should not widen to object-or-array, got {environment}"
    );
}

#[test]
fn destructured_range_map_with_len_guard_generalizes_to_open_string_map() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: test
              image: busybox
              {{- if (gt (len .Values.environment) 0) }}
              env:
                {{- range $key, $value := .Values.environment }}
                - name: {{ $key }}
                  value: {{ $value | quote }}
                {{- end }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        environment:
          INBUCKET_LOGLEVEL: debug
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    assert_eq!(
        environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "len-guarded destructured range should still generalize to an open string map, got {environment}"
    );
}

/// A scalar-item range that directly renders the sequence items should keep the
/// provider array metadata on the destination field, not collapse to a bare
/// `items.type` array inferred only from the item uses.
#[test]
fn scalar_item_range_keeps_provider_array_metadata() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        metadata:
          name: test
        spec:
          accessModes:
          {{- range .Values.accessModes }}
            - {{ . | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        accessModes:
          - ReadWriteOnce
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let access_modes = schema
        .pointer("/properties/accessModes")
        .expect("accessModes present");
    assert_eq!(
        access_modes.get("type").and_then(Value::as_str),
        Some("array"),
        "accessModes should be an array, got {access_modes}"
    );
    assert_eq!(
        access_modes.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "accessModes items should stay strings, got {access_modes}"
    );
    assert!(
        access_modes
            .pointer("/description")
            .and_then(Value::as_str)
            .is_some(),
        "accessModes should keep the provider description, got {access_modes}"
    );
    assert_eq!(
        access_modes
            .pointer("/x-kubernetes-list-type")
            .and_then(Value::as_str),
        Some("atomic"),
        "accessModes should keep the provider list metadata, got {access_modes}"
    );
}

/// A scalar input list that is wrapped into object-valued output items should
/// stay a scalar values array and must not inherit the provider object-item
/// schema for the rendered resource field.
#[test]
fn scalar_range_wrapped_into_object_items_stays_scalar_array() {
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
          {{- range .Values.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                {{- range .paths }}
                  - path: {{ . | quote }}
                    pathType: Prefix
                    backend:
                      service:
                        name: app
                        port:
                          number: 80
                {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        hosts:
          - host: example.test
            paths:
              - /
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let host_paths = schema
        .pointer("/properties/hosts/items/properties/paths")
        .expect("hosts[].paths present");
    assert_eq!(
        host_paths.get("type").and_then(Value::as_str),
        Some("array"),
        "hosts[].paths should stay an array input, got {host_paths}"
    );
    assert_eq!(
        host_paths.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "hosts[].paths items should stay strings, got {host_paths}"
    );
    assert!(
        host_paths.pointer("/items/anyOf").is_none(),
        "hosts[].paths should not widen to object|string items, got {host_paths}"
    );
}

#[test]
fn scalar_range_with_root_helper_stays_scalar_array() {
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
          {{- range .Values.hosts }}
            {{- $url := splitList "/" . }}
            - host: {{ first $url }}
              http:
                paths:
                  - path: /{{ rest $url | join "/" }}
                    pathType: Prefix
                    backend:
                      service:
                        name: {{ include "fullname" $ }}
                        port:
                          number: 80
          {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "fullname" -}}
        {{- .Chart.Name -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        hosts:
          - /
    "};
    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let hosts = schema.pointer("/properties/hosts").expect("hosts present");
    assert_eq!(
        hosts.get("type").and_then(Value::as_str),
        Some("array"),
        "hosts should stay an array, got {hosts}"
    );
    assert_eq!(
        hosts.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "hosts items should stay strings, got {hosts}"
    );
    assert!(
        hosts.pointer("/items/properties/Chart").is_none(),
        "root helper fields must not be projected onto range items, got {hosts}"
    );
}

#[test]
fn wildcard_source_path_creates_array_without_empty_object_variant() {
    let uses = vec![ContractUse {
        source_expr: "image.pullSecrets.*".to_string(),
        path: helm_schema_ir::YamlPath(vec![
            "spec".to_string(),
            "imagePullSecrets[*]".to_string(),
            "name".to_string(),
        ]),
        kind: ValueKind::Scalar,
        guards: Vec::new(),
        resource: Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
    }];
    let values_yaml = indoc! {"
        image:
          pullSecrets: []
    "};

    let schema = schema_for_values_yaml(&uses, Some(values_yaml));
    let pull_secrets = schema
        .pointer("/properties/image/properties/pullSecrets")
        .expect("image.pullSecrets present");

    assert_eq!(
        pull_secrets.get("type").and_then(Value::as_str),
        Some("array"),
        "wildcard source path should create an array schema, got {pull_secrets}"
    );
    assert!(
        pull_secrets.get("anyOf").is_none(),
        "wildcard source path should not create an empty-object variant, got {pull_secrets}"
    );
    assert_eq!(
        pull_secrets.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "source item should inherit the rendered name scalar type, got {pull_secrets}"
    );
}

/// Passing a structured values object into a helper via `dict` should map the
/// helper-local field accesses back to descendant values paths, not treat the
/// parent object itself as a scalar leaf at the rendered output path.
#[test]
fn dict_bound_helper_object_input_stays_object() {
    let helpers = indoc! {r#"
        {{- define "common.serviceAccountName" -}}
        {{- if .config.create -}}
        {{- .config.name | default "generated" -}}
        {{- else -}}
        {{- .config.name | default "default" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          serviceAccountName: {{ include "common.serviceAccountName" (dict "ctx" $ "config" .Values.serviceAccount) }}
    "#};
    let values_yaml = indoc! {"
        serviceAccount:
          create: true
          name: workload
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let service_account = schema
        .pointer("/properties/serviceAccount")
        .expect("serviceAccount present");
    assert_eq!(
        service_account.get("type").and_then(Value::as_str),
        Some("object"),
        "serviceAccount should remain an object-valued input, got {service_account}"
    );
    assert!(
        service_account.get("anyOf").is_none(),
        "serviceAccount should not widen to object-or-string, got {service_account}"
    );
}

#[test]
fn helper_defaulted_bound_name_allows_null() {
    let helpers = indoc! {r#"
        {{- define "common.serviceAccountName" -}}
        {{- if .config.create -}}
        {{- .config.name | default (include "common.fullname" .ctx) -}}
        {{- else -}}
        {{- .config.name | default "default" -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          serviceAccountName: {{ include "common.serviceAccountName" (dict "ctx" $ "config" .Values.serviceAccount) }}
    "#};
    let values_yaml = indoc! {r#"
        serviceAccount:
          create: true
          name: ""
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let name = schema
        .pointer("/properties/serviceAccount/properties/name")
        .expect("serviceAccount.name present");
    assert!(
        permits_null(name),
        "defaulted helper-bound serviceAccount.name should allow null, got {name}"
    );
}

#[test]
fn helper_direct_boolean_render_keeps_provider_shape() {
    let helpers = indoc! {r#"
        {{- define "common.service-account" -}}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ .config.name | default "generated" }}
        automountServiceAccountToken: {{ .config.automount }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{ include "common.service-account" (dict "ctx" $ "config" .Values.serviceAccount) }}
    "#};
    let values_yaml = indoc! {"
        serviceAccount:
          automount: true
          name: workload
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let automount = schema
        .pointer("/properties/serviceAccount/properties/automount")
        .expect("serviceAccount.automount present");
    assert!(
        permits_null(automount),
        "serviceAccount.automount should keep the provider's nullable boolean shape, got {automount}"
    );
    assert!(
        automount
            .get("anyOf")
            .and_then(Value::as_array)
            .is_some_and(|variants| !variants.is_empty()),
        "serviceAccount.automount should remain a union shaped by the provider, got {automount}"
    );
}

#[test]
fn nested_bound_helper_keeps_structured_parent_object() {
    let helpers = indoc! {r#"
        {{- define "common.tplvalues.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
        {{- if contains "{{" (toJson .value) }}
          {{- if .scope }}
              {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
          {{- else }}
            {{- tpl $value .context }}
          {{- end }}
        {{- else -}}
            {{- $value }}
        {{- end -}}
        {{- end -}}

        {{- define "common.images.image" -}}
        {{- printf "%s/%s:%s" .imageRoot.registry .imageRoot.repository .imageRoot.tag -}}
        {{- end -}}
        {{- define "workload.image" -}}
        {{ include "common.images.image" (dict "imageRoot" .Values.image) }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: {{ include "workload.image" . }}
    "#};
    let values_yaml = indoc! {"
        image:
          registry: docker.io
          repository: example/app
          tag: stable
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let image = schema.pointer("/properties/image").expect("image present");
    assert_eq!(
        image.get("type").and_then(Value::as_str),
        Some("object"),
        "image should stay object-valued, got {image}"
    );
    assert!(
        image.get("anyOf").is_none(),
        "image should not widen to object-or-string, got {image}"
    );
    assert_eq!(
        image
            .pointer("/properties/registry/type")
            .and_then(Value::as_str),
        Some("string"),
        "image.registry should stay string-typed, got {image}"
    );
}

#[test]
fn nested_scalar_helper_argument_to_yaml_fragment_stays_at_leaf_path() {
    let helpers = indoc! {r#"
        {{- define "common.names.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
        {{- else -}}
        {{- $name := default .Chart.Name .Values.nameOverride -}}
        {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
        {{- end -}}

        {{- define "common.ingress.backend" -}}
        service:
          name: {{ .serviceName }}
          port:
            name: {{ .servicePort }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        spec:
          rules:
            - http:
                paths:
                  - path: /
                    pathType: Prefix
                    backend: {{- include "common.ingress.backend" (dict "serviceName" (include "common.names.fullname" .) "servicePort" "http" "context" .) | nindent 22 }}
    "#};
    let values_yaml = indoc! {"
        nameOverride: \"\"
        fullnameOverride: \"\"
    "};

    let mut define_index = DefineIndex::new();
    define_index
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src, &define_index)
        .project();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    let name_override = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        permits_empty_string(name_override),
        "defaulted nameOverride should accept the chart's empty-string sentinel, got {name_override}; ir={ir:?}"
    );
    assert!(
        permits_type(name_override, "string"),
        "nameOverride should stay string-like, got {name_override}; ir={ir:?}"
    );
    assert!(
        !permits_type(name_override, "object"),
        "scalar helper input should not inherit the Ingress backend object schema, got {name_override}; ir={ir:?}"
    );
}

#[test]
fn image_pull_secret_fragment_helper_does_not_project_image_root_as_pod_spec() {
    let helpers = indoc! {r#"
        {{- define "common.images.image" -}}
        {{- printf "%s/%s:%s" .imageRoot.registry .imageRoot.repository .imageRoot.tag -}}
        {{- end -}}

        {{- define "common.images.renderPullSecrets" -}}
          {{- $pullSecrets := list }}
          {{- range .images -}}
            {{- range .pullSecrets -}}
              {{- if kindIs "map" . -}}
                {{- $pullSecrets = append $pullSecrets (include "common.tplvalues.render" (dict "value" .name "context" $.context)) -}}
              {{- else -}}
                {{- $pullSecrets = append $pullSecrets (include "common.tplvalues.render" (dict "value" . "context" $.context)) -}}
              {{- end -}}
            {{- end -}}
          {{- end -}}
          {{- if (not (empty $pullSecrets)) -}}
        imagePullSecrets:
            {{- range $pullSecrets | uniq }}
          - name: {{ . }}
            {{- end }}
          {{- end }}
        {{- end -}}

        {{- define "workload.image" -}}
        {{ include "common.images.image" (dict "imageRoot" .Values.image) }}
        {{- end -}}

        {{- define "workload.imagePullSecrets" -}}
        {{- include "common.images.renderPullSecrets" (dict "images" (list .Values.image .Values.clientImage) "context" $) -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          {{- include "workload.imagePullSecrets" . | nindent 2 }}
          containers:
            - name: app
              image: {{ include "workload.image" . }}
    "#};
    let values_yaml = indoc! {"
        image:
          registry: docker.io
          repository: example/app
          tag: stable
        clientImage:
          registry: docker.io
          repository: example/client
          tag: stable
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for pointer in ["/properties/image", "/properties/clientImage"] {
        let image = schema.pointer(pointer).expect("image root present");
        assert!(
            image
                .get("required")
                .and_then(Value::as_array)
                .is_none_or(|required| !required.iter().any(|key| key == "containers")),
            "{pointer} should not inherit PodSpec.required from imagePullSecrets, got {image}"
        );
        assert_eq!(
            image
                .pointer("/properties/registry/type")
                .and_then(Value::as_str),
            Some("string"),
            "{pointer}.registry should stay string-typed, got {image}"
        );
    }
}

#[test]
fn helper_string_output_conflicts_collapse_to_plain_string() {
    let helpers = indoc! {r#"
        {{- define "common.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride -}}
        {{- else -}}
        generated
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: {{ include "common.fullname" . }}
        spec:
          template:
            spec:
              serviceAccountName: {{ include "common.fullname" . }}
              containers:
                - name: app
                  image: nginx
                  env:
                    - name: TOKEN_SECRET
                      valueFrom:
                        secretKeyRef:
                          name: {{ include "common.fullname" . }}
                          key: token
    "#};
    let values_yaml = indoc! {"
        fullnameOverride: custom
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-gated helper output should still accept null, got {fullname}"
    );
    let variants = fullname
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected nullable string union");
    assert!(
        variants
            .iter()
            .any(|variant| variant.get("type").and_then(Value::as_str) == Some("string")),
        "helper-derived scalar outputs should still include a string branch, got {fullname}"
    );
}

#[test]
fn template_call_in_scalar_slot_propagates_helper_value_types() {
    let helpers = indoc! {r#"
        {{- define "common.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride -}}
        {{- else -}}
        generated
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: {{ template "common.fullname" . }}
    "#};
    let values_yaml = indoc! {"
        fullnameOverride: custom
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert_eq!(
        fullname,
        &serde_json::json!({ "type": "string" }),
        "template calls in scalar slots should propagate helper value types, got {fullname}"
    );
}

#[test]
fn nested_printf_helper_call_preserves_helper_output_guards() {
    let helpers = indoc! {r#"
        {{- define "common.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride -}}
        {{- else -}}
        {{- default .Chart.Name .Values.nameOverride -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ printf "%s-sfx" (include "common.fullname" .) }}
    "#};
    let values_yaml = indoc! {"
        fullnameOverride:
        nameOverride:
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "nested printf helper output should keep fullnameOverride nullable, got {fullname}; ir={ir:?}"
    );
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        permits_null(name),
        "nested printf helper output should keep nameOverride nullable, got {name}; ir={ir:?}"
    );
}

#[test]
fn assigned_nested_printf_helper_call_preserves_helper_output_guards() {
    let helpers = indoc! {r#"
        {{- define "common.fullname" -}}
        {{- if .Values.fullnameOverride -}}
        {{- .Values.fullnameOverride -}}
        {{- else -}}
        {{- default .Chart.Name .Values.nameOverride -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- $fullname := include "common.fullname" . }}
          name: {{ printf "%s-sfx" $fullname }}
    "#};
    let values_yaml = indoc! {"
        fullnameOverride:
        nameOverride:
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "assigned nested helper output should keep fullnameOverride nullable, got {fullname}; ir={ir:?}"
    );
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        permits_null(name),
        "assigned nested helper output should keep nameOverride nullable, got {name}; ir={ir:?}"
    );
}

#[test]
fn assigned_capability_helper_dependency_does_not_inherit_api_version_schema() {
    let helpers = indoc! {r#"
        {{- define "common.capabilities.kubeVersion" -}}
        {{- default (default .Capabilities.KubeVersion.Version .Values.kubeVersion) ((.Values.global).kubeVersion) -}}
        {{- end -}}

        {{- define "common.capabilities.hpa.apiVersion" -}}
        {{- $kubeVersion := include "common.capabilities.kubeVersion" .context -}}
        {{- print "autoscaling/v2" -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: {{ include "common.capabilities.hpa.apiVersion" (dict "context" .) }}
        kind: HorizontalPodAutoscaler
        metadata:
          name: console
        spec:
          scaleTargetRef:
            apiVersion: apps/v1
            kind: Deployment
            name: console
          minReplicas: 1
          maxReplicas: 2
    "#};
    let values_yaml = indoc! {r#"
        kubeVersion: ""
    "#};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let kube_version = schema
        .pointer("/properties/kubeVersion")
        .expect("kubeVersion present");

    assert!(
        schema_contains_type(kube_version, "string"),
        "kubeVersion should stay a chart input string, got {kube_version}; ir={ir:?}"
    );
    assert!(
        !kube_version
            .get("enum")
            .and_then(Value::as_array)
            .is_some_and(|values| values.iter().any(|value| value == "autoscaling/v2")),
        "kubeVersion must not inherit the rendered HPA apiVersion enum, got {kube_version}; ir={ir:?}"
    );
}

#[test]
fn guard_only_scalar_path_keeps_values_yaml_scalar_type() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: example
        {{- if .Values.existingSecret }}
        stringData:
          password: ignored
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        existingSecret: \"\"
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));
    let existing_secret = schema
        .pointer("/properties/existingSecret")
        .expect("existingSecret present");

    assert!(
        !permits_null(existing_secret),
        "plain guard-only scalar values should not be widened without a null-tolerant render use, got {existing_secret}"
    );
    assert!(
        schema_contains_type(existing_secret, "string"),
        "values.yaml string evidence should still be preserved, got {existing_secret}"
    );
}

#[test]
fn helper_yaml_rendered_inside_block_scalar_does_not_project_payload_shape() {
    let helpers = indoc! {r#"
        {{- define "collector.config" -}}
        receivers:
          k8s_cluster:
            collection_interval: {{ .Values.presets.clusterMetrics.collectionInterval }}
            allocatable_types_to_report:
              {{- toYaml .Values.presets.clusterMetrics.allocatableTypesToReport | nindent 10 }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: collector
        data:
          collector.yaml: |-
            {{- include "collector.config" . | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        presets:
          clusterMetrics:
            collectionInterval: 30s
            allocatableTypesToReport:
              - cpu
              - memory
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "presets": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "clusterMetrics": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "allocatableTypesToReport": {
                                "anyOf": [
                                    {
                                        "type": "array",
                                        "items": {
                                            "type": "string"
                                        }
                                    },
                                    {
                                        "type": "null"
                                    },
                                    {
                                        "type": "string"
                                    }
                                ]
                            },
                            "collectionInterval": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn helper_local_yaml_merge_inside_block_scalar_does_not_project_payload_shape() {
    let helpers = indoc! {r#"
        {{- define "collector.config" -}}
        {{- $config := include "collector.baseConfig" . | fromYaml }}
        {{- if .Values.presets.clusterMetrics.enabled }}
        {{- $config = (include "collector.applyClusterMetricsConfig" (dict "Values" . "config" $config) | fromYaml) }}
        {{- end }}
        {{- tpl (toYaml $config) . }}
        {{- end -}}

        {{- define "collector.baseConfig" -}}
        service:
          pipelines:
            metrics:
              receivers: []
              exporters: []
        {{- end -}}

        {{- define "collector.applyClusterMetricsConfig" -}}
        {{- $config := mustMergeOverwrite (include "collector.clusterMetricsConfig" .Values | fromYaml) .config }}
        {{- $config | toYaml }}
        {{- end -}}

        {{- define "collector.clusterMetricsConfig" -}}
        receivers:
          k8s_cluster:
            collection_interval: {{ .Values.presets.clusterMetrics.collectionInterval }}
            allocatable_types_to_report:
              {{- toYaml .Values.presets.clusterMetrics.allocatableTypesToReport | nindent 10 }}
        service:
          pipelines:
            metrics:
              receivers:
                - k8s_cluster
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: collector
        data:
          collector.yaml: |-
            {{- include "collector.config" . | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        presets:
          clusterMetrics:
            enabled: true
            collectionInterval: 30s
            allocatableTypesToReport:
              - cpu
              - memory
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "presets": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "clusterMetrics": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "allocatableTypesToReport": {
                                "type": "array",
                                "items": {
                                    "type": "string"
                                }
                            },
                            "collectionInterval": {
                                "type": "string"
                            },
                            "enabled": {
                                "type": "boolean"
                            }
                        }
                    }
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn local_default_alias_render_applies_provider_schema_to_fallback_path() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Widget
        spec:
          {{- $storageClass := default .Values.persistence.storageClass .Values.global.storageClass -}}
          {{- if $storageClass }}
          {{- if (eq "-" $storageClass) }}
          storageClassName: ""
          {{- else }}
          storageClassName: {{ $storageClass }}
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        global:
          storageClass:
        persistence:
          storageClass:
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "global": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "storageClass": {
                        "anyOf": [
                            {
                                "enum": [
                                    "-"
                                ]
                            },
                            {
                                "type": "null"
                            },
                            {
                                "type": "string"
                            }
                        ]
                    }
                }
            },
            "persistence": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "storageClass": {
                        "anyOf": [
                            {
                                "enum": [
                                    "-"
                                ]
                            },
                            {
                                "type": "null"
                            },
                            {
                                "type": "string"
                            }
                        ]
                    }
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

#[test]
fn unconstrained_object_fragment_keeps_nested_maps_open() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Widget
        spec:
          resources: {{ toYaml .Values.resources | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        resources:
          requests:
            cpu: 100m
            memory: 200Mi
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "resources": {
                "type": "object",
                "additionalProperties": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "string"
                    },
                    "properties": {
                        "cpu": {
                            "type": "string"
                        },
                        "memory": {
                            "type": "string"
                        }
                    }
                },
                "properties": {
                    "requests": {
                        "type": "object",
                        "additionalProperties": {
                            "type": "string"
                        },
                        "properties": {
                            "cpu": {
                                "type": "string"
                            },
                            "memory": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

/// A destructured `range $k, $v := .` inside an outer `with .Values.X` should
/// still attribute the rendered map field back to `X`, so provider schemas can
/// type it as an open string map.
#[test]
fn with_bound_range_dot_annotations_stay_string_map() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- with .Values.annotations }}
          annotations:
            {{- range $key, $value := . }}
            {{ $key }}: {{ $value | quote }}
            {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        annotations:
          foo: bar
    "};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let annotations = schema
        .pointer("/properties/annotations")
        .expect("annotations present");
    assert_eq!(
        annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "annotations should stay an open string map, got {annotations}"
    );
}

#[test]
fn with_defaulted_object_body_rebinds_dot_to_fallback_path() {
    let src = indoc! {r#"
        {{- range $db, $cfg := .Values.jobs }}
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: runner
              {{- with (.image | default $.Values.globalImage) }}
              image: "{{ .repository }}:{{ .tag }}"
              {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        globalImage:
          repository: repo/app
        jobs:
          first: {}
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/globalImage/properties/tag/type")
            .and_then(Value::as_str),
        Some("string"),
        "defaulted object in with-body should rebind dot so fallback object fields are attributed, got {schema}"
    );
}

#[test]
fn ranged_with_defaulted_object_body_attributes_defaulted_leaf_to_fallback_path() {
    let src = indoc! {r#"
        {{- $tag := .Values.image.tag | default .Chart.AppVersion -}}
        {{- range $db, $cfg := .Values.migrations.databases }}
        apiVersion: batch/v1
        kind: Job
        spec:
          template:
            spec:
              containers:
                - name: runner
                  {{- with (.image | default $.Values.migrations.image) }}
                  image: "{{ .repository }}:{{ .tag | default $tag }}"
                  imagePullPolicy: {{ .pullPolicy | default "Always" }}
                  {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        image:
          tag: app-version
        migrations:
          image:
            repository: repo/app
            pullPolicy: Always
          databases:
            first: {}
    "};

    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let image = schema
        .pointer("/properties/migrations/properties/image")
        .expect("migrations image schema present");

    let tag = image
        .pointer("/properties/tag")
        .expect("migrations image tag schema present");
    assert!(
        permits_type(tag, "string"),
        "with-body fallback image should attribute string .tag to migrations.image.tag, got {image}; ir={ir:?}"
    );
    assert!(
        permits_null(tag),
        "defaulted .tag should allow null/missing fallback, got {image}; ir={ir:?}"
    );
}

#[test]
fn self_guarded_fragment_object_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        spec:
          {{- with .Values.dataSource }}
          dataSource: {{- toYaml . | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        dataSource: {}
    "};

    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let parameters = schema
        .pointer("/properties/dataSource")
        .expect("dataSource present");

    let empty_variant = any_of_variant_matching(parameters, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("object")
            && variant.get("maxProperties").and_then(Value::as_u64) == Some(0)
    })
    .unwrap_or_else(|| {
        panic!("exact empty object placeholder variant missing: {parameters}; ir={ir:?}",)
    });
    assert_eq!(
        empty_variant
            .get("additionalProperties")
            .and_then(Value::as_bool),
        Some(false),
    );
}

#[test]
fn self_guarded_tplvalues_render_object_union_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        spec:
          {{- if .Values.persistence.dataSource }}
          dataSource: {{- include "common.tplvalues.render" (dict "value" .Values.persistence.dataSource "context" .) | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        persistence:
          dataSource: {}
    "};
    let helpers = bitnami_tplvalues_helpers();

    let mut define_index = DefineIndex::new();
    define_index.add_file_source("helpers.tpl", helpers);
    define_index
        .add_source(&TreeSitterParser, helpers)
        .expect("helpers parse");
    let schema_signals = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src, &define_index)
        .into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(Some(values_yaml)),
    );
    let data_source = schema
        .pointer("/properties/persistence/properties/dataSource")
        .expect("persistence.dataSource present");

    any_of_variant_matching(data_source, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("object")
            && variant.get("maxProperties").and_then(Value::as_u64) == Some(0)
    })
    .unwrap_or_else(|| {
        panic!(
            "exact empty object placeholder variant missing from helper-rendered object union: {data_source}",
        )
    });
}

#[test]
fn self_guarded_range_collection_keeps_exact_empty_object_placeholder() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              env:
              {{- range .Values.env }}
                - name: {{ .name }}
                  {{- if .valueFrom }}
                  valueFrom: {{- toYaml .valueFrom | nindent 20 }}
                  {{- else }}
                  value: {{ .value | quote }}
                  {{- end }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        env: {}
    "};

    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let env = schema.pointer("/properties/env").expect("env present");

    any_of_variant_matching(env, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("object")
            && variant.get("maxProperties").and_then(Value::as_u64) == Some(0)
    })
    .unwrap_or_else(|| panic!("exact empty object off-state missing: {env}; ir={ir:?}",));

    any_of_variant_matching(env, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("array")
    })
    .unwrap_or_else(|| panic!("non-empty array form missing: {env}"));
}

#[test]
fn guard_only_empty_map_default_stays_open_object() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- if .Values.config }}
          annotations:
            config-enabled: "true"
          {{- end }}
        spec:
          containers:
            - name: app
              image: busybox
    "#};
    let values_yaml = indoc! {"
        config: {}
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));
    let config = schema
        .pointer("/properties/config")
        .expect("config present");
    assert_eq!(
        config.get("type").and_then(Value::as_str),
        Some("object"),
        "guard-only empty-map default should keep the values.yaml object evidence, got {config}"
    );
    assert_eq!(
        config
            .get("additionalProperties")
            .and_then(Value::as_object)
            .map(serde_json::Map::len),
        Some(0),
        "guard-only empty-map default should remain open, got {config}"
    );
    assert!(
        config.get("anyOf").is_none(),
        "guard-only empty-map default should not become an exact-empty-or-boolean union, got {config}"
    );
}

/// A quoted YAML key inside a string-map field should still keep the concrete
/// leaf path, so the map value is typed as the string entry schema instead of
/// the parent object schema.
#[test]
fn quoted_matchlabels_key_value_stays_string() {
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: NetworkPolicy
        metadata:
          name: test
          namespace: "{{ .Values.networkPolicies.ingressController.namespace }}"
        spec:
          ingress:
            - from:
                - namespaceSelector:
                    matchLabels:
                      "kubernetes.io/metadata.name": "{{ .Values.networkPolicies.ingressController.namespace }}"
    "#};
    let values_yaml = indoc! {"
        networkPolicies:
          ingressController:
            namespace: ingress-nginx
    "};
    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let namespace = schema
        .pointer("/properties/networkPolicies/properties/ingressController/properties/namespace")
        .expect("namespace present");
    assert_eq!(
        namespace.get("type").and_then(Value::as_str),
        Some("string"),
        "quoted map-key value should stay string-valued, got {namespace}"
    );
    assert!(
        namespace.get("anyOf").is_none(),
        "quoted map-key value should not widen to object-or-string, got {namespace}"
    );
}

#[test]
fn mapping_key_template_does_not_project_scalar_onto_parent_map_value_schema() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{ .Values.account.name }}.json: |
            {}
    "#};
    let values_yaml = indoc! {"
        account:
          name: surveyor
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));
    let name = schema
        .pointer("/properties/account/properties/name")
        .expect("account.name present");

    assert_eq!(
        name.get("type").and_then(Value::as_str),
        Some("string"),
        "mapping-key interpolation should keep account.name string-valued, got {name}"
    );
    assert!(
        name.get("anyOf").is_none(),
        "mapping-key interpolation must not widen account.name with ConfigMap.data provider shape, got {name}"
    );
}

#[test]
fn exact_bound_helper_yaml_body_propagates_paths() {
    let helpers = indoc! {r#"
        {{- define "common.ingress" -}}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
          {{- if .config.tls }}
          tls:
            {{- range .config.tls }}
            - secretName: {{ .secretName }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .config.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    backend:
                      service:
                        port:
                          {{- if .servicePort -}}
                          {{- toYaml .servicePort | nindent 26 }}
                          {{- else -}}
                          number: {{ $.ctx.Values.service.port }}
                          {{- end }}
                  {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{ include "common.ingress" (dict "ctx" $ "config" .Values.ingress) }}
    "#};
    let values_yaml = indoc! {"
        ingress:
          className: nginx
          tls:
            - secretName: ingress-tls
          hosts:
            - host: inbucket.local
              paths:
                - path: /
        service:
          port: 9000
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    assert!(
        schema
            .pointer("/properties/ingress/properties/className")
            .is_some(),
        "helper body should propagate ingress.className, got {schema}"
    );
    assert!(
        permits_type(
            schema
                .pointer("/properties/ingress/properties/className")
                .expect("className present"),
            "string"
        ),
        "helper body should infer ingress.className as string-like, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        Some("string"),
        "helper body should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/service/properties/port")
            .is_some(),
        "helper body should propagate service.port from $.ctx.Values.service.port, got {schema}"
    );
}

#[test]
fn helper_defaulted_root_service_account_name_allows_null() {
    let helpers = indoc! {r#"
        {{- define "alertmanager.fullname" -}}
        {{- printf "%s-%s" "release" .Values.alertmanager.name | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
        {{- define "alertmanager.serviceAccountName" -}}
        {{- if .Values.alertmanager.serviceAccount.create -}}
            {{ default (include "alertmanager.fullname" .) .Values.alertmanager.serviceAccount.name }}
        {{- else -}}
            {{ default "default" .Values.alertmanager.serviceAccount.name }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.alertmanager.enabled }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ include "alertmanager.serviceAccountName" . }}
        ---
        apiVersion: apps/v1
        kind: StatefulSet
        spec:
          template:
            spec:
              serviceAccountName: {{ include "alertmanager.serviceAccountName" . }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        alertmanager:
          enabled: true
          name: alertmanager
          serviceAccount:
            create: true
            name:
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let name = schema
        .pointer("/properties/alertmanager/properties/serviceAccount/properties/name")
        .expect("alertmanager.serviceAccount.name present");
    assert!(
        schema_contains_type(name, "null"),
        "defaulted root serviceAccount.name should allow null, got {name}"
    );
    assert!(
        schema_contains_type(name, "string"),
        "defaulted root serviceAccount.name should stay string-like, got {name}"
    );
}

#[test]
fn exact_bound_helper_yaml_body_propagates_paths_from_with_bound_dot_arg() {
    let helpers = indoc! {r#"
        {{- define "common.ingress" -}}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
          {{- with .config.annotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
          {{- if .config.tls }}
          tls:
            {{- range .config.tls }}
            - secretName: {{ .secretName }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .config.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    backend:
                      service:
                        port:
                          number: {{ $.ctx.Values.service.port }}
                  {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- with .Values.ingress -}}
        {{- if .enabled -}}
        {{ include "common.ingress" (dict "ctx" $ "config" .) }}
        {{- end -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        ingress:
          enabled: true
          className: nginx
          annotations:
            cert-manager.io/cluster-issuer: letsencrypt
          tls:
            - secretName: ingress-tls
          hosts:
            - host: inbucket.local
              paths:
                - path: /
        service:
          port: 9000
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    assert!(
        schema
            .pointer("/properties/ingress/properties/className")
            .is_some(),
        "with-bound dot helper call should propagate ingress.className, got {schema}"
    );
    assert!(
        permits_type(
            schema
                .pointer("/properties/ingress/properties/className")
                .expect("className present"),
            "string"
        ),
        "with-bound dot helper call should propagate ingress.className as string-like, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/annotations/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.annotations, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/hosts/items/properties/host/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.hosts[*].host, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/service/properties/port")
            .is_some(),
        "with-bound dot helper call should preserve $.ctx.Values.service.port, got {schema}"
    );
}

#[test]
fn exact_bound_helper_with_bound_dot_arg_infers_classname_without_values_default() {
    let helpers = indoc! {r#"
        {{- define "common.ingress" -}}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- with .Values.ingress -}}
        {{- if .enabled -}}
        {{ include "common.ingress" (dict "ctx" $ "config" .) }}
        {{- end -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        ingress:
          enabled: true
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let class_name = schema
        .pointer("/properties/ingress/properties/className")
        .expect("className present");
    assert!(
        permits_type(class_name, "string"),
        "helper body should infer ingress.className from the output path even without a values.yaml example, got {schema}"
    );
}

#[test]
fn helper_list_bound_metadata_maps_stay_open_string_maps() {
    let helpers = indoc! {r#"
        {{- define "temporal.resourceAnnotations" -}}
        {{- $global := index . 0 -}}
        {{- $scope := index . 1 -}}
        {{- $resourceType := index . 2 -}}
        {{- $component := "server" -}}
        {{- if (or (eq $scope "admintools") (eq $scope "web")) -}}
        {{- $component = $scope -}}
        {{- end -}}
        {{- with $resourceType -}}
        {{- $resourceTypeKey := printf "%sAnnotations" . -}}
        {{- $componentAnnotations := (index $global.Values $component $resourceTypeKey) -}}
        {{- $scopeAnnotations := dict -}}
        {{- if hasKey (index $global.Values $component) $scope -}}
        {{- $scopeAnnotations = (index $global.Values $component $scope $resourceTypeKey) -}}
        {{- end -}}
        {{- $resourceAnnotations := merge $scopeAnnotations $componentAnnotations -}}
        {{- range $annotation_name, $annotation_value := $resourceAnnotations }}
        {{ $annotation_name }}: {{ $annotation_value | quote }}
        {{- end -}}
        {{- end -}}
        {{- range $annotation_name, $annotation_value := $global.Values.additionalAnnotations }}
        {{ $annotation_name }}: {{ $annotation_value | quote }}
        {{- end -}}
        {{- end -}}

        {{- define "temporal.resourceLabels" -}}
        {{- $global := index . 0 -}}
        {{- $scope := index . 1 -}}
        {{- $resourceType := index . 2 -}}
        {{- $component := "server" -}}
        {{- if (or (eq $scope "admintools") (eq $scope "web")) -}}
        {{- $component = $scope -}}
        {{- end -}}
        {{- with $resourceType -}}
        {{- $resourceTypeKey := printf "%sLabels" . -}}
        {{- $componentLabels := (index $global.Values $component $resourceTypeKey) -}}
        {{- $scopeLabels := dict -}}
        {{- if hasKey (index $global.Values $component) $scope -}}
        {{- $scopeLabels = (index $global.Values $component $scope $resourceTypeKey) -}}
        {{- end -}}
        {{- $resourceLabels := merge $scopeLabels $componentLabels -}}
        {{- range $label_name, $label_value := $resourceLabels }}
        {{ $label_name }}: {{ $label_value | quote }}
        {{- end -}}
        {{- end -}}
        {{- range $label_name, $label_value := $global.Values.additionalLabels }}
        {{ $label_name }}: {{ $label_value | quote }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
          annotations:
            {{- include "temporal.resourceAnnotations" (list $ "admintools" "pod") | nindent 4 }}
          labels:
            {{- include "temporal.resourceLabels" (list $ "admintools" "pod") | nindent 4 }}
    "#};
    let values_yaml = indoc! {r#"
        admintools:
          podAnnotations:
            team: platform
          podLabels:
            app: temporal
        additionalAnnotations:
          owner: infra
        additionalLabels:
          cluster: prod
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let pod_annotations = schema
        .pointer("/properties/admintools/properties/podAnnotations")
        .expect("admintools.podAnnotations present");
    assert_eq!(
        pod_annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "admintools.podAnnotations should stay an open string map, got {pod_annotations}"
    );

    let pod_labels = schema
        .pointer("/properties/admintools/properties/podLabels")
        .expect("admintools.podLabels present");
    assert_eq!(
        pod_labels
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "admintools.podLabels should stay an open string map, got {pod_labels}"
    );

    let additional_annotations = schema
        .pointer("/properties/additionalAnnotations")
        .expect("additionalAnnotations present");
    assert_eq!(
        additional_annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "additionalAnnotations should stay an open string map, got {additional_annotations}"
    );

    let additional_labels = schema
        .pointer("/properties/additionalLabels")
        .expect("additionalLabels present");
    assert_eq!(
        additional_labels
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "additionalLabels should stay an open string map, got {additional_labels}"
    );
}

#[test]
fn assigned_fragment_variable_keeps_open_string_map_when_reused_in_helper_call() {
    let helpers = bitnami_labels_helpers();
    let src = indoc! {r#"
        {{- $podLabels := include "common.tplvalues.merge" (dict "values" (list .Values.podLabels .Values.commonLabels) "context" .) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          labels: {{- include "common.labels.standard" (dict "customLabels" $podLabels "context" .) | nindent 4 }}
    "#};
    let values_yaml = indoc! {r#"
        commonLabels:
          team: platform
        podLabels:
          app: minio
          extra: enabled
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, &helpers), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    assert_open_string_map_or_templated_string(
        pod_labels,
        "podLabels reused through a local fragment variable",
    );
}

#[test]
fn assigned_annotations_fragment_variable_keeps_open_string_map() {
    let helpers = bitnami_tplvalues_helpers();
    let src = indoc! {r#"
        {{- $annotations := include "common.tplvalues.merge" (dict "values" (list .Values.serviceAccount.annotations .Values.commonAnnotations) "context" .) }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          annotations: {{- include "common.tplvalues.render" (dict "value" $annotations "context" .) | nindent 4 }}
    "#};
    let values_yaml = indoc! {r#"
        commonAnnotations:
          owner: infra
        serviceAccount:
          annotations:
            team: platform
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let annotations = schema
        .pointer("/properties/serviceAccount/properties/annotations")
        .expect("serviceAccount.annotations present");
    assert_open_string_map_or_templated_string(
        annotations,
        "serviceAccount.annotations reused through a local fragment variable",
    );
}

#[test]
fn direct_rendered_annotations_helper_keeps_open_string_map() {
    let helpers = bitnami_tplvalues_helpers();
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          selector:
            matchLabels:
              app: demo
          template:
            metadata:
              {{- if .Values.podAnnotations }}
              annotations: {{- include "common.tplvalues.render" (dict "value" .Values.podAnnotations "context" .) | nindent 8 }}
              {{- end }}
            spec:
              containers:
                - name: demo
                  image: nginx
    "#};
    let values_yaml = indoc! {r#"
        podAnnotations:
          owner: infra
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let pod_annotations = schema
        .pointer("/properties/podAnnotations")
        .expect("podAnnotations present");
    assert_open_string_map_or_templated_string(
        pod_annotations,
        "podAnnotations rendered through common.tplvalues.render",
    );
}

#[test]
fn direct_rendered_annotations_helper_with_empty_default_keeps_open_string_map() {
    let helpers = bitnami_tplvalues_helpers();
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          selector:
            matchLabels:
              app: demo
          template:
            metadata:
              annotations:
                checksum/config: abc
                {{- if .Values.podAnnotations }}
                {{- include "common.tplvalues.render" (dict "value" .Values.podAnnotations "context" .) | nindent 8 }}
                {{- end }}
            spec:
              containers:
                - name: demo
                  image: nginx
    "#};
    let values_yaml = indoc! {r#"
        podAnnotations: {}
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let pod_annotations = schema
        .pointer("/properties/podAnnotations")
        .expect("podAnnotations present");
    assert_open_string_map_or_templated_string(
        pod_annotations,
        "empty-map podAnnotations rendered through common.tplvalues.render",
    );
}

#[test]
fn tplvalues_render_of_omitted_probe_keeps_fragment_shape() {
    let helpers = bitnami_tplvalues_helpers();
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          selector:
            matchLabels:
              app: demo
          template:
            metadata:
              labels:
                app: demo
            spec:
              containers:
                - name: app
                  image: nginx
                  {{- if .Values.livenessProbe.enabled }}
                  livenessProbe: {{- include "common.tplvalues.render" (dict "value" (omit .Values.livenessProbe "enabled" "probeCommandTimeout") "context" $) | nindent 20 }}
                    exec:
                      command: ['/bin/bash', '-c', 'timeout {{ .Values.livenessProbe.probeCommandTimeout }} true']
                  {{- end }}
    "#};
    let values_yaml = indoc! {"
        livenessProbe:
          enabled: true
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 6
          successThreshold: 1
          probeCommandTimeout: 2
    "};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let probe = schema
        .pointer("/properties/livenessProbe")
        .expect("livenessProbe present");

    assert!(
        schema_property_contains_type(probe, "initialDelaySeconds", "integer"),
        "omitted probe fragment should retain rendered Kubernetes Probe fields, got {probe}"
    );
    assert!(
        schema_property_contains_type(probe, "probeCommandTimeout", "integer"),
        "explicit command interpolation should keep probeCommandTimeout, got {probe}"
    );
    assert!(
        schema_property_contains_type(probe, "enabled", "boolean"),
        "probe enabled guard should keep enabled as boolean, got {probe}"
    );
}

#[test]
fn assigned_fragment_variable_with_empty_defaults_keeps_open_string_map() {
    let helpers = bitnami_labels_helpers();
    let src = indoc! {r#"
        {{- $podLabels := include "common.tplvalues.merge" (dict "values" (list .Values.podLabels .Values.commonLabels) "context" .) }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          labels: {{- include "common.labels.standard" (dict "customLabels" $podLabels "context" .) | nindent 4 }}
            app.kubernetes.io/component: minio
    "#};
    let values_yaml = indoc! {r#"
        commonLabels: {}
        podLabels: {}
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, &helpers), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    assert_open_string_map_or_templated_string(
        pod_labels,
        "empty-map podLabels rendered through the assigned fragment helper path",
    );
}

#[test]
fn helper_built_matchlabels_keeps_name_override_scalar() {
    let helpers = format!(
        "{}\n{}",
        bitnami_tplvalues_helpers(),
        indoc! {r#"
            {{- define "common.names.name" -}}
            {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
            {{- end -}}

            {{- define "common.labels.matchLabels" -}}
            {{- if and (hasKey . "customLabels") (hasKey . "context") -}}
            {{ merge (pick (include "common.tplvalues.render" (dict "value" .customLabels "context" .context) | fromYaml) "app.kubernetes.io/name" "app.kubernetes.io/instance") (dict "app.kubernetes.io/name" (include "common.names.name" .context) "app.kubernetes.io/instance" .context.Release.Name ) | toYaml }}
            {{- else -}}
            app.kubernetes.io/name: {{ include "common.names.name" . }}
            app.kubernetes.io/instance: {{ .Release.Name }}
            {{- end -}}
            {{- end -}}
        "#}
    );
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: NetworkPolicy
        spec:
          podSelector:
            matchLabels: {{- include "common.labels.matchLabels" (dict "customLabels" .Values.podLabels "context" .) | nindent 6 }}
    "#};
    let values_yaml = indoc! {r#"
        nameOverride: ""
        podLabels: {}
    "#};

    let ir = parse_ir_with_helpers(src, &helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let name_override = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");

    assert!(
        permits_empty_string(name_override),
        "defaulted nameOverride should allow the shipped empty string, got {name_override}; ir={ir:?}"
    );
    assert!(
        permits_type(name_override, "string"),
        "nameOverride should stay string-valued, got {name_override}; ir={ir:?}"
    );
    assert!(
        !permits_type(name_override, "object"),
        "helper-built matchLabels map must not project its object schema onto nameOverride, got {name_override}; ir={ir:?}"
    );
}

#[test]
fn bitnami_standard_labels_merge_keeps_name_override_scalar() {
    let helpers = format!(
        "{}\n{}",
        bitnami_tplvalues_helpers(),
        indoc! {r#"
            {{- define "common.names.name" -}}
            {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
            {{- end -}}

            {{- define "common.names.chart" -}}postgresql{{- end -}}

            {{- define "common.labels.standard" -}}
            {{- if and (hasKey . "customLabels") (hasKey . "context") -}}
            {{- $default := dict "app.kubernetes.io/name" (include "common.names.name" .context) "helm.sh/chart" (include "common.names.chart" .context) "app.kubernetes.io/instance" .context.Release.Name "app.kubernetes.io/managed-by" .context.Release.Service -}}
            {{ template "common.tplvalues.merge" (dict "values" (list .customLabels $default) "context" .context) }}
            {{- else -}}
            app.kubernetes.io/name: {{ include "common.names.name" . }}
            {{- end -}}
            {{- end -}}
        "#}
    );
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          labels: {{- include "common.labels.standard" (dict "customLabels" .Values.commonLabels "context" .) | nindent 4 }}
    "#};
    let values_yaml = indoc! {r#"
        commonLabels: {}
        nameOverride: ""
    "#};

    let ir = parse_ir_with_helpers(src, &helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let name_override = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");

    assert!(
        permits_empty_string(name_override),
        "defaulted nameOverride should allow the shipped empty string, got {name_override}; ir={ir:?}"
    );
    assert!(
        permits_type(name_override, "string"),
        "nameOverride should stay string-valued, got {name_override}; ir={ir:?}"
    );
    assert!(
        !permits_type(name_override, "object"),
        "standard label merge must not project its labels map onto nameOverride, got {name_override}; ir={ir:?}"
    );
}

#[test]
fn scalar_slot_rendered_array_keeps_provider_item_schema() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        spec:
          {{- if .Values.service.loadBalancerSourceRanges }}
          loadBalancerSourceRanges: {{ .Values.service.loadBalancerSourceRanges }}
          {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        service:
          loadBalancerSourceRanges: []
    "#};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));
    let source_ranges = schema
        .pointer("/properties/service/properties/loadBalancerSourceRanges")
        .expect("service.loadBalancerSourceRanges present");

    assert_eq!(
        source_ranges.get("type").and_then(Value::as_str),
        Some("array"),
        "loadBalancerSourceRanges should remain array-valued, got {source_ranges}"
    );
    assert_eq!(
        source_ranges.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "loadBalancerSourceRanges items should keep the Kubernetes string schema, got {source_ranges}"
    );
}

#[test]
fn unresolved_workload_metadata_maps_still_infer_open_string_maps() {
    let helpers = bitnami_labels_helpers();
    let src = indoc! {r#"
        apiVersion: {{ ternary "apps/v1" "apps/v1" (eq .Values.mode "distributed") }}
        kind: {{ ternary "StatefulSet" "Deployment" (eq .Values.mode "distributed") }}
        {{- $podLabels := include "common.tplvalues.merge" (dict "values" (list .Values.podLabels .Values.commonLabels) "context" . ) }}
        metadata:
          name: test
        spec:
          template:
            metadata:
              labels: {{- include "common.labels.standard" (dict "customLabels" $podLabels "context" .) | nindent 8 }}
              {{- if .Values.podAnnotations }}
              annotations: {{- include "common.tplvalues.render" (dict "value" .Values.podAnnotations "context" .) | nindent 8 }}
              {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        mode: standalone
        commonLabels: {}
        podLabels:
          app: minio
        podAnnotations: {}
    "#};

    let schema = schema_for_values_yaml(&parse_ir_with_helpers(src, &helpers), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    assert_open_string_map_or_templated_string(
        pod_labels,
        "metadata.labels podLabels with unresolved workload kind",
    );

    let pod_annotations = schema
        .pointer("/properties/podAnnotations")
        .expect("podAnnotations present");
    assert_open_string_map_or_templated_string(
        pod_annotations,
        "metadata.annotations podAnnotations with unresolved workload kind",
    );
}

#[test]
fn inline_sequence_scalar_with_bound_dot_infers_string_type() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
              {{- with .Values.leaderElection }}
              {{- if .leaseDuration }}
              - --leader-election-lease-duration={{ .leaseDuration }}
              {{- end }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        leaderElection: {}
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    assert!(
        permits_type(
            schema
                .pointer("/properties/leaderElection/properties/leaseDuration")
                .expect("leaseDuration present"),
            "string"
        ),
        "inline sequence scalar interpolation should infer leaderElection.leaseDuration as string-like, got {schema}"
    );
}

#[test]
fn mixed_inline_template_gaps_in_scalar_sequence_item_keep_string_paths() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
                - --image={{- if .Values.image.registry -}}{{ .Values.image.registry }}/{{- end -}}{{ .Values.image.repository }}{{- if .Values.image.digest -}}@{{ .Values.image.digest }}{{- end -}}
    "#};
    let values_yaml = indoc! {"
        image:
          repository: jetstack/cert-manager-acmesolver
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    for pointer in [
        "/properties/image/properties/registry",
        "/properties/image/properties/repository",
        "/properties/image/properties/digest",
    ] {
        assert!(
            permits_type(schema.pointer(pointer).expect("pointer present"), "string"),
            "mixed inline template gaps should keep {pointer} string-like, got {schema}"
        );
    }
}

#[test]
fn with_bound_mixed_inline_template_gaps_in_scalar_sequence_item_keep_string_paths() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              args:
                {{- with .Values.image }}
                - --image={{- if .registry -}}{{ .registry }}/{{- end -}}{{ .repository }}{{- if .digest -}}@{{ .digest }}{{- end -}}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        image:
          repository: jetstack/cert-manager-acmesolver
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    for pointer in [
        "/properties/image/properties/registry",
        "/properties/image/properties/repository",
        "/properties/image/properties/digest",
    ] {
        assert!(
            permits_type(schema.pointer(pointer).expect("pointer present"), "string"),
            "with-bound mixed inline template gaps should keep {pointer} string-like, got {schema}"
        );
    }
}

#[test]
fn exact_realistic_common_ingress_helper_propagates_paths() {
    let helpers = indoc! {r#"
        {{- define "common.fullname" -}}app{{- end -}}
        {{- define "common.labels" -}}
        app.kubernetes.io/name: app
        {{- end -}}
        {{- define "common.ingress" }}
        ---
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: {{ include "common.fullname" .ctx }}
          labels:
            {{- include "common.labels" .ctx | nindent 4 }}
          {{- with .config.annotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
        spec:
          {{- with .config.className }}
          ingressClassName: {{ . }}
          {{- end }}
          {{- if .config.tls }}
          tls:
            {{- range .config.tls }}
            - hosts:
                {{- range .hosts }}
                - {{ . | quote }}
                {{- end }}
              secretName: {{ .secretName }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .config.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    {{- with .pathType }}
                    pathType: {{ . }}
                    {{- end }}
                    backend:
                      service:
                        name: {{ .serviceName | default (include "common.fullname" $.ctx) }}
                        {{ if .servicePort -}}
                        port:
                          {{- toYaml .servicePort | nindent 18 }}
                        {{ else -}}
                        port:
                          number: {{ $.ctx.Values.service.port }}
                        {{- end }}
                  {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- with .Values.ingress -}}
        {{- if .enabled -}}
        {{ include "common.ingress" (dict "ctx" $ "config" .) }}
        {{- end -}}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        ingress:
          enabled: true
          className: nginx
          annotations:
            cert-manager.io/cluster-issuer: letsencrypt
          tls:
            - hosts:
                - inbucket.local
              secretName: ingress-tls
          hosts:
            - host: inbucket.local
              paths:
                - path: /
                  pathType: Prefix
        service:
          port: 9000
    "};

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/annotations/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "realistic common.ingress helper should keep ingress.annotations open, got {schema}"
    );
    assert!(
        permits_type(
            schema
                .pointer("/properties/ingress/properties/className")
                .expect("className present"),
            "string"
        ),
        "realistic common.ingress helper should propagate ingress.className, got {schema}"
    );
    assert!(
        permits_type(
            schema
                .pointer("/properties/ingress/properties/tls/items/properties/secretName")
                .expect("secretName present"),
            "string"
        ),
        "realistic common.ingress helper should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert!(
        permits_type(
            schema
                .pointer("/properties/ingress/properties/hosts/items/properties/host")
                .expect("host present"),
            "string"
        ),
        "realistic common.ingress helper should propagate ingress.hosts[*].host, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/ingress/properties/hosts/items/properties/http")
            .is_none(),
        "realistic common.ingress helper should keep hosts input-shaped instead of projecting rendered http blocks, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/ingress/properties/hosts/items/properties/paths/items/properties/backend")
            .is_none(),
        "realistic common.ingress helper should keep paths input-shaped instead of projecting rendered backend blocks, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/service/properties/port")
            .is_some(),
        "realistic common.ingress helper should preserve $.ctx.Values.service.port, got {schema}"
    );
}

#[test]
fn direct_fragment_resource_requirements_keep_open_requests_and_limits() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: app
              image: busybox
              resources:
        {{ toYaml .Values.resources | indent 16 }}
    "#};
    let values_yaml = indoc! {"
        resources:
          limits:
            cpu: 500m
            memory: 500Mi
          requests:
            cpu: 100m
            memory: 250Mi
    "};

    let schema = schema_for_values_yaml(&parse_ir(src), Some(values_yaml));

    let requests = schema
        .pointer("/properties/resources/properties/requests")
        .expect("resources.requests present");
    assert!(
        requests
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.requests should stay an open quantity map, got {requests}"
    );
    let limits = schema
        .pointer("/properties/resources/properties/limits")
        .expect("resources.limits present");
    assert!(
        limits
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.limits should stay an open quantity map, got {limits}"
    );
}

#[test]
fn provider_schema_for_container_resources_path_keeps_open_quantity_maps() {
    let provider = production_chain_provider();
    let use_ = ProviderSchemaUse {
        value_path: "resources".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "containers[*]".to_string(),
            "resources".to_string(),
        ]),
        kind: helm_schema_ir::ValueKind::Fragment,
        resource: ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        },
        is_self_range_collection: false,
    };

    let schema = provider
        .schema_for_use(&use_)
        .expect("provider schema for container resources");

    assert!(
        schema
            .pointer("/properties/requests/additionalProperties")
            .is_some(),
        "provider should expose requests as an open quantity map, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/limits/additionalProperties")
            .is_some(),
        "provider should expose limits as an open quantity map, got {schema}"
    );
}

/// Step 2: negative-integer literal still recognised, type hint is integer.
#[test]
fn step2_default_negative_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default -3 .Values.replicas }}
    "};
    let hints = collect_hints(src);
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "expected integer hint for negative literal, got {schemas:?}"
    );
}

/// Step 2: rooted `$.Values.X` and `$root.Values.X` forms (used inside
/// ranges/withs where `.` is rebound) are recognised too — not just the
/// plain `.Values.X` form.
#[test]
fn step2_default_rooted_values_paths_recognised() {
    let src = indoc! {r#"
        {{- range .Values.servers }}
        name: {{ default "alertmanager" $.Values.alertmanager.nameOverride }}
        alias: {{ default "main" $root.Values.alertmanager.aliasOverride }}
        {{- end }}
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.contains_key("alertmanager.nameOverride"),
        "expected hint for $.Values.alertmanager.nameOverride, got {hints:?}"
    );
    assert!(
        hints.contains_key("alertmanager.aliasOverride"),
        "expected hint for $root.Values.alertmanager.aliasOverride, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a YAML comment
/// MUST NOT produce a type hint. (Acceptable known limitation if it does —
/// document with a SKIP marker — but flag the case explicitly.)
#[test]
fn step2_default_in_yaml_comment_no_hint() {
    let src = indoc! {r#"
        # example: {{ default "x" .Values.exampleName }}
        name: actual
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "YAML comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Helm template
/// comment (`{{/* ... */}}`) MUST NOT produce a type hint.
#[test]
fn step2_default_in_helm_comment_no_hint() {
    let src = indoc! {r#"
        {{/* default "x" .Values.exampleName */}}
        name: actual
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "Helm comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Go string
/// literal embedded in a template MUST NOT produce a type hint.
#[test]
fn step2_default_in_string_literal_no_hint() {
    // A real chart might emit a doc string mentioning the syntax it
    // supports. The extractor must not be fooled by syntax that's text data.
    let src = indoc! {r#"
        docs: {{- "see: default 5 .Values.example" | quote }}
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "Go-string-literal text must not produce hints, got {hints:?}"
    );
}

/// Strict per-use rule for contract nullable-path facts: a path is
/// only null-tolerant when *every* render use carries a null-tolerating
/// guard. Two uses of the same source expression - one with
/// `Guard::Default { path }` matching, one with no guards - must not
/// widen the path. Renders that hit the bare site would crash on null,
/// so the schema must reject null too.
///
/// This locks in the design line called out in review: do not widen a
/// path on the strength of "any single use has a Default guard." Only
/// the structural set-mutation pattern in a helper (see
/// `SymbolicWalker::set_default_chart_paths_for_text`) propagates the
/// guard to every read that runs after the mutation; under the strict
/// per-use rule, that path correctly widens. Mixed-guards paths stay
/// strict.
#[test]
fn contract_ir_nullable_paths_require_all_render_uses_to_be_null_tolerant() {
    let guarded = ContractUse {
        source_expr: "image.tag".into(),
        path: YamlPath(vec!["data".into(), "guarded".into()]),
        kind: ValueKind::Scalar,
        guards: vec![Guard::Default {
            path: "image.tag".into(),
        }],
        resource: None,
    };
    let bare = ContractUse {
        source_expr: "image.tag".into(),
        path: YamlPath(vec!["data".into(), "bare".into()]),
        kind: ValueKind::Scalar,
        guards: vec![],
        resource: None,
    };

    let null_paths = schema_signals_for(vec![guarded, bare]).nullable_value_paths;
    assert!(
        null_paths.is_empty(),
        "image.tag must not be widened to nullable when one render use is unguarded; got {null_paths:?}",
    );
}

/// in a helper template (`_helpers.tpl`), not in a manifest body. The
/// temporal chart's `temporal.serviceAccountName` is the canonical case.
/// The CLI must scan helper sources too, not just manifest templates.
#[test]
fn step2_default_in_helper_template_is_extracted() {
    // Mirror the structure of the temporal chart helper: the default lives
    // inside a `define`-bound helper that gets `include`d from manifests.
    let helper_src = indoc! {r#"
        {{- define "test.serviceAccountName" -}}
        {{- if .Values.serviceAccount.create -}}
            {{ default "default-name" .Values.serviceAccount.name }}
        {{- end -}}
        {{- end -}}
    "#};
    let hints = collect_hints(helper_src);
    assert!(
        hints.contains_key("serviceAccount.name"),
        "expected hint for serviceAccount.name in helper, got {hints:?}"
    );
}
