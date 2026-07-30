use super::*;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

/// an outer range binds each item to a local that an INNER range
/// iterates — the nested iterable requirement must reach the outer item
/// identity (reloader `deployment.env.existing` shape).
#[test]
fn nested_range_over_ranged_local_requires_iterable_items() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $values := .Values.existing }}
          {{- range $key, $value := $values }}
          {{ $key }}: {{ $value }}
          {{- end }}
          {{- end }}
    "};
    let values_yaml = "existing: ~\n";
    let ir = parse_ir(src);
    let signals = schema_signals_for(&ir);
    let inner_member = signals
        .evidence_for("existing.*.*")
        .expect("nested range preserves the inner member identity");
    assert!(
        !inner_member.provider_schema_uses.is_empty()
            || inner_member
                .conditional_overlays
                .iter()
                .any(|overlay| !overlay.evidence.provider_schema_uses.is_empty()),
        "nested range keeps the ConfigMap.data value provider use: {inner_member:#?}"
    );
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    for existing in [
        serde_json::json!([{ "A": "key" }]),
        serde_json::json!({ "group": { "A": "key" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "existing": existing })),
            "array and map lanes with string inner values satisfy both ranges: {schema}"
        );
    }
    for existing in [serde_json::json!(0), serde_json::json!(-1)] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "existing": existing })),
            "nonpositive outer integer ranges execute no body: {schema}"
        );
    }
    for existing in [serde_json::json!(["x"]), serde_json::json!(2)] {
        assert!(
            !schema_accepts_instance(&schema, &serde_json::json!({ "existing": existing })),
            "a live inner two-variable range cannot iterate a scalar member: {schema}"
        );
    }
    for existing in [
        serde_json::json!([{ "A": 7 }]),
        serde_json::json!({ "group": { "A": 7 } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &serde_json::json!({ "existing": existing })),
            "ConfigMap.data rejects numeric inner values on every outer lane: {schema}"
        );
    }
}

