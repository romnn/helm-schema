use clap::Parser;
use color_eyre::eyre::{WrapErr, eyre};
use helm_schema_cli::{Cli, GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use indoc::indoc;
use vfs::VfsPath;

fn into_eyre(e: helm_schema_cli::CliError) -> color_eyre::eyre::Report {
    e.into()
}

#[test]
fn cli_parses_defaults() -> color_eyre::eyre::Result<()> {
    let cli =
        Cli::try_parse_from(["helm-schema", "/tmp/chart"]).map_err(|e| eyre!(e.to_string()))?;

    assert_eq!(cli.k8s.k8s_version, vec!["v1.35.0".to_string()]);
    assert!(cli.output.output.is_none());
    assert!(!cli.k8s.offline);
    assert!(!cli.k8s.no_k8s_schemas);
    assert!(!cli.chart.exclude_tests);
    assert!(!cli.chart.no_subchart_values);
    assert!(cli.override_schema.is_empty());
    assert!(!cli.output.compact);

    Ok(())
}

#[test]
fn override_schema_flag_is_repeatable() -> color_eyre::eyre::Result<()> {
    let cli = Cli::try_parse_from([
        "helm-schema",
        "/tmp/chart",
        "--override-schema",
        "/tmp/shared.json",
        "--override-schema",
        "/tmp/per-chart.json",
    ])
    .map_err(|e| eyre!(e.to_string()))?;

    let paths: Vec<_> = cli
        .override_schema
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert_eq!(paths, vec!["/tmp/shared.json", "/tmp/per-chart.json"]);

    Ok(())
}

#[test]
fn generates_schema_for_fixture_chart_without_k8s_provider() -> color_eyre::eyre::Result<()> {
    let chart_dir = test_util::workspace_testdata().join("fixture-charts/full-fixture");
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/full_fixture.disable_k8s.schema.json"
    ))
    .wrap_err("parse expected schema fixture")?;

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn subchart_values_are_scoped_and_global_is_merged() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "kid:\n  persistence:\n    enabled: true\n",
    )?;

    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "foo: 1\nglobal:\n  bar: true\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/configmap.yaml")?,
        "apiVersion: v1\nkind: ConfigMap\nmetadata:\n  name: test\ndata:\n  foo: {{ .Values.foo | quote }}\n  bar: {{ .Values.global.bar | quote }}\n",
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    // The subchart's slot mirrors `global` because at Helm render time
    // the root's effective `global` is propagated into every subchart's
    // `.Values`. Same shape on both sides of the tree.
    let global_schema = serde_json::json!({
      "additionalProperties": false,
      "properties": {
        "bar": {
          "type": "boolean"
        }
      },
      "type": "object"
    });

    let expected = serde_json::json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "additionalProperties": false,
      "properties": {
        "global": global_schema,
        "kid": {
          "additionalProperties": false,
          "properties": {
            "foo": {
              "type": "integer"
            },
            "global": global_schema,
            "persistence": {
              "additionalProperties": false,
              "properties": {
                "enabled": {
                  "type": "boolean"
                }
              },
              "type": "object"
            },
          },
          "type": "object"
        }
      },
      "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn subchart_helper_descendant_access_does_not_widen_parent_objects() -> color_eyre::eyre::Result<()>
{
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;

    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "global:\n  defaultStorageClass: \"\"\npersistence:\n  enabled: true\n  storageClass: \"\"\n  size: 1Gi\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/_helpers.tpl")?,
        r#"{{- define "common.storage.class" -}}
{{- $storageClass := (.global).storageClass | default .persistence.storageClass | default (.global).defaultStorageClass | default "" -}}
{{- if $storageClass -}}
storageClassName: {{ $storageClass }}
{{- end -}}
{{- end -}}
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/pvc.yaml")?,
        r#"apiVersion: v1
kind: PersistentVolumeClaim
spec:
  resources:
    requests:
      storage: {{ .Values.persistence.size | quote }}
  {{- include "common.storage.class" (dict "persistence" .Values.persistence "global" .Values.global) | nindent 2 }}
