use clap::Parser;
use color_eyre::eyre::{WrapErr, eyre};
use helm_schema_cli::{Cli, GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
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
    test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;

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

    let expected = serde_json::json!({
      "$schema": "http://json-schema.org/draft-07/schema#",
      "additionalProperties": false,
      "properties": {
        "global": {
          "additionalProperties": false,
          "properties": {
            "bar": {
              "type": "boolean"
            }
          },
          "type": "object"
        },
        "kid": {
          "additionalProperties": false,
          "properties": {
            "foo": {
              "type": "integer"
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
