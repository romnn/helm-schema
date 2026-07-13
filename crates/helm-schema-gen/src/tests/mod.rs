use std::collections::{BTreeMap, BTreeSet};

use indoc::indoc;
use serde_json::Value;

use crate::{
    ValuesSchemaInput, generate_values_schema,
    resolve_policy::{
        ResolvePolicy, ValuePathSchemaFacts, ValuePathSchemaInputs,
        open_objects_rejecting_declared_members,
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
use test_util::prelude::sim_assert_eq;

fn provider() -> Chain {
    Chain::new(vec![Box::new(
        KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true),
    )])
}

fn production_chain_provider() -> Chain {
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
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

#[test]
fn pathless_dependency_fragment_root_keeps_values_mapping_open_with_descendants() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "webhook.serviceAccount.name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "webhook.enabled".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);
    contract.push_pathless_dependency_fragment("webhook");

    let schema = schema_for_values_yaml(
        contract,
        Some(indoc! {"
            webhook:
              enabled: false
              image:
                repository: webhook
              serviceAccount:
                name: webhook
        "}),
    );
    let webhook = schema
        .pointer("/properties/webhook")
        .expect("webhook schema");

    assert_ne!(
        webhook.get("additionalProperties"),
        Some(&Value::Bool(false)),
        "pathless dependency fragment roots should stay open when descendants are inserted: {webhook}",
    );
}

fn schema_accepts_instance(schema: &Value, instance: &Value) -> bool {
    jsonschema::validator_for(schema)
        .expect("schema validator")
        .is_valid(instance)
}

fn type_hints_for(source: impl SchemaSignalSource) -> BTreeMap<String, BTreeSet<String>> {
    // Union of base hints and overlay-scoped hints: callers pin that a hint
    // was extracted at all, not which scope it binds in.
    source
        .into_schema_signals()
        .schema_evidence_by_value_path()
        .iter()
        .map(|(path, evidence)| {
            let mut hints = evidence.type_hints.clone();
            for overlay in &evidence.conditional_overlays {
                hints.extend(overlay.evidence.type_hints.iter().cloned());
            }
            (path.clone(), hints)
        })
        .filter(|(_, hints)| !hints.is_empty())
        .collect()
}

#[test]
fn type_hint_only_descendant_preserves_object_input_branch() {
    let uses = vec![ContractUse {
        source_expr: "image".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "Service".to_string(),
        )),
        provenance: Vec::new(),
        has_string_contract: false,
    }];
    let contract = with_type_hints(
        ContractIr::from_contract_uses(uses),
        &[("image.tag", "string")],
    );
    let schema = schema_for_values_yaml(&contract, Some("image: {}\n"));
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
struct DescriptionProvider;

impl ResourceSchemaOracle for DescriptionProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "description": "provider description",
            "type": "string",
        })))
    }
}

#[derive(Debug)]
struct SharedObjectProvider;

impl ResourceSchemaOracle for SharedObjectProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        Some(ProviderSchemaFragment::new(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
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

#[test]
fn surveyor_metric_relabelings_keeps_crd_provider_evidence() {
    let src = test_util::read_testdata("charts/surveyor/templates/serviceMonitor.yaml");
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/surveyor/templates/_helpers.tpl",
        &test_util::read_testdata("charts/surveyor/templates/_helpers.tpl"),
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml: serde_yaml::Value =
        serde_yaml::from_str(&test_util::read_testdata("charts/surveyor/values.yaml"))
            .expect("values yaml");
    let provider = Chain::new(vec![
        Box::new(CrdsCatalogSchemaProvider::new().with_allow_download(true)),
        Box::new(
            KubernetesJsonSchemaProvider::new("v1.35.0")
                .with_allow_download(true)
                .with_api_version_guess(true),
        ),
    ])
    .with_inference_enabled(true);
    let resolved =
        crate::path_resolver::PathSchemaResolver::new(&schema_signals, &values_yaml, &provider)
            .resolve_all();
    let resolved_metric_relabelings = resolved
        .iter()
        .find(|path| path.value_path == "serviceMonitor.metricRelabelings")
        .expect("resolved metricRelabelings");
    assert!(
        resolved_metric_relabelings
            .provider_schema_candidate
            .is_some(),
        "metricRelabelings should preserve exact provider schema candidate"
    );
    let provider_schema_candidate = resolved_metric_relabelings
        .provider_schema_candidate
        .as_ref()
        .expect("metricRelabelings should keep CRD provider schema evidence");
    let provider_schema = provider_schema_candidate.schema();
    assert!(
        schema_signals
            .evidence_for("serviceMonitor.metricRelabelings")
            .is_some_and(|evidence| !evidence.provider_schema_uses.is_empty()),
        "metricRelabelings should keep CRD provider schema evidence"
    );
    sim_assert_eq!(
        have: provider_schema.get("type").and_then(Value::as_str),
        want: Some("array"),
        "metricRelabelings provider schema should stay array-shaped: {provider_schema}"
    );
    sim_assert_eq!(
        have: provider_schema
            .pointer("/items/properties/action/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "metricRelabelings provider schema should keep relabel config item shape: {provider_schema}"
    );
    let overlay = schema_signals
        .evidence_for("serviceMonitor.metricRelabelings")
        .and_then(|evidence| evidence.conditional_overlays.first())
        .expect("metricRelabelings conditional overlay");
    assert!(
        !overlay.evidence.provider_schema_uses.is_empty(),
        "metricRelabelings conditional overlay should keep CRD provider schema uses"
    );
    let resolved_overlay = crate::path_resolver::PathSchemaResolver::resolve_single_path_evidence(
        &overlay
            .evidence
            .as_path_evidence("serviceMonitor.metricRelabelings"),
        &provider,
    );
    sim_assert_eq!(
        have: resolved_overlay.schema.get("type").and_then(Value::as_str),
        want: Some("array"),
        "resolved overlay schema should stay array-shaped: {}",
        resolved_overlay.schema
    );
    sim_assert_eq!(
        have: resolved_overlay
            .schema
            .pointer("/items/properties/action/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "resolved overlay schema should keep relabel config item shape: {}",
        resolved_overlay.schema
    );

    let generated = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider).with_values_yaml(Some(
            &test_util::read_testdata("charts/surveyor/values.yaml"),
        )),
    );
    let metric_relabelings = generated
        .pointer("/properties/serviceMonitor/properties/metricRelabelings")
        .expect("generated metricRelabelings property");
    let provider_ref =
        any_of_variant_matching(metric_relabelings, |variant| variant.get("$ref").is_some())
            .and_then(|variant| variant.get("$ref"))
            .and_then(Value::as_str)
            .expect("generated metricRelabelings provider reference");
    let provider_schema = generated
        .pointer(
            provider_ref
                .strip_prefix('#')
                .expect("local provider reference"),
        )
        .expect("generated metricRelabelings provider definition");
    sim_assert_eq!(
        have: provider_schema.get("type").and_then(Value::as_str),
        want: Some("array"),
        "generated metricRelabelings schema should stay array-shaped: {generated}"
    );
    sim_assert_eq!(
        have: provider_schema
            .pointer("/items/properties/action/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "generated metricRelabelings item schema should stay precise: {generated}"
    );
}

#[test]
fn zalando_extra_envs_keeps_podspec_envvar_shape() {
    let src =
        test_util::read_testdata("charts/zalando-postgres-operator/templates/deployment.yaml");
    let mut idx = DefineIndex::new();
    idx.add_file_source(
        "charts/zalando-postgres-operator/templates/_helpers.tpl",
        &test_util::read_testdata("charts/zalando-postgres-operator/templates/_helpers.tpl"),
    );
    let contract = SymbolicIrContext::new(&idx).generate_contract_ir(&src);
    let schema_signals = contract.finalize().into_schema_signals();
    let values_yaml: serde_yaml::Value = serde_yaml::from_str(&test_util::read_testdata(
        "charts/zalando-postgres-operator/values.yaml",
    ))
    .expect("values yaml");
    let provider = production_chain_provider();

    let resolved =
        crate::path_resolver::PathSchemaResolver::new(&schema_signals, &values_yaml, &provider)
            .resolve_all();
    let resolved_extra_envs = resolved
        .iter()
        .find(|path| path.value_path == "extraEnvs")
        .expect("resolved extraEnvs");
    assert!(
        resolved_extra_envs.provider_schema_candidate.is_some(),
        "extraEnvs should preserve provider schema candidate: {}; evidence={:#?}",
        resolved_extra_envs.schema,
        schema_signals.evidence_for("extraEnvs")
    );
    sim_assert_eq!(
        have: resolved_extra_envs
            .schema
            .get("type")
            .and_then(Value::as_str),
        want: Some("array"),
        "extraEnvs should stay array-shaped: {}",
        resolved_extra_envs.schema
    );
    sim_assert_eq!(
        have: resolved_extra_envs
            .schema
            .pointer("/items/properties/name/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "extraEnvs should keep EnvVar item shape: {}",
        resolved_extra_envs.schema
    );

    let generated = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider).with_values_yaml(Some(
            &test_util::read_testdata("charts/zalando-postgres-operator/values.yaml"),
        )),
    );
    let extra_envs = generated
        .pointer("/properties/extraEnvs")
        .expect("generated extraEnvs property");
    sim_assert_eq!(
        have: extra_envs.get("type").and_then(Value::as_str),
        want: Some("array"),
        "generated extraEnvs should stay array-shaped: {extra_envs}"
    );
    sim_assert_eq!(
        have: extra_envs
            .pointer("/items/properties/name/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "generated extraEnvs should keep EnvVar item shape: {extra_envs}"
    );
}

#[test]
fn unrelated_default_inside_set_does_not_mark_target_as_defaulted() {
    let helpers = indoc! {r#"
        {{- define "synth.defaultValues" }}
        {{- with .Values }}
        {{- $_ := set .serviceAccount "name" (printf "%s" (.other | default "fallback")) }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- include "synth.defaultValues" . }}
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ .Values.serviceAccount.name | quote }}
    "#};

    let ir = parse_ir_with_helpers(src, helpers);
    let projection = ir.clone().finalize();
    let guarded_target_uses: Vec<_> = projection
        .uses()
        .iter()
        .filter(|use_| {
            use_.source_expr == "serviceAccount.name"
                && use_.path.0 == ["metadata".to_string(), "name".to_string()]
        })
        .collect();
    assert!(
        !guarded_target_uses.is_empty(),
        "expected a rendered use for serviceAccount.name, got {ir:?}"
    );
    assert!(
        guarded_target_uses.iter().all(|use_| {
            !use_.single_guard_conjunction().iter().any(|guard| {
                matches!(
                    guard,
                    Guard::Default { path } if path == "serviceAccount.name"
                )
            })
        }),
        "unrelated default must not mark serviceAccount.name as defaulted: {guarded_target_uses:#?}"
    );
}

#[test]
fn guarded_fragment_array_provider_schema_stays_precise() {
    #[derive(Debug)]
    struct RelabelingsProvider;

    impl ResourceSchemaOracle for RelabelingsProvider {
        fn schema_fragment_for_use(
            &self,
            use_: &ProviderSchemaUse,
        ) -> Option<ProviderSchemaFragment> {
            (use_.value_path == "serviceMonitor.metricRelabelings"
                && use_.path.0
                    == [
                        "spec".to_string(),
                        "endpoints[*]".to_string(),
                        "metricRelabelings".to_string(),
                    ])
            .then(|| {
                ProviderSchemaFragment::new(serde_json::json!({
                    "description": "provider relabelings",
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "action": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                }))
            })
        }
    }

    let uses = vec![
        ContractUse {
            source_expr: "serviceMonitor.metricRelabelings".to_string(),
            path: YamlPath(Vec::new()),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "serviceMonitor.enabled".to_string(),
            }]),
            resource: Some(ResourceRef::concrete(
                "monitoring.coreos.com/v1".to_string(),
                "ServiceMonitor".to_string(),
            )),
            provenance: Vec::new(),
            has_string_contract: false,
        },
        ContractUse {
            source_expr: "serviceMonitor.metricRelabelings".to_string(),
            path: YamlPath(vec![
                "spec".to_string(),
                "endpoints[*]".to_string(),
                "metricRelabelings".to_string(),
            ]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "serviceMonitor.enabled".to_string(),
            }]),
            resource: Some(ResourceRef::concrete(
                "monitoring.coreos.com/v1".to_string(),
                "ServiceMonitor".to_string(),
            )),
            provenance: Vec::new(),
            has_string_contract: false,
        },
    ];

    let schema_signals = schema_signals_for(uses);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &RelabelingsProvider)
            .with_values_yaml(Some("serviceMonitor:\n  metricRelabelings: []\n")),
    );

    let then_schema = schema
        .pointer("/properties/serviceMonitor/properties/metricRelabelings")
        .expect("metricRelabelings property");
    sim_assert_eq!(have: then_schema.get("type").and_then(Value::as_str), want: Some("array"));
    sim_assert_eq!(
        have: then_schema.pointer("/items/properties/action/type").and_then(Value::as_str),
        want: Some("string")
    );
}

