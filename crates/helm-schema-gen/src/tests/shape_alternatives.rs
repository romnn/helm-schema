use test_util::prelude::sim_assert_eq;

use super::*;

/// Simple template produces correct schema structure.
#[test]
fn simple_template_schema() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        foo: {{ .Values.name }}
        replicas: {{ .Values.replicas }}
        {{- end }}
    "};
    let schema = schema_for(parse_ir(src));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "enabled": {},
            "name": {},
            "replicas": {}
        }
    });
    sim_assert_eq!(have: schema, want: expected);
}

#[test]
fn literal_dotted_index_and_get_keys_generate_one_root_property() {
    let src = indoc! {r#"
        {{- $context := .Values.context -}}
        apiVersion: v1
        kind: ConfigMap
        data:
          direct: {{ index .Values "foo.bar" | quote }}
          selected: {{ (get .Values "foo.bar").baz | quote }}
    "#};
    let values_yaml = indoc! {r#"
        foo.bar:
          baz: value
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema.pointer("/properties/foo.bar").is_some(),
        "the literal dotted key should remain one root segment: {schema}"
    );
    assert!(
        schema.pointer("/properties/foo").is_none(),
        "the path currency must not fabricate a `foo` parent: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "foo.bar": { "baz": "value" } })
        ),
        "the chart's literal dotted-key default should validate: {schema}"
    );
}

#[test]
fn tpl_context_does_not_type_the_templated_value_as_an_object() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          {{- range .Values.items }}
          name: {{ tpl .name $ }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        items:
          - name: example
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let items = schema.pointer("/properties/items").expect("items present");
    let array_arm = ranged_arm_of_type(items, "array")
        .unwrap_or_else(|| panic!("items array arm missing, got {items}"));
    let Some(name) = array_arm.pointer("/items/properties/name") else {
        panic!("ranged item name missing from {schema}");
    };

    assert!(
        permits_type(name, "string"),
        "tpl's first argument is string content: {schema}"
    );
    assert!(
        !permits_type(name, "object"),
        "tpl's context must not become content: {schema}"
    );
}

#[test]
fn tpl_of_to_yaml_without_shape_evidence_stays_untyped() {
    let src = indoc! {r#"
        {{- if .Values.ingress.tls }}
        tls: {{ tpl (toYaml .Values.ingress.tls) $ | nindent 2 }}
        {{- end }}
    "#};
    let values_yaml = "ingress: {}\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for tls in [
        serde_json::json!([]),
        serde_json::json!({ "secretName": "tls" }),
    ] {
        let instance = serde_json::json!({ "ingress": { "tls": tls } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "toYaml provides provenance but no input shape: instance={instance}; schema={schema}"
        );
    }
}

#[test]
fn serialized_collection_owns_descendant_shape() {
    let src = indoc! {r#"
        {{- range .Values.ingress.extraPaths }}
        {{- if .backend.serviceName }}{{ fail "legacy backend" }}{{ end }}
        {{- end }}
        paths: {{ tpl (toYaml .Values.ingress.extraPaths) $ | nindent 2 }}
    "#};
    let values_yaml = "ingress:\n  extraPaths: []\n";
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let instance = serde_json::json!({
        "ingress": {
            "extraPaths": [{
                "path": "/health",
                "backend": {"service": {"name": "health", "port": {"number": 8080}}}
            }]
        }
    });

    assert!(
        schema_accepts_instance(&schema, &instance),
        "descendant reads must not reconstruct serialized input shape: {schema}"
    );
}

#[test]
fn ranged_type_branch_keeps_serialized_object_alternative() {
    let src = indoc! {r#"
        {{- range .Values.extraObjects }}
        {{- if typeIs "string" . }}
        {{ tpl . $ }}
        {{- else }}
        {{ tpl (. | toYaml) $ }}
        {{- end }}
        {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraObjects: []\n"));

    for item in [
        serde_json::json!("kind: ConfigMap"),
        serde_json::json!({"apiVersion": "v1", "kind": "ConfigMap"}),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({"extraObjects": [item]})),
            "typeIs string and serialized else branches must preserve both alternatives: {schema}"
        );
    }
}

#[test]
fn structural_conversion_and_kind_guards_preserve_input_shape_alternatives() {
    let src = indoc! {r#"
        {{- if kindIs "map" .Values.extraArgs }}
        args: {{ toYaml .Values.extraArgs }}
        {{- else if kindIs "slice" .Values.extraArgs }}
        args: {{ toYaml .Values.extraArgs }}
        {{- end }}
        parsed: {{ .Values.config | fromYaml | toYaml }}
        joined: {{ join "," .Values.urls }}
    "#};
    let values_yaml = indoc! {r#"
        extraArgs: {}
        config: "enabled: true"
        urls: sentinel:26379
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for extra_args in [
        serde_json::json!({ "flag": "value" }),
        serde_json::json!(["--flag"]),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "extraArgs": extra_args,
                    "config": "enabled: true",
                    "urls": "sentinel:26379"
                })
            ),
            "kindIs branches must preserve both advertised shapes: {schema}"
        );
    }
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "extraArgs": {},
                "config": "enabled: false",
                "urls": ["one:26379", "two:26379"]
            })
        ),
        "fromYaml consumes strings and join accepts any input: {schema}"
    );
}