"#,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let global = actual
        .pointer("/properties/global")
        .ok_or_else(|| eyre!("missing global schema"))?;
    assert!(
        global.get("required").is_none(),
        "global should not inherit required fields from helper output, got {global}"
    );
    assert!(
        global.pointer("/properties/kind").is_none(),
        "global should not gain typed-local-object-reference fields, got {global}"
    );
    assert!(
        global.pointer("/properties/selector").is_none(),
        "global should not gain pvc-spec fields from helper placement, got {global}"
    );

    let persistence = actual
        .pointer("/properties/kid/properties/persistence")
        .ok_or_else(|| eyre!("missing kid.persistence schema"))?;
    assert!(
        persistence.get("required").is_none(),
        "kid.persistence should not inherit required fields from helper output, got {persistence}"
    );
    assert!(
        persistence.pointer("/properties/kind").is_none(),
        "kid.persistence should not gain typed-local-object-reference fields, got {persistence}"
    );
    assert!(
        persistence.pointer("/properties/selector").is_none(),
        "kid.persistence should not gain pvc-spec sibling fields from helper placement, got {persistence}"
    );

    Ok(())
}

#[test]
fn library_subchart_helper_descendant_access_does_not_widen_parent_objects()
-> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;

    test_util::write(
        &chart_dir.join("charts/child/Chart.yaml")?,
        "apiVersion: v2\nname: child\nversion: 0.1.0\ndependencies:\n  - name: common\n    version: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/values.yaml")?,
        "global:\n  defaultStorageClass: \"\"\npersistence:\n  enabled: true\n  storageClass: \"\"\n  size: 1Gi\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/templates/pvc.yaml")?,
        r#"apiVersion: v1
kind: PersistentVolumeClaim
spec:
  resources:
    requests:
      storage: {{ .Values.persistence.size | quote }}
  {{- include "common.storage.class" (dict "persistence" .Values.persistence "global" .Values.global) | nindent 2 }}
"#,
    )?;
    test_util::write(
        &chart_dir.join("charts/child/charts/common/Chart.yaml")?,
        "apiVersion: v2\nname: common\nversion: 0.1.0\ntype: library\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/child/charts/common/templates/_storage.tpl")?,
        r#"{{- define "common.storage.class" -}}
{{- $storageClass := (.global).storageClass | default .persistence.storageClass | default (.global).defaultStorageClass | default "" -}}
{{- if $storageClass -}}
storageClassName: {{ $storageClass }}
{{- end -}}
{{- end -}}
"#,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let global = actual
        .pointer("/properties/global")
        .ok_or_else(|| eyre!("missing global schema"))?;
    assert!(
        global.get("required").is_none(),
        "global should not inherit required fields from library helper output, got {global}"
    );
    assert!(
        global.pointer("/properties/kind").is_none(),
        "global should not gain typed-local-object-reference fields, got {global}"
    );
    assert!(
        global.pointer("/properties/selector").is_none(),
        "global should not gain pvc-spec fields from library helper placement, got {global}"
    );

    let persistence = actual
        .pointer("/properties/kid/properties/persistence")
        .ok_or_else(|| eyre!("missing kid.persistence schema"))?;
    assert!(
        persistence.get("required").is_none(),
        "kid.persistence should not inherit required fields from library helper output, got {persistence}"
    );
    assert!(
        persistence.pointer("/properties/kind").is_none(),
        "kid.persistence should not gain typed-local-object-reference fields, got {persistence}"
    );
    assert!(
        persistence.pointer("/properties/selector").is_none(),
        "kid.persistence should not gain pvc-spec sibling fields from library helper placement, got {persistence}"
    );

    Ok(())
}

#[test]
fn deployment_annotations_fragment_stays_annotations_map() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "podAnnotations: {}\n")?;
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        r#"apiVersion: apps/v1
kind: Deployment
spec:
  selector:
    matchLabels:
      app: demo
  template:
    metadata:
      annotations:
        checksum/secret: {{ "abc" | quote }}
    {{- if .Values.podAnnotations }}
{{ toYaml .Values.podAnnotations | indent 8 }}
    {{- end }}
    spec:
      containers:
        - name: demo
          image: nginx
"#,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let pod_annotations = actual
        .pointer("/properties/podAnnotations")
        .ok_or_else(|| eyre!("missing podAnnotations schema"))?;
    assert!(
        pod_annotations.get("required").is_none(),
        "podAnnotations should not inherit deployment required fields, got {pod_annotations}"
    );
    assert_eq!(
        pod_annotations
            .pointer("/additionalProperties/type")
            .and_then(serde_json::Value::as_str),
        Some("string"),
        "podAnnotations should be an open annotations string map, got {pod_annotations}"
    );

    Ok(())
}

#[test]
fn defaulted_global_image_pull_secrets_do_not_widen_global_parent() -> color_eyre::eyre::Result<()>
{
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "global: {}\nimagePullSecrets: []\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/pod.yaml")?,
        r#"apiVersion: v1
kind: Pod
spec:
  {{- with (.Values.imagePullSecrets | default .Values.global.imagePullSecrets) }}
  imagePullSecrets:
    {{- toYaml . | nindent 4 }}
  {{- end }}
  containers:
    - name: demo
      image: nginx
