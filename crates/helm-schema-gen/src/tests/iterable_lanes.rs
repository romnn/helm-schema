use super::*;

/// a range body that reads MEMBER STRUCTURE (`.tls` on each item)
/// constrains every iterable lane — array items and map values must be
/// objects, and positive integer iteration produces integer members that
/// fail the access (surveyor `config.jetstream.accounts` shape).
#[test]
fn range_member_structure_constrains_all_iterable_lanes() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range .Values.accounts }}
          {{ .tls }}: enabled
          {{- end }}
    "};
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

/// string body: a range body that feeds each member to a STRING
/// consumer (`tpl $arg $`) requires string members on every lane — scalar
/// non-string items and integer iteration (int members) abort rendering
/// (jaeger `args` / jenkins `installPlugins` shape).
#[test]
fn range_string_consumer_constrains_all_iterable_lanes() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- range $index, $arg := .Values.args }}
          arg{{ $index }}: {{ tpl $arg $ | quote }}
          {{- end }}
    "};
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

/// guarded lane: the branch-scoped iterable domain of a GUARDED
/// two-variable range excludes integers too (kyverno/prometheus extraArgs
/// shape — the range sits under an enable guard).
#[test]
fn guarded_destructured_range_excludes_integer_iteration() {
    let src = indoc! {r"
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
    "};
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

/// a strict consumer under an UNDECODABLE outer guard (semverCompare)
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

/// control: the SAME consumer under a decodable guard keeps its
/// branch-scoped contract — abstention is only for guards the encoding
/// cannot represent.
#[test]
fn decodable_guard_keeps_child_string_contract() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: legacy
        data:
          airflow.cfg: |
            base_url = {{ trunc 63 .Values.baseUrl }}
        {{- end }}
    "};
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

/// hint degradation: when an approximate guard poisons a path's
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

/// bitnami's `common.images.pullSecrets` shape: a helper that ranges a
/// call-dict field's MEMBER (`.global.imagePullSecrets`) and then feeds a
/// SHARED accumulator from a second loop. The range header aborts on a
/// non-rangeable subject no matter what the body renders, so the iterable
/// claim must ride the header READ — the accumulator join buries the
/// rendered rows' range conjuncts inside `any_of` alternatives (the signoz
/// zookeeper re-widening).
#[test]
fn shared_accumulator_helper_ranges_bind_each_source_iterable_domain() {
    let helpers = indoc! {r#"
        {{- define "repro.pullSecrets" -}}
          {{- $pullSecrets := list }}
          {{- if .global }}
            {{- range .global.imagePullSecrets -}}
              {{- $pullSecrets = append $pullSecrets . -}}
            {{- end -}}
          {{- end }}
          {{- range .images -}}
            {{- range .pullSecrets -}}
              {{- $pullSecrets = append $pullSecrets . -}}
            {{- end -}}
          {{- end -}}
          {{- if (not (empty $pullSecrets)) }}
        imagePullSecrets:
            {{- range $pullSecrets | uniq }}
          - name: {{ . }}
            {{- end }}
          {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: StatefulSet
        metadata:
          name: repro
        spec:
          template:
            spec:
              {{- include "repro.pullSecrets" (dict "images" (list .Values.image) "global" .Values.global) | nindent 6 }}
              containers:
                - name: repro
                  image: {{ .Values.image.repository }}
    "#};
    let values_yaml = "global:\n  imageRegistry: \"\"\nimage:\n  repository: docker.io/repro\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (value, label) in [
        (serde_json::json!("oops"), "a truthy scalar"),
        (serde_json::json!(""), "the empty string"),
        (serde_json::json!(false), "a raw false"),
    ] {
        let instance = serde_json::json!({
            "global": { "imageRegistry": "", "imagePullSecrets": value },
            "image": { "repository": "docker.io/repro" },
        });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "{label} cannot be ranged; rendering aborts: instance={instance}; schema={schema}"
        );
    }
    for (value, label) in [
        (serde_json::json!(["secret"]), "an array"),
        (serde_json::json!({ "a": "b" }), "a map"),
        (serde_json::json!(null), "an explicit null"),
    ] {
        let instance = serde_json::json!({
            "global": { "imageRegistry": "", "imagePullSecrets": value },
            "image": { "repository": "docker.io/repro" },
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "{label} iterates (or skips) cleanly: instance={instance}; schema={schema}"
        );
    }
}

/// datadog's orchestrator custom-resources shape: a range over a binding
/// that SELECTS a fallback (`$crs := .Values.x | default list`) iterates
/// the fallback on every Helm-falsy input, so falsy scalars render through
/// the empty-list arm — while the selection chain still binds the truthy
/// arm exactly (`truthy(x) ⇒ iterable(x)`: a truthy scalar is selected and
/// aborts the range).
#[test]
fn fallback_selected_bindings_leave_the_source_unranged() {
    let helpers = indoc! {r#"
        {{- define "repro.customResources" -}}
        {{- $customResources := .Values.custom.resources | default list -}}
        {{- range $cr := $customResources -}}
        - {{ $cr }}
        {{ end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          resources: |
        {{ include "repro.customResources" . | indent 4 }}
    "#};
    let values_yaml = "custom:\n  resources: []\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (value, label) in [
        (serde_json::json!(""), "the empty string"),
        (serde_json::json!(false), "a raw false"),
        (serde_json::json!(0), "a raw zero"),
        (serde_json::json!(["a"]), "an array"),
    ] {
        let instance = serde_json::json!({ "custom": { "resources": value } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "{label} selects the list fallback (or iterates); rendering \
             proceeds: instance={instance}; schema={schema}"
        );
    }
    let instance = serde_json::json!({ "custom": { "resources": "oops" } });
    assert!(
        !schema_accepts_instance(&schema, &instance),
        "a truthy scalar is selected past the fallback and cannot be \
         ranged: instance={instance}; schema={schema}"
    );
}

/// kyverno's labels-merge shape: a helper ranging its BARE DOT over a
/// derived list of rendered fragments (`list ... (toYaml .Values.x)`)
/// iterates the derivation, not the influencing path — a scalar there
/// serializes fine and must stay accepted.
#[test]
fn bare_dot_ranges_over_derived_lists_leave_influences_open() {
    let helpers = indoc! {r#"
        {{- define "repro.labels.merge" -}}
        {{- $labels := dict -}}
        {{- range . -}}
          {{- $labels = merge $labels (fromYaml .) -}}
        {{- end -}}
        {{- with $labels -}}
          {{- toYaml $labels -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
          labels:
            {{- include "repro.labels.merge" (list "app: repro" (toYaml .Values.customLabels)) | nindent 4 }}
        data:
          a: "1"
    "#};
    let values_yaml = "customLabels: {}\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (value, label) in [
        (serde_json::json!("__junk__"), "a scalar"),
        (serde_json::json!(""), "the empty string"),
        (serde_json::json!({ "team": "x" }), "a map"),
    ] {
        let instance = serde_json::json!({ "customLabels": value });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "{label} only feeds toYaml; the range iterates the literal \
             list: instance={instance}; schema={schema}"
        );
    }
}

/// kyverno's per-controller pull-secrets shape: `with A | default B`
/// passes the SELECTED value into a helper that ranges its bare dot, so
/// each candidate owes an iterable shape exactly on its selected states —
/// `truthy(A) ⇒ iterable(A)` and `¬truthy(A) ∧ truthy(B) ⇒ iterable(B)`.
/// A truthy scalar beside a selected collection stays accepted: the
/// selection provenance on the chain value keeps the per-candidate claims
/// off the self-truthy approximation.
#[test]
fn selection_chain_dots_bind_per_candidate_iterable_domains() {
    let helpers = indoc! {r#"
        {{- define "repro.sortedImagePullSecrets" -}}
        {{- if . -}}
        {{- $secrets := list -}}
        {{- range . -}}
        {{- $secrets = append $secrets .name -}}
        {{- end -}}
        {{- $sortedRefs := list -}}
        {{- range sortAlpha $secrets -}}
        {{- $sortedRefs = append $sortedRefs (dict "name" .) -}}
        {{- end -}}
        {{- toYaml $sortedRefs -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: repro
        spec:
          template:
            spec:
              {{- with .Values.admissionController.imagePullSecrets | default .Values.global.imagePullSecrets }}
              imagePullSecrets:
                {{- tpl (include "repro.sortedImagePullSecrets" .) $ | nindent 8 }}
              {{- end }}
              containers:
                - name: repro
                  image: nginx
    "#};
    let values_yaml =
        "global:\n  imagePullSecrets: []\nadmissionController:\n  imagePullSecrets: []\n";
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (admission, global, label) in [
        (
            serde_json::json!("oops"),
            serde_json::json!([]),
            "a truthy scalar primary is selected and cannot be ranged",
        ),
        (
            serde_json::json!([]),
            serde_json::json!("oops"),
            "a truthy scalar fallback is selected past the falsy primary",
        ),
    ] {
        let instance = serde_json::json!({
            "admissionController": { "imagePullSecrets": admission },
            "global": { "imagePullSecrets": global },
        });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "{label}; rendering aborts: instance={instance}; schema={schema}"
        );
    }
    for (admission, global, label) in [
        (
            serde_json::json!([{ "name": "a" }]),
            serde_json::json!("oops"),
            "a truthy scalar fallback beside a selected list is never ranged",
        ),
        (
            serde_json::json!([]),
            serde_json::json!([{ "name": "b" }]),
            "a selected list fallback iterates",
        ),
        (
            serde_json::json!(""),
            serde_json::json!(null),
            "both candidates falsy skip the with-body",
        ),
    ] {
        let instance = serde_json::json!({
            "admissionController": { "imagePullSecrets": admission },
            "global": { "imagePullSecrets": global },
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "{label}; rendering proceeds: instance={instance}; schema={schema}"
        );
    }
}

/// the inline flavor of the selection-chain decode: a template-scope
/// `with A | default B` whose body ranges the bare dot binds the same
/// per-candidate iterable domains without a helper boundary.
#[test]
fn inline_selection_chain_ranges_bind_per_candidate_iterable_domains() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: repro
        spec:
          template:
            spec:
              {{- with .Values.controller.imagePullSecrets | default .Values.global.imagePullSecrets }}
              imagePullSecrets:
                {{- range . }}
                - name: {{ .name }}
                {{- end }}
              {{- end }}
              containers:
                - name: repro
                  image: nginx
    "};
    let values_yaml = "global:\n  imagePullSecrets: []\ncontroller:\n  imagePullSecrets: []\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let reject = serde_json::json!({
        "controller": { "imagePullSecrets": [] },
        "global": { "imagePullSecrets": "oops" },
    });
    assert!(
        !schema_accepts_instance(&schema, &reject),
        "the selected truthy scalar fallback cannot be ranged: schema={schema}"
    );
    let accept = serde_json::json!({
        "controller": { "imagePullSecrets": [{ "name": "a" }] },
        "global": { "imagePullSecrets": "oops" },
    });
    assert!(
        schema_accepts_instance(&schema, &accept),
        "a truthy scalar fallback beside a selected list is never ranged: schema={schema}"
    );
}
