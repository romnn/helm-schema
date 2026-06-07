use std::collections::BTreeMap;

use indoc::indoc;
use serde_json::Value;

use crate::{
    DefaultValuesSchemaGenerator, ValuesSchemaGenerator, generate_values_schema_full,
    generate_values_schema_with_values_yaml,
};
use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    IrGenerator, ResourceRef, SymbolicIrGenerator, ValueUse, YamlPath, extract_default_type_hints,
};
use helm_schema_k8s::{Chain, KubernetesJsonSchemaProvider};

fn provider() -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true)
}

fn production_chain_provider() -> Chain {
    let k8s_provider = KubernetesJsonSchemaProvider::new("v1.35.0")
        .with_allow_download(true)
        .with_api_version_guess(true);
    Chain::new(vec![Box::new(k8s_provider)]).with_inference_enabled(true)
}

fn parse_ir(src: &str) -> Vec<ValueUse> {
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    SymbolicIrGenerator.generate(src, &ast, &idx)
}

fn parse_ir_fused(src: &str) -> Vec<ValueUse> {
    let ast = FusedRustParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    SymbolicIrGenerator.generate(src, &ast, &idx)
}

fn parse_ir_with_helpers(src: &str, helpers: &str) -> Vec<ValueUse> {
    let ast = TreeSitterParser.parse(src).expect("parse");
    let mut idx = DefineIndex::new();
    if !helpers.trim().is_empty() {
        idx.add_file_source("helpers.tpl", helpers);
        idx.add_source(&TreeSitterParser, helpers)
            .expect("helpers parse");
    }
    SymbolicIrGenerator.generate(src, &ast, &idx)
}

fn collect_hints(src: &str) -> BTreeMap<String, Vec<Value>> {
    let mut hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for (path, schema) in extract_default_type_hints(src) {
        hints.entry(path).or_default().push(schema);
    }
    hints
}

/// True if the schema permits a `null` value — either directly via
/// `{"type": "null"}` or as one branch of an `anyOf` union.
fn permits_null(schema: &Value) -> bool {
    if schema.get("type").and_then(Value::as_str) == Some("null") {
        return true;
    }
    schema
        .get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|variants| {
            variants
                .iter()
                .any(|v| v.get("type").and_then(Value::as_str) == Some("null"))
        })
}

