use super::*;

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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let name = schema.pointer("/properties/name").expect("name present");
    assert!(permits_null(name));
    assert!(
        permits_type(name, "string"),
        "default fallback should keep the string branch, got {name}"
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let name = schema.pointer("/properties/name").expect("name present");
    assert!(permits_null(name));
    assert!(
        permits_type(name, "string"),
        "default fallback should keep the string branch, got {name}"
    );
}

#[test]
fn step2_default_after_intervening_required_call_no_hint() {
    let src = indoc! {r#"
        name: {{ .Values.name | required "name is required" | default "fallback" }}
    "#};
    let hints = type_hints_for(parse_ir(src));
    assert!(
        hints.is_empty(),
        "default after required must not type-hint the original values path, got {hints:?}"
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
    let hints = type_hints_for(parse_ir(src));
    assert!(hints.is_empty(), "expected no hints, got {hints:?}");
}

/// Step 2: integer literal → integer type hint (not string).
#[test]
fn step2_default_integer_literal() {
    let src = indoc! {r"
        replicas: {{ default 5 .Values.replicas }}
    "};
    let hints = type_hints_for(parse_ir(src));
    let schemas = hints.get("replicas").expect("replicas hint present");
    assert!(
        schemas.contains("integer"),
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let primary = schema
        .pointer("/properties/primary")
        .expect("primary property present");
    assert!(
        permits_null(primary),
        "primary should permit null after `with or` + explicit Fragment use, got {primary}"
    );
}

/// A `with`-guarded object fragment accepts explicit `null` even when the
/// declared default is non-null: helm null-deletion removes the key and the
/// falsy `with` skips the body (declared-null tolerance).
#[test]
fn step1_with_fragment_non_null_default_accepts_explicit_null() {
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let extra = schema
        .pointer("/properties/extraAnnotations")
        .expect("extraAnnotations present");
    assert!(
        permits_null(extra),
        "explicit null renders (null-deletion plus falsy `with`), got {extra}"
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
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    let priority = schema
        .pointer("/properties/priorityClassName")
        .expect("priorityClassName present");
    assert!(permits_null(priority));
    assert!(
        permits_type(priority, "string"),
        "priorityClassName should also accept the provider string type, got {priority}"
    );
}

/// A scalar rendered only from a truthy self-guard inside a larger condition
/// (optional Service nodePorts gated by `not (empty ...)`) lowers its
/// provider typing under the foreign condition: the base stays open (null and
/// everything else stay valid when the guard cannot fire), and the guarded
/// branch keeps the null alternative the self-guard implies.
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
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    for (instance, want, label) in [
        (
            serde_json::json!({
                "service": {
                    "type": "ClusterIP",
                    "ports": { "smtp": { "nodePort": { "ignored": true } } }
                }
            }),
            true,
            "inactive node port",
        ),
        (
            serde_json::json!({
                "service": {
                    "type": "NodePort",
                    "ports": { "smtp": { "nodePort": null } }
                }
            }),
            true,
            "falsy node port",
        ),
        (
            serde_json::json!({
                "service": {
                    "type": "NodePort",
                    "ports": { "smtp": { "nodePort": 30025 } }
                }
            }),
            true,
            "integer node port",
        ),
        (
            serde_json::json!({
                "service": {
                    "type": "NodePort",
                    "ports": { "smtp": { "nodePort": "30025" } }
                }
            }),
            true,
            "numeric-string node port",
        ),
        (
            serde_json::json!({
                "service": {
                    "type": "NodePort",
                    "ports": { "smtp": { "nodePort": { "bad": true } } }
                }
            }),
            false,
            "truthy object node port",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}; ir={ir:?}"
        );
    }
}

/// Explicit `null` defaults stay valid for range-only collection values.
/// Helm treats a nil range source as empty, so a chart that ships `snapshots:`
/// and later ranges over it accepts both null and concrete arrays.
#[test]
fn nullable_array_preserved_for_range_only_collection_use() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        data:
          initialize.sh: |
            exec ./entrypoint.sh {{ range .Values.snapshots }} --snapshot {{ . }} {{ end }}
    "};
    let values_yaml = indoc! {"
        snapshots:
    "};
    let ir = parse_ir(src);
    let signals = schema_signals_for(ir.clone());
    let nullable_paths = signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.is_nullable)
        .map(|(path, _)| path.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        nullable_paths.contains("snapshots"),
        "range-only collection should be classified nullable; nullable_paths={nullable_paths:?}; ir={ir:#?}"
    );
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let snapshots = schema
        .pointer("/properties/snapshots")
        .expect("snapshots present");
    assert!(
        permits_null(snapshots),
        "snapshots should allow null, got {snapshots}"
    );
    assert!(
        permits_type(snapshots, "array"),
        "snapshots should also allow concrete arrays, got {snapshots}"
    );
}

/// Truthy-guarded optional scalars should accept null even when values.yaml
/// chooses an empty-string default instead of an explicit YAML null.
#[test]
fn truthy_guarded_scalar_allows_null_without_explicit_null_default() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Service
        metadata:
          {{- if .Values.fullnameOverride }}
          name: {{ .Values.fullnameOverride }}
          {{- end }}
    "};
    let values_yaml = indoc! {"
        fullnameOverride: \"\"
    "};
    let ir = parse_ir(src);
    let schema = schema_for_values_yaml(&ir, Some(values_yaml));

    let fullname = schema
        .pointer("/properties/fullnameOverride")
        .expect("fullnameOverride present");
    assert!(
        permits_null(fullname),
        "truthy-guarded fullnameOverride should allow null, got {fullname}"
    );
}