#[test]
fn repeated_exact_provider_subtrees_emit_provider_definitions() {
    let resource = ResourceRef::concrete("example.io/v1".to_string(), "Example".to_string());
    let uses = vec![
        ContractUse {
            source_expr: "first".to_string(),
            path: YamlPath(vec!["spec".to_string(), "first".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: Some(resource.clone()),
            provenance: Vec::new(),
            has_string_contract: false,
        },
        ContractUse {
            source_expr: "second".to_string(),
            path: YamlPath(vec!["spec".to_string(), "second".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: Some(resource),
            provenance: Vec::new(),
            has_string_contract: false,
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
    sim_assert_eq!(
        have: schema.pointer("/properties/first"),
        want: Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/second"),
        want: Some(&serde_json::json!({ "$ref": "#/$defs/providerSchema1" }))
    );
    sim_assert_eq!(
        have: schema.pointer("/$defs/providerSchema1"),
        want: Some(&expected_definition)
    );
}

#[test]
fn values_yaml_comments_override_provider_descriptions() {
    let uses = vec![ContractUse {
        source_expr: "name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
        resource: Some(ResourceRef::concrete(
            "v1".to_string(),
            "ConfigMap".to_string(),
        )),
        provenance: Vec::new(),
        has_string_contract: false,
    }];
    let descriptions = BTreeMap::from([("name".to_string(), "chart description".to_string())]);
    let schema_signals = schema_signals_for(uses);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &DescriptionProvider)
            .with_values_yaml(Some("name: example\n"))
            .with_values_descriptions(&descriptions),
    );

    sim_assert_eq!(
        have: schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        want: Some("chart description")
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

    sim_assert_eq!(
        have: schema
            .pointer("/properties/name/description")
            .and_then(Value::as_str),
        want: Some("name description")
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
    if schema.get("const").is_some_and(Value::is_null) {
        return true;
    }
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

fn schema_has_format(schema: &Value, format: &str) -> bool {
    if schema.get("format").and_then(Value::as_str) == Some(format) {
        return true;
    }
    ["anyOf", "oneOf", "allOf"]
        .into_iter()
        .filter_map(|key| schema.get(key).and_then(Value::as_array))
        .flatten()
        .any(|variant| schema_has_format(variant, format))
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

#[test]
fn base64_encoded_secret_data_does_not_inherit_rendered_byte_format() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Secret
        metadata:
          name: example
        data:
          direct: {{ .Values.directSecretData }}
          encoded: {{ .Values.password | b64enc | quote }}
    "};
    let values_yaml = indoc! {r#"
        directSecretData: ""
        password: ""
    "#};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let direct = schema
        .pointer("/properties/directSecretData")
        .expect("directSecretData present");
    assert!(
        schema_has_format(direct, "byte"),
        "direct Secret.data input should keep provider byte format, got {direct}; schema={schema}"
    );

    let password = schema
        .pointer("/properties/password")
        .expect("password present");
    assert!(
        permits_type(password, "string"),
        "encoded input should remain string-like, got {password}; schema={schema}"
    );
    assert!(
        !schema_has_format(password, "byte"),
        "pre-encoded chart input must not inherit rendered Secret.data byte format, got {password}; schema={schema}"
    );
}

#[test]
fn included_encoded_secret_data_preserves_nullable_source_without_byte_format() {
    let helpers = indoc! {r#"
        {{- define "sample.passwordData" -}}
        {{- if .Values.password }}
        password: {{ .Values.password | b64enc | quote }}
        {{- end }}
        raw: {{ .Values.rawSecretData }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: example
        data:
          {{- include "sample.passwordData" . | nindent 2 }}
    "#};
    let values_yaml = indoc! {r#"
        password: ""
        rawSecretData: ""
    "#};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let password = schema
        .pointer("/properties/password")
        .expect("password present");

    assert!(
        permits_type(password, "string"),
        "encoded helper input should remain string-like, got {password}; schema={schema}"
    );
    assert!(
        permits_null(password),
        "truthy-guarded encoded helper input should allow null, got {password}; schema={schema}"
    );
    assert!(
        !schema_has_format(password, "byte"),
        "pre-encoded helper input must not inherit rendered Secret.data byte format, got {password}; schema={schema}"
    );

    let raw = schema
        .pointer("/properties/rawSecretData")
        .expect("rawSecretData present");
    assert!(
        schema_has_format(raw, "byte"),
        "unencoded sibling helper input should still inherit Secret.data byte format, got {raw}; schema={schema}"
    );
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
    let schema = schema_for(parse_ir(src));

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
    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn literal_dotted_index_and_get_keys_generate_one_root_property() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          direct: {{ index .Values "foo.bar" | quote }}
          selected: {{ (get .Values "foo.bar").baz | quote }}
    "#};
    let values_yaml = indoc! {r#"
        foo.bar:
          baz: value
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema.pointer("/properties/foo.bar").is_some(),
        "the literal dotted key should remain one root segment: {schema}"
    );
    assert!(
        schema.pointer("/properties/foo").is_none(),
        "the path currency must not fabricate a `foo` parent: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "foo.bar": { "baz": "value" } })
        ),
        "the chart's literal dotted-key default should validate: {schema}"
    );
}

#[test]
fn tpl_context_does_not_type_the_templated_value_as_an_object() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          {{- range .Values.items }}
          name: {{ tpl .name $ }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        items:
          - name: example
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let Some(name) = schema.pointer("/properties/items/items/properties/name") else {
        panic!("ranged item name missing from {schema}");
    };

    assert!(
        permits_type(name, "string"),
        "tpl's first argument is string content: {schema}"
    );
    assert!(
        !permits_type(name, "object"),
        "tpl's context must not become content: {schema}"
    );
}

#[test]
fn tpl_of_to_yaml_without_shape_evidence_stays_untyped() {
    let src = indoc! {r#"
        {{- if .Values.ingress.tls }}
        tls: {{ tpl (toYaml .Values.ingress.tls) $ | nindent 2 }}
        {{- end }}
    "#};
    let values_yaml = "ingress: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for tls in [
        serde_json::json!([]),
        serde_json::json!({ "secretName": "tls" }),
    ] {
        let instance = serde_json::json!({ "ingress": { "tls": tls } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "toYaml provides provenance but no input shape: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn serialized_collection_owns_descendant_shape() {
    let src = indoc! {r#"
        {{- range .Values.ingress.extraPaths }}
        {{- if .backend.serviceName }}{{ fail "legacy backend" }}{{ end }}
        {{- end }}
        paths: {{ tpl (toYaml .Values.ingress.extraPaths) $ | nindent 2 }}
    "#};
    let values_yaml = "ingress:\n  extraPaths: []\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let instance = serde_json::json!({
        "ingress": {
            "extraPaths": [{
                "path": "/health",
                "backend": {"service": {"name": "health", "port": {"number": 8080}}}
            }]
        }
    });

    assert!(
        schema_accepts_instance(&schema, &instance),
        "descendant reads must not reconstruct serialized input shape: {schema}"
    );
}

#[test]
fn ranged_type_branch_keeps_serialized_object_alternative() {
    let src = indoc! {r#"
        {{- range .Values.extraObjects }}
        {{- if typeIs "string" . }}
        {{ tpl . $ }}
        {{- else }}
        {{ tpl (. | toYaml) $ }}
        {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraObjects: []\n"));

    for item in [
        serde_json::json!("kind: ConfigMap"),
        serde_json::json!({"apiVersion": "v1", "kind": "ConfigMap"}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({"extraObjects": [item]})),
            "typeIs string and serialized else branches must preserve both alternatives: {schema}"
        );
    }
}

#[test]
fn structural_conversion_and_kind_guards_preserve_input_shape_alternatives() {
    let src = indoc! {r#"
        {{- if kindIs "map" .Values.extraArgs }}
        args: {{ toYaml .Values.extraArgs }}
        {{- else if kindIs "slice" .Values.extraArgs }}
        args: {{ toYaml .Values.extraArgs }}
        {{- end }}
        parsed: {{ .Values.config | fromYaml | toYaml }}
        joined: {{ join "," .Values.urls }}
    "#};
    let values_yaml = indoc! {r#"
        extraArgs: {}
        config: "enabled: true"
        urls: sentinel:26379
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for extra_args in [
        serde_json::json!({ "flag": "value" }),
        serde_json::json!(["--flag"]),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "extraArgs": extra_args,
                    "config": "enabled: true",
                    "urls": "sentinel:26379"
                })
            ),
            "kindIs branches must preserve both advertised shapes: {schema}"
        );
    }
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "extraArgs": {},
                "config": "enabled: false",
                "urls": ["one:26379", "two:26379"]
            })
        ),
        "fromYaml consumes strings and join accepts any input: {schema}"
    );
}

#[test]
fn destructured_range_over_declared_map_keeps_map_shape() {
    let src = indoc! {r#"
        ports:
          {{- range $name, $port := .Values.extraPorts }}
          {{ $name }}: {{ $port | quote }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraPorts: {}\n"));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraPorts": { "http": 8080 } })
        ),
        "a key/value range over a declared map must accept map inputs: {schema}"
    );
}

#[test]
fn helper_destructured_range_keeps_declared_map_open() {
    let helpers = indoc! {r#"
        {{- define "config" }}
        {{- range $key, $value := .Values.redis.config }}
        {{ $key }} {{ $value }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          redis.conf: |
            {{- include "config" . | nindent 4 }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("redis:\n  config:\n    save: \"\"\n"),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({"redis": {"config": {"appendonly": "no"}}})
        ),
        "a destructured helper range accepts arbitrary map keys: {schema}"
    );
}

#[test]
fn document_condition_keeps_helper_conversion_input_type() {
    let helpers = indoc! {r#"
        {{- define "config-has-processors" }}
        {{- $config := .Values.config | default "" | fromYaml }}
        {{- if $config.processors }}true{{ else }}false{{ end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- if eq (include "config-has-processors" .) "true" }}
        apiVersion: v1
        kind: ConfigMap
        {{- end }}
    "#};
    let schema =
        schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("config: null\n"));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({"config": "processors: {}"})),
        "fromYaml in a condition helper constrains its source as string input: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({"config": {"processors": {}}})),
        "fromYaml must not expose its parsed object as the source input shape: {schema}"
    );
}

#[test]
fn self_guarded_empty_string_preserves_empty_fallback_branch() {
    let provider_schema = serde_json::json!({
        "type": "string",
        "minLength": 1,
        "pattern": "^https?://"
    });
    let values_yaml_schema = serde_json::json!({
        "type": "string"
    });

    let schema = ResolvePolicy.resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                all_render_uses_self_guarded: true,
                is_nullable: true,
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                is_empty_string: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema,
        values_yaml_schema,
        guard_predicate_schema: serde_json::json!({}),
        type_hint_schema: serde_json::json!({}),
        guarded_type_hint_schema: serde_json::json!({}),
        fail_requirement_schema: serde_json::json!({}),
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
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!("not-a-url")),
        "the falsy fallback must not erase provider facets for non-empty values: {schema}"
    );
}

#[test]
fn declared_object_members_open_only_the_closed_levels_that_reject_them() {
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "known": {"type": "string"},
            "nested": {
                "type": "object",
                "additionalProperties": false,
                "properties": {"typed": {"type": "integer"}}
            }
        }
    });
    let declared = serde_json::json!({
        "known": "value",
        "extra": true,
        "nested": {"typed": 1, "extension": "value"}
    });

    let opened = open_objects_rejecting_declared_members(schema, &declared);

    sim_assert_eq!(
        have: opened,
        want: serde_json::json!({
            "type": "object",
            "properties": {
                "known": {"type": "string"},
                "nested": {
                    "type": "object",
                    "properties": {"typed": {"type": "integer"}}
                }
            }
        })
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
    let schema = schema_for(parse_ir(src));

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
    sim_assert_eq!(have: schema, want: expected);
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // nameOverride should remain `{}` — no signal points to a specific type.
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    sim_assert_eq!(have: name, want: &serde_json::json!({}));
}

