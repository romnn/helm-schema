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
            serde_json::json!({ "app.ini": { "database": { "password": 7 } } }),
            false,
            "regexMatch rejects a non-string sensitive value",
        ),
        (
            serde_json::json!({ "app.ini": { "database": { "password": "hunter2" } } }),
            false,
            "a plaintext sensitive value hits the fail",
        ),
        (
            serde_json::json!({ "app.ini": { "database": { "password": "$__env{PW}" } } }),
            true,
            "variable expansion renders",
        ),
        (
            serde_json::json!({ "app.ini": { "auth.basic": { "password": "leak" } } }),
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
        (
            serde_json::json!({ "extraEnv": [{ "name": "KUBE_CLIENT_BACKOFF_BASE", "value": "1" }] }),
            false,
            "forbidden name under the live guard",
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
        (serde_json::json!({ "clusterName": "123456789" }), false),
        (serde_json::json!({ "clusterName": "12345678" }), true),
        (serde_json::json!({ "maxClusters": 300 }), false),
        (serde_json::json!({ "maxClusters": 511 }), true),
        // A numeric string coerces to the same bound and stays outside
        // the raw-integer subset.
        (serde_json::json!({ "maxClusters": "255" }), true),
        (serde_json::json!({ "mode": "bogus" }), false),
        (serde_json::json!({ "mode": "external" }), true),
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
        // A leading zero flips ParseInt's base detection to octal, so the
        // spelling stays outside the claimed decimal preimage.
        (
            serde_json::json!({ "controller": { "replicas": "09" } }),
            true,
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
