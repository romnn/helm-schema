use test_util::prelude::sim_assert_eq;

use super::*;

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
    let signals = schema_signals_for(&ir);
    let secret_name = signals
        .evidence_for("ingress.tls.*.secretName")
        .expect("with-bound helper preserves ingress.tls member path");
    assert!(
        secret_name
            .conditional_overlays
            .iter()
            .any(|overlay| !overlay.evidence.provider_schema_uses.is_empty()),
        "with-bound helper keeps the guarded secretName provider use: {secret_name:#?}"
    );
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

    let instance = serde_json::json!({
        "admintools": {
            "podAnnotations": { "custom.example/annotation": "value" },
            "podLabels": { "custom.example/label": "value" }
        },
        "additionalAnnotations": { "custom.example/annotation": "value" },
        "additionalLabels": { "custom.example/label": "value" }
    });
    assert!(
        schema_accepts_instance(&schema, &instance),
        "helper-bound metadata maps should remain open to chart-defined string entries: instance={instance}; schema={schema}"
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

    let ir = parse_ir_with_helpers(src, helpers);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));
    let probe = schema
        .pointer("/properties/livenessProbe")
        .expect("livenessProbe present");

    assert!(
        schema_property_contains_type(probe, "initialDelaySeconds", "integer"),
        "omitted probe fragment should retain rendered Kubernetes Probe fields, got {probe}; ir={ir:#?}"
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
    for (instance, want, label) in [
        (
            serde_json::json!({ "nameOverride": "" }),
            true,
            "empty string",
        ),
        (
            serde_json::json!({ "nameOverride": {} }),
            true,
            "empty object",
        ),
        (
            serde_json::json!({ "nameOverride": "name" }),
            true,
            "string",
        ),
        (
            serde_json::json!({ "nameOverride": 7 }),
            false,
            "truthy number",
        ),
        (
            serde_json::json!({ "nameOverride": { "bad": true } }),
            false,
            "truthy object",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "helper-built matchLabels {label}: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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
    for (instance, want, label) in [
        (
            serde_json::json!({ "nameOverride": "" }),
            true,
            "empty string",
        ),
        (
            serde_json::json!({ "nameOverride": {} }),
            true,
            "empty object",
        ),
        (
            serde_json::json!({ "nameOverride": "name" }),
            true,
            "string",
        ),
        (
            serde_json::json!({ "nameOverride": 7 }),
            false,
            "truthy number",
        ),
        (
            serde_json::json!({ "nameOverride": { "bad": true } }),
            false,
            "truthy object",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "standard label merge {label}: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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
        have: source_ranges.pointer("/anyOf/0/type").and_then(Value::as_str),
        want: Some("array"),
        "loadBalancerSourceRanges should remain array-valued, got {source_ranges}"
    );
    sim_assert_eq!(
        have: source_ranges.pointer("/anyOf/0/items/type").and_then(Value::as_str),
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
    for (instance, want, label) in [
        (
            serde_json::json!({ "leaderElection": {} }),
            true,
            "empty map",
        ),
        (
            serde_json::json!({ "leaderElection": { "leaseDuration": "15s" } }),
            true,
            "string duration",
        ),
        (
            serde_json::json!({ "leaderElection": { "leaseDuration": 15 } }),
            true,
            "numeric duration",
        ),
        (
            serde_json::json!({
                "leaderElection": { "leaseDuration": { "bad": true } }
            }),
            true,
            "object duration",
        ),
        (
            serde_json::json!({ "leaderElection": false }),
            true,
            "falsy host",
        ),
        (
            serde_json::json!({ "leaderElection": 7 }),
            false,
            "truthy scalar host",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn mixed_inline_template_gaps_in_scalar_sequence_item_keep_textual_paths_open() {
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

    for (path, value) in [
        ("registry", serde_json::json!({ "host": "registry" })),
        ("repository", serde_json::json!(["example", "app"])),
        ("digest", serde_json::json!({ "algorithm": "sha256" })),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "image": { (path): value } }),),
            "a partial scalar formats {path} as text without imposing its input kind: {schema}"
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

    for (instance, want, label) in [
        (
            serde_json::json!({ "image": { "repository": "example/app" } }),
            true,
            "string repository",
        ),
        (
            serde_json::json!({ "image": { "repository": 7 } }),
            true,
            "numeric repository",
        ),
        (
            serde_json::json!({
                "image": { "repository": { "bad": true } }
            }),
            true,
            "object repository",
        ),
        (serde_json::json!({ "image": false }), true, "falsy image"),
        (
            serde_json::json!({ "image": 7 }),
            false,
            "truthy scalar image",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
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
    let signals = schema_signals_for(&ir);
    let secret_name = signals
        .evidence_for("ingress.tls.*.secretName")
        .expect("realistic helper preserves ingress.tls member path");
    assert!(
        secret_name
            .conditional_overlays
            .iter()
            .any(|overlay| !overlay.evidence.provider_schema_uses.is_empty()),
        "realistic helper keeps the guarded secretName provider use: {secret_name:#?}"
    );
    for path in [
        "ingress.tls.*.hosts.*",
        "ingress.hosts.*.host",
        "ingress.hosts.*.paths.*.path",
        "ingress.hosts.*.paths.*.pathType",
        "ingress.hosts.*.paths.*.serviceName",
        "ingress.hosts.*.paths.*.servicePort",
    ] {
        assert!(
            signals.evidence_for(path).is_some(),
            "realistic helper preserves nested input path {path}: {signals:#?}"
        );
    }
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
        property_schema_with_type_exists(&schema, "host", "string"),
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

    // `toYaml` preserves the object shape, including the provider's open
    // Quantity maps. Arbitrary resource names remain accepted while each
    // value keeps the provider's string-or-number domain.
    for member in ["requests", "limits"] {
        let node = schema
            .pointer(&format!("/properties/resources/properties/{member}"))
            .unwrap_or_else(|| panic!("resources.{member} present"));
        sim_assert_eq!(
            have: node.pointer("/additionalProperties"),
            want: Some(&serde_json::json!({
                "oneOf": [
                    { "type": "string" },
                    { "type": "number" }
                ]
            })),
            "resources.{member} stays an open map: {node}"
        );
    }
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
        template_supplied_member_keys: Default::default(),
        split_segment: None,
        merge_layers: None,
        range_key: false,
        omitted_members: Default::default(),
        outer_guards: Vec::new(),
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

/// A scalar spliced as a mapping KEY formats every scalar kind — a numeric
/// label key renders `7:` and YAML-to-JSON stringifies it — so the declared
/// string default widens to the scalar union while composite inputs stay
/// out of the key lane (external-secrets' `grafanaDashboard.sidecarLabel`).
#[test]
fn mapping_key_splice_accepts_every_scalar_kind() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          labels:
            {{ .Values.sidecarLabel }}: "1"
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("sidecarLabel: grafana_dashboard\n"));
    for (instance, want) in [
        (serde_json::json!({ "sidecarLabel": 7 }), true),
        (serde_json::json!({ "sidecarLabel": true }), true),
        (serde_json::json!({ "sidecarLabel": "ok" }), true),
        (serde_json::json!({ "sidecarLabel": { "bad": 1 } }), false),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "key positions format scalars and exclude composites: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A ranged member spliced as a whole fragment at column zero renders as
/// its own DOCUMENT, and Helm decodes every manifest as a mapping: present
/// non-null members must be objects while null members decode to empty
/// manifests (nats renders each `extraResources` item as a document).
#[test]
fn document_root_member_splices_require_object_items() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: anchor
        data: {}
        {{- range .Values.extraResources }}
        ---
        {{ . | toYaml }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraResources: []\n"));
    for (instance, want) in [
        (serde_json::json!({ "extraResources": [true] }), false),
        (serde_json::json!({ "extraResources": ["audit"] }), false),
        (
            serde_json::json!({ "extraResources": [{ "kind": "ConfigMap" }] }),
            true,
        ),
        (serde_json::json!({ "extraResources": [null] }), true),
        (serde_json::json!({ "extraResources": [] }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "document-root member splice: instance={instance}; schema={schema}"
        );
    }
}