#[test]
fn self_guarded_null_default_without_sink_type_stays_unconstrained() {
    let src = indoc! {r"
        {{- if .Values.terminationGracePeriodSeconds }}
        terminationGracePeriodSeconds: {{ .Values.terminationGracePeriodSeconds }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        terminationGracePeriodSeconds:
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let Some(termination_grace_period) =
        schema.pointer("/properties/terminationGracePeriodSeconds")
    else {
        panic!("terminationGracePeriodSeconds missing from {schema}");
    };

    sim_assert_eq!(
        have: termination_grace_period,
        want: &serde_json::json!({}),
        "a null default is an unset sentinel, not exclusive null typing: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "terminationGracePeriodSeconds": 90 })
        ),
        "a configured non-null value must remain accepted without stronger sink evidence: {schema}"
    );
}

/// `quote`, `squote`, and `toString` call Sprig's `strval`, whose
/// fallback is `fmt.Sprintf("%v", value)` — maps, lists, and nil all render
/// as text. The input domain is unconstrained; only the OUTPUT is a string.
#[test]
fn quote_stringification_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          {{- if .Values.enabled }}
          flag: {{ .Values.flag | quote }}
          count: {{ .Values.count | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        flag: false
        count: 7
        enabled: true
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "enabled": true, "flag": false, "count": 7 }),
        serde_json::json!({ "enabled": true, "flag": "false", "count": "7" }),
        serde_json::json!({ "enabled": true, "flag": {}, "count": 7 }),
        serde_json::json!({ "enabled": true, "flag": false, "count": [] }),
        serde_json::json!({ "enabled": true, "flag": null, "count": { "k": "v" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strval stringifies any input, so quote constrains nothing: instance={instance}; schema={schema}"
        );
    }
}

/// Direct-call forms: the total stringifications accept any input in
/// prefix position too, and `join` converts anything through `strslice`
/// (lists element-wise, non-lists as singletons, nil as empty).
#[test]
fn total_stringification_direct_forms_accept_any_input() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          quoted: {{ quote .Values.quoted }}
          squoted: {{ squote .Values.squoted }}
          stringified: {{ toString .Values.stringified }}
          joined: {{ join "," .Values.joined }}
    "#};
    let values_yaml = indoc! {"
        quoted: probe
        squoted: probe
        stringified: probe
        joined: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for probe in [
        serde_json::json!("text"),
        serde_json::json!(7),
        serde_json::json!(true),
        serde_json::json!(null),
        serde_json::json!({ "k": "v" }),
        serde_json::json!(["item"]),
    ] {
        let instance = serde_json::json!({
            "quoted": probe,
            "squoted": probe,
            "stringified": probe,
            "joined": probe,
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "total stringification accepts any input: instance={instance}; schema={schema}"
        );
    }
}

/// Sprig's numeric casts (`int`, `int64`, `float64`) convert through
/// `cast.ToXxx`, which coerces ANY input (junk becomes zero) instead of
/// failing: metrics-server passes `"365"` through
/// `int .Values.tls.helm.certDurationDays` and coredns emits
/// `.Values.autoscaler.coresPerReplica | float64`, and Helm renders both.
#[test]
fn numeric_casts_accept_any_input() {
    let src = indoc! {r#"
        {{- $days := int .Values.certDurationDays }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          days: {{ $days | quote }}
          cores: {{ .Values.coresPerReplica | float64 }}
    "#};
    let values_yaml = indoc! {"
        certDurationDays: 365
        coresPerReplica: 256
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "certDurationDays": 365, "coresPerReplica": 256 }),
        serde_json::json!({ "certDurationDays": "365", "coresPerReplica": "256" }),
        serde_json::json!({ "certDurationDays": { "bad": true }, "coresPerReplica": [1] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "numeric casts coerce any input: instance={instance}; schema={schema}"
        );
    }
}

/// A total stringification that appears only inside a CONDITION erases the
/// input shape exactly like the same conversion at a render site or in a
/// `set` expression (vault gates its PSP templates on
/// `eq (.Values.global.psp.enable | toString) "true"`, and Helm accepts the
/// string form).
#[test]
fn condition_only_to_string_erases_declared_typing() {
    let helpers = indoc! {r#"
        {{- define "repro.ha" -}}
        {{- if eq (.Values.ha.enabled | toString) "true" -}}
        true
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if eq (.Values.psp.enable | toString) "true" }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: psp
        {{- end }}
        {{- if eq (include "repro.ha" .) "true" }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: ha
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        psp:
          enable: false
        ha:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "psp": { "enable": true }, "ha": { "enabled": true } }),
        serde_json::json!({ "psp": { "enable": "true" }, "ha": { "enabled": "true" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "stringified flag comparisons accept boolean and string forms: instance={instance}; schema={schema}"
        );
    }
}

/// A `typeOf`/`kindOf` comparison dispatches on the value's runtime type
/// (velero: `eq (typeOf .Values.initContainers) "string"` chooses `tpl` vs
/// `toYaml`; vault binds `$type := typeOf .Values.server.affinity` first).
/// Every arm renders SOME types and unmatched types render nothing — also
/// valid — so the dispatch must not close the path to one arm's type, and
/// an arm's sink typing holds only under its test.
#[test]
fn type_dispatch_keeps_string_and_structured_alternatives() {
    let src = indoc! {r#"
        {{- $type := typeOf .Values.affinity }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: server
        spec:
          template:
            spec:
              affinity:
                {{- if eq $type "string" }}
                {{- tpl .Values.affinity . | nindent 8 }}
                {{- else }}
                {{- toYaml .Values.affinity | nindent 8 }}
                {{- end }}
              initContainers:
                {{- if eq (typeOf .Values.initContainers) "string" }}
                {{- tpl .Values.initContainers . | nindent 8 }}
                {{- else }}
                {{- toYaml .Values.initContainers | nindent 8 }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        affinity: {}
        initContainers: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "affinity": { "nodeAffinity": {} }, "initContainers": [{ "name": "init" }] }),
        serde_json::json!({ "affinity": "{{ .Values.name }}", "initContainers": "- name: init" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "type dispatch keeps every arm's form valid: instance={instance}; schema={schema}"
        );
    }
}

/// A partial type dispatch (loki's `hostUsers`: a `kindIs "bool"` arm and a
/// string arm, no catch-all) must not close the path to the tested types:
/// an unmatched type renders nothing, which Helm accepts.
#[test]
fn partial_type_dispatch_does_not_close_untested_types() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          {{- if kindIs "bool" .Values.hostUsers }}
          hostUsers: {{ .Values.hostUsers }}
          {{- else if kindIs "string" .Values.hostUsers }}
          hostUsers: {{ tpl .Values.hostUsers . }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        hostUsers: true
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "hostUsers": true }),
        serde_json::json!({ "hostUsers": "{{ .Values.global.hostUsers }}" }),
        serde_json::json!({ "hostUsers": { "unmatched": "renders nothing" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "untested types render nothing and stay valid: instance={instance}; schema={schema}"
        );
    }
}

/// Direct `typeIs`/`kindIs` tests also use exact Go type names (velero:
/// `typeIs "[]interface {}" .Values.configuration.backupStorageLocation`):
/// the guard is a partial type dispatch, so untested types skip the branch
/// and render nothing, which stays valid.
#[test]
fn type_is_decodes_exact_go_container_names() {
    let src = indoc! {r#"
        {{- if typeIs "[]interface {}" .Values.locations }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: locations
        data:
          {{- range .Values.locations }}
          {{ .name }}: configured
          {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        locations: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "locations": [{ "name": "a" }] }),
        serde_json::json!({ "locations": "ignored" }),
        serde_json::json!({ "locations": { "unmatched": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "exact Go names decode as type tests; untested types skip the branch: instance={instance}; schema={schema}"
        );
    }
}

/// Condition pipelines classify left-to-right (datadog:
/// `eq (.Values.agents.image.tag | toString | trimSuffix "-jmx") "latest"`):
/// a consumer AFTER a total conversion operates on converted text and
/// claims nothing about the raw value, while a consumer BEFORE any
/// conversion still binds the raw string contract.
#[test]
fn condition_pipeline_order_scopes_string_consumers() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if eq (.Values.tag | toString | trimSuffix "-jmx") "latest" }}
          latest: "true"
          {{- end }}
          {{- if eq (.Values.suffix | trimSuffix "-" | toString) "x" }}
          suffixed: "true"
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        tag: latest
        suffix: x-
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "tag": 7 })),
        "toString converts before the trim, so numbers render: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "suffix": { "bad": true } })),
        "a consumer ahead of the conversion still needs a string: {schema}"
    );
}

