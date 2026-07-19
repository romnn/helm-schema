use test_util::prelude::sim_assert_eq;

use super::*;

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
    let map_arm = ranged_arm_of_type(environment, "object")
        .unwrap_or_else(|| panic!("environment object arm missing, got {environment}"));
    sim_assert_eq!(
        have: map_arm
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "environment should generalize to an open string map when the chart ranges over its entries, got {environment}"
    );
    // Two-variable ranges cannot iterate integers ("can't use 2 to
    // iterate over more than one variable"), so the runtime widening
    // stays integer-free here.
    assert!(
        ranged_arm_of_type(environment, "integer").is_none(),
        "a destructured range must not admit integer counts, got {environment}"
    );
}

#[test]
fn destructured_range_with_len_guard_preserves_shape_erased_members() {
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
    let mut properties = serde_json::Map::new();
    properties.insert(
        "environment".to_string(),
        serde_json::json!({
            "allOf": [{
                "if": {
                    "anyOf": [
                        { "type": "object" },
                        { "$ref": "#/$defs/helm-truthy" },
                    ]
                },
                "then": {
                    "anyOf": [
                        { "type": "array" },
                        { "type": "object" },
                        { "type": "null" },
                    ]
                }
            }]
        }),
    );
    let range_condition = serde_json::json!({
        "anyOf": [
            {
                "not": {
                    "properties": { "environment": {} },
                    "required": ["environment"],
                    "type": "object",
                }
            },
            {
                "properties": {
                    "environment": {
                        "anyOf": [
                            { "type": "object" },
                            { "$ref": "#/$defs/helm-truthy" },
                        ]
                    }
                },
                "required": ["environment"],
                "type": "object",
            },
        ]
    });
    // The strict member implication already owns the iterable domain. The
    // remaining sibling preserves the fragment/default falsy arm without a
    // third, weaker range-only conditional that cannot narrow the result.
    let all_of = vec![
        serde_json::json!({
            "if": range_condition,
            "then": root_property_schema(
                "environment",
                serde_json::json!({
                    "anyOf": [
                        { "type": "array" },
                        { "type": "object" },
                        { "type": "null" },
                    ]
                }),
            ),
        }),
        // The range KEY renders at the string-only `name:` slot, so a
        // non-empty list's integer keys are excluded.
        root_property_schema(
            "environment",
            serde_json::json!({
                "anyOf": [
                    { "type": "object" },
                    { "maxItems": 0, "type": "array" },
                    { "type": "null" },
                ]
            }),
        ),
        root_property_schema(
            "environment",
            serde_json::json!({ "not": { "type": "boolean" } }),
        ),
        root_property_schema(
            "environment",
            serde_json::json!({ "not": { "type": "integer" } }),
        ),
        root_property_schema(
            "environment",
            serde_json::json!({ "not": { "type": "number" } }),
        ),
    ];
    sim_assert_eq!(
        have: &schema,
        want: &expected_values_schema(properties, all_of, true)
    );

    // The len guard is exact, but `quote` shape-erases each ranged value;
    // the live collection contract must not reintroduce string-only values.
    for environment in [
        serde_json::json!({ "LOG_LEVEL": "debug" }),
        serde_json::json!({ "RETRIES": 7 }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "environment": environment })),
            "quoted range values accept every input shape: {schema}"
        );
    }
}

/// Element- and list-preserving collection transforms keep item provenance,
/// so a total stringification widens source items without erasing a separate
/// strict item consumer.
#[test]
fn collection_selection_projects_item_conversion_to_source_items() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: busybox
              env:
                {{- range initial .Values.teams }}
                - name: TEAM
                  value: {{ . | quote }}
                {{- end }}
                - name: LAST_TEAM
                  value: {{ .Values.teams | last | quote }}
                {{- if .Values.strict }}
                {{- range .Values.teams }}
                - name: STRICT_TEAM
                  value: {{ . | b64enc | quote }}
                {{- end }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        teams:
          - first
          - last
        strict: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for teams in [
        serde_json::json!([]),
        serde_json::json!([7, 8]),
        serde_json::json!([{ "name": "first" }, { "name": "last" }]),
    ] {
        let instance = serde_json::json!({ "teams": teams, "strict": false });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "initial/range and last/quote observe formatted item text: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "teams": [7], "strict": true })
        ),
        "a live independent b64enc consumer still requires string items: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "teams": ["secret"], "strict": true })
        ),
        "string items satisfy the independent strict consumer: {schema}"
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
    let array_arm = any_of_variant_matching(access_modes, |variant| {
        variant.get("type").and_then(Value::as_str) == Some("array")
            && variant.get("description").is_some()
    })
    .unwrap_or_else(|| panic!("provider array arm missing, got {access_modes}"));
    sim_assert_eq!(
        have: array_arm.get("items"),
        want: Some(&serde_json::json!({})),
        "quoted items render any input through strval, so the provider string typing must not flow back, got {access_modes}"
    );
    assert!(
        array_arm
            .pointer("/description")
            .and_then(Value::as_str)
            .is_some(),
        "accessModes should keep the provider description, got {access_modes}"
    );
    sim_assert_eq!(
        have: array_arm
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

    let hosts = schema.pointer("/properties/hosts").expect("hosts present");
    let hosts_arm = ranged_arm_of_type(hosts, "array")
        .unwrap_or_else(|| panic!("hosts array arm missing, got {hosts}"));
    let host_paths = hosts_arm
        .pointer("/items/properties/paths")
        .expect("hosts[].paths present");
    let paths_arm = ranged_arm_of_type(host_paths, "array")
        .unwrap_or_else(|| panic!("hosts[].paths array arm missing, got {host_paths}"));
    let path_items = paths_arm.get("items").expect("hosts[].paths items");
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
    let array_arm = ranged_arm_of_type(hosts, "array")
        .unwrap_or_else(|| panic!("hosts array arm missing, got {hosts}"));
    sim_assert_eq!(
        have: array_arm.pointer("/items/type").and_then(Value::as_str),
        want: Some("string"),
        "hosts items should stay strings, got {hosts}"
    );
    assert!(
        array_arm.pointer("/items/properties/Chart").is_none(),
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
fn wildcard_source_path_types_both_collection_lanes_without_empty_variant() {
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        digest: false,
        merge_operand: false,
    }];
    let values_yaml = indoc! {"
        image:
          pullSecrets: []
    "};

    let schema = schema_for_values_yaml(&uses, Some(values_yaml));
    let pull_secrets = schema
        .pointer("/properties/image/properties/pullSecrets")
        .expect("image.pullSecrets present");

    // A bare `*` member row proves members exist, not which collection lane
    // hosts them (`range` iterates arrays and maps alike), so both lanes
    // carry the rendered name's scalar typing and no untyped artifact arm
    // survives.
    sim_assert_eq!(
        have: pull_secrets.pointer("/anyOf/0/items/type").and_then(Value::as_str),
        want: Some("string"),
        "array items should inherit the rendered name scalar type, got {pull_secrets}"
    );
    sim_assert_eq!(
        have: pull_secrets
            .pointer("/anyOf/1/additionalProperties/type")
            .and_then(Value::as_str),
        want: Some("string"),
        "map member values should inherit the rendered name scalar type, got {pull_secrets}"
    );
    let arms = pull_secrets
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("two-lane union");
    sim_assert_eq!(have: arms.len(), want: 2);
    assert!(
        arms.iter()
            .all(|arm| !crate::schema_model::is_empty_schema(arm)),
        "no empty artifact arm may survive: {pull_secrets}"
    );
}
