use super::*;

/// A total stringification is neutral evidence about its own input; an
/// INDEPENDENT unconditional string consumer still binds. Cilium's
/// `cluster.name` is quoted into the configmap, but `replace` also consumes
/// it in validation logic — a map value fails `helm template` there.
#[test]
fn stringified_use_keeps_unconditional_string_transform_contract() {
    let src = indoc! {r#"
        {{- if gt (len (.Values.cluster.name | replace "-" "")) 30 }}
        {{- fail "cluster name too long" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          cluster-name: {{ .Values.cluster.name | quote }}
    "#};
    let values_yaml = indoc! {"
        cluster:
          name: default
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": "prod" } })
        ),
        "string cluster names render: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": { "bad": true } } })
        ),
        "replace consumes the raw name, so a map fails rendering and must be rejected: {schema}"
    );
}

/// Mutually exclusive guarded uses lower their own domains under their own
/// conditions (falco's `rolearn`): the quote branch renders anything, the
/// b64enc branch fails rendering for non-strings.
#[test]
fn quote_branch_does_not_erase_b64enc_branch_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.aws.useirsa }}
          role-arn: {{ .Values.aws.rolearn | quote }}
          {{- else }}
          AWS_ROLEARN: "{{ .Values.aws.rolearn | b64enc }}"
          {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        aws:
          useirsa: true
          rolearn: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // The b64enc contract rides its own row's condition: it binds only
    // where that branch renders. In the quote branch the same map renders
    // fine (Helm prints it as text).
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": true, "rolearn": { "bad": true } } })
        ),
        "the quote branch renders any value: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": false, "rolearn": { "bad": true } } })
        ),
        "the b64enc branch rejects non-strings: {schema}"
    );
    for useirsa in [true, false] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({ "aws": { "useirsa": useirsa, "rolearn": "arn:aws:iam::1:role/x" } })
            ),
            "strings render in both branches (useirsa={useirsa}): {schema}"
        );
    }
}

/// A `join` occurrence proves nothing about OTHER occurrences: sealed-secrets
/// also `range`s `additionalNamespaces` under its namespaced-roles flag, and
/// a scalar fails that render (`range can\'t iterate over ns-a`).
#[test]
fn join_use_does_not_erase_range_branch() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.additionalNamespaces }}
          namespaces: {{ join "," .Values.additionalNamespaces | quote }}
          {{- end }}
        {{- if .Values.rbac.namespacedRoles }}
        {{- range .Values.additionalNamespaces }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: role-{{ . }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
        rbac:
          namespacedRoles: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "additionalNamespaces": "ns-a" })
        ),
        "with namespaced roles off, only the join renders and a scalar is fine: {schema}"
    );
    for namespaces in [
        serde_json::json!(["ns-a"]),
        serde_json::json!({ "a": "ns-a" }),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": namespaces
                })
            ),
            "range iterates lists and maps: {schema}"
        );
    }
    // `range` cannot iterate a string, so `namespacedRoles=true` plus a
    // string fails `helm template` and the guarded iterable domain rejects
    // the combination.
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": "ns-a"
            })
        ),
        "inside the ranged branch a string cannot iterate: {schema}"
    );
    // Integer counts iterate (Helm's `--set` channel delivers int64; a
    // JSON Schema cannot separate that from the failing values-file
    // float64 spelling, so the renderable channel wins); non-integral
    // numbers fail in every channel.
    for count in [2, 0, -1] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": count
                })
            ),
            "range iterates integer counts: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": 2.5
            })
        ),
        "non-integral numbers cannot iterate: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "rbac": { "namespacedRoles": true } })
        ),
        "an absent collection ranges zero times and renders: {schema}"
    );
}

/// printf's format parameter is a real Go `string`: NFS provisioner calls
/// `printf .Values.storageClass.provisionerName`, and a non-string value
/// fails template evaluation (`wrong type for value; expected string`).
#[test]
fn dynamic_printf_format_requires_string() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ printf .Values.storageClass.provisionerName }}
    "#};
    let values_yaml = indoc! {"
        storageClass:
          provisionerName: cluster.local/provisioner
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": "x/y" } })
        ),
        "string formats evaluate: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": 7 } })
        ),
        "a non-string printf format fails template evaluation and must be rejected: {schema}"
    );
}