/// An explicit `fail` branch is a VALIDATOR: rendering aborts whenever its
/// guards hold, so valid inputs must falsify the failing test wherever the
/// outer conditions hold (kyverno fails on non-string image tags inside a
/// helper; traefik fails on plugins missing moduleName/version while
/// ranging them; sealed-secrets fails on non-string annotation map values).
#[test]
fn fail_branches_bind_validator_requirements() {
    let helpers = indoc! {r#"
        {{- define "repro.image" -}}
        {{- $tag := default .defaultTag .image.tag -}}
        {{- if not (typeIs "string" $tag) -}}
          {{ fail "Image tags must be strings." }}
        {{- end -}}
        {{- print "img:" $tag -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          containers:
          - name: main
            image: {{ include "repro.image" (dict "image" .Values.image "defaultTag" .Chart.AppVersion) | quote }}
            args:
            {{- range $name, $plugin := .Values.plugins }}
            {{- if or (ne (typeOf $plugin) "map[string]interface {}") (not (hasKey $plugin "moduleName")) }}
              {{- fail (printf "plugin %s is missing moduleName" $name) }}
            {{- end }}
            - "--plugin={{ $name }}"
            {{- end }}
            env:
            {{- range $k, $v := .Values.annotations }}
              {{- if not (and $v (kindIs "string" $v)) }}
                {{ fail "Annotation values have to be strings" }}
              {{- end }}
            {{- end }}
            - name: PROBE
              value: "set"
    "#};
    let values_yaml = indoc! {"
        image:
          tag: latest
        plugins: {}
        annotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "image": { "tag": 7 } }),
            false,
            "non-string tag fails",
        ),
        (
            serde_json::json!({ "image": { "tag": "v1" } }),
            true,
            "string tag renders",
        ),
        (
            serde_json::json!({ "image": { "tag": null } }),
            true,
            "null tag takes the default",
        ),
        (
            serde_json::json!({ "plugins": { "bad": 7 } }),
            false,
            "scalar plugin fails",
        ),
        (
            serde_json::json!({ "plugins": { "bad": {} } }),
            false,
            "plugin without moduleName fails",
        ),
        (
            serde_json::json!({ "plugins": { "ok": { "moduleName": "m" } } }),
            true,
            "complete plugin renders",
        ),
        (
            serde_json::json!({ "annotations": { "bad": 7 } }),
            false,
            "non-string annotation fails",
        ),
        (
            serde_json::json!({ "annotations": { "ok": "v" } }),
            true,
            "string annotation renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A `fail` guarded by a condition the lowering can only APPROXIMATE on
/// the tested path must not become a requirement: kyverno's replicas
/// helper fails only when `eq (int .) 0`, which does not decode, so
/// negating the decodable remainder would reject every normal count.
#[test]
fn approximate_fail_guards_abstain() {
    let helpers = indoc! {r#"
        {{- define "repro.replicas" -}}
        {{- if and (not (kindIs "invalid" .)) (not (kindIs "string" .)) -}}
        {{- if eq (int .) 0 -}}
          {{- fail "0 replicas is not supported" -}}
        {{- end -}}
        {{- end -}}
        {{- . -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: probe
        spec:
          replicas: {{ include "repro.replicas" .Values.replicas }}
    "#};
    let values_yaml = indoc! {"
        replicas: 1
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "replicas": 3 })),
        "a normal replica count renders; the undecodable zero-check must not manufacture a requirement: {schema}"
    );
}

/// A total stringification is neutral evidence about its own input; an
/// INDEPENDENT unconditional string consumer still binds. Cilium's
/// `cluster.name` is quoted into the configmap, but `replace` also consumes
/// it in validation logic — a map value fails `helm template` there.
#[test]
fn stringified_use_keeps_unconditional_string_transform_contract() {
    let src = indoc! {r#"
        {{- if gt (len (.Values.cluster.name | replace "-" "")) 30 }}
        {{- fail "cluster name too long" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          cluster-name: {{ .Values.cluster.name | quote }}
    "#};
    let values_yaml = indoc! {"
        cluster:
          name: default
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": "prod" } })
        ),
        "string cluster names render: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": { "bad": true } } })
        ),
        "replace consumes the raw name, so a map fails rendering and must be rejected: {schema}"
    );
}

/// Mutually exclusive guarded uses lower their own domains under their own
/// conditions (falco's `rolearn`): the quote branch renders anything, the
/// b64enc branch fails rendering for non-strings.
#[test]
fn quote_branch_does_not_erase_b64enc_branch_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.aws.useirsa }}
          role-arn: {{ .Values.aws.rolearn | quote }}
          {{- else }}
          AWS_ROLEARN: "{{ .Values.aws.rolearn | b64enc }}"
          {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        aws:
          useirsa: true
          rolearn: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // The b64enc contract rides its own row's condition: it binds only
    // where that branch renders. In the quote branch the same map renders
    // fine (Helm prints it as text).
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": true, "rolearn": { "bad": true } } })
        ),
        "the quote branch renders any value: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": false, "rolearn": { "bad": true } } })
        ),
        "the b64enc branch rejects non-strings: {schema}"
    );
    for useirsa in [true, false] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({ "aws": { "useirsa": useirsa, "rolearn": "arn:aws:iam::1:role/x" } })
            ),
            "strings render in both branches (useirsa={useirsa}): {schema}"
        );
    }
}

/// A `join` occurrence proves nothing about OTHER occurrences: sealed-secrets
/// also `range`s `additionalNamespaces` under its namespaced-roles flag, and
/// a scalar fails that render (`range can\'t iterate over ns-a`).
#[test]
fn join_use_does_not_erase_range_branch() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.additionalNamespaces }}
          namespaces: {{ join "," .Values.additionalNamespaces | quote }}
          {{- end }}
        {{- if .Values.rbac.namespacedRoles }}
        {{- range .Values.additionalNamespaces }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: role-{{ . }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
        rbac:
          namespacedRoles: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "additionalNamespaces": "ns-a" })
        ),
        "with namespaced roles off, only the join renders and a scalar is fine: {schema}"
    );
    for namespaces in [
        serde_json::json!(["ns-a"]),
        serde_json::json!({ "a": "ns-a" }),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": namespaces
                })
            ),
            "range iterates lists and maps: {schema}"
        );
    }
    // `range` cannot iterate a string, so `namespacedRoles=true` plus a
    // string fails `helm template` and the guarded iterable domain rejects
    // the combination.
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": "ns-a"
            })
        ),
        "inside the ranged branch a string cannot iterate: {schema}"
    );
    // Integer counts iterate (Helm's `--set` channel delivers int64; a
    // JSON Schema cannot separate that from the failing values-file
    // float64 spelling, so the renderable channel wins); non-integral
    // numbers fail in every channel.
    for count in [2, 0, -1] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": count
                })
            ),
            "range iterates integer counts: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": 2.5
            })
        ),
        "non-integral numbers cannot iterate: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "rbac": { "namespacedRoles": true } })
        ),
        "an absent collection ranges zero times and renders: {schema}"
    );
}

/// printf's format parameter is a real Go `string`: NFS provisioner calls
/// `printf .Values.storageClass.provisionerName`, and a non-string value
/// fails template evaluation (`wrong type for value; expected string`).
#[test]
fn dynamic_printf_format_requires_string() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ printf .Values.storageClass.provisionerName }}
    "#};
    let values_yaml = indoc! {"
        storageClass:
          provisionerName: cluster.local/provisioner
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": "x/y" } })
        ),
        "string formats evaluate: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": 7 } })
        ),
        "a non-string printf format fails template evaluation and must be rejected: {schema}"
    );
}

/// printf's data parameters render through any verb (Go fmt embeds
/// mismatches in the output): airflow formats `dags.gitSync.subPath` with a
/// literal format and Helm renders `subPath: 7` as `%!s(int64=7)`.
#[test]
fn printf_data_argument_accepts_any_value_through_helper_sink() {
    let helpers = indoc! {r#"
        {{- define "airflow_dags" -}}
        {{- printf "%s/dags/repo/%s" .Values.airflowHome .Values.dags.gitSync.subPath -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          dags_folder: {{ include "airflow_dags" . }}
    "#};
    let values_yaml = indoc! {r#"
        airflowHome: /opt/airflow
        dags:
          gitSync:
            subPath: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for sub_path in [
        serde_json::json!("repo/dags"),
        serde_json::json!(7),
        serde_json::json!(null),
    ] {
        let instance = serde_json::json!({ "dags": { "gitSync": { "subPath": sub_path } } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "printf data arguments render any value: instance={instance}; schema={schema}"
        );
    }
}

/// Chart repro (sealed-secrets `additionalNamespaces`): a declared-list
/// value joined under a self-truthy guard renders map and scalar values
/// through Sprig's singleton fallback, so the declared array type must not
/// reject them.
#[test]
fn self_guarded_join_of_declared_list_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: controller
                  args:
                    {{- if .Values.additionalNamespaces }}
                    - --additional-namespaces
                    - {{ join "," .Values.additionalNamespaces | quote }}
                    {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for probe in [
        serde_json::json!(["ns-a", "ns-b"]),
        serde_json::json!("ns-a"),
        serde_json::json!({ "k": "v" }),
    ] {
        let instance = serde_json::json!({ "additionalNamespaces": probe });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strslice converts any joined input: instance={instance}; schema={schema}"
        );
    }
}

/// Chart repro (grafana `sidecar.alerts.skipTlsVerify`): an undeclared
/// value quoted into a typed string sink (`env[].value`) under a `with`
/// guard renders any type, so the sink typing must not flow back through the
/// stringification.
#[test]
fn with_guarded_quote_into_string_sink_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: sidecar
                  env:
                    {{- with .Values.sidecar.skipTlsVerify }}
                    - name: SKIP_TLS_VERIFY
                      value: {{ quote . }}
                    {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("sidecar: {}\n"));

    for probe in [
        serde_json::json!(true),
        serde_json::json!("true"),
        serde_json::json!({ "k": "v" }),
        serde_json::json!([1, 2]),
    ] {
        let instance = serde_json::json!({ "sidecar": { "skipTlsVerify": probe } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "quote erases input shape at the env sink: instance={instance}; schema={schema}"
        );
    }
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let name = schema.pointer("/properties/name").expect("name present");
    assert!(permits_null(name));
    assert!(
        permits_type(name, "string"),
        "default fallback should keep the string branch, got {name}"
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let name = schema.pointer("/properties/name").expect("name present");
    assert!(permits_null(name));
    assert!(
        permits_type(name, "string"),
        "default fallback should keep the string branch, got {name}"
    );
}

#[test]
fn step2_default_after_intervening_required_call_no_hint() {
    let src = indoc! {r#"
        name: {{ .Values.name | required "name is required" | default "fallback" }}
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.is_empty(),
        "default after required must not type-hint the original values path, got {hints:?}"
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
    let hints = type_hints_for(parse_ir(src));
    assert!(hints.is_empty(), "expected no hints, got {hints:?}");
}

/// Step 2: integer literal → integer type hint (not string).
#[test]
fn step2_default_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default 5 .Values.replicas }}
    "};
    let hints = type_hints_for(parse_ir(src));
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas.contains("integer"),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let priority = schema
        .pointer("/properties/priorityClassName")
        .expect("priorityClassName present");
    assert!(permits_null(priority));
    assert!(
        permits_type(priority, "string"),
        "priorityClassName should also accept the provider string type, got {priority}"
    );
}

/// A scalar rendered only from a truthy self-guard inside a larger condition
/// (optional Service nodePorts gated by `not (empty ...)`) lowers its
/// provider typing under the foreign condition: the base stays open (null and
/// everything else stay valid when the guard cannot fire), and the guarded
/// branch keeps the null alternative the self-guard implies.
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
    sim_assert_eq!(have: node_port, want: &serde_json::json!({}));
    let guarded_node_port = schema
        .pointer(
            "/properties/service/allOf/0/then/properties/ports/properties/smtp/properties/nodePort",
        )
        .expect("guarded nodePort overlay present");
    assert!(permits_null(guarded_node_port));
    assert!(
        permits_type(guarded_node_port, "integer"),
        "guarded nodePort should accept the provider integer type, got {guarded_node_port}"
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
    let signals = schema_signals_for(ir.clone());
    let nullable_paths = signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.is_nullable)
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-guarded fullnameOverride should allow null, got {fullname}"
    );
}

#[test]
fn exclusive_boolean_guarded_path_lowers_to_if_then_overlay() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: false\n")),
    );

    let base_host = schema
        .pointer("/properties/feature/properties/host")
        .expect("feature.host base schema");
    assert!(
        base_host.as_object().is_some_and(serde_json::Map::is_empty),
        "guarded-only paths should stay open outside the branch that consumes them: {schema}"
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/if/properties/enabled/$ref"),
        want: Some(&serde_json::json!("#/$defs/helm-truthy")),
        "guard should lower at the nearest common ancestor, not only at the root: {schema}"
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/then/properties/host/type"),
        want: Some(&serde_json::json!("string")),
        "guarded path should reappear under then-branch schema: {schema}"
    );
    assert!(
        schema.get("allOf").is_none(),
        "nested guard/target pairs should not be forced to the root schema: {schema}"
    );
}