"#,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let global = actual
        .pointer("/properties/global")
        .ok_or_else(|| eyre!("missing global schema"))?;
    assert!(
        global.get("required").is_none(),
        "global should not inherit local-object-reference requirements, got {global}"
    );
    assert!(
        global.pointer("/properties/name").is_none(),
        "global should not inherit local-object-reference fields, got {global}"
    );

    Ok(())
}

#[test]
fn parens_around_values_prefix_propagate_full_path_into_schema() -> color_eyre::eyre::Result<()> {
    // Regression: charts use `(.Values.image).tag` so a nil
    // `.Values.image` returns nil instead of erroring on the `.tag`
    // access (Helm idiom, see chart_template_guide). Pre-fix, the IR
    // saw the parens-wrapped prefix as opaque and never recognised
    // `.tag` as a Values path — `image.tag` was missing from the
    // generated schema, which caused luup3's `helm lint --strict` to
    // reject every chart whose values.yaml ships `image.tag`.
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "image:\n  repository: example/app\n  tag: latest\n",
    )?;
    // Two parens forms: the common Helm idiom plus a double-paren
    // variant. Both should produce identical Field paths.
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        indoc! {r#"
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: test
            spec:
              template:
                spec:
                  containers:
                    - name: app
                      image: "{{ .Values.image.repository }}:{{ (.Values.image).tag }}"
                      env:
                        - name: VARIANT
                          value: {{ ((.Values.image)).tag | quote }}
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let image = actual
        .pointer("/properties/image")
        .ok_or_else(|| eyre!("missing image schema"))?;
    assert!(
        image.pointer("/properties/tag").is_some(),
        "image.tag should be inferred even when the template uses `(.Values.image).tag` parens form; got {image}",
    );
    assert!(
        image.pointer("/properties/repository").is_some(),
        "image.repository should still be inferred alongside the parens-form access; got {image}",
    );

    Ok(())
}

#[test]
fn parens_form_does_not_lose_default_driven_nullability_on_inner_field()
-> color_eyre::eyre::Result<()> {
    // Charts pair the parens idiom with `| default` so a nil
    // `.Values.image.tag` falls back to `$appVersion`. The default
    // pattern is what makes the inner path nullable in the schema —
    // confirm that adding the parens to the *prefix* doesn't get in
    // the way of `collect_nullable_value_paths` finding the inner
    // `image.tag` path through the Default guard.
    //
    // Pre-fix the bug was structural: the IR didn't see
    // `image.tag` as a Values path at all, so there was nothing for
    // the nullability pass to mark. With the parens-collapse fix the
    // path is visible, and the Default guard (also pointed at
    // `image.tag`) should now flow through cleanly.
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: root\nversion: 0.1.0\n",
    )?;
    // values.yaml ships `tag` as an empty string so helm-schema has a
    // type signal to anchor the schema. With `tag: null` instead, the
    // YAML null gives no type information and the schema falls back to
    // `{}` (allow-anything), which is functionally null-tolerant but
    // not what most charts want to express.
    test_util::write(
        &chart_dir.join("values.yaml")?,
        "image:\n  repository: example/app\n  tag: \"\"\n",
    )?;
    test_util::write(
        &chart_dir.join("templates/deployment.yaml")?,
        indoc! {r#"
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: test
            spec:
              template:
                spec:
                  containers:
                    - name: app
                      image: "{{ .Values.image.repository }}:{{ (.Values.image).tag | default .Chart.AppVersion }}"
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let tag = actual
        .pointer("/properties/image/properties/tag")
        .ok_or_else(|| eyre!("image.tag missing from generated schema: {actual}"))?;

    // `image.tag` should accept null because (a) the values.yaml ships
    // it as null and (b) the template guards it with `| default`. The
    // exact shape can be `{type: ["null","string"]}` or
    // `{anyOf: [{type:"null"}, {type:"string"}]}` depending on
    // upstream-K8s merging; both encode "null is allowed".
    let accepts_null = match tag.get("type") {
        Some(serde_json::Value::Array(types)) => types.iter().any(|t| t == "null"),
        Some(serde_json::Value::String(t)) => t == "null",
        _ => false,
    } || tag
        .get("anyOf")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|variants| {
            variants
                .iter()
                .any(|v| matches!(v.get("type"), Some(serde_json::Value::String(t)) if t == "null"))
        });

    assert!(
        accepts_null,
        "image.tag should be inferred as nullable when guarded by `| default` even through the parens-form prefix; got {tag}",
    );

    Ok(())
}