/// Simple template produces correct schema structure.
#[test]
fn simple_template_schema() {
    let src = indoc! {r"
        {{- if .Values.enabled }}
        foo: {{ .Values.name }}
        replicas: {{ .Values.replicas }}
        {{- end }}
    "};
    let schema = DefaultValuesSchemaGenerator.generate(&parse_ir(src), &provider());

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "enabled": {"type": "boolean"},
            "name": {},
            "replicas": {}
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

/// Guard-like values (*.enabled) get boolean type.
#[test]
fn guard_values_get_boolean_type() {
    let src = indoc! {r"
        {{- if .Values.feature.enabled }}
        key: {{ .Values.feature.name }}
        {{- end }}
    "};
    let schema = DefaultValuesSchemaGenerator.generate(&parse_ir(src), &provider());

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "feature": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "name": {}
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

/// Step 1: a path used as a YAML fragment inside `with` with a null default in
/// values.yaml gets a nullable union (provider object | null), so the chart
/// can ship `extraAnnotations:` (null) without lint errors.
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
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    let variants = extra
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected anyOf union");
    assert!(
        permits_null(extra),
        "extraAnnotations should permit null, got {extra}"
    );
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("object")),
        "extraAnnotations should also accept the K8s annotations object, got {extra}"
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
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    // nameOverride should remain `{}` — no signal points to a specific type.
    let name = schema
        .pointer("/properties/nameOverride")
        .expect("nameOverride present");
    similar_asserts::assert_eq!(name, &serde_json::json!({}));
}

/// Step 2 (prefix form): `default <literal> .Values.X` with null default in
/// values.yaml produces a nullable-typed union for X.
#[test]
fn step2_default_prefix_string_literal_is_nullable_string() {
    let src = indoc! {r#"
        name: {{ default "fallback" .Values.name }}
    "#};
    let values_yaml = indoc! {"
        name:
    "};
    let schema = generate_values_schema_full(
        &parse_ir(src),
        &provider(),
        Some(values_yaml),
        &collect_hints(src),
    );

    let name = schema.pointer("/properties/name").expect("name present");
    let variants = name
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected anyOf union for nullable-string");
    assert!(permits_null(name));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string"))
    );
}

/// Step 2 (pipeline form): `.Values.X | default <literal>` is recognised
/// equivalently to the prefix form.
#[test]
fn step2_default_pipeline_string_literal_is_nullable_string() {
    let src = indoc! {r#"
        name: {{ .Values.name | default "fallback" }}
    "#};
    let values_yaml = indoc! {"
        name:
    "};
    let schema = generate_values_schema_full(
        &parse_ir(src),
        &provider(),
        Some(values_yaml),
        &collect_hints(src),
    );

    let name = schema.pointer("/properties/name").expect("name present");
    let variants = name
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected anyOf union for nullable-string");
    assert!(permits_null(name));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string"))
    );
}

/// Step 2 negative: `default $someVar .Values.x` with a non-literal first
/// argument emits no type hint. Schema is unchanged.
#[test]
fn step2_default_non_literal_first_arg_no_hint() {
    // The first arg is a variable, not a literal. Recognizer must skip.
    let src = indoc! {r#"
        {{- $fallback := "x" -}}
        name: {{ default $fallback .Values.name }}
    "#};
    let hints = collect_hints(src);
    assert!(hints.is_empty(), "expected no hints, got {hints:?}");
}

/// Step 2: integer literal → integer type hint (not string).
#[test]
fn step2_default_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default 5 .Values.replicas }}
    "};
    let hints = collect_hints(src);
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "expected integer hint, got {schemas:?}"
    );
}
/// `with or .Values.A .Values.B` now tags both A and B with `Guard::With`
/// (instead of keeping them as `Guard::Or`), so a downstream Fragment use of
/// either path qualifies for Step 1 null preservation. The body's `.` is not
/// rewritten in `with or` (dot-binding requires a single header path), so
/// this test references the path explicitly to drive a Fragment use.
#[test]
fn step1_with_or_per_path_guards_enable_null_preservation() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- with or .Values.primary .Values.fallback }}
          config: |
            {{- toYaml .Values.primary | nindent 4 }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        primary:
        fallback:
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let primary = schema
        .pointer("/properties/primary")
        .expect("primary property present");
    assert!(
        permits_null(primary),
        "primary should permit null after `with or` + explicit Fragment use, got {primary}"
    );
}

/// Step 1 must NOT widen a non-null default for a with-fragment path —
/// only null defaults qualify. Regression guard: a fixed values.yaml value
/// should remain the source of truth.
#[test]
fn step1_with_fragment_non_null_default_not_widened() {
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
          foo: bar
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    assert!(
        !permits_null(extra),
        "non-null default must not be widened to nullable, got {extra}"
    );
}

/// Explicit `null` defaults stay valid when a scalar is rendered only from a
/// `with` body that skips on nil. This is the common `priorityClassName`
/// pattern across many charts.
#[test]
fn nullable_scalar_preserved_for_with_guarded_render_use() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              {{- with .Values.priorityClassName }}
              priorityClassName: {{ . }}
              {{- end }}
    "};
    let values_yaml = indoc! {"
        priorityClassName:
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let priority = schema
        .pointer("/properties/priorityClassName")
        .expect("priorityClassName present");
    let variants = priority
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected nullable priorityClassName union");
    assert!(permits_null(priority));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("string")),
        "priorityClassName should also accept the provider string type, got {priority}"
    );
}