#[test]
fn default_true_boolean_guard_lowers_absence_as_active_branch() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: true\n")),
    );

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "omitted default-true guard should still activate the guarded host schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": false,
                    "host": 7
                }
            })
        ),
        "explicit false guard should leave the guarded-only host value unconstrained: {schema}"
    );
}

#[test]
fn negated_boolean_guard_lowers_to_not_condition() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Not {
                path: "feature.enabled".to_string(),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: false\n")),
    );

    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/if/not/properties/enabled/$ref"),
        want: Some(&serde_json::json!("#/$defs/helm-truthy")),
        "negated boolean guards should lower to JSON Schema `not`: {schema}"
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/then/properties/host/type"),
        want: Some(&serde_json::json!("string")),
        "negated guard should still reapply the target schema in the then branch: {schema}"
    );
}

#[test]
fn not_equal_guard_lowers_to_value_decidable_condition() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::NotEq {
                path: "feature.mode".to_string(),
                value: GuardValue::string("disabled"),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  mode: auto\n")),
    );

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "omitted default-not-disabled guard should activate the guarded host schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "mode": "disabled",
                    "host": 7
                }
            })
        ),
        "explicit disabled mode should leave the guarded-only host value unconstrained: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "mode": "custom",
                    "host": 7
                }
            })
        ),
        "explicit non-disabled mode should apply the guarded host schema: {schema}"
    );

    let schema_without_defaults =
        generate_values_schema(ValuesSchemaInput::new(&schema_signals, &NoopProvider));
    assert!(
        !schema_accepts_instance(
            &schema_without_defaults,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "missing not-equal guard path should activate the guarded branch: {schema_without_defaults}"
    );
}

#[test]
fn equal_false_guard_lowers_to_exact_default_aware_condition() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
                path: "feature.enabled".to_string(),
                value: GuardValue::Bool(false),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: false\n")),
    );

    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/if/anyOf/1/properties/enabled/enum"),
        want: Some(&serde_json::json!([false])),
        "exact false equality should lower to a typed enum, not truthiness: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "omitted default-false guard should activate the guarded host schema: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": false,
                    "host": 7
                }
            })
        ),
        "explicit false guard should activate the guarded host schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": true,
                    "host": 7
                }
            })
        ),
        "explicit true guard should leave the guarded-only host value unconstrained: {schema}"
    );
}

#[test]
fn equal_nil_guard_treats_absent_path_as_matching_nil() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
                path: "feature.tag".to_string(),
                value: GuardValue::Null,
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);
    let schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &NoopProvider));

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "missing path should satisfy `eq ... nil` and activate the guarded host schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "tag": "present",
                    "host": 7
                }
            })
        ),
        "present non-null path should deactivate the nil-guarded host schema: {schema}"
    );
}

#[test]
fn or_boolean_guards_lower_to_any_of_condition() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Or {
                paths: vec![
                    "feature.enabled".to_string(),
                    "global.featureEnabled".to_string(),
                ],
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider).with_values_yaml(Some(
            "feature:\n  enabled: false\nglobal:\n  featureEnabled: false\n",
        )),
    );

    sim_assert_eq!(
        have: schema.pointer("/allOf/0/if/anyOf/0/properties/feature/properties/enabled/$ref"),
        want: Some(&serde_json::json!("#/$defs/helm-truthy")),
        "disjunctions should lower to anyOf clauses: {schema}"
    );
    sim_assert_eq!(
        have: schema.pointer(
            "/allOf/0/if/anyOf/1/properties/global/properties/featureEnabled/$ref"
        ),
        want: Some(&serde_json::json!("#/$defs/helm-truthy")),
        "all boolean branches in the disjunction should be preserved: {schema}"
    );
    sim_assert_eq!(
        have: schema.pointer("/allOf/0/then/properties/feature/properties/host/type"),
        want: Some(&serde_json::json!("string")),
        "root-level disjunctions should still apply the guarded target schema: {schema}"
    );
}

#[test]
fn structural_any_of_guards_preserve_conjunctive_branches() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![ContractUse {
            source_expr: "feature.host".to_string(),
            path: YamlPath(vec!["data".to_string(), "host".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::AnyOf {
                alternatives: vec![
                    vec![
                        Guard::Truthy {
                            path: "feature.enabled".to_string(),
                        },
                        Guard::Eq {
                            path: "feature.mode".to_string(),
                            value: GuardValue::string("prod"),
                        },
                    ],
                    vec![Guard::Eq {
                        path: "global.mode".to_string(),
                        value: GuardValue::string("prod"),
                    }],
                ],
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        }]),
        &[("feature.enabled", "boolean"), ("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);
    let schema = generate_values_schema(ValuesSchemaInput::new(&schema_signals, &NoopProvider));

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": true,
                    "mode": "prod",
                    "host": 7
                }
            })
        ),
        "conjunctive alternative should activate only when all branch guards match: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": true,
                    "mode": "dev",
                    "host": 7
                }
            })
        ),
        "enabled=true alone must not activate the guarded host schema when mode differs: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "global": {
                    "mode": "prod"
                },
                "feature": {
                    "host": 7
                }
            })
        ),
        "second alternative should still activate the guarded host schema: {schema}"
    );
}

#[test]
fn multiple_guarded_variants_lower_branch_specific_target_schemas() {
    let schema_signals = schema_signals_for(vec![
        ContractUse {
            source_expr: "feature.value".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("name"),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
        ContractUse {
            source_expr: "feature.value".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("labels"),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
    ]);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("mode: name\nfeature:\n  value: example\n")),
    );

    let base_value = schema
        .pointer("/properties/feature/properties/value")
        .expect("feature.value base schema");
    assert!(
        base_value
            .as_object()
            .is_some_and(serde_json::Map::is_empty),
        "branch-specific variants should stay open on the base path and be enforced by overlays: {schema}"
    );

    let branches = schema
        .get("allOf")
        .and_then(Value::as_array)
        .expect("expected root conditionals for mode-switched target path");
    let name_branch = branches
        .iter()
        .find(|branch| branch_has_mode_enum(branch, "name"))
        .expect("expected mode=name branch");
    let labels_branch = branches
        .iter()
        .find(|branch| branch_has_mode_enum(branch, "labels"))
        .expect("expected mode=labels branch");

    sim_assert_eq!(
        have: name_branch.pointer("/then/properties/feature/properties/value/type"),
        want: Some(&serde_json::json!("string")),
        "name branch should keep the metadata.name string contract: {schema}"
    );
    let labels_value = labels_branch
        .pointer("/then/properties/feature/properties/value")
        .expect("labels branch value schema");
    assert!(
        schema_contains_type(labels_value, "object"),
        "labels branch should lower to an object contract: {schema}"
    );
    assert!(
        schema_contains_open_string_map(labels_value),
        "labels branch should preserve the metadata.labels string-map shape: {schema}"
    );
}

#[test]
fn inactive_scalar_branch_preserves_scalar_values_default_domain() {
    let schema_signals = schema_signals_for(vec![ContractUse {
        source_expr: "feature.value".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
            path: "mode".to_string(),
            value: GuardValue::string("enabled"),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("mode: disabled\nfeature:\n  value: false\n")),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "mode": "enabled",
                "feature": {
                    "value": false
                }
            })
        ),
        "inactive scalar branch should preserve scalar values.yaml input evidence: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "mode": "enabled",
                "feature": {
                    "value": {
                        "label": "example"
                    }
                }
            })
        ),
        "scalar branch must not become open to unrelated object input: {schema}"
    );
}

fn branch_has_mode_enum(branch: &Value, mode: &str) -> bool {
    branch.pointer("/if/properties/mode/enum") == Some(&serde_json::json!([mode]))
        || branch
            .pointer("/if/anyOf")
            .and_then(Value::as_array)
            .is_some_and(|clauses| {
                clauses.iter().any(|clause| {
                    clause.pointer("/properties/mode/enum") == Some(&serde_json::json!([mode]))
                })
            })
}

