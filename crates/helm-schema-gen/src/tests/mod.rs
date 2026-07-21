use std::collections::{BTreeMap, BTreeSet};

use indoc::indoc;
use serde_json::Value;

use crate::{
    ValuesSchemaInput, generate_values_schema,
    resolve_policy::{
        ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs,
        open_objects_rejecting_declared_members, preserve_declared_default_in_schema,
    },
    values_yaml::ValuesYamlPathFacts,
};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{ProviderSchemaFragment, ResourceSchemaOracle};
use helm_schema_ir::{
    ContractIr, ContractSchemaSignals, ContractUse, ContractValuePathFacts, Guard, GuardValue,
    ProviderSchemaUse, ResourceRef, SymbolicIrContext, ValueKind, YamlPath,
};
use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider, KubernetesJsonSchemaProvider};

mod block_scalar_projection;
mod bound_helpers;
mod completed_token_contracts;
mod default_hint_extraction;
mod empty_collections;
mod fail_validators;
mod fallback_selection;
mod fragment_projection;
mod fragment_seeds;
mod guard_lowering;
mod helper_projection;
mod int_cast_preimages;
mod iterable_lanes;
mod kind_partition_matrix;
mod member_access_contracts;
mod member_serialized_shapes;
mod merge_shadowing;
mod nullability_defaults;
mod operand_kind_contracts;
mod program_wrappers;
mod provider_evidence;
mod range_collections;
mod range_contracts;
mod range_key_contracts;
mod resolve_policy;
mod shape_alternatives;
mod string_transform_contracts;
mod validator_reachability;

/// Provider chains resolve against the COMMITTED bundle with downloads off:
/// provider availability is a test input, never ambient user-cache state.
pub(crate) fn bundle_cache_dir() -> std::path::PathBuf {
    test_util::workspace_testdata().join("provider-bundle/kubernetes-json-schema-cache")
}

fn provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.35.0")
            .with_cache_dir(bundle_cache_dir())
            .with_allow_download(false),
    )])
}

fn production_chain_provider() -> Chain {
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_cache_dir(bundle_cache_dir())
        .with_allow_download(false)
        .with_api_version_guess(true);
    Chain::new(vec![Box::new(k8s_provider)]).with_inference_enabled(true)
}

fn parse_ir(src: &str) -> ContractIr {
    let idx = DefineIndex::new();
    SymbolicIrContext::new(&idx).generate_contract_ir(src)
}

fn parse_ir_with_helpers(src: &str, helpers: &str) -> ContractIr {
    let mut idx = DefineIndex::new();
    if !helpers.trim().is_empty() {
        idx.add_file_source("helpers.tpl", helpers);
    }
    SymbolicIrContext::new(&idx).generate_contract_ir(src)
}

/// Like [`parse_ir_with_helpers`], with an analysis-policy Kubernetes
/// version so `.Capabilities.KubeVersion` conditions evaluate.
fn parse_ir_with_kubernetes_version(src: &str, kubernetes_version: &str) -> ContractIr {
    let idx = DefineIndex::new();
    SymbolicIrContext::with_policy(
        &idx,
        std::collections::BTreeMap::new(),
        Some(kubernetes_version.to_string()),
    )
    .generate_contract_ir(src)
}

fn with_type_hints(mut contract: ContractIr, hints: &[(&str, &str)]) -> ContractIr {
    for (path, schema_type) in hints {
        contract.add_type_hint(*path, *schema_type);
    }
    contract
}

trait SchemaSignalSource {
    fn into_schema_signals(self) -> ContractSchemaSignals;
}

impl SchemaSignalSource for Vec<ContractUse> {
    fn into_schema_signals(self) -> ContractSchemaSignals {
        ContractIr::from_contract_uses(self).into_schema_signals()
    }
}

impl SchemaSignalSource for &[ContractUse] {
    fn into_schema_signals(self) -> ContractSchemaSignals {
        ContractIr::from_contract_uses(self.to_vec()).into_schema_signals()
    }
}

impl SchemaSignalSource for &Vec<ContractUse> {
    fn into_schema_signals(self) -> ContractSchemaSignals {
        self.as_slice().into_schema_signals()
    }
}

impl SchemaSignalSource for &ContractIr {
    fn into_schema_signals(self) -> ContractSchemaSignals {
        self.clone().into_schema_signals()
    }
}

impl SchemaSignalSource for ContractIr {
    fn into_schema_signals(self) -> ContractSchemaSignals {
        self.finalize().into_schema_signals()
    }
}