/// Explicit `null` defaults also stay valid when a scalar is rendered only
/// from a truthy self-guard inside a larger condition, such as optional
/// Service nodePorts gated by `not (empty ...)`.
#[test]
fn nullable_scalar_preserved_for_truthy_guarded_render_use() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        spec:
          type: {{ .Values.service.type }}
          ports:
            {{- with .Values.service }}
            - port: 25
              {{- if (and (eq .type "NodePort") (not (empty .ports.smtp.nodePort))) }}
              nodePort: {{ .ports.smtp.nodePort }}
              {{- end }}
            {{- end }}
    "#};
    let values_yaml = indoc! {"
        service:
          type: ClusterIP
          ports:
            smtp:
              nodePort:
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let node_port = schema
        .pointer("/properties/service/properties/ports/properties/smtp/properties/nodePort")
        .expect("service.ports.smtp.nodePort present");
    let variants = node_port
        .get("anyOf")
        .and_then(Value::as_array)
        .expect("expected nullable nodePort union");
    assert!(permits_null(node_port));
    assert!(
        variants
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "nodePort should also accept the provider integer type, got {node_port}"
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
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let pod_labels = schema
        .pointer("/properties/podLabels")
        .expect("podLabels present");
    assert_eq!(
        pod_labels
            .get("additionalProperties")
            .and_then(Value::as_object)
            .and_then(|obj| obj.get("type"))
            .and_then(Value::as_str),
        Some("string"),
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
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let annotations = schema
        .pointer("/properties/annotations")
        .expect("annotations present");
    assert_eq!(
        annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "annotations should stay an open string map, got {annotations}"
    );
}

/// Destructured map ranges should keep the chart input as a map, even when the
/// rendered output lands in a K8s array field like `env:`.
#[test]
fn destructured_range_map_input_does_not_become_output_array() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: test
              image: busybox
              env:
                {{- range $key, $value := .Values.environment }}
                - name: {{ $key }}
                  value: {{ $value | quote }}
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        environment:
          INBUCKET_LOGLEVEL: debug
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    assert_eq!(
        environment.get("type").and_then(Value::as_str),
        Some("object"),
        "environment should stay an object-valued input, got {environment}"
    );
    assert_eq!(
        environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "environment should generalize to an open string map when the chart ranges over its entries, got {environment}"
    );
    assert!(
        environment.get("anyOf").is_none(),
        "environment should not widen to object-or-array, got {environment}"
    );
}

#[test]
fn destructured_range_map_with_len_guard_generalizes_to_open_string_map() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        spec:
          containers:
            - name: test
              image: busybox
              {{- if (gt (len .Values.environment) 0) }}
              env:
                {{- range $key, $value := .Values.environment }}
                - name: {{ $key }}
                  value: {{ $value | quote }}
                {{- end }}
              {{- end }}
    "#};
    let values_yaml = indoc! {"
        environment:
          INBUCKET_LOGLEVEL: debug
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let environment = schema
        .pointer("/properties/environment")
        .expect("environment present");
    assert_eq!(
        environment
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "len-guarded destructured range should still generalize to an open string map, got {environment}"
    );
}

/// A scalar-item range that directly renders the sequence items should keep the
/// provider array metadata on the destination field, not collapse to a bare
/// `items.type` array inferred only from the item uses.
#[test]
fn scalar_item_range_keeps_provider_array_metadata() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: PersistentVolumeClaim
        metadata:
          name: test
        spec:
          accessModes:
          {{- range .Values.accessModes }}
            - {{ . | quote }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        accessModes:
          - ReadWriteOnce
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let access_modes = schema
        .pointer("/properties/accessModes")
        .expect("accessModes present");
    assert_eq!(
        access_modes.get("type").and_then(Value::as_str),
        Some("array"),
        "accessModes should be an array, got {access_modes}"
    );
    assert_eq!(
        access_modes.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "accessModes items should stay strings, got {access_modes}"
    );
    assert!(
        access_modes
            .pointer("/description")
            .and_then(Value::as_str)
            .is_some(),
        "accessModes should keep the provider description, got {access_modes}"
    );
    assert_eq!(
        access_modes
            .pointer("/x-kubernetes-list-type")
            .and_then(Value::as_str),
        Some("atomic"),
        "accessModes should keep the provider list metadata, got {access_modes}"
    );
}

/// A scalar input list that is wrapped into object-valued output items should
/// stay a scalar values array and must not inherit the provider object-item
/// schema for the rendered resource field.
#[test]
fn scalar_range_wrapped_into_object_items_stays_scalar_array() {
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: Ingress
        metadata:
          name: test
        spec:
          rules:
          {{- range .Values.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                {{- range .paths }}
                  - path: {{ . | quote }}
                    pathType: Prefix
                    backend:
                      service:
                        name: app
                        port:
                          number: 80
                {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        hosts:
          - host: example.test
            paths:
              - /
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let host_paths = schema
        .pointer("/properties/hosts/items/properties/paths")
        .expect("hosts[].paths present");
    assert_eq!(
        host_paths.get("type").and_then(Value::as_str),
        Some("array"),
        "hosts[].paths should stay an array input, got {host_paths}"
    );
    assert_eq!(
        host_paths.pointer("/items/type").and_then(Value::as_str),
        Some("string"),
        "hosts[].paths items should stay strings, got {host_paths}"
    );
    assert!(
        host_paths.pointer("/items/anyOf").is_none(),
        "hosts[].paths should not widen to object|string items, got {host_paths}"
    );
}

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

    let schema = generate_values_schema_with_values_yaml(
        &parse_ir_with_helpers(src, helpers),
        &provider(),
        Some(values_yaml),
    );

    let service_account = schema
        .pointer("/properties/serviceAccount")
        .expect("serviceAccount present");
    assert_eq!(
        service_account.get("type").and_then(Value::as_str),
        Some("object"),
        "serviceAccount should remain an object-valued input, got {service_account}"
    );
    assert!(
        service_account.get("anyOf").is_none(),
        "serviceAccount should not widen to object-or-string, got {service_account}"
    );
}

/// A destructured `range $k, $v := .` inside an outer `with .Values.X` should
/// still attribute the rendered map field back to `X`, so provider schemas can
/// type it as an open string map.
#[test]
fn with_bound_range_dot_annotations_stay_string_map() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          {{- with .Values.annotations }}
          annotations:
            {{- range $key, $value := . }}
            {{ $key }}: {{ $value | quote }}
            {{- end }}
          {{- end }}
    "#};
    let values_yaml = indoc! {"
        annotations:
          foo: bar
    "};
    let ir = parse_ir(src);
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::to_value(&ir).expect("ir json"))
                .expect("pretty ir")
        );
    }
    let schema = generate_values_schema_with_values_yaml(&ir, &provider(), Some(values_yaml));

    let annotations = schema
        .pointer("/properties/annotations")
        .expect("annotations present");
    assert_eq!(
        annotations
            .pointer("/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "annotations should stay an open string map, got {annotations}"
    );
}

/// A quoted YAML key inside a string-map field should still keep the concrete
/// leaf path, so the map value is typed as the string entry schema instead of
/// the parent object schema.
#[test]
fn quoted_matchlabels_key_value_stays_string() {
    let src = indoc! {r#"
        apiVersion: networking.k8s.io/v1
        kind: NetworkPolicy
        metadata:
          name: test
          namespace: "{{ .Values.networkPolicies.ingressController.namespace }}"
        spec:
          ingress:
            - from:
                - namespaceSelector:
                    matchLabels:
                      "kubernetes.io/metadata.name": "{{ .Values.networkPolicies.ingressController.namespace }}"
    "#};
    let values_yaml = indoc! {"
        networkPolicies:
          ingressController:
            namespace: ingress-nginx
    "};
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let namespace = schema
        .pointer("/properties/networkPolicies/properties/ingressController/properties/namespace")
        .expect("namespace present");
    assert_eq!(
        namespace.get("type").and_then(Value::as_str),
        Some("string"),
        "quoted map-key value should stay string-valued, got {namespace}"
    );
    assert!(
        namespace.get("anyOf").is_none(),
        "quoted map-key value should not widen to object-or-string, got {namespace}"
    );
}

#[test]
fn mapping_key_template_does_not_project_scalar_onto_parent_map_value_schema() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{ .Values.account.name }}.json: |
            {}
    "#};
    let values_yaml = indoc! {"
        account:
          name: surveyor
    "};

    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));
    let name = schema
        .pointer("/properties/account/properties/name")
        .expect("account.name present");

    assert_eq!(
        name.get("type").and_then(Value::as_str),
        Some("string"),
        "mapping-key interpolation should keep account.name string-valued, got {name}"
    );
    assert!(
        name.get("anyOf").is_none(),
        "mapping-key interpolation must not widen account.name with ConfigMap.data provider shape, got {name}"
    );
}