#[test]
fn guarded_branch_keeps_unconditional_base_schema_when_both_exist() {
    let schema_signals = schema_signals_for(vec![
        ContractUse {
            source_expr: "feature.value".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
        ContractUse {
            source_expr: "feature.value".to_string(),
            path: YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            kind: ValueKind::Fragment,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("labels"),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
    ]);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("mode: name\nfeature:\n  value: example\n")),
    );

    let base_value_schema = schema
        .pointer("/properties/feature/properties/value")
        .expect("feature.value base schema");
    assert!(
        schema_contains_type(base_value_schema, "string"),
        "the unconditional base string contract should remain on the main path: {schema}"
    );
    assert!(
        schema_contains_open_string_map(base_value_schema),
        "the guarded fragment branch should also stay visible on the base path when both variants exist: {schema}"
    );
    let guarded_value = schema
        .pointer("/allOf/0/then/properties/feature/properties/value")
        .expect("guarded feature.value schema");
    assert!(
        schema_contains_type(guarded_value, "object"),
        "the guarded branch should still reapply the fragment/object schema: {schema}"
    );
    assert!(
        schema_contains_open_string_map(guarded_value),
        "the guarded object branch should preserve metadata.labels string-map shape: {schema}"
    );
}

#[test]
fn non_boolean_truthy_guard_lowers_to_typed_condition_overlay() {
    let schema_signals = schema_signals_for(vec![ContractUse {
        source_expr: "feature.host".to_string(),
        path: YamlPath(vec!["data".to_string(), "host".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "mode".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("mode: prod\nfeature:\n  host: example\n")),
    );

    // The truthiness condition encoding is type-generic, so a string-valued
    // guard lowers like a boolean flag: typing moves under the condition and
    // the base stays open for the values the guard never reads.
    sim_assert_eq!(
        have: schema.pointer("/properties/feature/properties/host"),
        want: Some(&serde_json::json!({})),
        "guarded-only host typing must leave the base open: {schema}"
    );
    let guarded_host = schema
        .pointer("/allOf/0/then/properties/feature/properties/host")
        .expect("guarded host overlay present");
    assert!(
        permits_type(guarded_host, "string"),
        "the mode-guarded branch should carry the string typing: {schema}"
    );
    // The `if` uses the default-aware form (absent mode falls back to the
    // truthy declared default), so the mode key sits inside its anyOf arms.
    assert!(
        schema
            .pointer("/allOf/0/if/anyOf/1/properties/mode")
            .is_some(),
        "the overlay must key on the mode condition: {schema}"
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
    define_index.add_file_source("helpers.tpl", helpers);
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize();
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
    define_index.add_file_source("helpers.tpl", helpers);
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize();
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
fn helper_local_printf_aliases_flow_without_input_typing() {
    let helpers = indoc! {r#"
        {{- define "common.image" -}}
        {{- $registryName := .imageRoot.registry -}}
        {{- $repositoryName := .imageRoot.repository -}}
        {{- $tag := default .imageRoot.version .imageRoot.tag | toString -}}
        {{- if $registryName -}}
          {{- printf "%s/%s:%s" $registryName $repositoryName $tag -}}
        {{- else -}}
          {{- printf "%s:%s" $repositoryName $tag -}}
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
                  image: {{ template "common.image" (dict "imageRoot" .Values.image) }}
    "#};

    // printf renders any argument and `toString` totally stringifies the
    // tag, so the helper-local aliases must not string-type the image
    // inputs: a numeric `tag: 1.25` renders fine and must validate.
    let hints = type_hints_for(parse_ir_with_helpers(src, helpers));
    for path in ["image.registry", "image.repository", "image.tag"] {
        assert!(
            hints
                .get(path)
                .is_none_or(|types| !types.contains("string")),
            "printf/toString must not bind a string contract on {path}, got {hints:?}"
        );
    }
    let values_yaml = indoc! {"
        image:
          registry: docker.io
          repository: bitnami/app
          tag: latest
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let instance = serde_json::json!({
        "image": { "registry": "docker.io", "repository": "bitnami/app", "tag": 1.25 }
    });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a numeric image tag renders through toString/printf: {schema}"
    );
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
    define_index.add_file_source("helpers.tpl", helpers);
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    let image = schema.pointer("/properties/image").expect("image present");
    for property in ["registry", "repository", "tag"] {
        assert!(
            object_variant_with_property(image, property).is_some(),
            "wrapper helper should preserve image.{property} output, got {image}; ir={ir:?}"
        );
    }
}

#[test]
fn wrapper_helper_digest_branch_keeps_explicit_null_nullable() {
    let helpers = indoc! {r#"
        {{- define "common.images.image" -}}
        {{- $registryName := .imageRoot.registry -}}
        {{- $repositoryName := .imageRoot.repository -}}
        {{- $separator := ":" -}}
        {{- $termination := .imageRoot.tag | toString -}}
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
          digest:
        global: {}
    "};

    let mut define_index = DefineIndex::new();
    define_index.add_file_source("helpers.tpl", helpers);
    let ir = SymbolicIrContext::new(&define_index).generate_contract_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    // The digest only renders through `toString` (a total stringification),
    // so the schema must accept the explicit-null default, a digest string,
    // and any other renderable value.
    for digest in [
        serde_json::json!("sha256:abc"),
        serde_json::json!(null),
        serde_json::json!(7),
    ] {
        let instance = serde_json::json!({
            "image": {
                "registry": "docker.io",
                "repository": "example/app",
                "tag": "latest",
                "digest": digest,
            },
            "global": {},
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "digest renders through toString, so {instance} must validate: {schema}"
        );
    }
}

#[test]
fn selector_chain_and_indexed_default_do_not_leak_parent_object_as_scalar_use() {
    let src = indoc! {r#"
        {{- $airtypeVersion := ((.Values.appVersions).airtype).global -}}
        {{- $apiVersion := index ((.Values.appVersions).airtype | default dict ) "api" -}}
        {{- $appVersion := $apiVersion | default $airtypeVersion | default .Chart.AppVersion -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          version: {{ $appVersion | quote }}
    "#};
    let ir = parse_ir(src).finalize();
    let uses = ir
        .uses()
        .iter()
        .map(|use_| use_.source_expr.as_str())
        .collect::<Vec<_>>();

    assert!(
        uses.contains(&"appVersions.airtype.global"),
        "expected descendant appVersions.airtype.global use, got {uses:?}"
    );
    assert!(
        uses.contains(&"appVersions.airtype.api"),
        "expected descendant appVersions.airtype.api use, got {uses:?}"
    );
    assert!(
        !uses.contains(&"appVersions.airtype"),
        "parent object should not be collapsed into a scalar render use, got {uses:?}"
    );
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    sim_assert_eq!(
        have: pod_labels
            .get("additionalProperties")
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("type"))
            .and_then(Value::as_str),
        want: Some("string"),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let annotations = schema
        .pointer("/properties/annotations")
        .expect("annotations present");
    assert!(
        schema_contains_open_string_map(annotations),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    sim_assert_eq!(
        have: environment.get("type").and_then(Value::as_str),
        want: Some("object"),
        "environment should stay an object-valued input, got {environment}"
    );
    sim_assert_eq!(
        have: environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    sim_assert_eq!(
        have: environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let access_modes = schema
        .pointer("/properties/accessModes")
        .expect("accessModes present");
    sim_assert_eq!(
        have: access_modes.get("type").and_then(Value::as_str),
        want: Some("array"),
        "accessModes should be an array, got {access_modes}"
    );
    sim_assert_eq!(
        have: access_modes.get("items"),
        want: Some(&serde_json::json!({})),
        "quoted items render any input through strval, so the provider string typing must not flow back, got {access_modes}"
    );
    assert!(
        access_modes
            .pointer("/description")
            .and_then(Value::as_str)
            .is_some(),
        "accessModes should keep the provider description, got {access_modes}"
    );
    sim_assert_eq!(
        have: access_modes
            .pointer("/x-kubernetes-list-type")
            .and_then(Value::as_str),
        want: Some("atomic"),
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
                  - path: {{ . }}
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let host_paths = schema
        .pointer("/properties/hosts/items/properties/paths")
        .expect("hosts[].paths present");
    sim_assert_eq!(
        have: host_paths.get("type").and_then(Value::as_str),
        want: Some("array"),
        "hosts[].paths should stay an array input, got {host_paths}"
    );
    let path_items = host_paths.get("items").expect("hosts[].paths items");
    assert!(
        schema_contains_type(path_items, "string"),
        "hosts[].paths items should retain the provider string branch, got {host_paths}"
    );
    assert!(
        !schema_contains_type(path_items, "object"),
        "quoted scalar inputs must not widen to object items, got {host_paths}"
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
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let hosts = schema.pointer("/properties/hosts").expect("hosts present");
    sim_assert_eq!(
        have: hosts.get("type").and_then(Value::as_str),
        want: Some("array"),
        "hosts should stay an array, got {hosts}"
    );
    sim_assert_eq!(
        have: hosts.pointer("/items/type").and_then(Value::as_str),
        want: Some("string"),
        "hosts items should stay strings, got {hosts}"
    );
    assert!(
        hosts.pointer("/items/properties/Chart").is_none(),
        "root helper fields must not be projected onto range items, got {hosts}"
    );
}

#[test]
fn map_entry_range_over_values_path_keeps_object_map_schema() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
        {{- range $key, $value := .Values.controller.config }}
          {{- $key | nindent 2 }}: {{ tpl (toString $value) $ | quote }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        controller:
          config: {}
    "};
    let contract = parse_ir(src);
    let signals = schema_signals_for(&contract);
    let facts = signals
        .evidence_for("controller.config")
        .map(|evidence| evidence.facts)
        .expect("controller.config fact present");
    assert!(
        facts.is_ranged_source,
        "range header should mark controller.config as a ranged source, facts={facts:#?}"
    );
    assert!(
        facts.used_as_fragment,
        "map-entry range should mark controller.config as a rendered fragment, facts={facts:#?}"
    );

    let schema = schema_for_values_yaml(&contract, Some(values_yaml));
    let config = schema
        .pointer("/properties/controller/properties/config")
        .expect("controller.config schema present");
    assert!(
        schema_contains_type(config, "object"),
        "controller.config should retain an object-valued branch, got {config}"
    );
    assert!(
        !schema_contains_type(config, "string"),
        "controller.config should not collapse to a scalar string branch, got {config}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({"controller": {"config": {"allow-snippet-annotations": "true"}}}),
        ),
        "controller.config should accept arbitrary ConfigMap data keys: {schema:#}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({"controller": {"config": "allow-snippet-annotations=true"}}),
        ),
        "controller.config should not collapse to a scalar string: {schema:#}"
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
        condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
        resource: Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
        provenance: Vec::new(),
        has_string_contract: false,
    }];
    let values_yaml = indoc! {"
        image:
          pullSecrets: []
    "};

    let schema = schema_for_values_yaml(&uses, Some(values_yaml));
    let pull_secrets = schema
        .pointer("/properties/image/properties/pullSecrets")
        .expect("image.pullSecrets present");

    sim_assert_eq!(
        have: pull_secrets.get("type").and_then(Value::as_str),
        want: Some("array"),
        "wildcard source path should create an array schema, got {pull_secrets}"
    );
    assert!(
        pull_secrets.get("anyOf").is_none(),
        "wildcard source path should not create an empty-object variant, got {pull_secrets}"
    );
    sim_assert_eq!(
        have: pull_secrets.pointer("/items/type").and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let service_account = schema
        .pointer("/properties/serviceAccount")
        .expect("serviceAccount present");
    sim_assert_eq!(
        have: service_account.get("type").and_then(Value::as_str),
        want: Some("object"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "create": true,
                    "name": null
                }
            })
        ),
        "defaulted helper-bound serviceAccount.name should allow null on the create=true branch: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "create": false,
                    "name": 7
                }
            })
        ),
        "defaulted helper-bound serviceAccount.name should remain string-like on the create=false branch: {schema}"
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let image = schema.pointer("/properties/image").expect("image present");
    sim_assert_eq!(
        have: image.get("type").and_then(Value::as_str),
        want: Some("object"),
        "image should stay object-valued, got {image}"
    );
    assert!(
        image.get("anyOf").is_none(),
        "image should not widen to object-or-string, got {image}"
    );
    // registry renders only through printf, which formats any argument, so
    // its slot stays untyped.
    sim_assert_eq!(
        have: image.pointer("/properties/registry"),
        want: Some(&serde_json::json!({})),
        "image.registry renders through printf and stays untyped, got {image}"
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
    define_index.add_file_source("helpers.tpl", helpers);
    let ir = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize();
    let schema = schema_for_values_yaml(ir.uses(), Some(values_yaml));

    // nameOverride's typing lives under the `not(fullnameOverride)` branch
    // where the chart actually reads it.
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "fullnameOverride": "", "nameOverride": "" })
        ),
        "defaulted nameOverride should accept the chart's empty-string sentinel, got {schema}; ir={ir:?}"
    );
    // The fullname flows through printf (formats any argument), so
    // nameOverride stays untyped — crucially, it must NOT inherit the
    // Ingress backend OBJECT schema its rendered text lands in.
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    assert!(
        !schema_contains_type(name, "object"),
        "scalar helper input should not inherit the Ingress backend object schema, got {name}; ir={ir:?}"
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for pointer in ["/properties/image", "/properties/clientImage"] {
        let image = schema.pointer(pointer).expect("image root present");
        assert!(
            image
                .get("required")
                .and_then(Value::as_array)
                .is_none_or(|required| !required.iter().any(|key| key == "containers")),
            "{pointer} should not inherit PodSpec.required from imagePullSecrets, got {image}"
        );
    }
    // image.registry renders only through printf, which formats any
    // argument, so its slot stays untyped; clientImage.registry never
    // reaches printf, so its declared string typing stands.
    sim_assert_eq!(
        have: schema.pointer("/properties/image/properties/registry"),
        want: Some(&serde_json::json!({})),
        "image.registry renders through printf and stays untyped, got {schema}"
    );
    sim_assert_eq!(
        have: schema
            .pointer("/properties/clientImage/properties/registry/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "clientImage.registry keeps its declared string typing, got {schema}"
    );
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-gated helper output should still accept null, got {fullname}"
    );
    assert!(
        permits_type(fullname, "string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-gated template helper output should still accept null, got {fullname}"
    );
    assert!(
        permits_type(fullname, "string"),
        "template calls in scalar slots should propagate helper string types, got {fullname}"
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

    // Both overrides render only through printf, which formats any
    // argument, so their slots stay untyped: the declared null defaults and
    // every other renderable value must validate.
    for instance in [
        serde_json::json!({ "fullnameOverride": null, "nameOverride": null }),
        serde_json::json!({ "fullnameOverride": "name", "nameOverride": 7 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "printf formats any override value: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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

    // Both overrides render only through printf, which formats any
    // argument, so their slots stay untyped: the declared null defaults and
    // every other renderable value must validate.
    for instance in [
        serde_json::json!({ "fullnameOverride": null, "nameOverride": null }),
        serde_json::json!({ "fullnameOverride": "name", "nameOverride": 7 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "printf formats any override value: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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
    sim_assert_eq!(have: schema, want: expected);
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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
    sim_assert_eq!(have: schema, want: expected);
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "global": {
                "type": "object",
                // Helm shares `global` across the chart tree; the namespace
                // stays open to keys only other charts read.
                "additionalProperties": {},
                "properties": {
                    "storageClass": {
                        "anyOf": [
                            {
                                "const": null
                            },
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
                                "const": null
                            },
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
    sim_assert_eq!(have: schema, want: expected);
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
    sim_assert_eq!(have: schema, want: expected);
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
    sim_assert_eq!(
        have: annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    sim_assert_eq!(
        have: schema
            .pointer("/properties/globalImage/properties/tag/type")
            .and_then(Value::as_str),
        want: Some("string"),
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
    sim_assert_eq!(
        have: empty_variant
            .get("additionalProperties")
            .and_then(Value::as_bool),
        want: Some(false),
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
    let schema_signals = SymbolicIrContext::new(&define_index)
        .generate_contract_ir(src)
        .finalize()
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let config = schema
        .pointer("/properties/config")
        .expect("config present");
    sim_assert_eq!(
        have: config.get("type").and_then(Value::as_str),
        want: Some("object"),
        "guard-only empty-map default should keep the values.yaml object evidence, got {config}"
    );
    sim_assert_eq!(
        have: config
            .get("additionalProperties")
            .and_then(Value::as_object)
            .map(serde_json::Map::len),
        want: Some(0),
        "guard-only empty-map default should remain open, got {config}"
    );
    assert!(
        config.get("anyOf").is_none(),
        "guard-only empty-map default should not become an exact-empty-or-boolean union, got {config}"
    );
}

/// The temporal chart declares `imagePullSecrets: {}` and splices it whole
/// (`with` + `toYaml`) into a Kubernetes LIST position. The shipped empty-map
/// off-state AND the real list form must both validate; the luup3 gate caught
/// a round-2 state where the list typing squeezed out the declared default.
#[test]
fn with_guarded_whole_splice_accepts_empty_map_default_and_list_form() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: repro
        spec:
          template:
            spec:
              {{- with .Values.imagePullSecrets }}
              imagePullSecrets:
              {{- toYaml . | nindent 8 }}
              {{- end }}
              containers:
                - name: app
                  image: busybox
    "#};
    let values_yaml = indoc! {"
        imagePullSecrets: {}
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "imagePullSecrets": {} })),
        "the declared empty-map off-state must stay accepted: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "imagePullSecrets": [{ "name": "regcred" }] })
        ),
        "the rendered list form must stay accepted: {schema}"
    );
}

/// An UNDECLARED map the chart itself iterates (istiod's `env` has no
/// values.yaml default) is user-populated; a typed member guard must not
/// close it.
#[test]
fn undeclared_self_ranged_map_stays_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          {{- if not .Values.env.FORCE }}
          forced: "no"
          {{- end }}
          {{- range $key, $val := .Values.env }}
          {{ $key }}: {{ $val | quote }}
          {{- end }}
    "#};

    let schema = schema_for_values_yaml(parse_ir(src), None);
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "env": { "ANY_KEY": "x" } })),
        "user-populated entries of an undeclared ranged map must stay accepted: {schema}"
    );
}

/// A declared-empty map spliced whole through `toYaml` (cert-manager's
/// `config`) is user-populated even when guard reads probe typed members;
/// the open arm of its off-state union hosts the members without closing.
#[test]
fn serialized_empty_map_union_keeps_open_arm_for_members() {
    let src = indoc! {r#"
        {{- if .Values.config.apiVersion }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: versioned
        data:
          v: {{ .Values.config.apiVersion | quote }}
        {{- end }}
        ---
        {{- if .Values.config }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          config.yaml: |
        {{ toYaml .Values.config | indent 4 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        config: {}
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for instance in [
        serde_json::json!({ "config": {} }),
        serde_json::json!({ "config": { "userField": true } }),
        serde_json::json!({ "config": { "apiVersion": "controller.config/v1alpha1" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "serialized user-populated map must accept {instance}: {schema}"
        );
    }
}

/// A guard probing one literal member of a user-populated map (datadog's
/// `envDict.HELM_FORCE_RENDER` pattern) must not close the map: the map is
/// declared `{}` and consumed by a helper that ranges over its entries, so
/// arbitrary user keys stay accepted alongside the probed member.
#[test]
fn member_probe_keeps_helper_ranged_empty_map_open() {
    let helpers = indoc! {r#"
        {{- define "repro.entries" -}}
        {{- range $key, $value := . }}
        - name: {{ $key }}
          value: {{ $value | quote }}
        {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if not .Values.envDict.FORCE }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: guarded
        data:
          on: "true"
        {{- end }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          entries: |
        {{- include "repro.entries" .Values.envDict | indent 4 }}
    "#};
    let values_yaml = indoc! {"
        envDict: {}
    "};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let env_dict = schema
        .pointer("/properties/envDict")
        .expect("envDict present");
    assert!(
        env_dict.get("additionalProperties") != Some(&Value::Bool(false)),
        "member probe must not close the user-populated map, got {env_dict}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "envDict": { "USER_KEY": "value" } })
        ),
        "user-populated entries must stay accepted, got {env_dict}"
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let namespace = schema
        .pointer("/properties/networkPolicies/properties/ingressController/properties/namespace")
        .expect("namespace present");
    sim_assert_eq!(
        have: namespace.get("type").and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let name = schema
        .pointer("/properties/account/properties/name")
        .expect("account.name present");

    sim_assert_eq!(
        have: name.get("type").and_then(Value::as_str),
        want: Some("string"),
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
    sim_assert_eq!(
        have: schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let name = schema
        .pointer("/properties/alertmanager/properties/serviceAccount/properties/name")
        .expect("alertmanager.serviceAccount.name present");
    assert!(
        name.as_object().is_some_and(serde_json::Map::is_empty),
        "defaulted helper serviceAccount.name should stay open at the guarded-only base path, got {name}; schema={schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "create": true,
                        "name": null
                    }
                }
            })
        ),
        "create=true defaulted serviceAccount.name should validate through branch-aware schema: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "create": false,
                        "name": null
                    }
                }
            })
        ),
        "create=false defaulted serviceAccount.name should also validate through the else branch: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "name": "alertmanager",
                    "serviceAccount": {
                        "create": false,
                        "name": 7
                    }
                }
            })
        ),
        "serviceAccount.name should not collapse to unconstrained schema on the else branch: {schema}"
    );
}

#[test]
fn parent_values_seed_does_not_override_exact_defaulted_child_path() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "signoz-otel-gateway.serviceAccount.name".to_string(),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![
            Guard::Truthy {
                path: "signoz-otel-gateway.serviceAccount.create".to_string(),
            },
            Guard::Default {
                path: "signoz-otel-gateway.serviceAccount.name".to_string(),
            },
        ]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);
    contract.push_pathless_scalar("signoz-otel-gateway");
    contract.add_type_hint("signoz-otel-gateway.serviceAccount.name", "string");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_yaml(
            Some(indoc! {r#"
                signoz-otel-gateway:
                  serviceAccount:
                    create: true
                    name: ""
            "#}),
        ),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "serviceAccount": {
                        "create": true,
                        "name": null
                    }
                }
            })
        ),
        "exact helper-defaulted child path should widen the parent values seed, got {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "signoz-otel-gateway": {
                    "serviceAccount": {
                        "create": true,
                        "name": 7
                    }
                }
            })
        ),
        "exact child path should still preserve string-like helper metadata shape, got {schema}"
    );
}

#[test]
fn guarded_fragment_parent_seed_stays_open_after_guard_child_insert() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "clickhouse.securityContext".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "securityContext".to_string(),
        ]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "clickhouse.securityContext.enabled".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);
    contract.push_pathless_scalar("clickhouse");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_yaml(
            Some(indoc! {r#"
                clickhouse:
                  securityContext:
                    enabled: true
                    fsGroup: 101
                    runAsUser: 1001
            "#}),
        ),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "clickhouse": {
                    "securityContext": {
                        "enabled": true,
                        "fsGroup": 101,
                        "runAsUser": 1001
                    }
                }
            })
        ),
        "guarded fragment base should stay open after inserting guard descendants, got {schema}"
    );
}