fn schema_signals_for(source: impl SchemaSignalSource) -> ContractSchemaSignals {
    source.into_schema_signals()
}

fn schema_for(source: impl SchemaSignalSource) -> Value {
    let schema_signals = source.into_schema_signals();
    generate_values_schema(ValuesSchemaInput::new(&schema_signals, &provider()))
}

fn schema_for_values_yaml(source: impl SchemaSignalSource, values_yaml: Option<&str>) -> Value {
    let schema_signals = source.into_schema_signals();
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider()).with_values_yaml(values_yaml),
    )
}

fn expected_values_schema(
    properties: serde_json::Map<String, Value>,
    all_of: Vec<Value>,
    uses_helm_truthy: bool,
) -> Value {
    let mut schema = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": properties,
        "type": "object",
    });
    if !all_of.is_empty() {
        schema["allOf"] = Value::Array(all_of);
    }
    if uses_helm_truthy {
        schema["$defs"] = serde_json::json!({
            "helm-truthy": {
                "anyOf": [
                    { "const": true },
                    { "not": { "const": 0 }, "type": "number" },
                    { "minLength": 1, "type": "string" },
                    { "minItems": 1, "type": "array" },
                    { "minProperties": 1, "type": "object" },
                ]
            }
        });
    }
    schema
}

fn root_property_schema(path: &str, schema: Value) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(path.to_string(), schema);
    // Conditional-arm carriers stay untyped so falsy ancestors skipped by a
    // `with` chain pass vacuously.
    serde_json::json!({ "additionalProperties": {}, "properties": properties })
}

fn helm_truthy_guard(path: &str) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(
        path.to_string(),
        serde_json::json!({ "$ref": "#/$defs/helm-truthy" }),
    );
    serde_json::json!({
        "properties": properties,
        "required": [path],
        "type": "object",
    })
}

fn expected_range_key_string_schema(path: &str) -> Value {
    let mut properties = serde_json::Map::new();
    properties.insert(
        path.to_string(),
        serde_json::json!({
            "anyOf": [
                { "additionalProperties": {}, "type": "object" },
                { "type": "array" },
                { "type": "null" },
                { "type": "object" },
            ]
        }),
    );
    expected_values_schema(
        properties,
        vec![
            // The unconditional two-variable range demands an iterable
            // collection in every state, beside the key-contract arm.
            root_property_schema(
                path,
                serde_json::json!({
                    "anyOf": [
                        { "type": "array" },
                        { "type": "object" },
                        { "type": "null" },
                    ]
                }),
            ),
            root_property_schema(
                path,
                serde_json::json!({
                    "anyOf": [
                        { "type": "object" },
                        { "maxItems": 0, "type": "array" },
                        { "type": "null" },
                    ]
                }),
            ),
        ],
        false,
    )
}

fn schema_accepts_instance(schema: &Value, instance: &Value) -> bool {
    // Schema FRAGMENTS reference helm-truthy without carrying the document
    // root that defines it; supply the definition then. A full document
    // resolves its own `$defs`, and wrapping it would hide them from `#/…`
    // pointers, so existing definitions are preserved and the wrap only
    // fires when the referenced definition is genuinely absent.
    let needs_truthy = crate::condition_encoding::value_references_helm_truthy(schema)
        && schema
            .get("$defs")
            .and_then(|defs| defs.get(crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME))
            .is_none();
    let document = needs_truthy.then(|| {
        let mut defs = schema
            .get("$defs")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        defs[crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME] =
            crate::condition_encoding::helm_truthy_definition_schema();
        serde_json::json!({ "$defs": defs, "allOf": [schema] })
    });
    jsonschema::validator_for(document.as_ref().unwrap_or(schema))
        .expect("schema validator")
        .is_valid(instance)
}