#[test]
fn exact_bound_helper_yaml_body_propagates_paths() {
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
          {{- if .config.tls }}
          tls:
            {{- range .config.tls }}
            - secretName: {{ .secretName }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .config.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    backend:
                      service:
                        port:
                          {{- if .servicePort -}}
                          {{- toYaml .servicePort | nindent 26 }}
                          {{- else -}}
                          number: {{ $.ctx.Values.service.port }}
                          {{- end }}
                  {{- end }}
            {{- end }}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        {{ include "common.ingress" (dict "ctx" $ "config" .Values.ingress) }}
    "#};
    let values_yaml = indoc! {"
        ingress:
          className: nginx
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
    if std::env::var("IR_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&serde_json::to_value(&ir).expect("ir json"))
                .expect("pretty ir")
        );
    }
    let schema = generate_values_schema_with_values_yaml(&ir, &provider(), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/className/type")
            .and_then(Value::as_str),
        Some("string"),
        "helper body should propagate ingress.className, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        Some("string"),
        "helper body should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert!(
        schema
            .pointer("/properties/service/properties/port")
            .is_some(),
        "helper body should propagate service.port from $.ctx.Values.service.port, got {schema}"
    );
}

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
            - host: {{ .host | quote }}
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
    let schema = generate_values_schema_with_values_yaml(&ir, &provider(), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/className/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.className, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/annotations/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.annotations, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        Some("string"),
        "with-bound dot helper call should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/hosts/items/properties/host/type")
            .and_then(Value::as_str),
        Some("string"),
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
    let schema = generate_values_schema_with_values_yaml(&ir, &provider(), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/className/type")
            .and_then(Value::as_str),
        Some("string"),
        "helper body should infer ingress.className from the output path even without a values.yaml example, got {schema}"
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

    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/leaderElection/properties/leaseDuration/type")
            .and_then(Value::as_str),
        Some("string"),
        "inline sequence scalar interpolation should infer leaderElection.leaseDuration as string, got {schema}"
    );
}

