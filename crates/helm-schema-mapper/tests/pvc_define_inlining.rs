#![allow(warnings)]
use color_eyre::eyre::{self, WrapErr};
use helm_schema_mapper::analyze::{self, Role, ValueUse, group_uses};
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

fn chart_with_pvc_includes(root: &VfsPath) -> eyre::Result<()> {
    // Minimal chart metadata
    write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: test-pvc-chart
            version: 0.1.0
        "#},
    )?;

    // values are optional for analysis; we keep it empty on purpose
    write(&root.join("values.yaml")?, "")?;

    // common.name + common.fullname (kept verbatim from your snippet)
    write(
        &root.join("templates/_name.tpl")?,
        indoc! {r#"
            {{/*
            Expand the name of the chart.
            */}}
            {{- define "common.name" -}}
            {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
            {{- end }}

            {{/*
            Create a default fully qualified app name.
            */}}
            {{- define "common.fullname" -}}
            {{- if .Values.fullnameOverride }}
            {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
            {{- else }}
            {{- $name := default .Chart.Name .Values.nameOverride }}
            {{- if contains $name .Release.Name }}
            {{- .Release.Name | trunc 63 | trimSuffix "-" }}
            {{- else }}
            {{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
            {{- end }}
            {{- end }}
            {{- end }}
        "#},
    )?;

    // selectorLabels + labels (kept verbatim from your snippet)
    write(
        &root.join("templates/_labels.tpl")?,
        indoc! {r#"
            {{/*
            Selector labels
            */}}
            {{- define "common.selectorLabels" -}}
            app.kubernetes.io/name: {{ include "common.name" . }}
            app.kubernetes.io/instance: {{ .Release.Name }}
            {{- end }}

            {{/*
            Common labels
            */}}
            {{- define "common.labels" -}}
            helm.sh/chart: {{ include "common.chart" . }}
            {{ include "common.selectorLabels" . }}
            {{- if ($.Chart).AppVersion }}
            app.kubernetes.io/version: {{ $.Chart.AppVersion | quote }}
            {{- end }}
            app.kubernetes.io/managed-by: {{ $.Release.Service }}
            {{- end }}
        "#},
    )?;

    // harmless stub for common.chart referenced by common.labels
    write(
        &root.join("templates/_chart.tpl")?,
        r#"{{- define "common.chart" -}}{{ .Chart.Name }}-{{ .Chart.Version }}{{- end -}}"#,
    )?;

    // the actual PVC helper define (kept verbatim, just normalized whitespace)
    write(
        &root.join("templates/_pvc.tpl")?,
        indoc! {r#"
            {{/* Persistent volume claim */}}
            {{- define "common.pvc" -}}
            ---
            apiVersion: v1
            kind: PersistentVolumeClaim
            metadata:
              name: {{ .config.name | default (include "common.fullname" .ctx) }}
              labels:
                {{- include "common.labels" .ctx | nindent 4 }}
                {{- with .config.labels }}
                {{- toYaml . | nindent 4 }}
                {{- end }}
              {{- with .config.annotations }}
              annotations:
                {{- toYaml . | nindent 4 }}
              {{- end }}
            spec:
              {{- with .config.accessModes }}
              accessModes:
                {{- range . }}
                - {{ . }}
                {{- end }}
              {{- end }}
              resources:
                requests:
                  storage: {{ .config.size }}
              {{- with .config.storageClassName }}
              storageClassName: {{ . }}
              {{- end }}
            {{- end }}
        "#},
    )?;

    // one file that includes the helper twice:
    // 1) once directly (empty config), and
    // 2) once for each item in .Values.persistentVolumeClaims
    write(
        &root.join("templates/pvc.yaml")?,
        indoc! {r#"
            {{- include "common.pvc" (dict "config" (dict) "ctx" .) -}}
            {{- range .Values.persistentVolumeClaims }}
            {{- include "common.pvc" (dict "ctx" $ "config" .) }}
            {{- end }}
        "#},
    )?;

    Ok(())
}

#[test]
fn maps_values_through_pvc_define_and_call_sites() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());

    chart_with_pvc_includes(&root)?;

    let template_path = root.join("templates/pvc.yaml")?;
    let uses: Vec<ValueUse> = analyze::analyze_template_file(&template_path)
        .wrap_err_with(|| eyre::eyre!("analyze {} failed", template_path.as_str()))?;

    // dbg!(&uses);

    let groups = group_uses(&uses);
    dbg!(&groups);

    // convenience
    let filter = |needle: &str| -> Vec<ValueUse> {
        uses.iter()
            .filter(|v| v.value_path == needle)
            .cloned()
            .collect()
    };

    // persistentVolumeClaims.name -> metadata.name (ScalarValue)
    let pvc_name = filter("persistentVolumeClaims.name");
    assert_that!(
        pvc_name,
        // Expect exactly two occurrences coming from the two items in the range;
        // if you don't materialize doc indices, both will have the same YAML path.
        unordered_elements_are![
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(eq("metadata.name"))),
                ..
            }),
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(eq("metadata.name"))),
                ..
            }),
        ]
    );

    // persistentVolumeClaims.size -> spec.resources.requests.storage (ScalarValue)
    let pvc_size = filter("persistentVolumeClaims.size");
    assert_that!(
        pvc_size,
        unordered_elements_are![
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(eq("spec.resources.requests.storage"))),
                ..
            }),
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(eq("spec.resources.requests.storage"))),
                ..
            }),
        ]
    );

    // persistentVolumeClaims.storageClassName -> spec.storageClassName (Guard + ScalarValue)
    // (Guard because the key is conditional inside a `with`.)
    let pvc_sc = filter("persistentVolumeClaims.storageClassName");
    assert_that!(
        pvc_sc,
        unordered_elements_are![
            // guard occurrence (no yaml_path)
            matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                yaml_path: none(),
                ..
            }),
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(eq("spec.storageClassName"))),
                ..
            }),
        ]
    );

    // persistentVolumeClaims.accessModes -> spec.accessModes[*]
    // We expect:
    //   - a Guard for the outer `with`
    //   - a ScalarValue for at least the first item produced by the inner `range`
    let pvc_modes = filter("persistentVolumeClaims.accessModes");
    assert_that!(
        pvc_modes,
        unordered_elements_are![
            // one guard (no yaml_path)
            matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                yaml_path: none(),
                ..
            }),
            // At least one list item scalar (index may be [0])
            matches_pattern!(ValueUse {
                role: eq(&Role::ScalarValue),
                yaml_path: some(displays_as(any![
                    // if you index list items:
                    eq("spec.accessModes[0]"),
                    // or if you normalize star indices:
                    eq("spec.accessModes[*]")
                ])),
                ..
            })
        ]
    );

    // persistentVolumeClaims.labels -> metadata.labels (Fragment)
    let pvc_labels = filter("persistentVolumeClaims.labels");
    assert_that!(
        pvc_labels,
        unordered_elements_are![
            matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                yaml_path: none(),
                ..
            }),
            matches_pattern!(ValueUse {
                role: eq(&Role::Fragment),
                yaml_path: some(displays_as(eq("metadata.labels"))),
                ..
            })
        ]
    );

    // persistentVolumeClaims.annotations -> metadata.annotations (Fragment)
    let pvc_ann = filter("persistentVolumeClaims.annotations");
    assert_that!(
        &pvc_ann,
        unordered_elements_are![
            matches_pattern!(ValueUse {
                role: eq(&Role::Guard),
                yaml_path: none(),
                ..
            }),
            matches_pattern!(ValueUse {
                role: eq(&Role::Fragment),
                yaml_path: some(displays_as(eq("metadata.annotations"))),
                ..
            })
        ]
    );

    // nameOverride / fullnameOverride should flow through common.fullname to metadata.name
    // We don't assert both branches exhaustively; one strict assertion is enough to prove inlining.
    let fullname_override = filter("fullnameOverride");
    assert_that!(
        fullname_override,
        unordered_elements_are![
            matches_pattern!(ValueUse {
                role: any!(eq(&Role::Guard), eq(&Role::ScalarValue)),
                ..
            }),
            matches_pattern!(ValueUse {
                yaml_path: some(displays_as(eq("metadata.name"))),
                ..
            })
        ],
    );

    Ok(())
}
