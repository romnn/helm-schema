//! Schema demands follow the YAML continuation Helm selects.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::SymbolicIrContext;
use helm_schema_k8s::Chain;
use helm_schema_test_support::generate::bundled_k8s_provider;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

fn schema(source: &str) -> Value {
    let signals = SymbolicIrContext::new(&DefineIndex::new())
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    let provider = Chain::new(vec![Box::new(bundled_k8s_provider(
        "v1.29.0-standalone-strict",
    ))]);
    generate_values_schema(ValuesSchemaInput::new(&signals, &provider))
}

fn assert_namespace_schema(source: &str, cases: &[Value]) -> eyre::Result<()> {
    // A fixed data sibling closes the optional entries before the dynamic key.
    // It changes no input demand in either manifest.
    let key = "  {{ .Values.key | quote }}: fixed";
    let reference = source.replace(key, &["  closure: fixed", key].join("\n"));
    let actual = schema(source);
    let validator = jsonschema::validator_for(&actual)?;
    for entry in cases {
        for (namespace, accepted) in [
            (json!(true), false),
            (json!("3"), true),
            (json!(1.5), false),
            (json!([{}]), false),
            (json!("ops"), true),
        ] {
            let mut values = entry.clone();
            values
                .as_object_mut()
                .ok_or_eyre("namespace case must be a values object")?
                .insert("namespaceOverride".to_owned(), namespace);
            sim_assert_eq!(have: validator.is_valid(&values), want: accepted, "values={values}");
        }
    }
    sim_assert_eq!(have: actual, want: schema(&reference));
    Ok(())
}

#[test]
fn later_manifest_choices_preserve_the_earlier_namespace_demand() -> eyre::Result<()> {
    let source = indoc! {r"
        {{- $namespace := .Values.namespaceOverride | toJson -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: first
          namespace: {{ $namespace }}
        data:
          {{- if .Values.a }}
          a: fixed
          {{- end }}
          {{- if .Values.b }}
          b: fixed
          {{- end }}
          {{- if .Values.c }}
          c: fixed
          {{- end }}
          {{ .Values.key | quote }}: fixed
        ---
        {{- if true -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: second
        data:
          {{- if .Values.d }}
          d: fixed
          {{- end }}
          {{- if .Values.e }}
          e: fixed
          {{- end }}
          {{- if .Values.f }}
          f: fixed
          {{- end }}
          {{ .Values.key | quote }}: fixed
        {{- end }}
    "};
    assert_namespace_schema(source, &[json!({"key": "tail"})])
}

#[test]
fn widening_later_mapping_choices_preserves_established_namespace_placement() -> eyre::Result<()> {
    let source = indoc! {r"
        {{- $namespace := .Values.namespaceOverride | toJson -}}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          namespace: {{ $namespace }}
        data:
          {{- if .Values.a }}
          a: fixed
          {{- end }}
          {{- if .Values.b }}
          b: fixed
          {{- end }}
          {{- if .Values.c }}
          c: fixed
          {{- end }}
          {{- if .Values.d }}
          d: fixed
          {{- end }}
          {{- if .Values.e }}
          e: fixed
          {{- end }}
          {{- if .Values.f }}
          f: fixed
          {{- end }}
          {{ .Values.key | quote }}: fixed
    "};
    let cases = (0..64)
        .map(|mask| {
            let mut values = serde_json::Map::from_iter([("key".to_owned(), json!("tail"))]);
            for (bit, key) in ["a", "b", "c", "d", "e", "f"].into_iter().enumerate() {
                values.insert(key.to_owned(), json!(mask & (1 << bit) != 0));
            }
            Value::Object(values)
        })
        .collect::<Vec<_>>();
    assert_namespace_schema(source, &cases)
}

#[test]
fn a_selected_separator_guards_the_namespace_demand() -> eyre::Result<()> {
    let source = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        {{- if .Values.split }}
        data: {}
        ---
        {{- else }}
        data: {}
        {{- end }}
        metadata:
          namespace: {{ .Values.namespaceOverride }}
    "};
    let reference = indoc! {r"
        apiVersion: v1
        kind: ConfigMap

        {{- if .Values.split }}
        data: {}
        ---
        metadata:
          namespace: {{ .Values.namespaceOverride }}
        {{- else }}
        data: {}
        metadata:
          namespace: {{ .Values.namespaceOverride }}
        {{- end }}
    "};
    let actual = schema(source);
    let validator = jsonschema::validator_for(&actual)?;
    for (split, namespace, accepted) in [
        (false, json!(true), false),
        (false, json!("ops"), true),
        (true, json!(true), true),
        (true, json!("ops"), true),
    ] {
        let values = json!({"split": split, "namespaceOverride": namespace});
        sim_assert_eq!(have: validator.is_valid(&values), want: accepted, "values={values}");
    }
    sim_assert_eq!(have: actual, want: schema(reference));
    Ok(())
}

#[test]
fn shared_probe_port_follows_the_selected_key() -> eyre::Result<()> {
    let source = indoc! {r"
        {{- $port := .Values.port | toJson -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: test
            image: busybox
            livenessProbe:
            {{- if .Values.http }}
              httpGet:
                path: /healthz
            {{- else }}
              grpc:
            {{- end }}
                port: {{ $port }}
    "};
    let reference = indoc! {r"
        {{- $port := .Values.port | toJson -}}
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
          - name: test
            image: busybox
            livenessProbe:
            {{- if .Values.http }}
              httpGet:
                path: /healthz
                port: {{ $port }}
            {{- else }}
              grpc:
                port: {{ $port }}
            {{- end }}
    "};
    let actual = schema(source);
    let validator = jsonschema::validator_for(&actual)?;
    for (http, port, accepted) in [
        (true, json!("http"), true),
        (false, json!("http"), false),
        (true, json!(8080), true),
        (false, json!(8080), true),
        (true, json!(true), false),
        (false, json!(true), false),
    ] {
        let values = json!({"http": http, "port": port});
        sim_assert_eq!(have: validator.is_valid(&values), want: accepted, "values={values}");
    }
    sim_assert_eq!(have: actual, want: schema(reference));
    Ok(())
}
