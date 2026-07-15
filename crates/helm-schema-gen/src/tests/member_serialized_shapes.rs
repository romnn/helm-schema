use super::*;

/// F36: an `else` arm that EXECUTES a member access closes the unmatched
/// scalar domain — `typeIs "string"` dispatch with a structural complement
/// must reject values neither arm renders (external-dns provider shape).
#[test]
fn executing_else_member_access_closes_unmatched_scalar_domain() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if typeIs "string" .Values.provider }}
          provider: {{ .Values.provider }}
          {{- else }}
          provider: {{ .Values.provider.name }}
          {{- end }}
    "#};
    let values_yaml = "provider: aws\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "provider": "aws" }),
        serde_json::json!({ "provider": { "name": "aws" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "both dispatch arms render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "provider": 7 })),
        "the else arm dereferences `.name`, so a non-string scalar fails \
         rendering and must be rejected: {schema}"
    );
}

/// F37: a type-dispatch complement nested under outer enable guards must
/// scope its object requirement to the complement arm — the string arm
/// stays valid when the outer guards are ACTIVE (cilium SPIRE image shape).
#[test]
fn nested_type_dispatch_keeps_string_arm_under_active_outer_guards() {
    let src = indoc! {r#"
        {{- if .Values.monitoring.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if typeIs "string" .Values.image }}
          image: {{ .Values.image }}
          {{- else }}
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
          {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        monitoring:
          enabled: false
        image:
          repository: repo
          tag: latest
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "monitoring": { "enabled": true }, "image": "repo:1.2" })
        ),
        "the string arm renders under active outer guards; the complement \
         arm's object shape must not leak over it: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "monitoring": { "enabled": true },
                "image": { "repository": "repo", "tag": "1.2" }
            })
        ),
        "the object arm stays valid under active outer guards: {schema}"
    );
}

/// F41: `with` rebinds dot, and a `typeOf .` dispatch inside the body must
/// bind to the originating value path — the executing `else` places dot
/// structurally, closing the unmatched scalar domain (minio
/// extraContainers shape).
#[test]
fn with_rebound_dot_type_dispatch_binds_source_path() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            {{- with .Values.extraContainers }}
            {{- if eq (typeOf .) "string" }}
            {{- tpl . $ | nindent 4 }}
            {{- else }}
            {{- toYaml . | nindent 4 }}
            {{- end }}
            {{- end }}
    "#};
    let values_yaml = "extraContainers: []\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "extraContainers": "- name: extra" }),
        serde_json::json!({ "extraContainers": [{ "name": "extra" }] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "string and structured arms both render: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "extraContainers": 7 })),
        "a non-string scalar reaches the structural else placement and \
         renders invalid YAML; it must be rejected: {schema}"
    );
}

/// F47: an UNDECLARED selector-style object observed through member reads
/// must stay open — reads prove keys exist, they do not bound the member
/// set (nats-account-server `credentials.secret` shape, where `name` and
/// `key` are read from different templates).
#[test]
fn partially_observed_selector_object_stays_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          volumes:
          - name: creds
            secret:
              secretName: {{ .Values.credentials.secret.name }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "credentials": { "secret": { "name": "n", "key": "k" } } })
        ),
        "a member read must not close the selector object to observed keys: {schema}"
    );
}

/// F46: a declared mapping the chart SERIALIZES (whole map or per-section)
/// is passthrough config — the declared default documents keys, it does not
/// bound them (grafana.ini / airflow config shape).
#[test]
fn serialized_declared_mapping_sections_stay_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          config.yaml: |
            {{- toYaml .Values.config | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        config:
          server:
            port: 80
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "config": {
                    "server": { "port": 80, "root_url": "http://x" },
                    "smtp": { "enabled": true, "host": "mail" }
                }
            })
        ),
        "serialized passthrough config must accept keys beyond the declared \
         default shape: {schema}"
    );
}

/// F46 (guarded-read sibling): the serialized fact must survive a truthy
/// member read at the same path — the exact coredns `service.clusterIPs`
/// verification shape from the plan, applied to a declared mapping.
#[test]
fn guard_read_beside_serialized_render_keeps_mapping_open() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.config.server }}
          port: {{ .Values.config.server.port }}
          {{- end }}
          config.yaml: |
            {{- toYaml .Values.config | nindent 4 }}
    "#};
    let values_yaml = indoc! {"
        config:
          server:
            port: 80
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "config": { "server": { "port": 80, "extra": 1 }, "another": {} }
            })
        ),
        "a truthy/member read beside the serialized render must not close \
         the mapping: {schema}"
    );
}

/// F48: an undeclared, truthy-guarded, `toYaml`-serialized leaf renders
/// LISTS as well as maps — it must not be pinned to `object` (coredns
/// `service.clusterIPs` / nats-operator `tolerations` shape).
#[test]
fn serialized_truthy_guarded_leaf_admits_arrays() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Widget
        metadata:
          name: test
        spec:
          {{- if .Values.service.clusterIPs }}
          clusterIPs:
          {{ toYaml .Values.service.clusterIPs | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        service: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "service": { "clusterIPs": ["10.96.0.10"] } })
        ),
        "a serialized guarded leaf renders arrays; the schema must not pin \
         it to object: {schema}"
    );
}

/// F49: a scalar spliced into a CLI-flag string slot renders for ANY
/// scalar — the empty-string declared default is intent, not a constraint
/// (nack `jetstream.klogLevel` shape, `- -v=8` renders).
#[test]
fn flag_splice_accepts_any_scalar_beyond_declared_string() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            args:
            - -v={{ .Values.klogLevel }}
    "#};
    let values_yaml = "klogLevel: \"\"\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "klogLevel": 8 }),
        serde_json::json!({ "klogLevel": "8" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "flag splices print any scalar: instance={instance}; schema={schema}"
        );
    }
}

