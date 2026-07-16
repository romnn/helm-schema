use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn helper_range_break_scopes_later_provider_candidates() {
    let helpers = indoc! {r#"
        {{- define "select.context" -}}
        {{- $result := dict -}}
        {{- range . -}}
          {{- if and (hasKey . "securityContexts") (hasKey .securityContexts "pod") .securityContexts.pod -}}
            {{- $result = .securityContexts.pod -}}
            {{- break -}}
          {{- end -}}
          {{- if and (hasKey . "securityContext") .securityContext -}}
            {{- $result = .securityContext -}}
            {{- break -}}
          {{- end -}}
        {{- end -}}
        {{- toYaml $result -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- $securityContext := include "select.context" (list .Values.worker .Values) -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          securityContext: {{ $securityContext | nindent 4 }}
          containers:
          - name: test
            image: busybox
    "#};
    let values_yaml = indoc! {"
        securityContext: {}
        securityContexts:
          pod: {}
        worker:
          securityContext: {}
          securityContexts:
            pod: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "worker": {
                    "securityContexts": { "pod": { "runAsUser": 50000 } },
                    "securityContext": 7
                }
            })
        ),
        "a later scalar candidate is dormant after the preferred branch breaks: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "worker": {
                    "securityContexts": { "pod": {} },
                    "securityContext": 7
                }
            })
        ),
        "the live scalar candidate reaches the object-typed Pod securityContext: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "worker": {
                    "securityContexts": { "pod": {} },
                    "securityContext": { "runAsUser": 50000 }
                }
            })
        ),
        "a valid fallback object remains accepted: {schema}"
    );
}

/// A helper that returns a YAML object and is decoded before placement keeps
/// each nested leaf's provider position; the outer `toYaml` only serializes
/// the constructed object and does not erase those leaf identities.
#[test]
fn decoded_yaml_helper_keeps_nested_provider_positions() {
    let helpers = indoc! {r#"
        {{- define "pod.template" -}}
        metadata:
          labels:
            app: test
        spec:
          {{- if not (kindIs "invalid" .Values.hostUsers) }}
          hostUsers: {{ .Values.hostUsers }}
          {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template: {{ include "pod.template" . | fromYaml | toYaml | nindent 4 }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("hostUsers: null\n"),
    );

    for value in [serde_json::json!(false), serde_json::json!(true)] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({ "hostUsers": value })),
            "Boolean PodSpec hostUsers values must validate: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "hostUsers": "audit" })),
        "a present string reaches the Boolean PodSpec field: {schema}"
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

/// A helper that substitutes the release namespace when an override is
/// Helm-empty must keep explicit null valid at every included sink.
#[test]
fn helper_truthy_branch_keeps_namespace_override_nullable() {
    let helpers = indoc! {r#"
        {{- define "sample.namespace" -}}
        {{- if .Values.namespaceOverride -}}
        {{- .Values.namespaceOverride -}}
        {{- else -}}
        {{- .Release.Namespace -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          namespace: {{ include "sample.namespace" . }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("namespaceOverride: \"\"\n"),
    );

    for instance in [
        serde_json::json!({ "namespaceOverride": null }),
        serde_json::json!({ "namespaceOverride": "audit" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the fallback handles null while a selected string reaches the sink: \
             instance={instance}; schema={schema}"
        );
    }
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

    for (instance, want, label) in [
        (serde_json::json!({ "nameOverride": null }), true, "null"),
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
            "nested label helper {label}: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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

    for (instance, want, label) in [
        (serde_json::json!({ "nameOverride": null }), true, "null"),
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
            "assigned helper input {label}: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
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

    // The self-guarded map also accepts explicit null (helm
    // null-deletion); the string-map shape rides the non-null arm.
    let pod_labels = schema
        .pointer("/properties/podLabels/anyOf/0")
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
