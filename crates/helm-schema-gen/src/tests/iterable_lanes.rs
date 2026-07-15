use super::*;

/// F59: a range body that reads MEMBER STRUCTURE (`.tls` on each item)
/// constrains every iterable lane — array items and map values must be
/// objects, and positive integer iteration produces integer members that
/// fail the access (surveyor `config.jetstream.accounts` shape).
#[test]
fn range_member_structure_constrains_all_iterable_lanes() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range .Values.accounts }}
          {{ .tls }}: enabled
          {{- end }}
    "#};
    let values_yaml = "accounts: ~
";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "accounts": [{ "tls": "on" }] }),
        serde_json::json!({ "accounts": { "A": { "tls": "on" } } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "object members provide `.tls`: instance={instance}; schema={schema}"
        );
    }
    for instance in [
        serde_json::json!({ "accounts": [7] }),
        serde_json::json!({ "accounts": { "A": 7 } }),
        serde_json::json!({ "accounts": 2 }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a scalar member cannot provide `.tls`; rendering aborts: \
             instance={instance}; schema={schema}"
        );
    }
}

/// F59 (string body): a range body that feeds each member to a STRING
/// consumer (`tpl $arg $`) requires string members on every lane — scalar
/// non-string items and integer iteration (int members) abort rendering
/// (jaeger `args` / jenkins `installPlugins` shape).
#[test]
fn range_string_consumer_constrains_all_iterable_lanes() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $index, $arg := .Values.args }}
          arg{{ $index }}: {{ tpl $arg $ | quote }}
          {{- end }}
    "#};
    let values_yaml = "args: ~
";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "args": ["--flag"] })),
        "string items feed tpl: {schema}"
    );
    for instance in [
        serde_json::json!({ "args": [7] }),
        serde_json::json!({ "args": 2 }),
    ] {
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a non-string member reaches tpl and aborts rendering: \
             instance={instance}; schema={schema}"
        );
    }
}

/// F58 (guarded lane): the branch-scoped iterable domain of a GUARDED
/// two-variable range excludes integers too (kyverno/prometheus extraArgs
/// shape — the range sits under an enable guard).
#[test]
fn guarded_destructured_range_excludes_integer_iteration() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: main
            args:
            {{- if .Values.server.enabled }}
            {{- range $key, $value := .Values.server.extraArgs }}
            - --{{ $key }}={{ $value }}
            {{- end }}
            {{- end }}
    "#};
    let values_yaml = indoc! {"
        server:
          enabled: false
          extraArgs: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "server": { "enabled": true, "extraArgs": { "a": "b" } } })
        ),
        "map iteration renders under the guard: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "server": { "enabled": true, "extraArgs": 7 } })
        ),
        "a two-variable range cannot iterate an integer in the live branch: {schema}"
    );
}

/// F64: a strict consumer under an UNDECODABLE outer guard (semverCompare)
/// must not bind its contract globally — with the shipped version the
/// branch is dead and the raw value renders through other paths (airflow
/// `config.webserver.base_url` shape).
#[test]
fn unlowerable_outer_guard_abstains_from_child_string_contract() {
    let src = indoc! {r#"
        {{- if semverCompare "<3.0.0" .Values.airflowVersion }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: legacy
        data:
          airflow.cfg: |
            base_url = {{ trunc 63 .Values.baseUrl }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        airflowVersion: "3.2.2"
        baseUrl: ~
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "baseUrl": { "a": 1 } })),
        "the semver guard cannot lower; the branch-scoped string contract \
         must abstain rather than bind globally: {schema}"
    );
}

/// F64 (control): the SAME consumer under a decodable guard keeps its
/// branch-scoped contract — abstention is only for guards the encoding
/// cannot represent.
#[test]
fn decodable_guard_keeps_child_string_contract() {
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: legacy
        data:
          airflow.cfg: |
            base_url = {{ trunc 63 .Values.baseUrl }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: false
        baseUrl: ~
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "enabled": true, "baseUrl": { "a": 1 } })
        ),
        "inside the live decodable branch the string contract holds: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "enabled": false, "baseUrl": { "a": 1 } })
        ),
        "outside the branch the raw value never reaches trunc: {schema}"
    );
}

/// F64 (hint degradation): when an approximate guard poisons a path's
/// conditional overlays, its branch-scoped "string" hint must stay a
/// widen-only guarded hint instead of degrading to path-level typing —
/// the unconditional total render proves non-strings pass (bitnami
/// postgresql `auth.password` through `common.secrets.passwords.manage`).
#[test]
fn approximate_guard_hints_stay_branch_scoped() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          plain: {{ .Values.password | toString | quote }}
          {{- if semverCompare ">=1.2.0" .Values.appVersion }}
          guarded: {{ .Values.password | default "pw" }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        password: ~
        appVersion: ~
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "password": "secret" })),
        "strings always render: {schema}"
    );
    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "password": 123 })),
        "the default-literal string hint lives behind a semver guard the \
         encoding cannot represent; it must not bind the base the total \
         stringification renders: {schema}"
    );
}
