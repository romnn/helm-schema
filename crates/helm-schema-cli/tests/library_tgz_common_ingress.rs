use std::io;

use color_eyre::eyre::{Report, WrapErr};
use flate2::Compression;
use flate2::write::GzEncoder;
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use vfs::VfsPath;

const ROOT_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
dependencies:
  - name: common
    version: 0.1.0
";

const ROOT_VALUES_YAML: &str = "\
ingress:
  enabled: true
service:
  port: 9000
";

const ROOT_TEMPLATE: &str = "\
{{- with .Values.ingress -}}
{{- if .enabled -}}
{{ include \"common.ingress\" (dict \"ctx\" $ \"config\" .) }}
{{- end -}}
{{- end -}}
";

const COMMON_CHART_YAML: &str = "\
apiVersion: v2
name: common
version: 0.1.0
type: library
";

const COMMON_HELPERS: &str = "\
{{- define \"common.fullname\" -}}app{{- end -}}
{{- define \"common.labels\" -}}
app.kubernetes.io/name: app
{{- end -}}
{{- define \"common.ingress\" }}
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: {{ include \"common.fullname\" .ctx }}
  labels:
    {{- include \"common.labels\" .ctx | nindent 4 }}
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
                name: {{ .serviceName | default (include \"common.fullname\" $.ctx) }}
                {{ if .servicePort -}}
                port:
                  {{- toYaml .servicePort | nindent 18 }}
                {{ else -}}
                port:
                  number: {{ $.ctx.Values.service.port }}
                {{- end }}
          {{- end }}
    {{- end }}
{{- end }}
";

fn into_eyre(err: helm_schema_cli::CliError) -> Report {
    err.into()
}

fn append_dir_entry<W: io::Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
) -> color_eyre::eyre::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).wrap_err("set dir path")?;
    header.set_mode(0o755);
    header.set_size(0);
    header.set_entry_type(tar::EntryType::Directory);
    header.set_cksum();
    builder
        .append(&header, io::empty())
        .wrap_err("append dir entry")?;
    Ok(())
}

fn append_file_entry<W: io::Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    contents: &[u8],
) -> color_eyre::eyre::Result<()> {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).wrap_err("set file path")?;
    header.set_mode(0o644);
    header.set_size(u64::try_from(contents.len()).wrap_err("file size fits in u64")?);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_cksum();
    builder
        .append(&header, contents)
        .wrap_err("append file entry")?;
    Ok(())
}

fn build_common_tarball() -> color_eyre::eyre::Result<Vec<u8>> {
    let mut buf = Vec::new();
    {
        let gz = GzEncoder::new(&mut buf, Compression::default());
        let mut builder = tar::Builder::new(gz);

        append_dir_entry(&mut builder, "common/")?;
        append_dir_entry(&mut builder, "common/templates/")?;
        append_file_entry(
            &mut builder,
            "common/Chart.yaml",
            COMMON_CHART_YAML.as_bytes(),
        )?;
        append_file_entry(
            &mut builder,
            "common/templates/_ingress.yaml",
            COMMON_HELPERS.as_bytes(),
        )?;

        builder.finish().wrap_err("finalize tarball")?;
    }
    Ok(buf)
}

#[test]
fn packaged_library_common_ingress_helper_propagates_schema() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, ROOT_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, ROOT_VALUES_YAML)?;
    test_util::write(&chart_dir.join("templates/ingress.yaml")?, ROOT_TEMPLATE)?;
    test_util::write(
        &chart_dir.join("charts/common-0.1.0.tgz")?,
        build_common_tarball()?,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
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

    let schema = AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(into_eyre)
        .wrap_err("generate schema")?;

    let has_ingress_property = |property: &str| {
        schema
            .pointer(&format!("/properties/ingress/properties/{property}"))
            .is_some()
            || schema
                .get("allOf")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|entries| {
                    entries.iter().any(|entry| {
                        entry
                            .pointer(&format!("/then/properties/ingress/properties/{property}"))
                            .is_some()
                    })
                })
    };

    assert!(
        has_ingress_property("className"),
        "expected packaged library helper to surface ingress.className, got {schema}",
    );
    assert!(
        has_ingress_property("annotations"),
        "expected packaged library helper to surface ingress.annotations, got {schema}",
    );
    assert!(
        has_ingress_property("hosts"),
        "expected packaged library helper to surface ingress.hosts, got {schema}",
    );
    assert!(
        has_ingress_property("tls"),
        "expected packaged library helper to surface ingress.tls, got {schema}",
    );
    assert!(
        schema
            .pointer("/properties/service/properties/port")
            .is_some(),
        "expected packaged library helper to preserve $.ctx.Values.service.port, got {schema}",
    );

    Ok(())
}
