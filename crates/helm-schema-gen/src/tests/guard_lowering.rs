use test_util::prelude::sim_assert_eq;

use super::*;

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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
fn guarded_declared_ancestor_keeps_referenced_siblings_beside_child_overlay() {
    let src = indoc! {r#"
        {{- if .Values.alertmanager.enabled }}
        {{- if .Values.alertmanager.ingress.enabled }}
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
          annotations:
            {{- toYaml .Values.alertmanager.ingress.annotations | nindent 4 }}
        spec:
          {{- if and .Values.alertmanager.ingress.className (semverCompare ">=1.18-0" .Capabilities.KubeVersion.GitVersion) }}
          ingressClassName: {{ .Values.alertmanager.ingress.className }}
          {{- end }}
          rules: []
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        alertmanager:
          enabled: true
          ingress:
            enabled: false
            className: ""
            annotations: {}
    "#};
    let descriptions = BTreeMap::from([
        (
            "alertmanager.ingress.enabled".to_string(),
            "Enable ingress for Alertmanager".to_string(),
        ),
        (
            "alertmanager.ingress.className".to_string(),
            "Ingress Class Name to be used to identify ingress controllers".to_string(),
        ),
        (
            "alertmanager.ingress.annotations".to_string(),
            "Annotations to Alertmanager Ingress".to_string(),
        ),
    ]);
    let signals = schema_signals_for(parse_ir(src));
    let enabled = signals
        .evidence_for("alertmanager.ingress.enabled")
        .expect("guard-only enabled evidence");
    assert!(
        enabled.provider_schema_uses.is_empty(),
        "the enabled path must remain guard-only: {enabled:#?}"
    );
    let class_name = signals
        .evidence_for("alertmanager.ingress.className")
        .expect("approximate semver-guarded className evidence");
    assert!(
        class_name.provider_schema_uses.is_empty(),
        "the opaque semver branch must abstain from provider typing while preserving the path: {class_name:#?}"
    );
    let annotations = signals
        .evidence_for("alertmanager.ingress.annotations")
        .expect("conditional child evidence");
    assert!(
        !annotations.conditional_overlays.is_empty(),
        "the guarded annotations child must exercise conditional overlay ownership: {annotations:#?}"
    );

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_values_descriptions(&descriptions),
    );

    sim_assert_eq!(
        have: schema
            .pointer("/properties/alertmanager/properties/ingress/properties/enabled/description")
            .and_then(Value::as_str),
        want: Some("Enable ingress for Alertmanager")
    );
    sim_assert_eq!(
        have: schema
            .pointer("/properties/alertmanager/properties/ingress/properties/className/description")
            .and_then(Value::as_str),
        want: Some("Ingress Class Name to be used to identify ingress controllers")
    );
    sim_assert_eq!(
        have: schema
            .pointer("/properties/alertmanager/properties/ingress/properties/annotations/description")
            .and_then(Value::as_str),
        want: Some("Annotations to Alertmanager Ingress")
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: true\n")),
    );

    // Helm validates the coalesced document: the declared default only
    // goes missing through null-deletion, which reads as nil — falsy —
    // so the omitted state leaves the guarded branch dormant.
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "a null-deleted default-true guard leaves the guarded host open: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "enabled": true,
                    "host": 7
                }
            })
        ),
        "explicit true guard should activate the guarded host schema: {schema}"
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
        }]),
        &[("feature.host", "string")],
    );
    let schema_signals = schema_signals_for(contract);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &NoopProvider)
            .with_values_yaml(Some("feature:\n  enabled: false\n")),
    );

    sim_assert_eq!(
        have: schema.pointer("/properties/feature/allOf/0/if/properties/enabled/enum"),
        want: Some(&serde_json::json!([false])),
        "exact false equality should lower to a typed enum, not truthiness: {schema}"
    );
    // The coalesced document reads an absent guard as null-deleted: at
    // render `eq nil false` is FALSE, so the branch stays dormant.
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "feature": {
                    "host": 7
                }
            })
        ),
        "a null-deleted equal-false guard leaves the guarded host open: {schema}"
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
        merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            omitted_members: Default::default(),
            digest: false,
            merge_operand: false,
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
    for (instance, want, label) in [
        (
            serde_json::json!({ "mode": "name", "feature": { "value": "example" } }),
            true,
            "unconditional scalar sink",
        ),
        (
            serde_json::json!({ "mode": "name", "feature": { "value": { "app": "x" } } }),
            false,
            "scalar sink with object input",
        ),
        (
            serde_json::json!({ "mode": "labels", "feature": { "value": "example" } }),
            false,
            "active fragment sink with scalar input",
        ),
        (
            serde_json::json!({ "mode": "labels", "feature": { "value": { "app": "x" } } }),
            false,
            "active scalar and fragment sinks with object input",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
        merge_operand: false,
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
    // Helm validates the coalesced document, where an absent mode was
    // null-deleted and reads as nil (Helm-falsy), so the `if` keys the
    // mode's own truthiness directly.
    assert!(
        schema.pointer("/allOf/0/if/properties/mode").is_some(),
        "the overlay must key on the mode condition: {schema}"
    );
}