/// printf's data parameters render through any verb (Go fmt embeds
/// mismatches in the output): airflow formats `dags.gitSync.subPath` with a
/// literal format and Helm renders `subPath: 7` as `%!s(int64=7)`.
#[test]
fn printf_data_argument_accepts_any_value_through_helper_sink() {
    let helpers = indoc! {r#"
        {{- define "airflow_dags" -}}
        {{- printf "%s/dags/repo/%s" .Values.airflowHome .Values.dags.gitSync.subPath -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          dags_folder: {{ include "airflow_dags" . }}
    "#};
    let values_yaml = indoc! {r#"
        airflowHome: /opt/airflow
        dags:
          gitSync:
            subPath: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for sub_path in [
        serde_json::json!("repo/dags"),
        serde_json::json!(7),
        serde_json::json!(null),
    ] {
        let instance = serde_json::json!({ "dags": { "gitSync": { "subPath": sub_path } } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "printf data arguments render any value: instance={instance}; schema={schema}"
        );
    }
}

/// Chart repro (sealed-secrets `additionalNamespaces`): a declared-list
/// value joined under a self-truthy guard renders map and scalar values
/// through Sprig's singleton fallback, so the declared array type must not
/// reject them.
#[test]
fn self_guarded_join_of_declared_list_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: controller
                  args:
                    {{- if .Values.additionalNamespaces }}
                    - --additional-namespaces
                    - {{ join "," .Values.additionalNamespaces | quote }}
                    {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for probe in [
        serde_json::json!(["ns-a", "ns-b"]),
        serde_json::json!("ns-a"),
        serde_json::json!({ "k": "v" }),
    ] {
        let instance = serde_json::json!({ "additionalNamespaces": probe });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strslice converts any joined input: instance={instance}; schema={schema}"
        );
    }
}

/// Chart repro (grafana `sidecar.alerts.skipTlsVerify`): an undeclared
/// value quoted into a typed string sink (`env[].value`) under a `with`
/// guard renders any type, so the sink typing must not flow back through the
/// stringification.
#[test]
fn with_guarded_quote_into_string_sink_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: sidecar
                  env:
                    {{- with .Values.sidecar.skipTlsVerify }}
                    - name: SKIP_TLS_VERIFY
                      value: {{ quote . }}
                    {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("sidecar: {}\n"));

    for probe in [
        serde_json::json!(true),
        serde_json::json!("true"),
        serde_json::json!({ "k": "v" }),
        serde_json::json!([1, 2]),
    ] {
        let instance = serde_json::json!({ "sidecar": { "skipTlsVerify": probe } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "quote erases input shape at the env sink: instance={instance}; schema={schema}"
        );
    }
}

/// `htpasswd` bcrypt-hashes two Go strings, so a non-string member value
/// aborts rendering — including through a destructured range and a helper
/// include (prometheus-pushgateway's `basicAuthUsers`).
#[test]
fn htpasswd_operands_require_strings() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          direct: {{ htpasswd "" .Values.adminPassword | quote }}
          config: |
            {{- include "repro.webConfiguration" . | nindent 4 }}
    "#};
    let helpers = indoc! {r#"
        {{- define "repro.webConfiguration" -}}
        basic_auth_users:
        {{- range $k, $v := .Values.basicAuthUsers }}
          {{ $k }}: {{ htpasswd "" $v | trimPrefix ":" }}
        {{- end }}
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("adminPassword: hunter2\nbasicAuthUsers: {}\n"),
    );

    for (instance, want) in [
        (serde_json::json!({ "adminPassword": 7 }), false),
        (serde_json::json!({ "adminPassword": "ok" }), true),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": 7 } }),
            false,
        ),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": { "bad": 1 } } }),
            false,
        ),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": "hunter2" } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "htpasswd consumes Go strings only: instance={instance}; schema={schema}"
        );
    }
}

/// A direct `tpl` program input keeps its Go string contract through a
/// `default` selection chain: `tpl` parses the RAW value before any
/// truthiness selection runs, so a map aborts rendering even when its
/// Helm-falsy spelling would select a later arm (oauth2-proxy's
/// `tpl .Values.image.registry $ | default (tpl .Values.global.imageRegistry $) | default "quay.io"`).
#[test]
fn tpl_program_contract_survives_default_chain() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: "{{ tpl .Values.image.registry $ | default (tpl .Values.global.imageRegistry $) | default "quay.io" }}/proxy"
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some("image:\n  registry: \"\"\nglobal:\n  imageRegistry: \"\"\n"),
    );

    for (instance, want) in [
        (serde_json::json!({ "image": { "registry": {} } }), false),
        (serde_json::json!({ "image": { "registry": ["x"] } }), false),
        (
            serde_json::json!({ "image": { "registry": "quay.io" } }),
            true,
        ),
        (serde_json::json!({ "image": { "registry": "" } }), true),
        // The eagerly evaluated fallback arm parses its own program too
        (
            serde_json::json!({ "global": { "imageRegistry": {} } }),
            false,
        ),
        (
            serde_json::json!({ "global": { "imageRegistry": "ghcr.io" } }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "tpl parses raw program text before default selection: \
             instance={instance}; schema={schema}"
        );
    }
}