#[test]
fn direct_nested_range_keeps_the_map_lane_beside_a_declared_list_default() {
    let src = indoc! {r"
        ports:
        {{- range .Values.ports }}
          -
            {{- range $key, $value := . }}
            {{ $key }}: {{ $value }}
            {{- end }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        ports:
          - name: http
            port: 80
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let rendered_scalar = serde_json::json!({
        "anyOf": [
            { "type": "integer" },
            { "type": "string" },
        ],
    });
    let member_shape = serde_json::json!({
        "anyOf": [
            {
                "additionalProperties": rendered_scalar,
                "properties": {
                    "name": { "type": "string" },
                    "port": { "type": "integer" },
                },
                "type": "object",
            },
            { "items": {}, "type": "array" },
            { "type": "null" },
            { "additionalProperties": {}, "type": "object" },
        ],
    });
    // The live inner range owns its member domain. The declared sample fields
    // remain ordinary base evidence and cannot narrow this runtime arm.
    let iterable_member = serde_json::json!({
        "anyOf": [
            { "items": {}, "type": "array" },
            { "additionalProperties": {}, "type": "object" },
            { "type": "null" },
        ],
    });
    let expected = expected_values_schema(
        serde_json::Map::from_iter([(
            "ports".to_string(),
            serde_json::json!({
                "anyOf": [
                    { "items": member_shape.clone(), "type": "array" },
                    { "items": member_shape.clone(), "type": "array" },
                    { "type": "integer" },
                    { "type": "null" },
                    {
                        "additionalProperties": member_shape,
                        "type": "object",
                    },
                ],
            }),
        )]),
        vec![root_property_schema(
            "ports",
            serde_json::json!({
                "anyOf": [
                    { "items": iterable_member.clone(), "type": "array" },
                    {
                        "additionalProperties": iterable_member,
                        "type": "object",
                    },
                    { "maximum": 0, "type": "integer" },
                    { "type": "null" },
                ],
            }),
        )],
        false,
    );

    sim_assert_eq!(have: schema, want: expected);
}

/// A default-selected field on each ranged member binds the provider
/// contract to whichever candidate supplies the rendered value. Named
/// members declared in `values.yaml` obey the same wildcard relation:
/// Traefik's `ports.metrics.port` is numeric when `containerPort` is falsy,
/// but becomes dormant when a valid `containerPort` wins.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn named_member_keeps_the_ranged_default_selection_contract() {
    let helpers = indoc! {r#"
        {{- define "test.pod-template" }}
        metadata:
          labels:
            app: test
        spec:
          containers:
            - name: test
              image: example
              args:
                {{- range $name, $config := .Values.ports }}
                - {{ $config.port | quote }}
                {{- end }}
              ports:
                {{- range $name, $config := .Values.ports }}
                {{- if $config }}
                - name: {{ $name }}
                  containerPort: {{ default $config.port $config.containerPort }}
                  protocol: {{ default "TCP" $config.protocol }}
                  {{- if ($config.http3).enabled }}
                - name: {{ printf "%s-http3" $name }}
                  containerPort: {{ $config.port }}
                  protocol: UDP
                  {{- end }}
                {{- end }}
                {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if and .Values.deployment.enabled (eq .Values.deployment.kind "Deployment") }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template: {{ include "test.pod-template" . | fromYaml | toYaml | nindent 4 }}
        {{- end }}
        ---
        {{- if and .Values.deployment.enabled (eq .Values.deployment.kind "DaemonSet") }}
        apiVersion: apps/v1
        kind: DaemonSet
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template: {{ include "test.pod-template" . | fromYaml | toYaml | nindent 4 }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        deployment:
          enabled: true
          kind: Deployment
        ports:
          metrics:
            port: 9100
    "};
    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "deployment": { "enabled": true, "kind": "Deployment" },
                "ports": { "metrics": { "port": 9100 } },
            }),
        ),
        "the declared integer satisfies the wildcard member contract: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "deployment": { "enabled": true, "kind": "Deployment" },
                "ports": { "metrics": { "port": "audit" } },
            }),
        ),
        "the selected port must satisfy the wildcard provider contract: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "deployment": { "enabled": true, "kind": "Deployment" },
                "ports": { "metrics": { "port": "dormant", "containerPort": 9200 } },
            }),
        ),
        "a valid containerPort makes the fallback port dormant: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "deployment": { "enabled": false, "kind": "Deployment" },
                "ports": { "metrics": { "port": "dormant" } },
            }),
        ),
        "the member contract stays dormant with its outer branch: {schema}"
    );
}

fn expected_block_scalar_nested_range_schema() -> Value {
    let nested_range_member = serde_json::json!({
        "anyOf": [
            { "items": {}, "type": "array" },
            { "additionalProperties": {}, "type": "object" },
            { "type": "null" },
        ],
    });
    let declared_range_member = serde_json::json!({
        "anyOf": [
            { "type": "array" },
            { "type": "object" },
            { "type": "null" },
        ],
    });
    let mut properties = serde_json::Map::new();
    properties.insert(
        "config".to_string(),
        serde_json::json!({
            "additionalProperties": nested_range_member.clone(),
            "properties": {
                "logging": declared_range_member.clone(),
                "traces": declared_range_member,
            },
            "type": "object",
        }),
    );
    expected_values_schema(
        properties,
        vec![
            root_property_schema(
                "config",
                serde_json::json!({
                    "anyOf": [
                        {
                            "items": nested_range_member.clone(),
                            "type": "array",
                        },
                        {
                            "additionalProperties": nested_range_member,
                            "type": "object",
                        },
                        { "type": "null" },
                    ],
                }),
            ),
            root_property_schema(
                "config",
                serde_json::json!({
                    "anyOf": [
                        { "type": "object" },
                        { "type": "null" },
                    ],
                }),
            ),
            navigated_host_clause(&["config"]),
        ],
        false,
    )
}

