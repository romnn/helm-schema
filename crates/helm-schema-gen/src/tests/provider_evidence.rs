use test_util::prelude::sim_assert_eq;

use super::*;

/// A conditional branch rendering a DIRECT scalar hole into a
/// provider-required field (a Service `port`) backprojects presence and
/// non-nullability of the source leaf under the branch's guards: Helm
/// renders a missing or null source as an explicit null the provider
/// rejects. The dormant arm stays open, and a `default` fallback abstains
/// (absence renders the fallback instead).
#[test]
fn provider_required_field_requires_direct_source_leaf() {
    let guarded = indoc! {r#"
        {{- if .Values.svc.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
          - port: {{ .Values.svc.port }}
            name: http
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        svc:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir(guarded), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "svc": { "enabled": false } }),
            true,
            "dormant branch stays open",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true, "port": 80 } }),
            true,
            "present integer port renders a valid Service",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true } }),
            false,
            "missing port renders a provider-invalid null",
        ),
        (
            serde_json::json!({ "svc": { "enabled": true, "port": null } }),
            false,
            "explicit null port renders a provider-invalid null",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }

    let defaulted = indoc! {r#"
        {{- if .Values.svc.enabled }}
        apiVersion: v1
        kind: Service
        metadata:
          name: probe
        spec:
          ports:
          - port: {{ .Values.svc.port | default 9090 }}
            name: http
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(defaulted), Some(values_yaml));
    let instance = serde_json::json!({ "svc": { "enabled": true, "port": null } });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "a default fallback renders on absence, so the source stays optional; schema={schema}"
    );
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
        schema_signals
            .evidence_for("serviceMonitor.metricRelabelings")
            .is_some_and(|evidence| evidence.provider_schema_uses.is_empty()),
        "metricRelabelings provider evidence should not escape its render guard"
    );
    assert!(
        resolved_metric_relabelings
            .provider_schema_candidate
            .is_none(),
        "metricRelabelings should not have an unconditional provider candidate"
    );
    let overlay = schema_signals
        .evidence_for("serviceMonitor.metricRelabelings")
        .and_then(|evidence| evidence.conditional_overlays.first())
        .expect("metricRelabelings conditional overlay");
    assert!(
        !overlay.evidence.provider_schema_uses.is_empty(),
        "metricRelabelings conditional overlay should keep CRD provider schema uses"
    );
    assert!(
        !overlay.preserve_base_schema,
        "guarded-only metricRelabelings evidence should not preserve a typed base: {overlay:#?}"
    );
    let resolved_overlay = crate::path_resolver::PathSchemaResolver::resolve_single_path_evidence(
        &overlay
            .evidence
            .as_path_evidence("serviceMonitor.metricRelabelings"),
        &provider,
    );
    sim_assert_eq!(
        have: resolved_overlay.schema.pointer("/anyOf/0/type").and_then(Value::as_str),
        want: Some("array"),
        "resolved overlay schema should stay array-shaped: {}",
        resolved_overlay.schema
    );
    sim_assert_eq!(
        have: resolved_overlay
            .schema
            .pointer("/anyOf/0/items/properties/action/type")
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
    for (instance, want, label) in [
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": "replace" }]
                }
            }),
            true,
            "enabled provider-shaped relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": 7 }]
                }
            }),
            false,
            "enabled invalid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": false,
                    "metricRelabelings": 7
                }
            }),
            true,
            "disabled unconstrained relabeling",
        ),
    ] {
        assert!(
            schema_accepts_instance(&generated, &instance) == want,
            "{label}: instance={instance}; schema={generated}"
        );
    }
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
            .pointer("/anyOf/0/type")
            .and_then(Value::as_str),
        want: Some("array"),
        "extraEnvs should stay array-shaped: {}",
        resolved_extra_envs.schema
    );
    sim_assert_eq!(
        have: resolved_extra_envs
            .schema
            .pointer("/anyOf/0/items/properties/name/type")
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
        have: extra_envs.pointer("/anyOf/0/type").and_then(Value::as_str),
        want: Some("array"),
        "generated extraEnvs should stay array-shaped: {extra_envs}"
    );
    sim_assert_eq!(
        have: extra_envs
            .pointer("/anyOf/0/items/properties/name/type")
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

    for (instance, want, label) in [
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": "replace" }]
                }
            }),
            true,
            "enabled valid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": true,
                    "metricRelabelings": [{ "action": 7 }]
                }
            }),
            false,
            "enabled invalid relabeling",
        ),
        (
            serde_json::json!({
                "serviceMonitor": {
                    "enabled": false,
                    "metricRelabelings": 7
                }
            }),
            true,
            "disabled unconstrained relabeling",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
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
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "additionalProperties": false
        }
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
        !schema_has_format(password, "byte"),
        "pre-encoded helper input must not inherit rendered Secret.data byte format, got {password}; schema={schema}"
    );
    for (instance, want, label) in [
        (serde_json::json!({ "password": null }), true, "null"),
        (serde_json::json!({ "password": {} }), true, "empty object"),
        (serde_json::json!({ "password": "secret" }), true, "string"),
        (serde_json::json!({ "password": 7 }), false, "truthy number"),
        (
            serde_json::json!({ "password": { "bad": true } }),
            false,
            "truthy object",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "encoded helper input {label}: instance={instance}; schema={schema}"
        );
    }

    let raw = schema
        .pointer("/properties/rawSecretData")
        .expect("rawSecretData present");
    assert!(
        schema_has_format(raw, "byte"),
        "unencoded sibling helper input should still inherit Secret.data byte format, got {raw}; schema={schema}"
    );
}