/// F49: a declared BOOLEAN spliced into a flag slot accepts the string
/// form too (nack `readOnly` shape, `--read-only=true` renders either way).
#[test]
fn declared_boolean_flag_splice_accepts_string_form() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            args:
            - --read-only={{ .Values.readOnly }}
    "#};
    let values_yaml = "readOnly: false\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "readOnly": true }),
        serde_json::json!({ "readOnly": "true" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a flag splice prints booleans and strings alike: instance={instance}; schema={schema}"
        );
    }
}

/// F49: a declared boolean rendered inside a QUOTED string value accepts
/// the string form (nfs-subdir-external-provisioner `archiveOnDelete`
/// shape: `archiveOnDelete: "{{ .Values.storageClass.archiveOnDelete }}"`).
#[test]
fn quoted_string_slot_widen_declared_boolean_to_scalars() {
    let src = indoc! {r#"
        apiVersion: storage.k8s.io/v1
        kind: StorageClass
        metadata:
          name: test
        parameters:
          archiveOnDelete: "{{ .Values.storageClass.archiveOnDelete }}"
    "#};
    let values_yaml = indoc! {"
        storageClass:
          archiveOnDelete: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "storageClass": { "archiveOnDelete": "false" } }),
        serde_json::json!({ "storageClass": { "archiveOnDelete": false } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a quoted scalar slot prints booleans and strings alike: instance={instance}; schema={schema}"
        );
    }
}

/// F48: a declared-`{}` self-guarded fragment splices whatever the user
/// supplies — `toYaml` renders sequences as readily as maps, so the
/// empty-map placeholder union needs the array arm (nats-kafka
/// `additionalVolumes` shape).
#[test]
fn declared_empty_map_guarded_fragment_admits_arrays() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Widget
        metadata:
          name: test
        spec:
          {{- if .Values.additionalVolumes }}
          volumes:
          {{- toYaml .Values.additionalVolumes | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = "additionalVolumes: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "additionalVolumes": [{ "name": "v" }] }),
        serde_json::json!({ "additionalVolumes": { "v": { "hostPath": "/" } } }),
        serde_json::json!({ "additionalVolumes": {} }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "toYaml fragments render lists and maps alike: instance={instance}; schema={schema}"
        );
    }
}

/// F50: a value consumed only through `tpl` (here via a `with`-bound dot
/// inside an included helper) is a template STRING — the schema must keep
/// the string form valid (airflow `extraEnv` shape, declared `~`).
#[test]
fn with_dot_tpl_keeps_string_form_valid() {
    let helper_src = indoc! {r#"
        {{- define "test.env" }}
          {{- with .Values.extraEnv }}
            {{- tpl . $ | nindent 2 }}
          {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          env: |
            {{- include "test.env" . | nindent 4 }}
    "#};
    let values_yaml = "extraEnv: ~\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helper_src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "extraEnv": "- name: FOO\n  value: bar" }),
        serde_json::json!({ "extraEnv": null }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "tpl consumes a template string; the string and declared-null \
             forms stay valid: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "extraEnv": { "a": 1 } })),
        "a truthy non-string reaches `tpl` and aborts rendering: {schema}"
    );
}

/// F50: a values-declared OBJECT that only renders under its own truthy
/// guard accepts explicit `null` — helm null-deletion removes the key and
/// the falsy guard skips the branch (datadog `datadog.securityContext`
/// shape, declared `{runAsUser: 0}`).
#[test]
fn self_guarded_declared_object_accepts_explicit_null() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          {{- if .Values.securityContext }}
          securityContext:
            {{- toYaml .Values.securityContext | nindent 4 }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        securityContext:
          runAsUser: 0
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "securityContext": null }),
        serde_json::json!({ "securityContext": { "runAsUser": 0 } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "helm null-deletion plus the falsy self-guard makes explicit \
             null render fine: instance={instance}; schema={schema}"
        );
    }
}

/// F34 (trivy half): literal-key `dig` evaluates structurally — sprig
/// type-asserts every step, so the subject carries a truthy⇒object
/// contract while the dug leaf may be any type.
#[test]
fn literal_key_dig_binds_intermediate_object_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ dig "section" "leaf" "fallback" .Values.cfg }}
    "#};
    let values_yaml = "cfg: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "cfg": { "section": { "leaf": "seven" } } }),
        serde_json::json!({ "cfg": {} }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "dig walks maps and falls back on missing keys: instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "cfg": "scalar" })),
        "sprig `dig` type-asserts its subject to a map; a truthy non-map \
         aborts rendering: {schema}"
    );
}

/// F35: an `if (include …)` condition hole absorbs the called helper's
/// `kindIs` type-dispatch facts — the dispatched alternatives survive
/// beside the declared default shape (grafana hpa apiVersion shape).
#[test]
fn include_condition_absorbs_helper_type_dispatch_alternatives() {
    let helper_src = indoc! {r#"
        {{- define "test.enabled" -}}
        {{- if kindIs "map" .Values.autoscaling -}}
        {{- if .Values.autoscaling.enabled -}}
        true
        {{- end -}}
        {{- else if kindIs "string" .Values.autoscaling -}}
        {{- .Values.autoscaling -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if (include "test.enabled" .) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          enabled: "yes"
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        autoscaling:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helper_src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "autoscaling": { "enabled": true } }),
        serde_json::json!({ "autoscaling": "on" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "the helper's kindIs dispatch proves both forms render: \
             instance={instance}; schema={schema}"
        );
    }
}