/// Airflow ranges a merged config map inside a block scalar, then ranges
/// each section's members and renders every leaf through
/// `tpl(toString(...))`. The outer values path influences the merge, while
/// `config.*` is the member identity that the inner range actually
/// iterates. Arrays, objects, and null sections therefore render; only a
/// non-iterable scalar section aborts.
#[test]
fn block_scalar_nested_range_uses_member_identity_not_influence() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |-
            {{- $root := . }}
            {{- $config := deepCopy .Values.config | merge (dict "core" dict) }}
            {{- range $section, $settings := $config }}
            [{{ $section }}]
            {{- range $key, $value := $settings }}
            {{ $key }} = {{ tpl ($value | toString) $root }}
            {{- end }}
            {{- end }}
    "#};
    let values_yaml = indoc! {"
        config:
          logging:
            remote_logging: \"True\"
          traces:
            otel_on: false
    "};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(ir, Some(values_yaml));
    let expected = expected_block_scalar_nested_range_schema();

    for (config, want, label) in [
        (
            serde_json::json!({
                "logging": { "remote_logging": "True" },
                "traces": { "otel_on": false },
            }),
            true,
            "the declared defaults render",
        ),
        (
            serde_json::json!({ "logging": 7 }),
            false,
            "a scalar section aborts the inner range",
        ),
        (
            serde_json::json!({ "traces": [] }),
            true,
            "an empty array section renders",
        ),
        (
            serde_json::json!({ "traces": null }),
            true,
            "a null section skips the inner range",
        ),
        (
            serde_json::json!({ "traces": { "otel_on": [] } }),
            true,
            "toString accepts an array leaf",
        ),
        (
            serde_json::json!({ "traces": { "otel_on": { "probe-member": {} } } }),
            true,
            "toString accepts an object leaf",
        ),
    ] {
        let instance = serde_json::json!({ "config": config });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "block-scalar nested range ({label}): \
            instance={instance}; want={want}; schema={schema}"
        );
    }
    sim_assert_eq!(have: schema, want: expected);
}

fn conditional_duplicate_config_schema(nested_range_member: &Value) -> Value {
    let declared_section = serde_json::json!({
        "anyOf": [
            { "type": "array" },
            { "type": "object" },
            { "type": "null" },
        ],
    });
    serde_json::json!({
        "additionalProperties": nested_range_member,
        "properties": {
            "traces": declared_section,
        },
        "type": "object",
    })
}

fn expected_conditional_duplicate_nested_range_schema() -> Value {
    let nested_range_member = serde_json::json!({
        "anyOf": [
            { "items": {}, "type": "array" },
            { "additionalProperties": {}, "type": "object" },
            { "type": "null" },
        ],
    });
    let mut properties = serde_json::Map::new();
    properties.insert(
        "config".to_string(),
        conditional_duplicate_config_schema(&nested_range_member),
    );
    properties.insert("enabled".to_string(), serde_json::json!({}));
    let missing_config = serde_json::json!({
        "anyOf": [
            {
                "not": {
                    "properties": { "config": {} },
                    "required": ["config"],
                    "type": "object",
                },
            },
            {
                "properties": {
                    "config": { "enum": [null] },
                },
                "required": ["config"],
                "type": "object",
            },
        ],
    });
    let mut expected = expected_values_schema(
        properties,
        vec![
            root_property_schema(
                "config",
                serde_json::json!({
                    "anyOf": [
                        {
                            "items": nested_range_member.clone(),
                            "type": "array",
                        },
                        {
                            "additionalProperties": nested_range_member,
                            "type": "object",
                        },
                        { "type": "null" },
                    ],
                }),
            ),
            root_property_schema(
                "config",
                serde_json::json!({
                    "anyOf": [
                        { "type": "object" },
                        { "type": "null" },
                    ],
                }),
            ),
            serde_json::json!({
                "if": {
                    "allOf": [
                        helm_truthy_guard("enabled"),
                        missing_config,
                    ],
                },
                "then": false,
            }),
            navigated_host_clause(&["config"]),
        ],
        true,
    );
    expected["additionalProperties"] = serde_json::json!({});
    expected
}

