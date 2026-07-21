use super::*;

/// A `regexMatch` fail whose subject reached the match through `tpl` (a
/// raw template PROGRAM, not the rendered text) constrains only the
/// output: a values string carrying a template action is admitted (its
/// render may match), a matching literal is accepted, and an action-free
/// non-matching literal still terminates. redis-ha's `masterGroupName`
/// helper is the driving case.
#[test]
fn post_tpl_regex_admits_template_programs() {
    let helpers = indoc! {r#"
        {{- define "repro.masterGroupName" -}}
        {{- $name := tpl (.Values.masterGroupName | default "") . -}}
        {{- if regexMatch "^[\w.-]+$" $name -}}
        {{ $name }}
        {{- else -}}
        {{ required "a valid masterGroupName is required" "" }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ include "repro.masterGroupName" . }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), None);

    for (instance, want, label) in [
        (
            serde_json::json!({ "masterGroupName": "mymaster" }),
            true,
            "a literal matching the pattern renders",
        ),
        (
            serde_json::json!({ "masterGroupName": "{{ .Release.Name }}" }),
            true,
            "a template program is admitted (its render may match)",
        ),
        (
            serde_json::json!({ "masterGroupName": "bad group" }),
            false,
            "an action-free non-matching literal terminates",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// The grafana `assertNoLeakedSecrets` traversal: a folded literal table
/// of sensitive paths is ranged with per-item bindings, an indexed inner
/// range advances a traversal local under `hasKey` guards, and the last
/// segment applies a `regexMatch`-guarded `fail`. The traversal must
/// interpret to exact values paths: non-strings and plain strings at a
/// sensitive path reject while variable-expansion syntax and disabled
/// assertion render.
#[test]
fn literal_table_traversal_binds_pattern_validators() {
    let helpers = indoc! {r#"
        {{- define "repro.assertNoLeakedSecrets" -}}
          {{- $sensitiveKeysYaml := `
        sensitiveKeys:
        - path: ["database", "password"]
        - path: ["auth.basic", "password"]
        ` | fromYaml -}}
          {{- if .Values.assertNoLeakedSecrets -}}
            {{- $ini := index .Values "app.ini" -}}
            {{- range $_, $secret := $sensitiveKeysYaml.sensitiveKeys -}}
              {{- $currentMap := $ini -}}
              {{- $shouldContinue := true -}}
              {{- range $index, $elem := $secret.path -}}
                {{- if and $shouldContinue (hasKey $currentMap $elem) -}}
                  {{- if eq (len $secret.path) (add1 $index) -}}
                    {{- if not (regexMatch "\$(?:__(?:env|file|vault))?{[^}]+}" (index $currentMap $elem)) -}}
                      {{- fail (printf "Sensitive key '%s' should use variable expansion" (join "." $secret.path)) -}}
                    {{- end -}}
                  {{- else -}}
                    {{- $currentMap = index $currentMap $elem -}}
                  {{- end -}}
                {{- else -}}
                    {{- $shouldContinue = false -}}
                {{- end -}}
              {{- end -}}
            {{- end -}}
          {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- include "repro.assertNoLeakedSecrets" . }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let values_yaml = indoc! {"
        assertNoLeakedSecrets: true
        app.ini: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults render"),
        (
            serde_json::json!({ "assertNoLeakedSecrets": true, "app.ini": { "database": { "password": 7 } } }),
            false,
            "regexMatch rejects a non-string sensitive value",
        ),
        (
            serde_json::json!({ "assertNoLeakedSecrets": true, "app.ini": { "database": { "password": "hunter2" } } }),
            false,
            "a plaintext sensitive value hits the fail",
        ),
        (
            serde_json::json!({ "app.ini": { "database": { "password": "$__env{PW}" } } }),
            true,
            "variable expansion renders",
        ),
        (
            serde_json::json!({ "assertNoLeakedSecrets": true, "app.ini": { "auth.basic": { "password": "leak" } } }),
            false,
            "dotted path segments stay atomic",
        ),
        (
            serde_json::json!({ "app.ini": { "database": { "host": "ok" } } }),
            true,
            "non-sensitive members render",
        ),
        (
            serde_json::json!({
                "assertNoLeakedSecrets": false,
                "app.ini": { "database": { "password": "hunter2" } },
            }),
            true,
            "the outer flag gates the whole validator",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A literal YAML table decoded with `fromYaml` constant-folds into a
/// typed abstract dict: membership
/// probes over it decode to exact live/dead branches, so a `fail` behind
/// a present key binds its validator while one behind an absent key
/// never fires.
#[test]
fn literal_from_yaml_table_folds_into_exact_membership_branches() {
    let src = indoc! {r#"
        {{- $removed := `
        legacyMode:
          since: "1.16"
        ` | fromYaml }}
        {{- if hasKey $removed "legacyMode" }}
        {{- if .Values.legacyMode }}
        {{ fail "legacyMode has been removed" }}
        {{- end }}
        {{- end }}
        {{- if hasKey $removed "activeMode" }}
        {{- if .Values.activeMode }}
        {{ fail "unreachable: activeMode is not in the table" }}
        {{- end }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let values_yaml = indoc! {"
        legacyMode: false
        activeMode: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "legacyMode": true }),
            false,
            "the folded table contains legacyMode, so its fail binds",
        ),
        (
            serde_json::json!({ "legacyMode": false }),
            true,
            "falsy legacyMode renders",
        ),
        (
            serde_json::json!({ "activeMode": true }),
            true,
            "the dead absent-key branch must not bind its fail",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// An explicit `fail` branch is a VALIDATOR: rendering aborts whenever its
/// guards hold, so valid inputs must falsify the failing test wherever the
/// outer conditions hold (kyverno fails on non-string image tags inside a
/// helper; traefik fails on plugins missing moduleName/version while
/// ranging them; sealed-secrets fails on non-string annotation map values).
#[test]
fn fail_branches_bind_validator_requirements() {
    let helpers = indoc! {r#"
        {{- define "repro.image" -}}
        {{- $tag := default .defaultTag .image.tag -}}
        {{- if not (typeIs "string" $tag) -}}
          {{ fail "Image tags must be strings." }}
        {{- end -}}
        {{- print "img:" $tag -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          containers:
          - name: main
            image: {{ include "repro.image" (dict "image" .Values.image "defaultTag" .Chart.AppVersion) | quote }}
            args:
            {{- range $name, $plugin := .Values.plugins }}
            {{- if or (ne (typeOf $plugin) "map[string]interface {}") (not (hasKey $plugin "moduleName")) }}
              {{- fail (printf "plugin %s is missing moduleName" $name) }}
            {{- end }}
            - "--plugin={{ $name }}"
            {{- end }}
            env:
            {{- range $k, $v := .Values.annotations }}
              {{- if not (and $v (kindIs "string" $v)) }}
                {{ fail "Annotation values have to be strings" }}
              {{- end }}
            {{- end }}
            - name: PROBE
              value: "set"
    "#};
    let values_yaml = indoc! {"
        image:
          tag: latest
        plugins: {}
        annotations: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "image": { "tag": 7 } }),
            false,
            "non-string tag fails",
        ),
        (
            serde_json::json!({ "image": { "tag": "v1" } }),
            true,
            "string tag renders",
        ),
        (
            serde_json::json!({ "image": { "tag": null } }),
            true,
            "null tag takes the default",
        ),
        (
            serde_json::json!({ "plugins": { "bad": 7 } }),
            false,
            "scalar plugin fails",
        ),
        (
            serde_json::json!({ "plugins": { "bad": {} } }),
            false,
            "plugin without moduleName fails",
        ),
        (
            serde_json::json!({ "plugins": { "ok": { "moduleName": "m" } } }),
            true,
            "complete plugin renders",
        ),
        (
            serde_json::json!({ "annotations": { "bad": 7 } }),
            false,
            "non-string annotation fails",
        ),
        (
            serde_json::json!({ "annotations": { "ok": "v" } }),
            true,
            "string annotation renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// `.Values.AsMap` is Go-template METHOD resolution on Helm's typed root
/// values object, returning the receiver map itself — never a user path
/// named `AsMap`. Literal-key `dig` probes through it must bind their fail
/// validators to the real root paths (cilium's `validate.yaml` deprecation
/// checks), and no `AsMap` property may be fabricated.
#[test]
fn values_asmap_method_digs_bind_root_fail_validators() {
    let src = indoc! {r#"
        {{- if (dig "removed" "" .Values.AsMap) }}
          {{ fail "removed has been removed" }}
        {{- end }}
        {{- if (dig "legacy" "mode" "" .Values.AsMap) }}
          {{ fail "legacy.mode has been removed" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);

    assert!(
        schema["properties"].get("AsMap").is_none(),
        "AsMap is a method on the typed root, not a values path; schema={schema}"
    );
    for (instance, want, label) in [
        (
            serde_json::json!({ "removed": true }),
            false,
            "truthy removed option fails rendering",
        ),
        (
            serde_json::json!({ "removed": false }),
            true,
            "falsy removed option renders",
        ),
        (
            serde_json::json!({ "legacy": { "mode": "audit" } }),
            false,
            "truthy nested removed option fails rendering",
        ),
        (
            serde_json::json!({ "legacy": { "mode": "" } }),
            true,
            "falsy nested removed option renders",
        ),
        (serde_json::json!({}), true, "defaults render"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// Only the ROOT receiver is Helm's typed values object: nested values are
/// plain maps, so a nested `AsMap` segment stays an ordinary key, and a
/// genuine uppercase root key that is not a method name stays a normal
/// path. Selecting a derived-text method (`.Values.YAML`) claims no path.
#[test]
fn values_typed_method_resolution_keeps_genuine_keys() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
        data:
          upper: {{ .Values.Upper | quote }}
          nested: {{ .Values.foo.AsMap | quote }}
          derived: {{ .Values.YAML | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);

    assert!(
        schema["properties"].get("Upper").is_some(),
        "a genuine uppercase root key stays a values path; schema={schema}"
    );
    assert!(
        schema["properties"]["foo"]["properties"]
            .get("AsMap")
            .is_some(),
        "nested values are plain maps, so AsMap is an ordinary key there; schema={schema}"
    );
    assert!(
        schema["properties"].get("YAML").is_none(),
        "derived-text Values methods claim no user path; schema={schema}"
    );
}

/// A `fail` guarded by a condition the lowering can only APPROXIMATE on
/// the tested path must not become a requirement: kyverno's replicas
/// helper fails only when `eq (int .) 0`, which does not decode, so
/// negating the decodable remainder would reject every normal count.
#[test]
fn approximate_fail_guards_abstain() {
    let helpers = indoc! {r#"
        {{- define "repro.replicas" -}}
        {{- if and (not (kindIs "invalid" .)) (not (kindIs "string" .)) -}}
        {{- if eq (int .) 0 -}}
          {{- fail "0 replicas is not supported" -}}
        {{- end -}}
        {{- end -}}
        {{- . -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: probe
        spec:
          replicas: {{ include "repro.replicas" .Values.replicas }}
    "#};
    let values_yaml = indoc! {"
        replicas: 1
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "replicas": 3 })),
        "a normal replica count renders; the undecodable zero-check must not manufacture a requirement: {schema}"
    );
}

/// Helm's `required(message, subject)` terminates rendering when the
/// subject is Helm-empty (absent, null, or ""): a direct subject binds a
/// document-level requirement under the ambient guards, and a ranged
/// member subject requires the member on every entry.
#[test]
fn required_subjects_bind_nonempty_requirements() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Probe
        metadata:
          name: {{ required "a cluster name is required" .Values.clusterName }}
        spec:
          {{- range $name, $item := .Values.envSecrets }}
          - name: {{ $name }}
            key: {{ required "key is required" $item.key }}
          {{- end }}
          {{- if .Values.gate.enabled }}
          - name: guarded
            key: {{ required "target required when gated" .Values.gate.target }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        clusterName: \"\"
        envSecrets: {}
        gate:
          enabled: false
          target: \"\"
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "clusterName": "" }),
            false,
            "empty subject fails",
        ),
        (
            serde_json::json!({ "clusterName": null }),
            false,
            "null subject fails",
        ),
        (
            serde_json::json!({ "clusterName": "prod" }),
            true,
            "nonempty renders",
        ),
        (
            serde_json::json!({ "clusterName": "prod", "envSecrets": { "A": { "name": "s" } } }),
            false,
            "ranged member missing key fails",
        ),
        (
            serde_json::json!({ "clusterName": "prod", "envSecrets": { "A": { "key": "k" } } }),
            true,
            "ranged member with key renders",
        ),
        (
            serde_json::json!({ "clusterName": "prod", "gate": { "enabled": true, "target": "" } }),
            false,
            "guarded empty subject fails when the guard holds",
        ),
        (
            serde_json::json!({ "clusterName": "prod", "gate": { "enabled": false, "target": "" } }),
            true,
            "guarded subject stays free when the guard is off",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A guarded `required` subject with a declared non-null default rejects
/// the ABSENT document state: helm validates the coalesced values — the
/// same document the templates render from — so the key can only be
/// missing there when the user's explicit `null` deleted the declared
/// default, and the live branch then aborts at the `required` call (the
/// AWS Load Balancer Controller HPA's `maxReplicas`). A document carrying
/// the key stays accepted, and the dormant guard keeps every spelling
/// open.
#[test]
fn guarded_required_rejects_null_deleted_declared_defaults() {
    let src = indoc! {r#"
        {{- if .Values.autoscaling.enabled }}
        apiVersion: autoscaling/v2
        kind: HorizontalPodAutoscaler
        metadata:
          name: example
        spec:
          maxReplicas: {{ required "a valid maxReplicas value is required" .Values.autoscaling.maxReplicas }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        autoscaling:
          enabled: false
          maxReplicas: 5
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "autoscaling": { "enabled": true } }),
            false,
            "null-deleted subject fails under the live guard",
        ),
        (
            serde_json::json!({ "autoscaling": { "enabled": true, "maxReplicas": "" } }),
            false,
            "empty subject fails under the live guard",
        ),
        (
            serde_json::json!({ "autoscaling": { "enabled": true, "maxReplicas": 5 } }),
            true,
            "filled subject renders",
        ),
        (
            serde_json::json!({ "autoscaling": { "enabled": false } }),
            true,
            "dormant guard keeps the absent state open",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// A terminating validator over SEVERAL paths (mutual exclusion,
/// conditional requirements) lowers as a whole formula: no valid document
/// may satisfy all of its guards (external-dns forbids txtPrefix+txtSuffix
/// together; coredns requires dnsConfig when dnsPolicy is "None").
#[test]
fn cross_path_fail_formulas_lower_as_terminal_clauses() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if and .Values.txtPrefix .Values.txtSuffix }}
          {{- fail "'txtPrefix' and 'txtSuffix' are mutually exclusive" }}
          {{- end }}
          {{- if and (eq .Values.dnsPolicy "None") (not .Values.dnsConfig) }}
          {{- fail "dnsConfig is required when dnsPolicy is set to None" }}
          {{- end }}
          ok: "true"
    "#};
    let values_yaml = indoc! {"
        txtPrefix: \"\"
        txtSuffix: \"\"
        dnsPolicy: ClusterFirst
        dnsConfig: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "txtPrefix": "a", "txtSuffix": "b" }),
            false,
            "mutually exclusive pair fails",
        ),
        (
            serde_json::json!({ "txtPrefix": "a" }),
            true,
            "one of the pair renders",
        ),
        (
            serde_json::json!({ "dnsPolicy": "None" }),
            false,
            "None without dnsConfig fails",
        ),
        (
            serde_json::json!({ "dnsPolicy": "None", "dnsConfig": { "nameservers": ["1.1.1.1"] } }),
            true,
            "None with dnsConfig renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// Range domains compose with their consumers: a single-variable direct
/// range admits integer counts, a loop body reading item members removes
/// them, a nested range over each member value requires rangeable members,
/// and literal member reads elsewhere make a truthy value an object.
#[test]
fn range_domains_compose_with_body_and_sibling_contracts() {
    let src = indoc! {r#"
        apiVersion: example.com/v1
        kind: Probe
        metadata:
          name: probe
        spec:
          plain:
          {{- range .Values.plain }}
          - {{ . | quote }}
          {{- end }}
          structured:
          {{- range .Values.structured }}
          - name: {{ .name }}
          {{- end }}
          nested:
          {{- range $group, $members := .Values.nested }}
          {{- range $name, $key := $members }}
          - {{ $group }}/{{ $name }}: {{ $key }}
          {{- end }}
          {{- end }}
          {{- if .Values.lookup }}
          lookup: {{ .Values.lookup.TARGET }}
          {{- end }}
          also-ranged:
          {{- range $k, $v := .Values.lookup }}
          - {{ $k }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        plain: []
        structured: []
        nested: {}
        lookup: {}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({ "plain": 2 }),
            true,
            "single-variable ranges iterate integer counts",
        ),
        (
            serde_json::json!({ "plain": false }),
            false,
            "a bare range cannot iterate false",
        ),
        (
            serde_json::json!({ "plain": "" }),
            false,
            "a bare range cannot iterate strings",
        ),
        (
            serde_json::json!({ "structured": 2 }),
            false,
            "item member reads exclude integer iteration",
        ),
        (
            serde_json::json!({ "structured": [{ "name": "a" }] }),
            true,
            "structured items render",
        ),
        (
            serde_json::json!({ "nested": { "g": "x" } }),
            false,
            "nested ranges need rangeable members",
        ),
        (
            serde_json::json!({ "nested": { "g": { "a": "k" } } }),
            true,
            "rangeable members render",
        ),
        (
            serde_json::json!({ "lookup": ["x"] }),
            false,
            "a truthy value with literal member reads must be an object",
        ),
        (
            serde_json::json!({ "lookup": [] }),
            true,
            "an empty (falsy) collection skips the member template",
        ),
        (
            serde_json::json!({ "lookup": { "TARGET": "v" } }),
            true,
            "object lookups render",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// a JSON roundtrip changes integer values into non-iterable JSON
/// numbers, while a direct Helm range retains integer-count semantics.
#[test]
fn json_decoded_range_excludes_integer_without_changing_raw_range() {
    let helpers = indoc! {r#"
        {{- define "normalize" -}}
        {{- $values := get (dict "doc" .Values | toJson | fromJson) "doc" -}}
        {{- $_ := set . "Values" $values -}}
        {{- end -}}
    "#};
    let decoded_source = indoc! {r#"
        {{- include "normalize" . }}
        apiVersion: v1
        kind: List
        items:
        {{- range .Values.extraResources }}
        - {{ . | toYaml | nindent 2 }}
        {{- end }}
    "#};
    let decoded_ir = parse_ir_with_helpers(decoded_source, helpers);
    let decoded_signals = schema_signals_for(&decoded_ir);
    let decoded_facts = &decoded_signals
        .schema_evidence_by_value_path()
        .get("extraResources")
        .expect("decoded range source evidence")
        .facts;
    assert!(
        decoded_facts.has_json_decoded_range_use,
        "the range source must retain its decoded runtime representation: facts={decoded_facts:#?}; ir={decoded_ir:#?}"
    );
    let decoded_schema = schema_for_values_yaml(decoded_ir, Some("extraResources: []\n"));

    for (value, want, label) in [
        (serde_json::json!([]), true, "decoded lists iterate"),
        (
            serde_json::json!({ "one": { "apiVersion": "v1", "kind": "ConfigMap" } }),
            true,
            "decoded maps iterate",
        ),
        (
            serde_json::json!(7),
            false,
            "decoded numbers do not iterate",
        ),
    ] {
        assert!(
            schema_accepts_instance(
                &decoded_schema,
                &serde_json::json!({ "extraResources": value })
            ) == want,
            "{label}: {decoded_schema}"
        );
    }

    let guarded_source = indoc! {r#"
        {{- include "normalize" . }}
        {{- if .Values.enabled }}
        {{- range .Values.extraResources }}
        {{ . | toYaml }}
        {{- end }}
        {{- end }}
    "#};
    let guarded_schema = schema_for_values_yaml(
        parse_ir_with_helpers(guarded_source, helpers),
        Some("enabled: false\nextraResources: []\n"),
    );
    assert!(
        schema_accepts_instance(
            &guarded_schema,
            &serde_json::json!({ "enabled": false, "extraResources": 7 })
        ),
        "a dead decoded range does not constrain its collection: {guarded_schema}"
    );
    assert!(
        !schema_accepts_instance(
            &guarded_schema,
            &serde_json::json!({ "enabled": true, "extraResources": 7 })
        ),
        "a live decoded range rejects JSON numbers: {guarded_schema}"
    );

    let raw_schema = schema_for_values_yaml(
        parse_ir("{{- range .Values.count }}{{ . }}{{- end }}"),
        Some("count: null\n"),
    );
    for count in [-1, 0, 2] {
        assert!(
            schema_accepts_instance(&raw_schema, &serde_json::json!({ "count": count })),
            "raw Helm integer counts must remain rangeable: {raw_schema}"
        );
    }
}

#[test]
fn root_values_merge_defaults_activate_live_consumer_contracts() {
    let mutation = indoc! {r#"
        {{- $defaults := .Values._internal_defaults -}}
        {{- $_ := set $ "Values" (mustMergeOverwrite $defaults $.Values) -}}
    "#};
    let consumers = indoc! {r#"
        {{- if or (eq .Values.global.resourceScope "all") (eq .Values.global.resourceScope "namespace") }}
        {{- $_ := pick .Values.gateways "securityContext" }}
        {{- if .Values.remotePilotAddress }}
        {{- $_ := regexMatch "^[0-9.]+$" .Values.remotePilotAddress }}
        {{- end }}
        {{- end }}
    "#};
    let mut contract = parse_ir(mutation);
    contract.append(parse_ir(consumers));
    let schema = schema_for_values_yaml(
        contract,
        Some(indoc! {r#"
            _internal_defaults:
              global:
                resourceScope: all
              gateways: {}
              remotePilotAddress: ""
        "#}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "gateways": 7 }),
            false,
            "the effective default activates the object consumer",
        ),
        (
            serde_json::json!({ "remotePilotAddress": { "host": "1.2.3.4" } }),
            false,
            "the effective default activates the string consumer",
        ),
        (
            serde_json::json!({
                "global": { "resourceScope": "cluster" },
                "gateways": 7,
                "remotePilotAddress": { "host": "1.2.3.4" }
            }),
            true,
            "an explicit inactive scope skips both consumers",
        ),
        (
            serde_json::json!({
                "global": {},
                "gateways": {},
                "remotePilotAddress": "1.2.3.4"
            }),
            true,
            "valid live-branch operands render",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn multiple_default_sources_for_one_values_target_abstain() {
    let first = indoc! {r#"
        {{- $defaults := .Values.first_defaults -}}
        {{- $_ := set $ "Values" (mustMergeOverwrite $defaults $.Values) -}}
    "#};
    let second = indoc! {r#"
        {{- $defaults := .Values.second_defaults -}}
        {{- $_ := set $ "Values" (mustMergeOverwrite $defaults $.Values) -}}
    "#};
    let consumer = indoc! {r#"
        {{- if eq .Values.mode "live" }}
        {{- $_ := pick .Values.payload "name" }}
        {{- end }}
    "#};
    let mut contract = parse_ir(first);
    contract.append(parse_ir(second));
    contract.append(parse_ir(consumer));
    let schema = schema_for_values_yaml(
        contract,
        Some(indoc! {"
            first_defaults:
              mode: live
              payload: {}
            second_defaults:
              mode: inactive
              payload: {}
        "}),
    );

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "payload": 7 })),
        "order-sensitive default sources must abstain instead of selecting lexical set order: {schema}"
    );
}

/// airflow's celery-broker sentinel accumulates a Boolean while ranging
/// `env` and terminates when neither `brokerUrlSecretName` nor an item
/// named `BROKER_URL_CMD` exists. The flag's truthiness is the existential
/// "some ranged item's member equals the literal", which Draft-07 encodes
/// with `contains`.
#[test]
fn existential_range_sentinel_lowers_to_contains() {
    let src = indoc! {r#"
        {{- if .Values.redis.enabled }}
        {{- $found := false }}
        {{- range .Values.env }}
        {{- if eq .name "BROKER_URL_CMD" }}
        {{- $found = true }}
        {{- break -}}
        {{- end }}
        {{- end }}
        {{- if not (or .Values.brokerUrlSecretName $found) }}
        {{ required "set brokerUrlSecretName or BROKER_URL_CMD in env" nil }}
        {{- end }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data: {}
    "#};
    let values_yaml = indoc! {r#"
        redis:
          enabled: true
        brokerUrlSecretName: ""
        env: []
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want) in [
        (serde_json::json!({ "redis": { "enabled": true } }), false),
        (
            serde_json::json!({ "redis": { "enabled": true }, "env": [{ "name": "OTHER" }] }),
            false,
        ),
        (
            serde_json::json!({ "redis": { "enabled": true }, "brokerUrlSecretName": "s" }),
            true,
        ),
        (
            serde_json::json!({ "redis": { "enabled": true }, "env": [{ "name": "BROKER_URL_CMD" }] }),
            true,
        ),
        (serde_json::json!({ "redis": { "enabled": false } }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "existential sentinel: instance={instance}; schema={schema}"
        );
    }
}

/// A per-member `fail` under a truthy-and-type test requires every member
/// to be a TRUTHY string: sealed-secrets aborts on empty-string
/// `privateKeyAnnotations` members, not only on non-strings.
#[test]
fn ranged_member_truthy_string_test_requires_truthy_members() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          args: |-
            {{- if .Values.privateKeyAnnotations }}
            {{- $flags := "" }}
            {{- range $k, $v := .Values.privateKeyAnnotations }}
              {{- if not (and $v (kindIs "string" $v)) }}
                {{ fail "Annotation values have to be strings" }}
              {{- end }}
              {{- $flags = printf "%s=%s,%s" $k $v $flags }}
            {{- end }}
            {{ trimSuffix "," $flags }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("privateKeyAnnotations: {}\n"));

    for (instance, want, label) in [
        (
            serde_json::json!({ "privateKeyAnnotations": { "audit": "ok" } }),
            true,
            "truthy string member",
        ),
        (
            serde_json::json!({ "privateKeyAnnotations": {} }),
            true,
            "empty map",
        ),
        (
            serde_json::json!({ "privateKeyAnnotations": { "audit": 7 } }),
            false,
            "numeric member",
        ),
        (
            serde_json::json!({ "privateKeyAnnotations": { "audit": "" } }),
            false,
            "empty-string member is falsy",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "truthy string member {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A per-member `fail` keyed on literal name equality forbids exactly those
/// member names, under the clause's live outer guard: cilium rejects
/// `extraEnv` entries colliding with its backoff variables only while the
/// backoff feature is enabled.
#[test]
fn ranged_member_name_equality_fail_forbids_the_literal_names() {
    let src = indoc! {r#"
        {{- if .Values.k8sClientExponentialBackoff.enabled }}
        {{- range .Values.extraEnv }}
        {{- if or (eq .name "KUBE_CLIENT_BACKOFF_BASE") (eq .name "KUBE_CLIENT_BACKOFF_DURATION") }}
        {{ fail "k8sClientExponentialBackoff cannot be enabled when extraEnv contains KUBE_CLIENT_BACKOFF_BASE or KUBE_CLIENT_BACKOFF_DURATION" }}
        {{- end }}
        {{- end }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data: {}
    "#};
    let values_yaml = indoc! {r#"
        k8sClientExponentialBackoff:
          enabled: true
        extraEnv: []
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (instance, want, label) in [
        // The coalesced document carries the declared `enabled: true`; a
        // document missing it had the guard null-deleted and stays open.
        (
            serde_json::json!({
                "k8sClientExponentialBackoff": { "enabled": true },
                "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }]
            }),
            false,
            "forbidden name under the live guard",
        ),
        (
            serde_json::json!({ "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }] }),
            true,
            "forbidden name under a null-deleted guard",
        ),
        (
            serde_json::json!({ "extraEnv": [{ "name": "OTHER", "value": "1" }] }),
            true,
            "unrelated name",
        ),
        (
            serde_json::json!({
                "k8sClientExponentialBackoff": { "enabled": false },
                "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }]
            }),
            true,
            "forbidden name in the dead arm",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "forbidden member name {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A `fail` keyed on a range-KEY regex constrains the collection's key
/// domain through `propertyNames`: traefik aborts on uppercase
/// `ingressRoute` keys.
#[test]
fn range_key_regex_fail_lowers_to_property_names() {
    let src = indoc! {r#"
        {{- range $name, $config := .Values.ingressRoute }}
        {{- if regexMatch "[A-Z]" $name }}
        {{- fail (printf "ERROR: ingressRoute key %q contains uppercase characters." $name) }}
        {{- end }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data: {}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("ingressRoute: {}\n"));

    for (instance, want, label) in [
        (
            serde_json::json!({ "ingressRoute": { "dashboard": {} } }),
            true,
            "lowercase key",
        ),
        (serde_json::json!({ "ingressRoute": {} }), true, "empty map"),
        (
            serde_json::json!({ "ingressRoute": { "Dashboard": {} } }),
            false,
            "uppercase key",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "range key regex {label}: instance={instance}; schema={schema}"
        );
    }
}

/// cilium's validators state finite scalar domains through `fail` guards: a
/// `len` bound, an `int`-coerced inequality pair, and a negated literal
/// membership. Each conjunct lowers through its sound subset, so the
/// terminal clauses reject exactly the strengthened domains while coerced
/// spellings outside the subsets stay open.
#[test]
fn scalar_domain_fail_guards_lower_through_sound_subsets() {
    let src = indoc! {r#"
        {{- if gt (len .Values.clusterName) 8 }}
        {{ fail "cluster name too long" }}
        {{- end }}
        {{- if and (ne (int .Values.maxClusters) 255) (ne (int .Values.maxClusters) 511) }}
        {{ fail "must be 255 or 511" }}
        {{- end }}
        {{- if not (list "internal" "external" | has .Values.mode) }}
        {{ fail "mode must be internal or external" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data: {}
    "#};
    let values_yaml = indoc! {r#"
        clusterName: default
        maxClusters: 255
        mode: internal
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want) in [
        (
            serde_json::json!({ "clusterName": "123456789", "maxClusters": 255, "mode": "internal" }),
            false,
        ),
        (
            serde_json::json!({ "clusterName": "12345678", "maxClusters": 255, "mode": "internal" }),
            true,
        ),
        (
            serde_json::json!({ "maxClusters": 300, "mode": "internal" }),
            false,
        ),
        (
            serde_json::json!({ "maxClusters": 511, "mode": "internal" }),
            true,
        ),
        // A numeric string coerces exactly like the raw integer: the
        // region disjunction claims spellings certainly parsing outside
        // {255, 511} while the bound spellings stay accepted.
        (
            serde_json::json!({ "maxClusters": "255", "mode": "internal" }),
            true,
        ),
        (
            serde_json::json!({ "maxClusters": "0x1ff", "mode": "internal" }),
            true,
        ),
        (
            serde_json::json!({ "maxClusters": "300", "mode": "internal" }),
            false,
        ),
        (
            serde_json::json!({ "maxClusters": "bogus", "mode": "internal" }),
            false,
        ),
        (
            serde_json::json!({ "mode": "bogus", "maxClusters": 255 }),
            false,
        ),
        (
            serde_json::json!({ "mode": "external", "maxClusters": 255 }),
            true,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "scalar-domain fail guards: instance={instance}; schema={schema}"
        );
    }
}

/// jenkins' `controller.replicas` validator binds the int cast to a LOCAL
/// (`$replicas := int (default 1 …)`) inside a helper and fails outside
/// 0..=1. The cast provenance rides the binding, so both disjuncts lower
/// through the raw-integer subsets exactly as the inline spellings would —
/// including the new below-bound direction.
#[test]
fn variable_bound_coercion_fail_guards_lower_through_sound_subsets() {
    let helpers = indoc! {r#"
        {{- define "controller.replicas" -}}
        {{- $replicas := int (default 1 .Values.controller.replicas) -}}
        {{- if or (lt $replicas 0) (gt $replicas 1) -}}
        {{- fail "controller.replicas must be 0 or 1" -}}
        {{- end -}}
        {{- .Values.controller.replicas -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          replicas: {{ include "controller.replicas" . | quote }}
    "#};
    let values_yaml = indoc! {r#"
        controller:
          replicas: 1
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want) in [
        (
            serde_json::json!({ "controller": { "replicas": 2 } }),
            false,
        ),
        (
            serde_json::json!({ "controller": { "replicas": -1 } }),
            false,
        ),
        (serde_json::json!({ "controller": { "replicas": 1 } }), true),
        (serde_json::json!({ "controller": { "replicas": 0 } }), true),
        // Clean decimal spellings coerce into the failing domain at render
        // time, so the string preimage rejects them alongside raw integers.
        (
            serde_json::json!({ "controller": { "replicas": "5" } }),
            false,
        ),
        (
            serde_json::json!({ "controller": { "replicas": "-1" } }),
            false,
        ),
        (
            serde_json::json!({ "controller": { "replicas": "1" } }),
            true,
        ),
        // A leading zero flips ParseInt's base detection to octal: "09"
        // is a parse ERROR coercing to 0 — inside the domain — while
        // valid octal and hex spellings coerce beyond it and reject.
        (
            serde_json::json!({ "controller": { "replicas": "09" } }),
            true,
        ),
        (
            serde_json::json!({ "controller": { "replicas": "05" } }),
            false,
        ),
        (
            serde_json::json!({ "controller": { "replicas": "0x5" } }),
            false,
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "variable-bound coercion fail guards: instance={instance}; schema={schema}"
        );
    }
}

/// cilium `kubeProxyReplacement`: the configmap stringifies the value
/// (`toString`, a `<nil>` → `""` rewrite, `coalesce` with a literal
/// default) before comparing it against `"true"`/`"false"` and failing
/// otherwise. The equality binds the raw path through the `toString`
/// PREIMAGE, so raw Booleans render exactly like their string spellings
/// while any other truthy scalar aborts.
#[test]
fn stringified_equality_binds_the_tostring_preimage() {
    let src = indoc! {r#"
        {{- $default := "false" -}}
        {{- $string := (toString .Values.kubeProxyReplacement) -}}
        {{- if (eq $string "<nil>") }}
          {{- $string = "" -}}
        {{- end }}
        {{- $mode := (coalesce $string $default) -}}
        {{- if and (ne $mode "true") (ne $mode "false") }}
        {{ fail "kubeProxyReplacement must be true or false" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          mode: {{ $mode | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);
    for (instance, want) in [
        (serde_json::json!({ "kubeProxyReplacement": true }), true),
        (serde_json::json!({ "kubeProxyReplacement": false }), true),
        (serde_json::json!({ "kubeProxyReplacement": "true" }), true),
        (serde_json::json!({ "kubeProxyReplacement": "false" }), true),
        // An EMPTY stringification selects the coalesce's constant default
        // ("false"), so the empty and null raw spellings render too: "" is
        // Helm-empty directly, and null reaches "" through the chain's
        // `"<nil>"` rewrite.
        (serde_json::json!({ "kubeProxyReplacement": "" }), true),
        (serde_json::json!({ "kubeProxyReplacement": null }), true),
        (
            serde_json::json!({ "kubeProxyReplacement": "strict" }),
            false,
        ),
        (serde_json::json!({ "kubeProxyReplacement": 1 }), false),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "stringified equality preimage: instance={instance}; schema={schema}"
        );
    }
}

/// cilium's removed-option guards stringify a `dig` result before testing
/// truthiness: `"false"`, `"0"`, and `"<nil>"` are truthy STRINGS, so an
/// explicitly-disabled removed option still aborts the render. Only an
/// absent chain (the dig's empty-string default) or a raw empty string is
/// falsy; the sibling raw-`dig` disjunct keeps ordinary Helm truthiness.
#[test]
fn stringified_dig_truthiness_rejects_falsy_raw_spellings() {
    let src = indoc! {r#"
        {{- if or
          ((dig "proxy" "prometheus" "enabled" "" .Values.AsMap) | toString)
          (dig "proxy" "prometheus" "port" "" .Values.AsMap)
        }}
        {{ fail "proxy.prometheus.enabled and proxy.prometheus.port were removed" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          ok: "yes"
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);
    for (instance, want, label) in [
        (serde_json::json!({}), true, "absent chain renders"),
        (
            serde_json::json!({ "proxy": { "prometheus": {} } }),
            true,
            "absent leaf renders",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "enabled": "" } } }),
            true,
            "raw empty string stringifies to the falsy empty rendering",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "enabled": false } } }),
            false,
            "raw false renders truthy \"false\"",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "enabled": true } } }),
            false,
            "raw true renders truthy \"true\"",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "enabled": null } } }),
            false,
            "explicit null renders truthy \"<nil>\"",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "enabled": 0 } } }),
            false,
            "raw zero renders truthy \"0\"",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "port": 9095 } } }),
            false,
            "the sibling raw-dig disjunct keeps Helm truthiness",
        ),
        (
            serde_json::json!({ "proxy": { "prometheus": { "port": "" } } }),
            true,
            "a falsy sibling value renders",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "stringified dig truthiness ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// Truthiness of a DIRECT total stringification tests the rendered text:
/// `toString nil` is the truthy `"<nil>"`, so an absent or null subject
/// passes a `not (.Values.mode | toString)` gate and only the raw empty
/// string fails it. traefik's `with .addX | toString` flag family rides
/// the same decode — its bodies run for raw `false` too.
#[test]
fn direct_tostring_truthiness_is_a_rendering_test() {
    let src = indoc! {r#"
        {{- if not (.Values.mode | toString) }}
        {{ fail "mode must not stringify empty" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          mode: {{ .Values.mode | toString | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), None);
    for (instance, want, label) in [
        (
            serde_json::json!({}),
            true,
            "absent renders truthy \"<nil>\"",
        ),
        (
            serde_json::json!({ "mode": null }),
            true,
            "null renders truthy \"<nil>\"",
        ),
        (
            serde_json::json!({ "mode": false }),
            true,
            "raw false renders truthy \"false\"",
        ),
        (
            serde_json::json!({ "mode": 0 }),
            true,
            "raw zero renders truthy \"0\"",
        ),
        (
            serde_json::json!({ "mode": "" }),
            false,
            "only the raw empty string stringifies empty",
        ),
        (serde_json::json!({ "mode": "x" }), true, "text renders"),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "direct toString truthiness ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// traefik's local-plugin type helper renders each ranged member through
/// mutually exclusive arms — a `type` from a literal enum, or the legacy
/// bare `hostPath` — and `fail`s otherwise. The member requirements are
/// the DISJUNCTION of the arm negations: either documented shape renders
/// alone, while an unknown `type` (even beside a hostPath) and a member
/// with neither field abort.
#[test]
fn multi_test_fail_negations_lower_as_member_alternatives() {
    let helpers = indoc! {r#"
        {{- define "repro.pluginType" -}}
            {{- $plugin := .plugin -}}
            {{- if $plugin.type -}}
                {{- if eq $plugin.type "hostPath" -}}
                    {{- printf "hostPath" -}}
                {{- else if eq $plugin.type "inlinePlugin" -}}
                    {{- printf "inlinePlugin" -}}
                {{- else -}}
                    {{- fail (printf "plugin %s has an invalid type" .pluginName) -}}
                {{- end -}}
            {{- else if $plugin.hostPath -}}
                {{- printf "hostPath" -}}
            {{- else -}}
                {{- fail (printf "plugin %s must set hostPath or type" .pluginName) -}}
            {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if .Values.plugins }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: plugins
        data:
          {{- range $name, $plugin := .Values.plugins }}
          {{ $name }}: {{ include "repro.pluginType" (dict "plugin" $plugin "pluginName" $name) | quote }}
          {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        plugins: {}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (member, want) in [
        (serde_json::json!({ "hostPath": "/plugins/x" }), true),
        (
            serde_json::json!({ "type": "hostPath", "hostPath": "/plugins/x" }),
            true,
        ),
        (serde_json::json!({ "type": "inlinePlugin" }), true),
        (serde_json::json!({ "type": "bogus" }), false),
        (
            serde_json::json!({ "type": "bogus", "hostPath": "/plugins/x" }),
            false,
        ),
        (serde_json::json!({ "moduleName": "x" }), false),
        (serde_json::json!("scalar"), false),
    ] {
        let instance = serde_json::json!({ "plugins": { "p": member } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "member alternatives: instance={instance}; schema={schema}"
        );
    }
}

/// kyverno's `kyverno.deployment.replicas` helper (called through
/// `{{ template … .Values.X.replicas }}`) fails on `eq (int .) 0` when the
/// argument is neither nil nor a string: a raw integer (or integral
/// float) zero certainly satisfies the coercing equality, so the fail arm
/// rejects it while strings and null keep the helper's own escapes. The
/// equality lowers as the [IntGt bound-1, IntLt bound+1] region pair —
/// coercible non-integers (booleans, fractional floats) stay a documented
/// sound abstention.
#[test]
fn int_cast_zero_equality_fails_reject_raw_zero() {
    let helpers = indoc! {r#"
        {{- define "repro.replicas" -}}
          {{- if and (not (kindIs "invalid" .)) (not (kindIs "string" .)) -}}
          {{- if eq (int .) 0 -}}
            {{- fail "no zero replicas" -}}
          {{- end -}}
          {{- end -}}
          {{- . -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        config:
          replicas: {{ template "repro.replicas" .Values.replicas }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("replicas: 1\n"));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "absent renders empty"),
        (serde_json::json!({ "replicas": null }), true, "nil escapes"),
        (
            serde_json::json!({ "replicas": 1 }),
            true,
            "nonzero renders",
        ),
        (
            serde_json::json!({ "replicas": -1 }),
            true,
            "negative renders",
        ),
        (
            serde_json::json!({ "replicas": "0" }),
            true,
            "strings escape the kind dispatch",
        ),
        (
            serde_json::json!({ "replicas": 0 }),
            false,
            "raw zero aborts",
        ),
        (
            serde_json::json!({ "replicas": 0.0 }),
            false,
            "integral float zero aborts",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "int-cast zero equality ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// The int-cast regions' STRING preimages follow `strconv.ParseInt` base
/// 0 (all polarities helm-verified): single-sign regions add the radix
/// spellings that certainly parse inside (`"0x10"`/`"017"` abort a
/// positive-bound gate like raw 16/15 do), a below-zero region keeps
/// zero-padded VALID octal while an 8/9 digit is a parse error coercing
/// to 0 (`"-018"` renders — the old pattern falsely rejected it), and a
/// MIXED-sign region (positive `lt` bound) claims the complement of the
/// parse-escape language: every unparseable, empty, or negative spelling
/// coerces to 0 inside the region while a successful parse past the
/// bound escapes.
#[test]
fn int_cast_string_preimages_cover_radix_and_complement_lanes() {
    let src = indoc! {r#"
        {{- if gt (int64 .Values.count) 0 }}
        {{- fail "count must not be positive" }}
        {{- end }}
        {{- if lt (int .Values.floor) 3 }}
        {{- fail "floor too low" }}
        {{- end }}
        {{- if lt (int .Values.neg) 0 }}
        {{- fail "neg must not be negative" }}
        {{- end }}
        config:
          ok: true
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("count: 0\nfloor: 5\nneg: 1\n"));
    for (instance, want, label) in [
        (
            serde_json::json!({ "count": "0x10", "floor": 5 }),
            false,
            "hex above 0",
        ),
        (
            serde_json::json!({ "count": "017", "floor": 5 }),
            false,
            "legacy octal above 0",
        ),
        (
            serde_json::json!({ "count": "0", "floor": 5 }),
            true,
            "zero renders",
        ),
        (
            serde_json::json!({ "floor": "abc" }),
            false,
            "unparseable coerces to 0 below the floor",
        ),
        (
            serde_json::json!({ "floor": "" }),
            false,
            "empty coerces to 0 below the floor",
        ),
        (
            serde_json::json!({ "floor": "-5" }),
            false,
            "negative parse lands below the floor",
        ),
        (
            serde_json::json!({ "floor": "0x10" }),
            true,
            "hex parse escapes past the floor",
        ),
        (
            serde_json::json!({ "floor": "3" }),
            true,
            "boundary parse escapes",
        ),
        // "2" coerces below the floor and aborts Helm: the exact escape
        // windows see it certainly parsing below 3, so the complement
        // lane claims it (the old char-count escape widened here).
        (
            serde_json::json!({ "floor": "2" }),
            false,
            "in-language low parse is claimed exactly",
        ),
        (
            serde_json::json!({ "neg": "-018", "floor": 5 }),
            true,
            "invalid octal digit coerces to 0 and renders",
        ),
        (
            serde_json::json!({ "neg": "-09", "floor": 5 }),
            true,
            "invalid octal 9 coerces to 0 and renders",
        ),
        (
            serde_json::json!({ "neg": "-017", "floor": 5 }),
            false,
            "valid zero-padded octal parses negative",
        ),
        (
            serde_json::json!({ "neg": "-0x10", "floor": 5 }),
            false,
            "negative hex parses negative",
        ),
        (
            serde_json::json!({ "neg": "-5", "floor": 5 }),
            false,
            "clean negative decimal",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "int-cast string preimage ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A conjunction of `ne $item.field "…"` inequalities guarding a ranged
/// fail negates to the DISJUNCTION of the equalities — the field's value
/// enum (nats' jsonpatch `op` gate, in the direct-range shape). Each
/// `FieldEquals` alternative carries presence, so a member missing the
/// field rejects too.
#[test]
fn ranged_not_equals_chains_negate_to_the_field_enum() {
    let src = indoc! {r#"
        {{- range $patch := .Values.service.patch }}
        {{- if and (ne $patch.op "add") (ne $patch.op "remove") }}
        {{- fail "patch has invalid op" }}
        {{- end }}
        {{- end }}
        config:
          ok: true
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("service:\n  patch: []\n"));
    for (item, want, label) in [
        (serde_json::json!({ "op": "add" }), true, "add allowed"),
        (
            serde_json::json!({ "op": "remove" }),
            true,
            "remove allowed",
        ),
        (
            serde_json::json!({ "op": "bogus" }),
            false,
            "unknown op aborts",
        ),
        (serde_json::json!({}), false, "missing op aborts"),
    ] {
        let instance = serde_json::json!({ "service": { "patch": [item] } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "ranged ne-chain enum ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A fail nested under a range over a LOCAL-DICT overlay (`$services :=
/// .Values.service.additionalServices` + `set $services "default"
/// (omit …)`) still terminates: the overlay's literal entry iterates on
/// every render, so the member gate re-decodes under that DEFINITE
/// binding as a sound subset and the inner terminal binds (traefik's
/// http3-without-tls abort under the always-present "default" service).
#[test]
fn overlay_range_member_gates_carry_definite_entry_sound_subsets() {
    let src = indoc! {r#"
        {{- $services := .Values.service.additionalServices -}}
        {{- $services = set $services "default" (omit .Values.service "additionalServices") }}
        {{- range $name, $service := $services -}}
        {{- if ne $service.enabled false -}}
        {{- range $portName, $config := $.Values.ports -}}
          {{- if $config -}}
            {{- if ($config.http3).enabled -}}
              {{- if (not ($config.http).tls.enabled) -}}
                {{- fail "ERROR: You cannot enable http3 without enabling tls" -}}
              {{- end -}}
            {{- end -}}
          {{- end -}}
        {{- end -}}
        kind: Service
        apiVersion: v1
        metadata:
          name: {{ $name }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        service:
          enabled: true
          additionalServices: {}
        ports:
          web:
            port: 8000
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want, label) in [
        (
            serde_json::json!({ "ports": { "web": { "http3": { "enabled": true } } } }),
            false,
            "http3 without tls aborts through the default service",
        ),
        (
            serde_json::json!({ "ports": { "web": { "http3": { "enabled": true },
                "http": { "tls": { "enabled": true } } } } }),
            true,
            "http3 with tls renders",
        ),
        (
            serde_json::json!({ "service": { "enabled": false },
                "ports": { "web": { "http3": { "enabled": true } } } }),
            true,
            "a disabled default service keeps the terminal dormant",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "overlay-range terminal ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A ranged fail whose test CONJOINS several member conditions negates to
/// the disjunction of their negations, per member: an equality on a member
/// field flips to the absence-tolerant `FieldNotEquals`, a negated
/// truthiness over a nested field flips to `FieldHelmTruthy`, and the
/// member's own truthiness gate contributes the Helm-falsy escape
/// (traefik's HTTPS-listener certificateRefs and http3-without-tls
/// terminals).
#[test]
fn compound_ranged_terminals_negate_to_member_alternatives() {
    let src = indoc! {r#"
        {{- range $name, $config := .Values.gateway.listeners }}
        {{- if and (eq .protocol "HTTPS") (not .certificateRefs) }}
        {{- fail "ERROR: certificateRefs needs to be specified using HTTPS" }}
        {{- end }}
        {{- end }}
        {{- range $portName, $config := .Values.ports }}
        {{- if $config }}
        {{- if ($config.http3).enabled }}
        {{- if not ($config.http).tls.enabled }}
        {{- fail "ERROR: You cannot enable http3 without enabling tls" }}
        {{- end }}
        {{- end }}
        {{- end }}
        {{- end }}
        config:
          ok: true
    "#};
    let values_yaml = indoc! {r#"
        gateway:
          listeners: {}
        ports: {}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (listener, want, label) in [
        (
            serde_json::json!({ "protocol": "HTTPS", "port": 443 }),
            false,
            "an HTTPS listener without certificateRefs aborts",
        ),
        (
            serde_json::json!({ "protocol": "HTTPS", "port": 443,
                "certificateRefs": [{ "name": "tls" }] }),
            true,
            "an HTTPS listener with certificateRefs renders",
        ),
        (
            serde_json::json!({ "protocol": "HTTPS", "port": 443,
                "certificateRefs": [] }),
            false,
            "an empty certificateRefs list is Helm-falsy and aborts",
        ),
        (
            serde_json::json!({ "protocol": "HTTP", "port": 80 }),
            true,
            "a non-HTTPS listener escapes the terminal",
        ),
    ] {
        let instance = serde_json::json!({ "gateway": { "listeners": { "web": listener } } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "listener terminal ({label}): instance={instance}; schema={schema}"
        );
    }
    for (port, want, label) in [
        (
            serde_json::json!({ "http3": { "enabled": true } }),
            false,
            "http3 without tls aborts",
        ),
        (
            serde_json::json!({ "http3": { "enabled": true },
                "http": { "tls": { "enabled": true } } }),
            true,
            "http3 with tls renders",
        ),
        (
            serde_json::json!({ "http3": { "enabled": false } }),
            true,
            "disabled http3 escapes",
        ),
        (serde_json::json!(null), true, "a falsy port config escapes"),
    ] {
        let instance = serde_json::json!({ "ports": { "web": port } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "http3 terminal ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A HELPER-SCOPE range over a JSON-roundtripped dict member carries the
/// member identity into its fail captures: the nats `jsonpatch` shape —
/// `$params := fromJson (toJson .)`, `$patches := $params.patch`,
/// `range $patch := $patches` with `hasKey`/`ne $patch.op` gates — must
/// bind the caller's `service.patch` members instead of truncating to
/// `service.patch.op` and leaking document-level terminals.
#[test]
fn helper_scope_ranges_bind_member_identities_in_fail_captures() {
    let helpers = indoc! {r#"
        {{- define "repro.jsonpatch" -}}
          {{- $params := fromJson (toJson .) -}}
          {{- $patches := $params.patch -}}
          {{- range $patch := $patches -}}
            {{- if not (hasKey $patch "op") -}}
              {{- fail "patch is missing op key" -}}
            {{- end -}}
            {{- if and (ne $patch.op "add") (ne $patch.op "remove") (ne $patch.op "replace") -}}
              {{- fail (cat "patch has invalid op" $patch.op) -}}
            {{- end -}}
          {{- end -}}
          {{- toJson . -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: t
        data:
          out: {{ include "repro.jsonpatch" (dict "doc" (dict) "patch" (.Values.service.patch | default list)) | quote }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("service:\n  patch: []\n"),
    );
    for (patch, want, label) in [
        (serde_json::json!([]), true, "an empty patch list renders"),
        (
            serde_json::json!([{ "op": "add", "path": "/a" }]),
            true,
            "a valid op renders",
        ),
        (
            serde_json::json!([{ "op": "bogus" }]),
            false,
            "an unknown op aborts",
        ),
        (
            serde_json::json!([{ "path": "/a" }]),
            false,
            "a patch without op aborts",
        ),
    ] {
        let instance = serde_json::json!({ "service": { "patch": patch } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "helper-range member identity ({label}): instance={instance}; schema={schema}"
        );
    }
    let unrelated = serde_json::json!({ "service": { "patch": [{ "op": "add" }], "extra": 1 } });
    assert!(
        schema_accepts_instance(&schema, &unrelated),
        "sibling members stay open: {schema}"
    );
}

/// cilium's provider-mode gates spell their tests through defaulted
/// pipelines and negated equality disjunctions: `ne (.Values.routingMode
/// | default "native") "native"` aborts GKE+tunnel while the unset and
/// explicit-native spellings render, and `not (or (eq P "Cluster")
/// (eq P "Local"))` aborts any other traffic policy. Both must decode
/// exactly — the truthiness weakenings accept the invalid spellings.
#[test]
fn defaulted_pipeline_and_negated_disjunction_tests_decode() {
    let src = indoc! {r#"
        config:
          {{- if .Values.gke.enabled }}
          {{- if ne (.Values.routingMode | default "native") "native" }}
          {{- fail "RoutingMode must be set to native when gke.enabled=true" }}
          {{- end }}
          endpointRoutes: true
          {{- end }}
          {{- if .Values.ingress.enabled }}
          {{- if not (or (eq .Values.ingress.policy "Cluster") (eq .Values.ingress.policy "Local")) }}
          {{- fail "policy must be Cluster or Local" }}
          {{- end }}
          policy: {{ .Values.ingress.policy }}
          {{- end }}
          ok: true
    "#};
    let values_yaml = indoc! {r#"
        gke:
          enabled: false
        routingMode: ""
        ingress:
          enabled: false
          policy: Cluster
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults render"),
        (
            serde_json::json!({ "gke": { "enabled": true } }),
            true,
            "gke with the unset routing mode takes the default",
        ),
        (
            serde_json::json!({ "gke": { "enabled": true }, "routingMode": "native" }),
            true,
            "gke with explicit native renders",
        ),
        (
            serde_json::json!({ "gke": { "enabled": true }, "routingMode": "tunnel" }),
            false,
            "gke with tunnel aborts",
        ),
        (
            serde_json::json!({ "routingMode": "tunnel" }),
            true,
            "tunnel without gke stays open",
        ),
        (
            serde_json::json!({ "ingress": { "enabled": true, "policy": "Local" } }),
            true,
            "a listed policy renders",
        ),
        (
            serde_json::json!({ "ingress": { "enabled": true, "policy": "Foo" } }),
            false,
            "an unlisted policy aborts",
        ),
        (
            serde_json::json!({ "ingress": { "enabled": false, "policy": "Foo" } }),
            true,
            "the disabled gate keeps the policy open",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "defaulted pipeline and negated disjunction ({label}): \
             instance={instance}; schema={schema}"
        );
    }
}

/// vault's `validateRedundancyZones` helper spells every gate through a
/// `| toString` pipeline (`eq (.Values.…enabled | toString) "true"` as
/// the outer guard, `ne (.Values.server.ha.enabled | toString) "true"`
/// as the failing tests): the pipeline stringification must decode like
/// the `toString X` call form so the values-decidable combination
/// implications reach the schema. The helper's Kubernetes-version semver
/// fail is cluster-dependent and must abstain, keeping the valid
/// combination open.
#[test]
fn pipeline_tostring_gates_decode_in_helper_terminals() {
    let helpers = indoc! {r#"
        {{- define "repro.validate" -}}
        {{- if eq (.Values.zones.enabled | toString) "true" -}}
        {{- if ne (.Values.ha.enabled | toString) "true" -}}
        {{- fail "zones.enabled=true requires ha.enabled=true" -}}
        {{- end -}}
        {{- if ne (.Values.raft.enabled | toString) "true" -}}
        {{- fail "zones.enabled=true requires raft.enabled=true" -}}
        {{- end -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- include "repro.validate" . -}}
        replicas: {{ .Values.replicas }}
    "#};
    let values_yaml = indoc! {r#"
        replicas: 1
        zones:
          enabled: false
        ha:
          enabled: false
        raft:
          enabled: false
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults skip the gates"),
        (
            serde_json::json!({ "zones": { "enabled": true },
                "ha": { "enabled": true }, "raft": { "enabled": true } }),
            true,
            "the full combination renders",
        ),
        (
            serde_json::json!({ "zones": { "enabled": true } }),
            false,
            "zones without ha aborts",
        ),
        (
            serde_json::json!({ "zones": { "enabled": true }, "ha": { "enabled": true } }),
            false,
            "zones without raft aborts",
        ),
        (
            serde_json::json!({ "ha": { "enabled": true } }),
            true,
            "ha alone stays open",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "pipeline tostring gates ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// datadog's OTLP verify helpers are included with the dot bound to a
/// SCALAR (`include "verify-…" .grpc.endpoint` under a `with` over the
/// protocols map): their `hasPrefix "unix:" .` / `not (regexMatch
/// ":[0-9]+$" .)` fails must bind the caller's endpoint path with the
/// enabling guards retained (helm rejects the unix and portless
/// endpoints, renders the host:port one).
#[test]
fn scalar_dot_helper_terminals_bind_the_caller_argument_path() {
    let helpers = indoc! {r#"
        {{- define "repro.verifyPrefix" -}}
        {{- if hasPrefix "unix:" . }}
        {{ fail "'unix' protocol is not supported" }}
        {{- end }}
        {{- end -}}
        {{- define "repro.verifyPort" -}}
        {{- if not ( regexMatch ":[0-9]+$" . ) }}
        {{ fail "port must be set explicitly" }}
        {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        ports:
          {{- with .Values.otlp.protocols }}
          {{- if (and .grpc .grpc.enabled) }}
          {{- include "repro.verifyPrefix" .grpc.endpoint }}
          {{- include "repro.verifyPort" .grpc.endpoint }}
          - port: 4317
          {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        otlp:
          protocols:
            grpc:
              enabled: false
              endpoint: "0.0.0.0:4317"
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (endpoint, want, label) in [
        ("0.0.0.0:4317", true, "host:port renders"),
        ("unix:///tmp/otlp.sock", false, "unix protocol aborts"),
        ("0.0.0.0", false, "a portless endpoint aborts"),
        // A port-suffixed unix endpoint passes the port test, so only the
        // decoded prefix terminal can reject it (helm-verified on datadog).
        (
            "unix:///tmp/otlp.sock:4317",
            false,
            "unix protocol aborts despite a port",
        ),
    ] {
        let enabled = serde_json::json!({ "otlp": { "protocols": { "grpc": {
            "enabled": true, "endpoint": endpoint } } } });
        assert!(
            schema_accepts_instance(&schema, &enabled) == want,
            "scalar-dot helper terminal ({label}): instance={enabled}; schema={schema}"
        );
        let disabled = serde_json::json!({ "otlp": { "protocols": { "grpc": {
            "enabled": false, "endpoint": endpoint } } } });
        assert!(
            schema_accepts_instance(&schema, &disabled),
            "scalar-dot helper terminal (disabled gate keeps {label} open): \
             instance={disabled}; schema={schema}"
        );
    }
}

/// oauth2-proxy's `redis.StandaloneUrl` helper terminates rendering when
/// neither `connectionUrl` is set nor the redis subchart enabled; the
/// caller invokes it only for the `standalone` client type. The helper's
/// fail must reach the caller with BOTH its internal guards (the url
/// truthiness and the subchart-enabled include, whose helper renders a
/// single decodable boolean expression) AND the caller's live clientType
/// guard.
#[test]
fn helper_terminals_keep_caller_guards_and_boolean_include_arms() {
    let helpers = indoc! {r#"
        {{- define "repro.enabled" -}}
          {{- eq (index .Values "redis-ha" "enabled") true -}}
        {{- end -}}
        {{- define "repro.url" -}}
        {{- if .Values.session.url -}}
        {{ .Values.session.url }}
        {{- else if eq (include "repro.enabled" .) "true" -}}
        {{- printf "redis://auto" -}}
        {{- else -}}
        {{ fail "please set session.url or enable the redis subchart" }}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        config:
          {{- if eq (default "" .Values.session.clientType) "standalone" }}
          url: {{ include "repro.url" . }}
          {{- end }}
          ok: true
    "#};
    let values_yaml = indoc! {r#"
        session:
          clientType: ""
        redis-ha:
          enabled: false
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults skip the include"),
        (
            serde_json::json!({ "session": { "clientType": "standalone",
                "url": "redis://myredis:6379" } }),
            true,
            "an explicit url renders",
        ),
        (
            serde_json::json!({ "session": { "clientType": "standalone" },
                "redis-ha": { "enabled": true } }),
            true,
            "the enabled subchart computes the url",
        ),
        (
            serde_json::json!({ "session": { "clientType": "standalone" } }),
            false,
            "standalone without a url aborts",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "helper terminal caller guards ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A root-context key assigned a literal in EVERY arm of a complete
/// if/else chain (vault's five-arm `vault.mode`) joins into a value
/// dispatch: `ne .mode "external"` / `eq .mode "ha"` decode as the exact
/// disjunction of the assigning arms. Fails behind those guards reach the
/// schema, and a configuration selecting the "external" arm keeps the
/// gated documents dormant.
#[test]
fn root_set_literal_chains_decode_as_value_dispatch_guards() {
    let helpers = indoc! {r#"
        {{- define "repro.mode" -}}
          {{- if .Values.externalAddr -}}
            {{- $_ := set . "mode" "external" -}}
          {{- else if eq (.Values.ha.enabled | toString) "true" -}}
            {{- $_ := set . "mode" "ha" -}}
          {{- else -}}
            {{- $_ := set . "mode" "standalone" -}}
          {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{ template "repro.mode" . }}
        {{- if ne .mode "external" }}
        {{- if .Values.route.enabled }}
        {{- if not .Values.route.parentRefs }}
        {{- fail "route.parentRefs must be set when route is enabled" -}}
        {{- end }}
        {{- end }}
        {{- if eq .mode "ha" }}
        {{- if not .Values.ha.replicas }}
        {{- fail "ha mode requires ha.replicas" -}}
        {{- end }}
        {{- end }}
        kind: ConfigMap
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        externalAddr: ""
        ha:
          enabled: false
          replicas: 0
        route:
          enabled: false
          parentRefs: []
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults skip every gate"),
        (
            serde_json::json!({ "route": { "enabled": true } }),
            false,
            "an enabled route without parentRefs aborts in standalone mode",
        ),
        (
            serde_json::json!({ "route": { "enabled": true,
                "parentRefs": [ { "name": "gw" } ] } }),
            true,
            "an enabled route with parentRefs renders",
        ),
        (
            serde_json::json!({ "route": { "enabled": true },
                "externalAddr": "https://vault.example.com" }),
            true,
            "the external arm keeps the gated document dormant",
        ),
        (
            serde_json::json!({ "ha": { "enabled": true } }),
            false,
            "ha mode without replicas aborts through the eq dispatch",
        ),
        (
            serde_json::json!({ "ha": { "enabled": true, "replicas": 3 } }),
            true,
            "ha mode with replicas renders",
        ),
        (
            serde_json::json!({ "ha": { "enabled": true },
                "externalAddr": "https://vault.example.com" }),
            true,
            "the external arm outranks the ha arm in the chain",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "root-set value dispatch ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// `semverCompare` over a Capabilities-defaulted version local decodes
/// against the analysis-policy Kubernetes version: with the override unset
/// the policy version decides the gate constantly, and a truthy override
/// substitutes its own exact constraint language (kube-prometheus-stack's
/// grafana dashboard document gates; helm-verified with
/// `--kube-version` / `kubeTargetVersionOverride` probes).
#[test]
fn capabilities_defaulted_semver_gates_decode_against_the_policy_version() {
    let src = indoc! {r#"
        {{- $kubeTargetVersion := default .Capabilities.KubeVersion.GitVersion .Values.versionOverride }}
        {{- if and .Values.gate (semverCompare ">=1.14.0-0" $kubeTargetVersion) }}
        {{- if not .Values.selector }}
        {{- fail "selector must be specified" }}
        {{- end }}
        kind: ConfigMap
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        gate: false
        versionOverride: ""
        selector: {}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_kubernetes_version(src, "1.29.0"),
        Some(values_yaml),
    );
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults keep the gate off"),
        (
            serde_json::json!({ "gate": true }),
            false,
            "the policy version satisfies the constraint, so the fail binds",
        ),
        (
            serde_json::json!({ "gate": true, "selector": { "app": "x" } }),
            true,
            "a selector satisfies the terminal",
        ),
        (
            serde_json::json!({ "gate": true, "versionOverride": "1.13.0" }),
            true,
            "an old override turns the gate off exactly",
        ),
        (
            serde_json::json!({ "gate": true, "versionOverride": "1.20.0" }),
            false,
            "a satisfying override keeps the fail bound",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "capabilities semver gate ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// Sprig `dig` splits its subject and intermediate-step contracts: the
/// SUBJECT is type-asserted before any missing-key handling (an explicit
/// null aborts; absence stays open to the caller's defaults), while an
/// INTERMEDIATE step falls back to the dig default when nil but aborts on
/// any other non-map — including Helm-falsy scalars (KPS's nulled
/// `customRules` and trivy-operator's nulled `trivy.resources`).
#[test]
fn dig_subjects_reject_null_while_intermediate_nils_fall_back() {
    let src = indoc! {r#"
        {{- if .Values.rules.create }}
        config:
          severity: {{ dig "alpha" "severity" "critical" .Values.customRules }}
          cpu: {{ dig "resources" "requests" "cpu" "100m" .Values.trivy }}
        {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        rules:
          create: true
        customRules: {}
        trivy: {}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    for (instance, want, label) in [
        (serde_json::json!({}), true, "defaults render"),
        (
            serde_json::json!({ "rules": { "create": true }, "customRules": null }),
            false,
            "a null dig subject aborts the type assertion",
        ),
        (
            serde_json::json!({ "rules": { "create": true }, "customRules": "junk" }),
            false,
            "a scalar dig subject aborts",
        ),
        (
            serde_json::json!({ "customRules": { "alpha": { "severity": "warning" } } }),
            true,
            "a map subject renders",
        ),
        (
            serde_json::json!({ "customRules": null, "rules": { "create": false } }),
            true,
            "the create gate keeps the dig dormant",
        ),
        (
            serde_json::json!({ "trivy": { "resources": null } }),
            true,
            "a nil intermediate step falls back to the default",
        ),
        (
            serde_json::json!({ "rules": { "create": true }, "trivy": { "resources": false } }),
            false,
            "a falsy non-nil intermediate aborts",
        ),
        (
            serde_json::json!({ "rules": { "create": true }, "trivy": { "resources": "junk" } }),
            false,
            "a scalar intermediate aborts",
        ),
        (
            serde_json::json!({ "trivy": { "resources": { "requests": { "cpu": "1" } } } }),
            true,
            "map intermediates render",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "dig subject/intermediate contract ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// A per-op requirement fail over a DIRECT range (`and (or (eq .op …))
/// (not (hasKey . "from"))` → fail) negates to the exact per-member
/// disjunction: complete patches of the gated ops render while a gated op
/// missing its companion key aborts (the nats jsonpatch engine's
/// `copy`/`move`-without-`from` shape).
#[test]
fn per_op_requirement_binds_in_a_direct_range() {
    let src = indoc! {r#"
        {{- range $patch := .Values.service.patch }}
        {{- if and (or (eq $patch.op "copy") (eq $patch.op "move")) (not (hasKey $patch "from")) }}
        {{- fail "missing from" }}
        {{- end }}
        {{- end }}
        config:
          ok: true
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("service:\n  patch: []\n"));
    for (item, want, label) in [
        (
            serde_json::json!({ "op": "copy", "from": "/x" }),
            true,
            "copy with from",
        ),
        (
            serde_json::json!({ "op": "add" }),
            true,
            "add needs no from",
        ),
        (
            serde_json::json!({ "op": "copy" }),
            false,
            "copy without from",
        ),
        (
            serde_json::json!({ "op": "move" }),
            false,
            "move without from",
        ),
    ] {
        let instance = serde_json::json!({ "service": { "patch": [item] } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "per-op requirement direct range ({label}): instance={instance}; schema={schema}"
        );
    }
}

/// The same per-op requirement binds through the helper JSON roundtrip
/// (`$params := fromJson (toJson .)` + `range $params.patch`): the
/// roundtripped member keeps its identity, so the `hasKey` conjuncts
/// decode instead of poisoning the capture with approximates — the
/// helper's call-dict `patch` field must not shadow the range variable of
/// the same name.
#[test]
fn per_op_requirement_binds_through_the_helper_roundtrip() {
    let helpers = indoc! {r#"
        {{- define "repro.jsonpatch" -}}
        {{- $params := fromJson (toJson .) -}}
        {{- $patches := $params.patch -}}
        {{- $docContainer := pick $params "doc" -}}
        {{- range $patch := $patches -}}
        {{- if not (hasKey $patch "op") -}}{{- fail "missing op" -}}{{- end -}}
        {{- if and (or (eq $patch.op "copy") (eq $patch.op "move")) (not (hasKey $patch "from")) -}}
        {{- fail "missing from" -}}
        {{- end -}}
        {{- end -}}
        {{- toJson $docContainer -}}
        {{- end -}}

        {{- define "repro.load" -}}
        {{- $doc := dict -}}
        {{- get (include "repro.jsonpatch" (dict "doc" $doc "patch" (.patch | default list)) | fromJson ) "doc" | toYaml -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: cm
        data:
          out: {{ include "repro.load" (dict "patch" .Values.service.patch) | quote }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("service:\n  patch: []\n"),
    );
    for (item, want, label) in [
        (
            serde_json::json!({ "op": "copy", "from": "/x" }),
            true,
            "copy with from",
        ),
        (
            serde_json::json!({ "op": "add" }),
            true,
            "add needs no from",
        ),
        (serde_json::json!({ "path": "/x" }), false, "missing op"),
        (
            serde_json::json!({ "op": "copy" }),
            false,
            "copy without from",
        ),
    ] {
        let instance = serde_json::json!({ "service": { "patch": [item] } });
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "per-op requirement helper roundtrip ({label}): instance={instance}; schema={schema}"
        );
    }
}