#[test]
fn destructured_range_over_declared_map_keeps_map_shape() {
    let src = indoc! {r#"
        ports:
          {{- range $name, $port := .Values.extraPorts }}
          {{ $name }}: {{ $port | quote }}
          {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("extraPorts: {}\n"));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "extraPorts": { "http": 8080 } })
        ),
        "a key/value range over a declared map must accept map inputs: {schema}"
    );
}

#[test]
fn helper_destructured_range_keeps_declared_map_open() {
    let helpers = indoc! {r#"
        {{- define "config-inner" }}
        {{- range $key, $value := .content }}
        {{ $key }} {{ $value }}
        {{- end }}
        {{- end }}
        {{- define "config" }}
        {{- include "config-inner" (dict "content" .content) }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          redis.conf: |
            {{- include "config" (dict "content" .Values.redis.config) | nindent 4 }}
    "#};
    let ir = parse_ir_with_helpers(src, helpers);
    let facts = schema_signals_for(&ir)
        .evidence_for("redis.config")
        .expect("bound helper range evidence")
        .facts;
    assert!(
        facts.is_direct_ranged_source && facts.has_destructured_range_use,
        "a bound helper range must export its direct two-variable domain"
    );
    let schema = schema_for_values_yaml(&ir, Some("redis:\n  config:\n    save: \"\"\n"));

    for config in [
        serde_json::json!({"appendonly": "no"}),
        serde_json::json!(["appendonly no"]),
    ] {
        assert!(
            schema_accepts_instance(&schema, &serde_json::json!({"redis": {"config": config}})),
            "a two-variable helper range accepts map and array lanes: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({"redis": {"config": "appendonly no"}})
        ),
        "an unconditional helper range rejects non-collections: {schema}"
    );
}

#[test]
fn document_condition_keeps_helper_conversion_input_type() {
    let helpers = indoc! {r#"
        {{- define "config-has-processors" }}
        {{- $config := .Values.config | default "" | fromYaml }}
        {{- if $config.processors }}true{{ else }}false{{ end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- if eq (include "config-has-processors" .) "true" }}
        apiVersion: v1
        kind: ConfigMap
        {{- end }}
    "#};
    let schema =
        schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("config: null\n"));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({"config": "processors: {}"})),
        "fromYaml in a condition helper constrains its source as string input: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({"config": {"processors": {}}})),
        "fromYaml must not expose its parsed object as the source input shape: {schema}"
    );
}