#[test]
fn conditional_duplicate_of_nested_range_does_not_restore_declared_leaf_types() {
    let helpers = indoc! {r#"
        {{- define "render-config" -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config: |-
            {{- $root := . }}
            {{- $config := deepCopy .Values.config | merge (dict "core" dict) }}
            {{- range $section, $settings := $config }}
            [{{ $section }}]
            {{- range $key, $value := $settings }}
            {{ $key }} = {{ tpl ($value | toString) $root }}
            {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{ include "render-config" . }}
        {{- if .Values.enabled }}
        {{ include "render-config" . }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: true
        config:
          traces:
            otel_on: \"False\"
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for leaf in [
        serde_json::json!([]),
        serde_json::json!({ "probe-member": {} }),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "enabled": true,
                    "config": { "traces": { "otel_on": leaf } },
                }),
            ),
            "a conditional duplicate cannot restore the declared leaf type: {schema}"
        );
    }

    let expected = expected_conditional_duplicate_nested_range_schema();
    sim_assert_eq!(have: schema, want: expected);
}

/// A split list carries its input path as influence only. Ranging a local
/// bound to that derived list must not impose an iterable input domain on
/// the raw string that `splitList` consumes.
#[test]
fn derived_split_range_does_not_reclassify_its_string_source() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          namespaces: |-
            {{- $namespaces := splitList "," .Values.namespaces }}
            {{- range $namespace := $namespaces }}
            {{ tpl ($namespace | toString) $ }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("namespaces: a,b\n"));
    let mut properties = serde_json::Map::new();
    properties.insert(
        "namespaces".to_string(),
        serde_json::json!({ "type": "string" }),
    );
    let expected = expected_values_schema(
        properties,
        vec![navigated_host_clause(&["namespaces"])],
        false,
    );

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "namespaces": "one,two" }),),
        "a string is the strict splitList operand and must remain accepted: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "namespaces": ["one", "two"] }),
        ),
        "splitList rejects a list operand even though its result is ranged: {schema}"
    );
    sim_assert_eq!(have: schema, want: expected);
}

/// a string consumer behind `default` constrains the raw subject only
/// where it survives the fallback — `truthy(nameOverride) ⇒ string`, never
/// an unconditional type (zalando/promtail fullname helper shape).
#[test]
fn default_guarded_string_consumer_binds_conditional_contract() {
    let helper_src = indoc! {r#"
        {{- define "test.fullname" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ include "test.fullname" . }}
    "#};
    let values_yaml = "nameOverride: \"\"\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helper_src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "nameOverride": "custom" }),
        serde_json::json!({ "nameOverride": "" }),
        serde_json::json!({}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "falsy values take the fallback; strings feed trunc: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "nameOverride": { "a": 1 } })),
        "a truthy non-string reaches `trunc` and aborts rendering: {schema}"
    );
}

/// a range in one template must not bypass an independent member
/// contract from another — the object requirement holds wherever the value
/// is truthy, while the Helm-empty array off-state stays valid (reloader
/// `deployment.env.secret` shape).
#[test]
fn range_alternative_does_not_bypass_member_contract() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: first
        data:
          {{- range $key, $value := .Values.secret }}
          {{ $key }}: {{ $value }}
          {{- end }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: second
        data:
          {{- if .Values.secret }}
          alert: {{ .Values.secret.ALERT_ON_RELOAD }}
          {{- end }}
    "};
    let values_yaml = "secret: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "secret": { "ALERT_ON_RELOAD": "enabled" } }),
        serde_json::json!({ "secret": {} }),
        serde_json::json!({ "secret": [] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "maps render both templates; an EMPTY array is Helm-falsy and \
             skips the member read: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "secret": ["x"] })),
        "a truthy array reaches the second template's member access and \
         aborts rendering: {schema}"
    );
}

/// independent positive type-guarded blocks with NO catch-all leave
/// unmatched types valid — they execute neither block (external-dns
/// `extraArgs` shape, declared `{}`).
#[test]
fn independent_type_blocks_keep_silent_complement_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            args:
              {{- if kindIs "map" .Values.extraArgs }}
              {{- range $key, $value := .Values.extraArgs }}
              - --{{ $key }}={{ $value }}
              {{- end }}
              {{- end }}
              {{- if kindIs "slice" .Values.extraArgs }}
              {{- range $value := .Values.extraArgs }}
              - {{ $value }}
              {{- end }}
              {{- end }}
    "#};
    let values_yaml = "extraArgs: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "extraArgs": { "a": "b" } }),
        serde_json::json!({ "extraArgs": ["--x"] }),
        serde_json::json!({ "extraArgs": 7 }),
        serde_json::json!({ "extraArgs": "s" }),
        serde_json::json!({ "extraArgs": true }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "unmatched types execute neither block and render: \
             instance={instance}; schema={schema}"
        );
    }
}

