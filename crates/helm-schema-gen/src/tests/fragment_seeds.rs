use indoc::indoc;
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
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{ .Values.account.name }}.json: |
            {}
    "};
    let values_yaml = indoc! {"
        account:
          name: surveyor
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let name = schema
        .pointer("/properties/account/properties/name")
        .expect("account.name present");

    assert!(
        name.get("anyOf").is_none(),
        "mapping-key interpolation must not widen account.name with ConfigMap.data provider shape, got {name}"
    );
    // Helm 4.2.3 verdicts (transfer/receipts/mapping_key_template_*{,-r2}): Go formats scalars and
    // maps (`map[bad:1]`) into a plain mapping key, while a list renders as a flow sequence key and
    // aborts YAML-to-JSON conversion. The ConfigMap value schema imposes no type on the key source.
    // A string that breaks the key's YAML (`x: y`) also aborts; that lexical rejection is outside
    // this test's scope and stays an accepted uncertainty (ADR 0013).
    for (account, want) in [
        (serde_json::json!({ "name": "surveyor" }), true),
        (serde_json::json!({ "name": 7 }), true),
        (serde_json::json!({ "name": 1.5 }), true),
        (serde_json::json!({ "name": true }), true),
        (serde_json::json!({ "name": { "bad": 1 } }), true),
        (serde_json::json!({}), true),
        (serde_json::json!({ "name": ["a"] }), false),
        (serde_json::json!({ "name": [] }), false),
    ] {
        let instance = serde_json::json!({ "account": account });
        sim_assert_eq!(
            have: schema_accepts_instance(&schema, &instance),
            want: want,
            "mapping-key source verdict: instance={instance}; schema={schema}"
        );
    }
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
    let values_yaml = indoc! {r"
        alertmanager:
          enabled: true
          name: alertmanager
          serviceAccount:
            create: true
            name:
    "};

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
        strip_marker: false,
        source_expr: helm_schema_core::ValuesPath::parse("signoz-otel-gateway.serviceAccount.name"),
        path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        kind: ValueKind::Scalar,
        condition: helm_schema_core::GuardDnf::from_guards(vec![
            Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse(
                    "signoz-otel-gateway.serviceAccount.create",
                ),
            },
            Guard::Default {
                path: helm_schema_core::ValuesPath::parse(
                    "signoz-otel-gateway.serviceAccount.name",
                ),
            },
        ]),
        resource: None,
        provenance: Vec::new(),
        stringified: false,
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        text_preimage: None,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_scalar(helm_schema_core::ValuesPath::parse("signoz-otel-gateway"));
    contract.add_type_hint("signoz-otel-gateway.serviceAccount.name", "string");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {r#"
                signoz-otel-gateway:
                  serviceAccount:
                    create: true
                    name: ""
            "#})),
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
        strip_marker: false,
        source_expr: helm_schema_core::ValuesPath::parse("clickhouse.securityContext"),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "securityContext".to_string(),
        ]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("clickhouse.securityContext.enabled"),
        }]),
        resource: None,
        provenance: Vec::new(),
        stringified: false,
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        text_preimage: None,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_scalar(helm_schema_core::ValuesPath::parse("clickhouse"));
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {r"
                clickhouse:
                  securityContext:
                    enabled: true
                    fsGroup: 101
                    runAsUser: 1001
            "})),
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
            strip_marker: false,
            source_expr: helm_schema_core::ValuesPath::parse(
                "signoz.smtpVars.existingSecret.fromKey",
            ),
            path: YamlPath(vec!["env[*]".to_string(), "valueFrom".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("signoz.smtpVars.enabled"),
            }]),
            resource: None,
            provenance: Vec::new(),
            stringified: false,
            plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
            text_preimage: None,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
        ContractUse {
            strip_marker: false,
            source_expr: helm_schema_core::ValuesPath::parse("signoz.smtpVars.existingSecret.name"),
            path: YamlPath(vec![
                "env[*]".to_string(),
                "valueFrom".to_string(),
                "secretKeyRef".to_string(),
                "name".to_string(),
            ]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("signoz.smtpVars.enabled"),
            }]),
            resource: None,
            provenance: Vec::new(),
            stringified: false,
            plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
            text_preimage: None,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
    ]);
    contract.push_pathless_scalar(helm_schema_core::ValuesPath::parse("signoz"));
    contract.add_type_hint("signoz.smtpVars.enabled", "boolean");

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {r#"
                signoz:
                  smtpVars:
                    enabled: false
                    existingSecret:
                      name: ""
                      fromKey: ""
            "#})),
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
        strip_marker: false,
        source_expr: helm_schema_core::ValuesPath::parse("alertmanager.tolerations"),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "tolerations".to_string(),
        ]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("alertmanager.enabled"),
        }]),
        resource: None,
        provenance: Vec::new(),
        stringified: false,
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        text_preimage: None,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_scalar(helm_schema_core::ValuesPath::parse("alertmanager"));
    contract.add_type_hint("alertmanager.enabled", "boolean");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {"
                alertmanager:
                  enabled: true
                  tolerations: []
            "})),
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
        strip_marker: false,
        source_expr: helm_schema_core::ValuesPath::parse(
            "clickhouse.clickhouseOperator.configs.confdFiles",
        ),
        path: YamlPath(vec!["data".to_string()]),
        kind: ValueKind::Fragment,
        condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("clickhouse.enabled"),
        }]),
        resource: None,
        provenance: Vec::new(),
        stringified: false,
        plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
        text_preimage: None,
        template_supplied_member_keys: std::collections::BTreeSet::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        nil_omitting: false,
        omitted_members: std::collections::BTreeMap::default(),
        digest: false,
        merge_operand: false,
    }]);
    contract.push_pathless_scalar(helm_schema_core::ValuesPath::parse("clickhouse"));
    contract.add_type_hint("clickhouse.enabled", "boolean");
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {"
                clickhouse:
                  enabled: true
                  clickhouseOperator:
                    configs:
                      confdFiles:
            "})),
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
    let values_yaml = indoc! {r"
        service:
          name:
        serviceAccount:
          name:
    "};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    // Marker-preservation proofs may encode nullable acceptance through conditionals.
    // Pin the input domain rather than requiring a particular union spelling.
    for (name, accepted) in [
        (serde_json::Value::Null, true),
        (serde_json::json!("safe-service"), true),
        (serde_json::json!("<no value>safe"), true),
        (serde_json::json!("a: b"), false),
        (serde_json::json!(true), false),
        (serde_json::json!(17), false),
        (serde_json::json!({}), true),
        (serde_json::json!({ "k": "v" }), true),
        (serde_json::json!(["safe"]), false),
    ] {
        let instance = serde_json::json!({
            "service": { "name": null },
            "serviceAccount": { "name": name },
        });
        sim_assert_eq!(have: schema_accepts_instance(&schema, &instance), want: accepted, "{instance}");
    }
}

#[test]
fn self_default_guarded_branch_lowers_without_losing_else_branch_precision() {
    let contract = with_type_hints(
        ContractIr::from_contract_uses(vec![
            ContractUse {
                strip_marker: false,
                source_expr: helm_schema_core::ValuesPath::parse("serviceAccount.name"),
                path: YamlPath(vec!["metadata".to_string(), "name".to_string()]),
                kind: ValueKind::Scalar,
                condition: helm_schema_core::GuardDnf::from_guards(vec![
                    Guard::Truthy {
                        path: helm_schema_core::ValuesPath::parse("serviceAccount.create"),
                    },
                    Guard::Default {
                        path: helm_schema_core::ValuesPath::parse("serviceAccount.name"),
                    },
                ]),
                resource: None,
                provenance: Vec::new(),
                stringified: false,
                plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
                text_preimage: None,
                template_supplied_member_keys: std::collections::BTreeSet::default(),
                split_segment: None,
                merge_layers: None,
                range_key: false,
                nil_omitting: false,
                omitted_members: std::collections::BTreeMap::default(),
                digest: false,
                merge_operand: false,
            },
            ContractUse {
                strip_marker: false,
                source_expr: helm_schema_core::ValuesPath::parse("serviceAccount.name"),
                path: YamlPath(vec![
                    "spec".to_string(),
                    "template".to_string(),
                    "spec".to_string(),
                    "serviceAccountName".to_string(),
                ]),
                kind: ValueKind::Scalar,
                condition: helm_schema_core::GuardDnf::from_guards(vec![Guard::Not {
                    path: helm_schema_core::ValuesPath::parse("serviceAccount.create"),
                }]),
                resource: None,
                provenance: Vec::new(),
                stringified: false,
                plain_scalar_placement: helm_schema_core::PlainScalarPlacement::Unknown,
                text_preimage: None,
                template_supplied_member_keys: std::collections::BTreeSet::default(),
                split_segment: None,
                merge_layers: None,
                range_key: false,
                nil_omitting: false,
                omitted_members: std::collections::BTreeMap::default(),
                digest: false,
                merge_operand: false,
            },
        ]),
        &[("serviceAccount.name", "string")],
    );
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals_for(contract), &NoopProvider).with_values_documents(
            &prepared_values_documents(Some(indoc! {"
                serviceAccount:
                  create: true
                  name:
            "})),
        ),
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