#[test]
fn mixed_inline_template_gaps_in_scalar_sequence_item_keep_string_paths() {
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

    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    for pointer in [
        "/properties/image/properties/registry/type",
        "/properties/image/properties/repository/type",
        "/properties/image/properties/digest/type",
    ] {
        assert_eq!(
            schema.pointer(pointer).and_then(Value::as_str),
            Some("string"),
            "mixed inline template gaps should keep {pointer} string-valued, got {schema}"
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

    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    for pointer in [
        "/properties/image/properties/registry/type",
        "/properties/image/properties/repository/type",
        "/properties/image/properties/digest/type",
    ] {
        assert_eq!(
            schema.pointer(pointer).and_then(Value::as_str),
            Some("string"),
            "with-bound mixed inline template gaps should keep {pointer} string-valued, got {schema}"
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
            - host: {{ .host | quote }}
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
    let schema = generate_values_schema_with_values_yaml(&ir, &provider(), Some(values_yaml));

    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/annotations/additionalProperties/type")
            .and_then(Value::as_str),
        Some("string"),
        "realistic common.ingress helper should keep ingress.annotations open, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/className/type")
            .and_then(Value::as_str),
        Some("string"),
        "realistic common.ingress helper should propagate ingress.className, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/tls/items/properties/secretName/type")
            .and_then(Value::as_str),
        Some("string"),
        "realistic common.ingress helper should propagate ingress.tls[*].secretName, got {schema}"
    );
    assert_eq!(
        schema
            .pointer("/properties/ingress/properties/hosts/items/properties/host/type")
            .and_then(Value::as_str),
        Some("string"),
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

    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider(), Some(values_yaml));

    let requests = schema
        .pointer("/properties/resources/properties/requests")
        .expect("resources.requests present");
    assert!(
        requests
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.requests should stay an open quantity map, got {requests}"
    );
    let limits = schema
        .pointer("/properties/resources/properties/limits")
        .expect("resources.limits present");
    assert!(
        limits
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.limits should stay an open quantity map, got {limits}"
    );
}

#[test]
fn direct_fragment_resource_requirements_keep_open_requests_and_limits_on_chain() {
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

    let provider = production_chain_provider();
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir(src), &provider, Some(values_yaml));

    let requests = schema
        .pointer("/properties/resources/properties/requests")
        .expect("resources.requests present");
    assert!(
        requests
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.requests should stay an open quantity map on the chain path, got {requests}"
    );
    let limits = schema
        .pointer("/properties/resources/properties/limits")
        .expect("resources.limits present");
    assert!(
        limits
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.limits should stay an open quantity map on the chain path, got {limits}"
    );
}

#[test]
fn direct_fragment_resource_requirements_keep_open_requests_and_limits_on_chain_fused() {
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

    let provider = production_chain_provider();
    let schema =
        generate_values_schema_with_values_yaml(&parse_ir_fused(src), &provider, Some(values_yaml));

    let requests = schema
        .pointer("/properties/resources/properties/requests")
        .expect("resources.requests present");
    assert!(
        requests
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.requests should stay an open quantity map on the fused parser path, got {requests}"
    );
    let limits = schema
        .pointer("/properties/resources/properties/limits")
        .expect("resources.limits present");
    assert!(
        limits
            .pointer("/additionalProperties/oneOf")
            .and_then(Value::as_array)
            .is_some(),
        "resources.limits should stay an open quantity map on the fused parser path, got {limits}"
    );
}