/// a `kindIs "slice"` arm whose body serializes the value must stay
/// SATISFIABLE for arrays — the branch resolve must never contradict its
/// own partition (oauth2-proxy `extraArgs` list form, which the chart's
/// own ci values render).
#[test]
fn slice_partition_overlay_accepts_arrays() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            args:
              {{- if kindIs "map" .Values.extraArgs }}
              {{- range $key, $value := .Values.extraArgs }}
              - --{{ $key }}={{ $value }}
              {{- end }}
              {{- end }}
              {{- if kindIs "slice" .Values.extraArgs }}
              {{- with .Values.extraArgs }}
              {{- toYaml . | nindent 10 }}
              {{- end }}
              {{- end }}
    "#};
    let values_yaml = "extraArgs: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraArgs": ["--a=1", "--b=2"] })
        ),
        "the slice arm serializes the list; its own partition must accept \
         arrays: {schema}"
    );
}

/// `toYaml` preserves its input kind at a mapping-value provider sink:
/// falsy values skip the `with`, while a truthy scalar renders an invalid
/// Pod affinity value.
#[test]
fn mapping_value_yaml_serialization_keeps_provider_shape() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          {{- with .Values.affinity }}
          affinity:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = "affinity: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "affinity": false }),
        serde_json::json!({ "affinity": {} }),
        serde_json::json!({ "affinity": { "nodeAffinity": {} } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a skipped or provider-valid affinity must validate: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "affinity": 7 })),
        "a truthy scalar reaches the object-typed affinity sink: {schema}"
    );
}

/// a shape-neutral `toYaml` input still inherits the sequence domain of
/// its structural sink. The surrounding truthy guard preserves Helm-falsy
/// skip values without admitting a truthy scalar into the sequence.
#[test]
fn sequence_fragment_keeps_provider_array_domain() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: main
              image: busybox
              env:
                {{- if .Values.extraEnvs }}
                {{- toYaml .Values.extraEnvs | nindent 8 }}
                {{- end }}
    "};
    let signals = parse_ir(src).finalize().into_schema_signals();
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &SharedObjectProvider)
            .with_values_yaml(Some("extraEnvs: []\n")),
    );

    for instance in [
        serde_json::json!({ "extraEnvs": false }),
        serde_json::json!({ "extraEnvs": 0 }),
        serde_json::json!({ "extraEnvs": "" }),
        serde_json::json!({ "extraEnvs": [{ "name": "AUDIT" }] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the guard skips falsy values and EnvVar lists satisfy the sequence sink: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "extraEnvs": "audit" })),
        "a truthy scalar cannot occupy the sequence sink: {schema}"
    );
}

