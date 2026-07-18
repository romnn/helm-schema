use test_util::prelude::sim_assert_eq;

use super::*;

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

    for (namespace, want, label) in [
        (serde_json::json!("ingress-nginx"), true, "string"),
        (serde_json::json!(7), true, "number"),
        (serde_json::json!(false), true, "boolean"),
        (serde_json::json!({ "bad": true }), true, "object"),
    ] {
        let instance = serde_json::json!({
            "networkPolicies": {
                "ingressController": {
                    "namespace": namespace
                }
            }
        });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "quoted map-key {label}: instance={instance}; schema={schema}"
        );
    }
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

    // A key position formats every scalar (a numeric key renders and
    // YAML-to-JSON stringifies it), so the declared string default widens
    // to the scalar union rather than pinning the raw input as a string.
    sim_assert_eq!(
        have: name.get("type"),
        want: Some(&serde_json::json!(["boolean", "integer", "number", "string"])),
        "mapping-key interpolation accepts the scalar union, got {name}"
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
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "ingress": {
                    "tls": [{ "secretName": "ingress-tls" }]
                }
            })
        ),
        "helper body should accept string ingress.tls[*].secretName values: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "ingress": {
                    "tls": [{ "secretName": 7 }]
                }
            })
        ),
        "helper body should reject non-string ingress.tls[*].secretName values: {schema}"
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
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
                template_supplied_member_keys: Default::default(),
                split_segment: None,
                merge_layers: None,
                range_key: false,
                omitted_members: Default::default(),
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
                template_supplied_member_keys: Default::default(),
                split_segment: None,
                merge_layers: None,
                range_key: false,
                omitted_members: Default::default(),
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