#[test]
fn provider_schema_for_container_resources_path_keeps_open_quantity_maps() {
    let provider = production_chain_provider();
    let use_ = ValueUse {
        source_expr: "resources".to_string(),
        path: YamlPath(vec![
            "spec".to_string(),
            "template".to_string(),
            "spec".to_string(),
            "containers[*]".to_string(),
            "resources".to_string(),
        ]),
        kind: helm_schema_ir::ValueKind::Fragment,
        guards: Vec::new(),
        resource: Some(ResourceRef {
            api_version: "apps/v1".to_string(),
            kind: "Deployment".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
    };

    let schema = provider
        .schema_for_use(&use_)
        .expect("provider schema for container resources");

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

/// Step 2: negative-integer literal still recognised, type hint is integer.
#[test]
fn step2_default_negative_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default -3 .Values.replicas }}
    "};
    let hints = collect_hints(src);
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas
            .iter()
            .any(|v| v.get("type").and_then(Value::as_str) == Some("integer")),
        "expected integer hint for negative literal, got {schemas:?}"
    );
}

/// Step 2: rooted `$.Values.X` and `$root.Values.X` forms (used inside
/// ranges/withs where `.` is rebound) are recognised too — not just the
/// plain `.Values.X` form.
#[test]
fn step2_default_rooted_values_paths_recognised() {
    let src = indoc! {r#"
        {{- range .Values.servers }}
        name: {{ default "alertmanager" $.Values.alertmanager.nameOverride }}
        alias: {{ default "main" $root.Values.alertmanager.aliasOverride }}
        {{- end }}
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.contains_key("alertmanager.nameOverride"),
        "expected hint for $.Values.alertmanager.nameOverride, got {hints:?}"
    );
    assert!(
        hints.contains_key("alertmanager.aliasOverride"),
        "expected hint for $root.Values.alertmanager.aliasOverride, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a YAML comment
/// MUST NOT produce a type hint. (Acceptable known limitation if it does —
/// document with a SKIP marker — but flag the case explicitly.)
#[test]
fn step2_default_in_yaml_comment_no_hint() {
    let src = indoc! {r#"
        # example: {{ default "x" .Values.exampleName }}
        name: actual
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "YAML comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Helm template
/// comment (`{{/* ... */}}`) MUST NOT produce a type hint.
#[test]
fn step2_default_in_helm_comment_no_hint() {
    let src = indoc! {r#"
        {{/* default "x" .Values.exampleName */}}
        name: actual
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "Helm comments must not produce hints, got {hints:?}"
    );
}

/// Step 2 false-positive guard: a `default` pattern inside a Go string
/// literal embedded in a template MUST NOT produce a type hint.
#[test]
fn step2_default_in_string_literal_no_hint() {
    // A real chart might emit a doc string mentioning the syntax it
    // supports. The extractor must not be fooled by syntax that's text data.
    let src = indoc! {r#"
        docs: {{- "see: default 5 .Values.example" | quote }}
    "#};
    let hints = collect_hints(src);
    assert!(
        hints.is_empty(),
        "Go-string-literal text must not produce hints, got {hints:?}"
    );
}

/// Step 2 real-world pattern: the `default <literal> .Values.X` site lives
/// in a helper template (`_helpers.tpl`), not in a manifest body. The
/// temporal chart's `temporal.serviceAccountName` is the canonical case.
/// The CLI must scan helper sources too, not just manifest templates.
#[test]
fn step2_default_in_helper_template_is_extracted() {
    // Mirror the structure of the temporal chart helper: the default lives
    // inside a `define`-bound helper that gets `include`d from manifests.
    let helper_src = indoc! {r#"
        {{- define "test.serviceAccountName" -}}
        {{- if .Values.serviceAccount.create -}}
            {{ default "default-name" .Values.serviceAccount.name }}
        {{- end -}}
        {{- end -}}
    "#};
    let hints = collect_hints(helper_src);
    assert!(
        hints.contains_key("serviceAccount.name"),
        "expected hint for serviceAccount.name in helper, got {hints:?}"
    );
}
