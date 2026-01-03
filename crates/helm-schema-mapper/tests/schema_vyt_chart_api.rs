use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use indoc::indoc;
use test_util::prelude::*;
use vfs::VfsPath;

#[test]
fn generates_values_schema_for_loaded_chart_vyt() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());
    let _ = write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: signoz
            version: 0.1.0
        "#},
    )?;

    let helpers_src = indoc! {r#"
        {{/* vim: set filetype=mustache: */}}
        {{- define "signoz.name" -}}
        {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
        {{- define "signoz.fullname" -}}
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
        {{- define "signoz.chart" -}}
        {{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
        {{- end -}}
        {{- define "signoz.labels" -}}
        helm.sh/chart: {{ include "signoz.chart" . }}
        {{ include "signoz.selectorLabels" . }}
        {{- if .Chart.AppVersion }}
        app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
        {{- end }}
        app.kubernetes.io/managed-by: {{ .Release.Service }}
        {{- end -}}
        {{- define "signoz.selectorLabels" -}}
        app.kubernetes.io/name: {{ include "signoz.name" . }}
        app.kubernetes.io/instance: {{ .Release.Name }}
        app.kubernetes.io/component: {{ default "signoz" .Values.signoz.name }}
        {{- end -}}
        {{- define "ingress.apiVersion" -}}
          {{- print "networking.k8s.io/v1" -}}
        {{- end -}}
        {{- define "ingress.supportsPathType" -}}
          true
        {{- end -}}
        {{- define "ingress.isStable" -}}
          true
        {{- end -}}
    "#};

    let ingress_src = indoc! {r#"
        {{- if .Values.signoz.ingress.enabled -}}
        {{- $fullName := include "signoz.fullname" . -}}
        {{- $ingressApiIsStable := eq (include "ingress.isStable" .) "true" -}}
        {{- $ingressSupportsPathType := eq (include "ingress.supportsPathType" .) "true" -}}
        apiVersion: {{ include "ingress.apiVersion" . }}
        kind: Ingress
        metadata:
          name: {{ $fullName }}
          labels:
            {{- include "signoz.labels" . | nindent 4 }}
          {{- with .Values.signoz.ingress.annotations }}
          annotations:
            {{- toYaml . | nindent 4 }}
          {{- end }}
        spec:
          {{- if .Values.signoz.ingress.className }}
          ingressClassName: {{ .Values.signoz.ingress.className }}
          {{- end }}
          {{- if .Values.signoz.ingress.tls }}
          tls:
            {{- range .Values.signoz.ingress.tls }}
            - hosts:
                {{- range .hosts }}
                - {{ . | quote }}
                {{- end }}
              {{- with .secretName }}
              secretName: {{ . }}
              {{- end }}
            {{- end }}
          {{- end }}
          rules:
            {{- range .Values.signoz.ingress.hosts }}
            - host: {{ .host | quote }}
              http:
                paths:
                  {{- range .paths }}
                  - path: {{ .path }}
                    {{- if $ingressSupportsPathType }}
                    pathType: {{ .pathType }}
                    {{- end }}
                    backend:
                      {{- if $ingressApiIsStable }}
                      service:
                        name: {{ $fullName }}
                        port:
                          number: {{ .port }}
                      {{- else }}
                      serviceName: {{ $fullName }}
                      servicePort: {{ .port }}
                      {{- end }}
                  {{- end }}
            {{- end }}
        {{- end }}
    "#};

    let _ = write(&root.join("templates/_helpers.tpl")?, helpers_src)?;
    let _ = write(&root.join("templates/ingress.yaml")?, ingress_src)?;

    let chart = load_chart(&root, &LoadOptions::default())?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // annotations map
    let annotations = schema
        .pointer("/properties/signoz/properties/ingress/properties/annotations")
        .ok_or_eyre("missing signoz.ingress.annotations schema")?;
    let ap = annotations
        .get("additionalProperties")
        .and_then(|v| v.as_object())
        .ok_or_eyre("missing additionalProperties")?;
    assert_eq!(ap.get("type").and_then(|v| v.as_str()), Some("string"));

    // hosts array
    let hosts_ty = schema
        .pointer("/properties/signoz/properties/ingress/properties/hosts/type")
        .ok_or_eyre("missing signoz.ingress.hosts schema")?;
    assert_eq!(hosts_ty.as_str(), Some("array"));

    // paths array
    let paths_ty = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/hosts/items/properties/paths/type",
        )
        .ok_or_eyre("missing signoz.ingress.hosts.*.paths schema")?;
    assert_eq!(paths_ty.as_str(), Some("array"));

    // pathType enum
    let path_type = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/hosts/items/properties/paths/items/properties/pathType",
        )
        .ok_or_eyre("missing signoz.ingress.hosts.*.paths.*.pathType schema")?;
    let enum_vals = path_type
        .get("enum")
        .and_then(|v| v.as_array())
        .ok_or_eyre("missing enum")?;
    assert!(
        enum_vals
            .iter()
            .any(|v| v.as_str() == Some("ImplementationSpecific")),
        "{path_type}"
    );

    // port integer
    let port = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/hosts/items/properties/paths/items/properties/port",
        )
        .ok_or_eyre("missing signoz.ingress.hosts.*.paths.*.port schema")?;
    assert_eq!(port.get("type").and_then(|v| v.as_str()), Some("integer"));

    Ok(())
}
