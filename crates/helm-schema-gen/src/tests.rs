use std::collections::BTreeMap;

use indoc::indoc;
use serde_json::Value;

use crate::{
    DefaultValuesSchemaGenerator, ValuesSchemaGenerator, generate_values_schema_full,
    generate_values_schema_with_values_yaml,
};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator, ValueUse, extract_default_type_hints};
use helm_schema_k8s::KubernetesJsonSchemaProvider;

fn provider() -> KubernetesJsonSchemaProvider {
    KubernetesJsonSchemaProvider::new("v1.35.0").with_allow_download(true)
}

fn parse_ir(src: &str) -> Vec<ValueUse> {
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
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