#[test]
fn referenced_empty_string_child_survives_parent_pruning() {
    let mut contract = ContractIr::from_contract_uses(vec![
        ContractUse {
            source_expr: "signoz.smtpVars.existingSecret.fromKey".to_string(),
            path: YamlPath(vec!["env[*]".to_string(), "valueFrom".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "signoz.smtpVars.enabled".to_string(),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
        ContractUse {
            source_expr: "signoz.smtpVars.existingSecret.name".to_string(),
            path: YamlPath(vec![
                "env[*]".to_string(),
                "valueFrom".to_string(),
                "secretKeyRef".to_string(),
                "name".to_string(),
            ]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: "signoz.smtpVars.enabled".to_string(),
            }]),
            resource: None,
            provenance: Vec::new(),
            has_string_contract: false,
        },
    ]);
    contract.push_pathless_scalar("signoz");
    contract.add_type_hint("signoz.smtpVars.enabled", "boolean");

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_yaml(
            Some(indoc! {r#"
                signoz:
                  smtpVars:
                    enabled: false
                    existingSecret:
                      name: ""
                      fromKey: ""
            "#}),
        ),
    );

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "smtpVars": {
                        "enabled": true,
                        "existingSecret": {
                            "name": 7
                        }
                    }
                }
            })
        ),
        "active guarded child path should keep its own values.yaml scalar schema after parent pruning, got {schema}",
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "signoz": {
                    "smtpVars": {
                        "enabled": false,
                        "existingSecret": {
                            "name": 7
                        }
                    }
                }
            })
        ),
        "inactive guarded child path should not be constrained by branch-local evidence, got {schema}",
    );
}