#[test]
fn self_guarded_empty_string_preserves_empty_fallback_branch() {
    let provider_schema = serde_json::json!({
        "type": "string",
        "minLength": 1,
        "pattern": "^https?://"
    });
    let values_yaml_schema = serde_json::json!({
        "type": "string"
    });

    let schema = ResolvePolicy.resolve_schema_for_value_path(ValuePathSchemaInputs {
        facts: ValuePathSchemaFacts::new(
            ContractValuePathFacts {
                has_render_use: true,
                all_render_uses_self_guarded: true,
                is_nullable: true,
                ..ContractValuePathFacts::default()
            },
            ValuesYamlPathFacts {
                is_empty_string: true,
                ..ValuesYamlPathFacts::default()
            },
        ),
        provider_schema,
        values_yaml_schema,
        guard_predicate_schema: serde_json::json!({}),
        type_hint_schema: serde_json::json!({}),
        guarded_type_hint_schema: serde_json::json!({}),
    });

    assert!(
        permits_empty_string(&schema),
        "self-guarded empty-string default should stay valid, got {schema}"
    );
    assert!(
        any_of_variant_matching(&schema, |variant| {
            variant.get("minLength").and_then(Value::as_u64) == Some(1)
        })
        .is_some(),
        "non-empty rendered values should keep provider constraints, got {schema}"
    );
    assert!(
        permits_null(&schema),
        "nullable wrapping should preserve the empty-string branch, got {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!("not-a-url")),
        "the falsy fallback must not erase provider facets for non-empty values: {schema}"
    );
}

#[test]
fn declared_object_members_open_only_the_closed_levels_that_reject_them() {
    let schema = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "known": {"type": "string"},
            "nested": {
                "type": "object",
                "additionalProperties": false,
                "properties": {"typed": {"type": "integer"}}
            }
        }
    });
    let declared = serde_json::json!({
        "known": "value",
        "extra": true,
        "nested": {"typed": 1, "extension": "value"}
    });

    let opened = open_objects_rejecting_declared_members(schema, &declared);

    sim_assert_eq!(
        have: opened,
        want: serde_json::json!({
            "type": "object",
            "properties": {
                "known": {"type": "string"},
                "nested": {
                    "type": "object",
                    "properties": {"typed": {"type": "integer"}}
                }
            }
        })
    );
}

/// A truthy guard is a control-flow fact, not a type assertion.
#[test]
fn guard_only_values_without_type_evidence_stay_unconstrained() {
    let src = indoc! {r"
        {{- if .Values.feature.enabled }}
        key: {{ .Values.feature.name }}
        {{- end }}
    "};
    let schema = schema_for(parse_ir(src));

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "feature": {
                "additionalProperties": {},
                "properties": {
                    "enabled": {},
                    "name": {}
                },
                "type": "object"
            }
        }
    });
    sim_assert_eq!(have: schema, want: expected);
}