/// Mirrors the nats `defaultValues` shape end-to-end: a `_helpers.tpl`
/// defines a helper that, when included, sets a default on a values
/// path via the `with .Values` + `set X "K" (X.K | default V)` pattern;
/// the consumer template `include`s that helper at top-of-file before
/// reading the path. The values.yaml ships the path as `null`.
///
/// Asserts the *whole* generated schema, not a single subschema —
/// nullability needs to land on the right field without leaking to
/// neighbours. The schema must widen `serviceAccount.name` to
/// `string | null` because the helper's `set ... | default` mutation
/// runs before any read, while every other path on `serviceAccount`
/// stays narrowly typed.
#[test]
fn helper_set_default_mutation_widens_target_path_to_nullable() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: synth-nats\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {"
            serviceAccount:
              name:
              labels: {}
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc! {r#"
            {{- define "synth.fullname" -}}
            {{- .Release.Name | default "synth" -}}
            {{- end }}

            {{- define "synth.defaultValues" }}
            {{- $name := include "synth.fullname" . }}
            {{- with .Values }}
            {{- $_ := set .serviceAccount "name" (.serviceAccount.name | default $name) }}
            {{- end }}
            {{- end }}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/sa.yaml")?,
        indoc! {r#"
            {{- include "synth.defaultValues" . }}
            apiVersion: v1
            kind: ServiceAccount
            metadata:
              name: {{ .Values.serviceAccount.name | quote }}
              labels:
                {{- toYaml .Values.serviceAccount.labels | nindent 4 }}
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "serviceAccount": {
                "additionalProperties": false,
                "properties": {
                    "labels": {
                        "additionalProperties": { "type": "string" },
                        "type": "object"
                    },
                    "name": {
                        "anyOf": [
                            { "type": "null" },
                            { "type": "string" }
                        ]
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

/// Negative guard for the structural `set ... (X.K | default V)` matcher:
/// a helper that mutates `serviceAccount.name` but only defaults some
/// *other* path inside the RHS must not make `serviceAccount.name`
/// nullable. The target path is nullable only when the static analysis
/// can prove the `default` is applied to that exact target field.
#[test]
fn helper_set_with_unrelated_default_does_not_widen_target_path() -> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: synth-negative\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {"
            serviceAccount:
              name:
            other:
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc! {r#"
            {{- define "synth.defaultValues" }}
            {{- with .Values }}
            {{- $_ := set .serviceAccount "name" (printf "%s" (.other | default "fallback")) }}
            {{- end }}
            {{- end }}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/sa.yaml")?,
        indoc! {r#"
            {{- include "synth.defaultValues" . }}
            apiVersion: v1
            kind: ServiceAccount
            metadata:
              name: {{ .Values.serviceAccount.name | quote }}
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: true,
            disable_k8s_schemas: false,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "other": {},
            "serviceAccount": {
                "additionalProperties": false,
                "properties": {
                    "name": {
                        "description": "Name must be unique within a namespace. Is required when creating resources, although some resources may allow a client to request the generation of an appropriate name automatically. Name is primarily intended for creation idempotence and configuration definition. Cannot be updated. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/names#names",
                        "type": "string"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}

/// Focused guardrail for a nested helper consumption shape that shows up in
/// real charts: `common.names.fullname` is not rendered directly, but wrapped
/// in a larger scalar expression (`printf "%s-sfx" (...)`). The helper itself
/// carries the default-driven nullability for both `fullnameOverride` and
/// `nameOverride`; the surrounding `printf` must not erase that signal.
#[test]
fn nested_printf_around_common_fullname_keeps_name_overrides_nullable()
-> color_eyre::eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());

    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        "apiVersion: v2\nname: hs-nested\nversion: 0.1.0\n",
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {"
            nameOverride:
            fullnameOverride:
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc! {r#"
            {{- define "common.names.fullname" -}}
            {{- if .Values.fullnameOverride -}}
            {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- $name := default .Chart.Name .Values.nameOverride -}}
            {{- if contains $name .Release.Name -}}
            {{- .Release.Name | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" -}}
            {{- end -}}
            {{- end -}}
            {{- end -}}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/cm.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: {{ printf "%s-sfx" (include "common.names.fullname" .) }}
        "#},
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let actual = generate_values_schema_for_chart(&opts)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "fullnameOverride": {
                "anyOf": [
                    { "type": "null" },
                    { "type": "string" }
                ]
            },
            "nameOverride": {
                "anyOf": [
                    { "type": "null" },
                    { "type": "string" }
                ]
            }
        },
        "type": "object"
    });

    similar_asserts::assert_eq!(actual, expected);
    Ok(())
}