fn type_hints_for(source: impl SchemaSignalSource) -> BTreeMap<String, BTreeSet<String>> {
    // Union of base, fallback-scoped, and overlay-scoped hints: callers pin
    // that a hint was extracted at all, not which scope it binds in.
    source
        .into_schema_signals()
        .schema_evidence_by_value_path()
        .iter()
        .map(|(path, evidence)| {
            let mut hints = evidence.type_hints.clone();
            hints.extend(evidence.fallback_type_hints.iter().cloned());
            for overlay in &evidence.conditional_overlays {
                hints.extend(overlay.evidence.type_hints.iter().cloned());
            }
            (path.clone(), hints)
        })
        .filter(|(_, hints)| !hints.is_empty())
        .collect()
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
    if schema.get("const").is_some_and(|value| match schema_type {
        "null" => value.is_null(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        _ => false,
    }) {
        return true;
    }
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
        .chain(
            schema
                .get("allOf")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        )
        .any(|variant| schema_property_contains_type(variant, property, schema_type))
        || ["then", "else"]
            .into_iter()
            .filter_map(|key| schema.get(key))
            .any(|child| schema_property_contains_type(child, property, schema_type))
}

fn property_schema_with_type_exists(schema: &Value, property: &str, schema_type: &str) -> bool {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        if let Some(property_schema) = properties.get(property)
            && permits_type(property_schema, schema_type)
        {
            return true;
        }
        if properties.values().any(|property_schema| {
            property_schema_with_type_exists(property_schema, property, schema_type)
        }) {
            return true;
        }
    }

    if let Some(array) = schema.get("allOf").and_then(Value::as_array)
        && array
            .iter()
            .any(|entry| property_schema_with_type_exists(entry, property, schema_type))
    {
        return true;
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(array) = schema.get(key).and_then(Value::as_array)
            && array
                .iter()
                .any(|entry| property_schema_with_type_exists(entry, property, schema_type))
        {
            return true;
        }
    }

    for key in ["items", "additionalProperties"] {
        if let Some(child) = schema.get(key)
            && property_schema_with_type_exists(child, property, schema_type)
        {
            return true;
        }
    }

    for key in ["then", "else"] {
        if let Some(child) = schema.get(key)
            && property_schema_with_type_exists(child, property, schema_type)
        {
            return true;
        }
    }

    false
}

fn property_schema_contains_open_string_map(schema: &Value, property: &str) -> bool {
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        if let Some(property_schema) = properties.get(property)
            && schema_contains_open_string_map(property_schema)
        {
            return true;
        }
        if properties.values().any(|property_schema| {
            property_schema_contains_open_string_map(property_schema, property)
        }) {
            return true;
        }
    }

    if let Some(array) = schema.get("allOf").and_then(Value::as_array)
        && array
            .iter()
            .any(|entry| property_schema_contains_open_string_map(entry, property))
    {
        return true;
    }

    for key in ["anyOf", "oneOf"] {
        if let Some(array) = schema.get(key).and_then(Value::as_array)
            && array
                .iter()
                .any(|entry| property_schema_contains_open_string_map(entry, property))
        {
            return true;
        }
    }

    for key in ["items", "additionalProperties"] {
        if let Some(child) = schema.get(key)
            && property_schema_contains_open_string_map(child, property)
        {
            return true;
        }
    }

    for key in ["then", "else"] {
        if let Some(child) = schema.get(key)
            && property_schema_contains_open_string_map(child, property)
        {
            return true;
        }
    }

    false
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
struct SharedObjectProvider;

impl ResourceSchemaOracle for SharedObjectProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        // An ARRAY subtree: object-typed provider positions no longer bound
        // `toYaml` fragment inputs, and this stub's consumers pin the
        // `$defs` sharing machinery, not fragment typing.
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "additionalProperties": false
            }
        })))
    }
}

#[derive(Debug)]
struct NoopProvider;

impl ResourceSchemaOracle for NoopProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        None
    }
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

/// Returns whether the schema accepts `null`, including through the local Helm-falsy definition.
fn permits_null(schema: &Value) -> bool {
    schema_accepts_instance(schema, &Value::Null)
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

/// The union arm of a directly ranged node with the given `type`. Ranged
/// paths accept the runtime iterable domain, so their declared shape lives
/// in one arm beside the runtime alternatives (or stands alone when no
/// range widened the node).
fn ranged_arm_of_type<'a>(schema: &'a Value, ty: &str) -> Option<&'a Value> {
    if schema.get("type").and_then(Value::as_str) == Some(ty) {
        return Some(schema);
    }
    any_of_variant_matching(schema, |variant| {
        variant.get("type").and_then(Value::as_str) == Some(ty)
    })
}

fn object_variant_with_property<'a>(schema: &'a Value, property: &str) -> Option<&'a Value> {
    if schema.pointer(&format!("/properties/{property}")).is_some() {
        return Some(schema);
    }
    // Union lanes may nest (`anyOf` inside `anyOf`): descend until a
    // variant carries the property.
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .and_then(|variants| {
            variants
                .iter()
                .find_map(|variant| object_variant_with_property(variant, property))
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
