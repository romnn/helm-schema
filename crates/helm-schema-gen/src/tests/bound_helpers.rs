use test_util::prelude::sim_assert_eq;

use super::*;

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
fn template_call_stringifies_the_helper_value_before_provider_projection() {
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

    for (value, want, label) in [
        (
            serde_json::json!({ "safe": "value" }),
            true,
            "a safely formatted mapping",
        ),
        (serde_json::json!("custom"), true, "an ordinary string"),
        (serde_json::json!(7), false, "a numeric YAML token"),
        (serde_json::json!(["custom"]), false, "a flow sequence"),
        (serde_json::json!(null), true, "the helper's falsy fallback"),
    ] {
        let instance = serde_json::json!({ "fullnameOverride": value });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "helper stringification preimage ({label}): instance={instance}; schema={schema}"
        );
    }

    let mut ordinary_string_exclusions = plain_token_exclusions(true);
    ordinary_string_exclusions.extend([
        serde_json::json!({ "not": { "pattern": "^(|~|null|Null|NULL)$" } }),
        serde_json::json!({
            "not": {
                "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$"
            }
        }),
        serde_json::json!({
            "not": {
                "pattern": "^([0-9][0-9_]{0,50}(\\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*[0-9][0-9_]{0,50}(\\.[0-9_]{0,50})?([eE][+-]?[0-9]{1,2})?|[+-]_*\\._*[0-9][0-9_]{0,50}([eE][+-]?[0-9]{1,2})?|\\.[0-9]{1,50}([eE][+-]?[0-9]{1,2})?)$"
            }
        }),
        serde_json::json!({
            "not": {
                "pattern": "^[+-]?(0[xX][0-9a-fA-F]{1,15}|0[bB][01]{1,62}|0[oO][0-7]{1,20})$"
            }
        }),
        serde_json::json!({
            "not": {
                "pattern": "^([+-]?\\.(inf|Inf|INF)|\\.(nan|NaN|NAN))$"
            }
        }),
    ]);

    sim_assert_eq!(
        have: schema,
        want: expected_values_schema(
            serde_json::Map::from_iter([(
                "fullnameOverride".to_string(),
                serde_json::json!({
                    "anyOf": [
                        crate::resolve_policy::printf_string_formattable_mapping_schema(),
                        {
                            "type": "string",
                            "description": "Name must be unique within a namespace. Is required when creating resources, although some resources may allow a client to request the generation of an appropriate name automatically. Name is primarily intended for creation idempotence and configuration definition. Cannot be updated. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/names#names",
                            "allOf": ordinary_string_exclusions
                        },
                        crate::resolve_policy::plain_scalar_safe_comment_string_schema(),
                        { "not": { "$ref": "#/$defs/t" } },
                    ]
                }),
            )]),
            Vec::new(),
            true,
        )
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

    sim_assert_eq!(have: kube_version, want: &serde_json::json!({}));
}