/// a truthy-guarded object that is BOTH serialized and member-read is
/// falsy-or-object — the fragment lane must not bypass the member access
/// (coredns `podDisruptionBudget` shape). Encoded size-aware: one folded
/// arm per path, pruned where the schema tree already types the node.
#[test]
fn member_read_beside_serialize_requires_object_when_truthy() {
    let src = indoc! {r"
        {{- if .Values.podDisruptionBudget }}
        apiVersion: policy/v1
        kind: PodDisruptionBudget
        metadata:
          name: test
        spec:
          selector: {{ .Values.podDisruptionBudget.selector }}
          {{- toYaml .Values.podDisruptionBudget | nindent 2 }}
        {{- end }}
    "};
    let values_yaml = "podDisruptionBudget: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "podDisruptionBudget": false }),
        serde_json::json!({ "podDisruptionBudget": 0 }),
        serde_json::json!({ "podDisruptionBudget": { "selector": {} } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "falsy skips the branch; objects render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "podDisruptionBudget": "audit" }),
        serde_json::json!({ "podDisruptionBudget": ["x"] }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a truthy non-object reaches the `.selector` access and aborts: \
             instance={instance}; schema={schema}"
        );
    }
}

/// a total serialized use in one document cannot bypass an independent
/// range contract in another. Only the range's live truthy branch constrains
/// the shared input.
#[test]
fn serialized_fragment_does_not_bypass_independent_range_contract() {
    let src = indoc! {r"
        {{- with .Values.config }}
        apiVersion: example.com/v1
        kind: Widget
        metadata:
          name: serialized
        spec:
          config:
            {{- toYaml . | nindent 4 }}
        {{- end }}
        ---
        {{- if .Values.config }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: ranged
        data:
          {{- range $key, $value := .Values.config }}
          {{ $key }}: {{ $value | quote }}
          {{- end }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("config: {}\n"));

    for instance in [
        serde_json::json!({ "config": false }),
        serde_json::json!({ "config": 0 }),
        serde_json::json!({ "config": "" }),
        serde_json::json!({ "config": { "AUDIT": "true" } }),
        serde_json::json!({ "config": ["true"] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "falsy values skip the range and iterable values satisfy both consumers: \
             instance={instance}; schema={schema}"
        );
    }
    for config in [serde_json::json!(7), serde_json::json!("audit")] {
        let instance = serde_json::json!({ "config": config });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a truthy non-iterable reaches the independent range: \
             instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn yaml_serialization_does_not_erase_unconditional_range_domain() {
    let src = indoc! {r"
        apiVersion: example.com/v1
        kind: Widget
        metadata:
          name: serialized
        spec:
          config:
            {{- toYaml .Values.config | nindent 4 }}
        ---
        {{- range $key, $value := .Values.config }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: ranged
        {{- end }}
    "};
    let ir = parse_ir(src);
    let evidence = schema_signals_for(&ir)
        .evidence_for("config")
        .expect("serialized and ranged config evidence")
        .facts;
    assert!(
        evidence.used_as_yaml_serialized,
        "toYaml must retain its total-use semantics at the placed row"
    );
    assert!(
        evidence.is_direct_ranged_source && evidence.has_destructured_range_use,
        "the independent two-variable range must retain its runtime domain"
    );
    let schema = schema_for_values_yaml(ir, Some("config: {}\n"));

    for config in [serde_json::json!([]), serde_json::json!({}), Value::Null] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "config": config })),
            "two-variable ranges accept collection and null lanes: {schema}"
        );
    }
    for config in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &serde_json::json!({ "config": config })),
            "an unconditional two-variable range rejects non-collections despite its serialized sibling: {schema}"
        );
    }
}

/// a ranged member access executes for every member after its outer
/// guard and range are live. The member's own Helm truthiness does not guard
/// the access.
#[test]
fn ranged_member_access_rejects_falsy_members_only_when_live() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $name, $provider := .Values.providers }}
          {{ $name }}: {{ $provider.name | quote }}
          {{- end }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            enabled: false
            providers: {}
        "}),
    );

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "enabled": true, "providers": { "main": { "name": "default" } } })
        ),
        "object members host the live field access: {schema}"
    );
    for provider in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
        serde_json::json!("audit"),
        serde_json::json!([]),
    ] {
        let live =
            serde_json::json!({ "enabled": true, "providers": { "main": provider.clone() } });
        assert!(
            !schema_accepts_instance(&schema, &live),
            "every live non-object member reaches `.name`: instance={live}; schema={schema}"
        );
        let dead = serde_json::json!({ "enabled": false, "providers": { "main": provider } });
        assert!(
            schema_accepts_instance(&schema, &dead),
            "the disabled outer branch imposes no member-host contract: \
             instance={dead}; schema={schema}"
        );
    }
}