#[test]
fn guarded_array_fragment_parent_seed_stays_array_shaped() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "alertmanager.tolerations".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "tolerations".to_string(),
        ]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "alertmanager.enabled".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);
    contract.push_pathless_scalar("alertmanager");
    contract.add_type_hint("alertmanager.enabled", "boolean");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_yaml(
            Some(indoc! {"
                alertmanager:
                  enabled: true
                  tolerations: []
            "}),
        ),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "alertmanager": {
                    "enabled": true,
                    "tolerations": []
                }
            })
        ),
        "guarded array fragment base should preserve array shape, got {schema}"
    );
}

#[test]
fn guarded_null_object_fragment_parent_seed_preserves_null_default() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse {
        source_expr: "clickhouse.clickhouseOperator.configs.confdFiles".to_string(),
        path: YamlPath(vec!["data".to_string()]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: "clickhouse.enabled".to_string(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    }]);
    contract.push_pathless_scalar("clickhouse");
    contract.add_type_hint("clickhouse.enabled", "boolean");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_yaml(
            Some(indoc! {"
                clickhouse:
                  enabled: true
                  clickhouseOperator:
                    configs:
                      confdFiles:
            "}),
        ),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "clickhouse": {
                    "enabled": true,
                    "clickhouseOperator": {
                        "configs": {
                            "confdFiles": null
                        }
                    }
                }
            })
        ),
        "guarded object fragment base should preserve explicit null defaults, got {schema}"
    );
}

#[test]
fn helper_default_with_nonliteral_string_fallback_stays_nullable_string() {
    let helpers = indoc! {r#"
        {{- define "service.fullname" -}}
        {{- printf "%s-%s" .Release.Name (.Values.service.name | default "svc") | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
        {{- define "service.accountName" -}}
        {{ default (include "service.fullname" .) .Values.serviceAccount.name }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: {{ include "service.accountName" . }}
    "#};
    let values_yaml = indoc! {r#"
        service:
          name:
        serviceAccount:
          name:
    "#};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    let name = schema
        .pointer("/properties/serviceAccount/properties/name")
        .expect("serviceAccount.name present");
    assert!(
        schema_contains_type(name, "null"),
        "helper default should preserve the explicit null default, got {name}"
    );
    assert!(
        schema_contains_type(name, "string"),
        "non-literal include/printf fallback should still infer string, got {name}"
    );
}

#[test]
fn self_default_guarded_branch_lowers_without_losing_else_branch_precision() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![
            ContractUse {
                source_expr: "serviceAccount.name".to_string(),
                path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                kind: ValueKind::Scalar,
                condition: helm_schema_core::GuardDnf::from_guards(vec![
                    Guard::Truthy {
                        path: "serviceAccount.create".to_string(),
                    },
                    Guard::Default {
                        path: "serviceAccount.name".to_string(),
                    },
                ]),
                resource: None,
                provenance: Vec::new(),
                has_string_contract: false,
            },
            ContractUse {
                source_expr: "serviceAccount.name".to_string(),
                path: YamlPath(vec![
                    "spec".to_string(),
                    "template".to_string(),
                    "spec".to_string(),
                    "serviceAccountName".to_string(),
                ]),
                kind: ValueKind::Scalar,
                condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Not {
                    path: "serviceAccount.create".to_string(),
                }]),
                resource: None,
                provenance: Vec::new(),
                has_string_contract: false,
            },
        ]),
        &[("serviceAccount.name", "string")],
    );
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider)
            .with_values_yaml(Some("serviceAccount:\n  create: true\n  name:\n")),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "create": true,
                    "name": null
                }
            })
        ),
        "create=true branch should preserve null-tolerant helper default semantics: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "serviceAccount": {
                    "create": false,
                    "name": null
                }
            })
        ),
        "create=false branch should keep the raw string requirement and reject null: {schema}"
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
            - host: {{ .host }}
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
        property_schema_with_type_exists(&schema, "className", "string"),
        "with-bound dot helper call should propagate ingress.className, got {schema}"
    );
    assert!(
        property_schema_with_type_exists(&schema, "className", "string"),
        "with-bound dot helper call should propagate ingress.className as string-like, got {schema}"
    );
    assert!(
        property_schema_contains_open_string_map(&schema, "annotations"),
        "with-bound dot helper call should propagate ingress.annotations, got {schema}"
    );
    assert!(
        property_schema_with_type_exists(&schema, "secretName", "string"),
        "with-bound dot helper call should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert!(
        property_schema_with_type_exists(&schema, "host", "string"),
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

    assert!(
        property_schema_with_type_exists(&schema, "className", "string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    let pod_annotations = schema
        .pointer("/properties/admintools/properties/podAnnotations")
        .expect("admintools.podAnnotations present");
    sim_assert_eq!(
        have: pod_annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "admintools.podAnnotations should stay an open string map, got {pod_annotations}"
    );

    let pod_labels = schema
        .pointer("/properties/admintools/properties/podLabels")
        .expect("admintools.podLabels present");
    sim_assert_eq!(
        have: pod_labels
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "admintools.podLabels should stay an open string map, got {pod_labels}"
    );

    let additional_annotations = schema
        .pointer("/properties/additionalAnnotations")
        .expect("additionalAnnotations present");
    sim_assert_eq!(
        have: additional_annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "additionalAnnotations should stay an open string map, got {additional_annotations}"
    );

    let additional_labels = schema
        .pointer("/properties/additionalLabels")
        .expect("additionalLabels present");
    sim_assert_eq!(
        have: additional_labels
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, &helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
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
    // The whole render is gated on `if .Values.livenessProbe.enabled`, so the
    // Probe typing must live under that condition, not at the base.
    assert!(
        !probe
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| properties.contains_key("initialDelaySeconds")),
        "Probe fields must be guard-scoped, not unconditional, got {probe}"
    );
    let guard = probe
        .pointer("/allOf/0/if")
        .expect("probe overlay guard present");
    assert!(
        guard.to_string().contains("enabled"),
        "probe overlay must key on the enabled guard, got {guard}"
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, &helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let source_ranges = schema
        .pointer("/properties/service/properties/loadBalancerSourceRanges")
        .expect("service.loadBalancerSourceRanges present");

    sim_assert_eq!(
        have: source_ranges.get("type").and_then(Value::as_str),
        want: Some("array"),
        "loadBalancerSourceRanges should remain array-valued, got {source_ranges}"
    );
    sim_assert_eq!(
        have: source_ranges.pointer("/items/type").and_then(Value::as_str),
        want: Some("string"),
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

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, &helpers), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let leader_election = schema
        .pointer("/properties/leaderElection")
        .expect("leaderElection present");
    let leader_election_object = object_variant_with_property(leader_election, "leaseDuration")
        .expect("leaderElection object branch with leaseDuration");

    assert!(
        permits_type(
            leader_election_object
                .pointer("/properties/leaseDuration")
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
            - host: {{ .host }}
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

    assert!(
        property_schema_contains_open_string_map(&schema, "annotations"),
        "realistic common.ingress helper should keep ingress.annotations open, got {schema}"
    );
    assert!(
        property_schema_with_type_exists(&schema, "className", "string"),
        "realistic common.ingress helper should propagate ingress.className, got {schema}"
    );
    assert!(
        property_schema_with_type_exists(&schema, "secretName", "string"),
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

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

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
        resource: ResourceRef::concrete("apps/v1".to_string(), "Deployment".to_string()),
        is_self_range_collection: false,
    };

    let schema = provider
        .schema_fragment_for_use(&use_)
        .expect("provider schema for container resources")
        .into_schema();

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
    let hints = type_hints_for(parse_ir(src));
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas.contains("integer"),
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
    let hints = type_hints_for(parse_ir(src));
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
    let hints = type_hints_for(parse_ir(src));
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
    let hints = type_hints_for(parse_ir(src));
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
    let hints = type_hints_for(parse_ir(src));
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
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Default {
            path: "image.tag".into(),
        }]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    };
    let bare = ContractUse {
        source_expr: "image.tag".into(),
        path: YamlPath(vec!["data".into(), "bare".into()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![]),
        resource: None,
        provenance: Vec::new(),
        has_string_contract: false,
    };

    let signals = schema_signals_for(vec![guarded, bare]);
    let null_paths = signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.is_nullable)
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
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
    let hints = type_hints_for(parse_ir_with_helpers(
        r#"
        name: {{ include "test.serviceAccountName" . }}
        "#,
        helper_src,
    ));
    assert!(
        hints.contains_key("serviceAccount.name"),
        "expected hint for serviceAccount.name in helper, got {hints:?}"
    );
}
