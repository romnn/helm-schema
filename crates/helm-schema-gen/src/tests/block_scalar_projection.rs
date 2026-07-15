use test_util::prelude::sim_assert_eq;

use super::*;

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
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "presets": {
                        "type": "object"
                    }
                }
            }
        ],
        "properties": {
            "presets": {
                "type": "object",
                "additionalProperties": {},
                "properties": {
                    "clusterMetrics": {
                        "type": "object",
                        "additionalProperties": {},
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
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "presets": {
                        "type": "object"
                    }
                }
            }
        ],
        "properties": {
            "presets": {
                "type": "object",
                "additionalProperties": {},
                "properties": {
                    "clusterMetrics": {
                        "type": "object",
                        "additionalProperties": {},
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

    for (instance, want, label) in [
        (
            serde_json::json!({
                "global": { "storageClass": null },
                "persistence": { "storageClass": "fast" }
            }),
            true,
            "string fallback",
        ),
        (
            serde_json::json!({
                "global": { "storageClass": null },
                "persistence": { "storageClass": 7 }
            }),
            false,
            "invalid selected fallback",
        ),
        (
            serde_json::json!({
                "global": { "storageClass": "fast" },
                "persistence": { "storageClass": 7 }
            }),
            true,
            "shadowed invalid fallback",
        ),
        (
            serde_json::json!({
                "global": { "storageClass": 7 },
                "persistence": { "storageClass": "fast" }
            }),
            false,
            "invalid selected primary",
        ),
        (
            serde_json::json!({
                "global": { "storageClass": "-" },
                "persistence": { "storageClass": null }
            }),
            true,
            "special unset marker",
        ),
        (
            serde_json::json!({
                "global": { "storageClass": null },
                "persistence": { "storageClass": null }
            }),
            true,
            "empty effective value",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
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

    // Undeclared members of a spliced fragment are passthrough: typing them
    // as the merge of declared property schemas rejected legitimate keys
    // whose shape differs from the declared ones (`requests: {cpu: "1",
    // "nvidia.com/gpu": 2}` renders fine).
    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "resources": {
                "type": "object",
                "additionalProperties": {},
                "properties": {
                    "requests": {
                        "type": "object",
                        "additionalProperties": {},
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
    let map_arm = ranged_arm_of_type(annotations, "object")
        .unwrap_or_else(|| panic!("annotations object arm missing, got {annotations}"));
    sim_assert_eq!(
        have: map_arm
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

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "globalImage": {
                    "repository": "repo/app",
                    "tag": 1.25
                },
                "jobs": { "first": {} }
            })
        ),
        "the fallback image accepts scalar tag interpolation: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "globalImage": {
                    "repository": "repo/app",
                    "tag": { "bad": true }
                },
                "jobs": { "first": {} }
            })
        ),
        "the range-member-dependent fallback cannot be lowered safely, so its tag contract must abstain: {schema}"
    );
}

#[test]
fn ranged_with_defaulted_object_body_abstains_on_cross_range_fallback() {
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

    // The `with` subject chooses between a ranged member and a root
    // fallback. Draft 7 cannot express that cross-collection relation, and
    // the quoted partial scalar stringifies every selected tag shape. Keep
    // the fallback open instead of attributing the member arm's shape to it.
    for instance in [
        serde_json::json!({
            "image": { "tag": "app-version" },
            "migrations": {
                "databases": { "first": {} },
                "image": {
                    "repository": "repo/app",
                    "pullPolicy": "Always",
                    "tag": 7
                }
            }
        }),
        serde_json::json!({
            "image": { "tag": "app-version" },
            "migrations": {
                "databases": { "first": { "image": {
                    "repository": "member/app",
                    "tag": "member",
                    "pullPolicy": "Always"
                } } },
                "image": { "tag": 7 }
            }
        }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "an unlowerable member/fallback choice must abstain: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
}