/// A `with`-guarded fragment accepts null for object inputs too: Helm skips
/// the body when the guarded value is nil, so the chart input contract includes
/// both the rendered object shape and null.
#[test]
fn step1_with_fragment_null_default_is_nullable() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ServiceAccount
        metadata:
          name: test
          {{- with .Values.extraAnnotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        extraAnnotations:
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    assert!(
        permits_type(extra, "object"),
        "extraAnnotations should keep the K8s annotations object shape, got {extra}"
    );
    assert!(
        permits_null(extra),
        "with-guarded fragment object should allow null, got {extra}"
    );
}

/// Step 1 negative: a path with no `with`-fragment use does not get widened
/// to include null on the strength of Step 1 alone. (When the same fixture
/// is run through Step 2, the type hint adds the nullable-string union.)
#[test]
fn step1_no_with_fragment_does_not_widen_to_null() {
    // No `with`, no `default` — just a plain reference. Step 1's predicate
    // requires a Fragment use, which doesn't exist here.
    let src = indoc! {r"
        name: {{ .Values.nameOverride }}
    "};
    let values_yaml = indoc! {"
        nameOverride:
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // nameOverride should remain `{}` — no signal points to a specific type.
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    sim_assert_eq!(have: name, want: &serde_json::json!({}));
}

#[test]
fn self_guarded_null_default_without_sink_type_stays_unconstrained() {
    let src = indoc! {r"
        {{- if .Values.terminationGracePeriodSeconds }}
        terminationGracePeriodSeconds: {{ .Values.terminationGracePeriodSeconds }}
        {{- end }}
    "};
    let values_yaml = indoc! {"
        terminationGracePeriodSeconds:
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
    let Some(termination_grace_period) =
        schema.pointer("/properties/terminationGracePeriodSeconds")
    else {
        panic!("terminationGracePeriodSeconds missing from {schema}");
    };

    sim_assert_eq!(
        have: termination_grace_period,
        want: &serde_json::json!({}),
        "a null default is an unset sentinel, not exclusive null typing: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "terminationGracePeriodSeconds": 90 })
        ),
        "a configured non-null value must remain accepted without stronger sink evidence: {schema}"
    );
}

/// `quote`, `squote`, and `toString` call Sprig's `strval`, whose
/// fallback is `fmt.Sprintf("%v", value)` — maps, lists, and nil all render
/// as text. The input domain is unconstrained; only the OUTPUT is a string.
#[test]
fn quote_stringification_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          {{- if .Values.enabled }}
          flag: {{ .Values.flag | quote }}
          count: {{ .Values.count | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        flag: false
        count: 7
        enabled: true
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "enabled": true, "flag": false, "count": 7 }),
        serde_json::json!({ "enabled": true, "flag": "false", "count": "7" }),
        serde_json::json!({ "enabled": true, "flag": {}, "count": 7 }),
        serde_json::json!({ "enabled": true, "flag": false, "count": [] }),
        serde_json::json!({ "enabled": true, "flag": null, "count": { "k": "v" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strval stringifies any input, so quote constrains nothing: instance={instance}; schema={schema}"
        );
    }
}

/// Direct-call forms: the total stringifications accept any input in
/// prefix position too, and `join` converts anything through `strslice`
/// (lists element-wise, non-lists as singletons, nil as empty).
#[test]
fn total_stringification_direct_forms_accept_any_input() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          quoted: {{ quote .Values.quoted }}
          squoted: {{ squote .Values.squoted }}
          stringified: {{ toString .Values.stringified }}
          joined: {{ join "," .Values.joined }}
    "#};
    let values_yaml = indoc! {"
        quoted: probe
        squoted: probe
        stringified: probe
        joined: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for probe in [
        serde_json::json!("text"),
        serde_json::json!(7),
        serde_json::json!(true),
        serde_json::json!(null),
        serde_json::json!({ "k": "v" }),
        serde_json::json!(["item"]),
    ] {
        let instance = serde_json::json!({
            "quoted": probe,
            "squoted": probe,
            "stringified": probe,
            "joined": probe,
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "total stringification accepts any input: instance={instance}; schema={schema}"
        );
    }
}

/// Sprig's numeric casts (`int`, `int64`, `float64`) convert through
/// `cast.ToXxx`, which coerces ANY input (junk becomes zero) instead of
/// failing: metrics-server passes `"365"` through
/// `int .Values.tls.helm.certDurationDays` and coredns emits
/// `.Values.autoscaler.coresPerReplica | float64`, and Helm renders both.
#[test]
fn numeric_casts_accept_any_input() {
    let src = indoc! {r#"
        {{- $days := int .Values.certDurationDays }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          days: {{ $days | quote }}
          cores: {{ .Values.coresPerReplica | float64 }}
    "#};
    let values_yaml = indoc! {"
        certDurationDays: 365
        coresPerReplica: 256
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "certDurationDays": 365, "coresPerReplica": 256 }),
        serde_json::json!({ "certDurationDays": "365", "coresPerReplica": "256" }),
        serde_json::json!({ "certDurationDays": { "bad": true }, "coresPerReplica": [1] }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "numeric casts coerce any input: instance={instance}; schema={schema}"
        );
    }
}

/// A total stringification that appears only inside a CONDITION erases the
/// input shape exactly like the same conversion at a render site or in a
/// `set` expression (vault gates its PSP templates on
/// `eq (.Values.global.psp.enable | toString) "true"`, and Helm accepts the
/// string form).
#[test]
fn condition_only_to_string_erases_declared_typing() {
    let helpers = indoc! {r#"
        {{- define "repro.ha" -}}
        {{- if eq (.Values.ha.enabled | toString) "true" -}}
        true
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{- if eq (.Values.psp.enable | toString) "true" }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: psp
        {{- end }}
        {{- if eq (include "repro.ha" .) "true" }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: ha
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        psp:
          enable: false
        ha:
          enabled: false
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for instance in [
        serde_json::json!({ "psp": { "enable": true }, "ha": { "enabled": true } }),
        serde_json::json!({ "psp": { "enable": "true" }, "ha": { "enabled": "true" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "stringified flag comparisons accept boolean and string forms: instance={instance}; schema={schema}"
        );
    }
}

/// A `typeOf`/`kindOf` comparison dispatches on the value's runtime type
/// (velero: `eq (typeOf .Values.initContainers) "string"` chooses `tpl` vs
/// `toYaml`; vault binds `$type := typeOf .Values.server.affinity` first).
/// Every arm renders SOME types and unmatched types render nothing — also
/// valid — so the dispatch must not close the path to one arm's type, and
/// an arm's sink typing holds only under its test.
#[test]
fn type_dispatch_keeps_string_and_structured_alternatives() {
    let src = indoc! {r#"
        {{- $type := typeOf .Values.affinity }}
        apiVersion: apps/v1
        kind: Deployment
        metadata:
          name: server
        spec:
          template:
            spec:
              affinity:
                {{- if eq $type "string" }}
                {{- tpl .Values.affinity . | nindent 8 }}
                {{- else }}
                {{- toYaml .Values.affinity | nindent 8 }}
                {{- end }}
              initContainers:
                {{- if eq (typeOf .Values.initContainers) "string" }}
                {{- tpl .Values.initContainers . | nindent 8 }}
                {{- else }}
                {{- toYaml .Values.initContainers | nindent 8 }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        affinity: {}
        initContainers: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "affinity": { "nodeAffinity": {} }, "initContainers": [{ "name": "init" }] }),
        serde_json::json!({ "affinity": "{{ .Values.name }}", "initContainers": "- name: init" }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "type dispatch keeps every arm's form valid: instance={instance}; schema={schema}"
        );
    }
}

/// A partial type dispatch (loki's `hostUsers`: a `kindIs "bool"` arm and a
/// string arm, no catch-all) must not close the path to the tested types:
/// an unmatched type renders nothing, which Helm accepts.
#[test]
fn partial_type_dispatch_does_not_close_untested_types() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: probe
        spec:
          {{- if kindIs "bool" .Values.hostUsers }}
          hostUsers: {{ .Values.hostUsers }}
          {{- else if kindIs "string" .Values.hostUsers }}
          hostUsers: {{ tpl .Values.hostUsers . }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        hostUsers: true
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "hostUsers": true }),
        serde_json::json!({ "hostUsers": "{{ .Values.global.hostUsers }}" }),
        serde_json::json!({ "hostUsers": { "unmatched": "renders nothing" } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "untested types render nothing and stay valid: instance={instance}; schema={schema}"
        );
    }
}

/// Direct `typeIs`/`kindIs` tests also use exact Go type names (velero:
/// `typeIs "[]interface {}" .Values.configuration.backupStorageLocation`):
/// the guard is a partial type dispatch, so untested types skip the branch
/// and render nothing, which stays valid.
#[test]
fn type_is_decodes_exact_go_container_names() {
    let src = indoc! {r#"
        {{- if typeIs "[]interface {}" .Values.locations }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: locations
        data:
          {{- range .Values.locations }}
          {{ .name }}: configured
          {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        locations: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for instance in [
        serde_json::json!({ "locations": [{ "name": "a" }] }),
        serde_json::json!({ "locations": "ignored" }),
        serde_json::json!({ "locations": { "unmatched": true } }),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance),
            "exact Go names decode as type tests; untested types skip the branch: instance={instance}; schema={schema}"
        );
    }
}

/// Condition pipelines classify left-to-right (datadog:
/// `eq (.Values.agents.image.tag | toString | trimSuffix "-jmx") "latest"`):
/// a consumer AFTER a total conversion operates on converted text and
/// claims nothing about the raw value, while a consumer BEFORE any
/// conversion still binds the raw string contract.
#[test]
fn condition_pipeline_order_scopes_string_consumers() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if eq (.Values.tag | toString | trimSuffix "-jmx") "latest" }}
          latest: "true"
          {{- end }}
          {{- if eq (.Values.suffix | trimSuffix "-" | toString) "x" }}
          suffixed: "true"
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        tag: latest
        suffix: x-
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(&schema, &serde_json::json!({ "tag": 7 })),
        "toString converts before the trim, so numbers render: {schema}"
    );
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "suffix": { "bad": true } })),
        "a consumer ahead of the conversion still needs a string: {schema}"
    );
}