/// opening a declared empty mapping must retain its object domain.
/// Arbitrary members remain valid, but a scalar cannot host the member read.
#[test]
fn opened_empty_member_host_keeps_object_type() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            image: busybox
            {{- if .Values.livenessProbe.enabled }}
            livenessProbe:
              exec:
                command: ["true"]
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("livenessProbe: {}\n"));

    for instance in [
        serde_json::json!({ "livenessProbe": {} }),
        serde_json::json!({ "livenessProbe": { "enabled": true, "extension": 1 } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "open object forms render: instance={instance}; schema={schema}"
        );
    }
    for liveness_probe in [
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!(""),
        serde_json::json!([]),
    ] {
        let instance = serde_json::json!({ "livenessProbe": liveness_probe });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a scalar or list cannot host `.enabled`: instance={instance}; schema={schema}"
        );
    }
}

/// a chained selector read requires every nonterminal segment to be
/// present and object-shaped under the executing guard; the leaf stays free
/// (surveyor `config.credentials.secret.key` shape).
#[test]
fn chained_member_read_requires_intermediate_objects() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.config.credentials }}
          key: {{ .Values.config.credentials.secret.key }}
          {{- end }}
    "};
    let values_yaml = "config: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "config": { "credentials": { "secret": { "key": "k" } } } }),
        serde_json::json!({ "config": { "credentials": { "secret": {} } } }),
        serde_json::json!({ "config": {} }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "objects and missing leaves render: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "config": { "credentials": { "audit": 1 } } }),
        serde_json::json!({ "config": { "credentials": { "secret": "audit" } } }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a missing or non-object intermediate `secret` aborts the chained access: \
             instance={instance}; schema={schema}"
        );
    }
}

/// integer iteration is a zero/one-variable range feature — a
/// TWO-variable range aborts on integers (`can't use 7 to iterate over
/// more than one variable`), so the iterable domain must follow the
/// parsed binding arity (ingress-nginx `controller.containerPort` shape).
#[test]
fn destructured_range_excludes_integer_iteration() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            ports:
            {{- range $key, $value := .Values.containerPort }}
            - name: {{ $key }}
              containerPort: {{ $value }}
            {{- end }}
    "};
    let values_yaml = indoc! {"
        containerPort:
          http: 80
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "containerPort": { "http": 80 } })
        ),
        "map iteration renders: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "containerPort": 7 })),
        "a two-variable range cannot iterate an integer; rendering aborts: {schema}"
    );
}

/// A destructured range over a LITERAL dict binds the key variable's
/// exact domain, so `get map $k` selector reads resolve to the finite
/// member set, and the enabled-scoped secret-name sink typing stays
/// branch-scoped (signoz `smtpVars.existingSecret` shape). Scoping the
/// arm further to "some literal key set" needs per-key iteration or a
/// lowerable any-of guard — the no-keys state is deliberately unpinned.
#[test]
fn literal_dict_range_key_domain_decodes_get_conditions() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: main
              env:
                {{- if .Values.smtp.enabled }}
                {{- range $keyName, $envName := dict "fromKey" "SMTP_FROM" "hostKey" "SMTP_HOST" }}
                {{- $keyInSecret := get $.Values.smtp.existingSecret $keyName }}
                {{- if $keyInSecret }}
                - name: {{ $envName }}
                  valueFrom:
                    secretKeyRef:
                      name: {{ $.Values.smtp.existingSecret.name }}
                      key: {{ $keyInSecret }}
                {{- end }}
                {{- end }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        smtp:
          enabled: false
          existingSecret: {}
    "};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    for instance in [
        serde_json::json!({ "smtp": {
            "enabled": true,
            "existingSecret": { "fromKey": "smtp-from", "name": "creds" }
        } }),
        serde_json::json!({ "smtp": {
            "enabled": false,
            "existingSecret": { "fromKey": "smtp-from", "name": 7 }
        } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "named secrets and disabled SMTP render: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "smtp": {
                "enabled": true,
                "existingSecret": { "fromKey": "smtp-from", "name": 7 }
            } })
        ),
        "a configured key renders the secretKeyRef and its name sink \
         requires a string: {schema}"
    );
}

/// jenkins' `JCasC` config: a guarded document-level `range $key, $val`
/// aborts rendering on scalar collections (the two-variable form never
/// admits Go's integer ranging), so the iterable requirement binds behind
/// the guard instead of vanishing with it.
#[test]
fn guarded_destructured_range_rejects_scalar_collections() {
    let src = indoc! {r"
        {{- if .Values.autoReload }}
        {{- range $key, $val := .Values.configScripts }}
        {{- if $val }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: cfg-{{ $key }}
        data:
          config.yaml: |-
            {{- $val | nindent 4 }}
        {{- end }}
        {{- end }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            autoReload: true
            configScripts: {}
        "}),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "autoReload": true, "configScripts": 7 }),
            false,
        ),
        (
            serde_json::json!({ "autoReload": true, "configScripts": { "a": "b: c" } }),
            true,
        ),
        (
            serde_json::json!({ "autoReload": false, "configScripts": 7 }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "a two-variable range needs a real collection where its guard holds: \
             instance={instance}; schema={schema}"
        );
    }
}

/// jenkins' additionalAgents: `hasKey` and `merge` consume each ranged
/// member as a mapping, so scalar members abort rendering and the member
/// requirement must say object.
#[test]
fn ranged_member_map_consumers_reject_scalar_members() {
    let helpers = indoc! {r#"
        {{- define "test.podTemplate" -}}
        template: {{ .Values.agent.image | default "agent" }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          templates: |-
            {{- $agent := .Values.agent }}
            {{- range $name, $additionalAgent := .Values.additionalAgents }}
              {{- $additionalContainersEmpty := and (hasKey $additionalAgent "additionalContainers") (empty $additionalAgent.additionalContainers) }}
              {{- $additionalAgent := merge $additionalAgent $agent }}
              {{- $_ := set $.Values "agent" $additionalAgent }}
              {{- include "test.podTemplate" $ | nindent 4 }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            agent: {}
            additionalAgents: {}
        "}),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "additionalAgents": { "audit": 7 } }),
            false,
        ),
        (
            serde_json::json!({ "additionalAgents": { "kaniko": { "image": "k" } } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "map consumers of a ranged member reject scalar members: \
             instance={instance}; schema={schema}"
        );
    }
}

/// jenkins ranges `configScripts` under BOTH sidecar-reload states (the
/// jcasc `ConfigMaps` when auto-reload is on, the startup script when it is
/// off): complementary guards must compose into an unconditional iterable
/// requirement, not cancel each other.
#[test]
fn complementary_guarded_ranges_keep_the_iterable_requirement() {
    let src = indoc! {r"
        {{- if .Values.autoReload }}
        {{- range $key, $val := .Values.configScripts }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: cfg-{{ $key }}
        data:
          config.yaml: |-
        {{ tpl $val $| indent 4 }}
        {{- end }}
        {{- end }}
        {{- if not .Values.autoReload }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: static-config
        data:
          {{- range $key, $val := .Values.configScripts }}
          {{ $key }}.yaml: |-
        {{ tpl $val $| indent 4 }}
          {{- end }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            autoReload: true
            configScripts: {}
        "}),
    );
    for (instance, want) in [
        (
            serde_json::json!({ "autoReload": true, "configScripts": 7 }),
            false,
        ),
        (
            serde_json::json!({ "autoReload": false, "configScripts": 7 }),
            false,
        ),
        (
            serde_json::json!({ "autoReload": true, "configScripts": { "a": "b: c" } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "complementary range guards keep the collection contract: \
             instance={instance}; schema={schema}"
        );
    }
}
