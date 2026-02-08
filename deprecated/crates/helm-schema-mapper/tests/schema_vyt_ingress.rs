use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use helm_schema_mapper::{
    schema::generate_values_schema_for_ingress_vyt,
    vyt::{DefineIndex, VYKind, VYT},
};
use indoc::indoc;
use std::sync::Arc;
use test_util::prelude::*;
use vfs::VfsPath;

fn index_defines_from_str(src: &str) -> eyre::Result<Arc<DefineIndex>> {
    let parsed =
        helm_schema_template::parse::parse_gotmpl_document(src).ok_or_eyre("parse helpers")?;
    let mut idx = DefineIndex::default();

    let mut stack = vec![parsed.tree.root_node()];
    while let Some(n) = stack.pop() {
        if n.kind() == "define_action" {
            let block = n.utf8_text(src.as_bytes()).unwrap_or_default();
            let name = regex::Regex::new(r#"define\s+\"([^\"]+)\""#)
                .unwrap()
                .captures(block)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                .ok_or_eyre("missing define name")?;
            let parsed_block = helm_schema_template::parse::parse_gotmpl_document(block)
                .ok_or_eyre("parse define block")?;
            idx.insert(name, parsed_block.tree, block.to_string());
        }

        let mut w = n.walk();
        for ch in n.children(&mut w) {
            if ch.is_named() {
                stack.push(ch);
            }
        }
    }

    Ok(Arc::new(idx))
}

#[test]
fn generates_values_schema_from_signoz_ingress_vyt() -> eyre::Result<()> {
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

    let defs = index_defines_from_str(helpers_src)?;
    let parsed = helm_schema_template::parse::parse_gotmpl_document(ingress_src)
        .ok_or_eyre("parse ingress")?;
    let uses = VYT::new(ingress_src.to_string())
        .with_defines(defs)
        .run(&parsed.tree);

    assert!(
        uses.iter().any(|u| {
            u.source_expr == "signoz.ingress.hosts.*.paths.*.pathType" && u.kind == VYKind::Scalar
        }),
        "{:#?}",
        uses
    );

    let schema = generate_values_schema_for_ingress_vyt(&uses);

    let annotations = schema
        .pointer("/properties/signoz/properties/ingress/properties/annotations")
        .ok_or_eyre("missing signoz.ingress.annotations schema")?;
    let ap = annotations
        .get("additionalProperties")
        .and_then(|v| v.as_object())
        .ok_or_eyre("missing additionalProperties")?;
    assert_eq!(ap.get("type").and_then(|v| v.as_str()), Some("string"));

    let path_type = schema
        .pointer("/properties/signoz/properties/ingress/properties/hosts/type")
        .ok_or_eyre("missing signoz.ingress.hosts schema")?;
    assert_eq!(path_type.as_str(), Some("array"));

    let path_type = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/hosts/items/properties/paths/type",
        )
        .ok_or_eyre("missing signoz.ingress.hosts.*.paths schema")?;
    assert_eq!(path_type.as_str(), Some("array"));

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

    let port = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/hosts/items/properties/paths/items/properties/port",
        )
        .ok_or_eyre("missing signoz.ingress.hosts.*.paths.*.port schema")?;
    assert_eq!(port.get("type").and_then(|v| v.as_str()), Some("integer"));

    // tls should be an array; tls.items.hosts should be an array of strings
    let tls_ty = schema
        .pointer("/properties/signoz/properties/ingress/properties/tls/type")
        .ok_or_eyre("missing signoz.ingress.tls schema")?;
    assert_eq!(tls_ty.as_str(), Some("array"));
    let tls_hosts_items = schema
        .pointer(
            "/properties/signoz/properties/ingress/properties/tls/items/properties/hosts/items/type",
        )
        .ok_or_eyre("missing signoz.ingress.tls.*.hosts.* schema")?;
    assert_eq!(tls_hosts_items.as_str(), Some("string"));

    Ok(())
}

#[test]
fn generates_values_schema_from_bitnami_ingress_vyt() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());

    let _ = write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
            apiVersion: v2
            name: minio
            version: 0.1.0
        "#},
    )?;

    let ingress_src = indoc! {r#"
        {{- /*
        Copyright Broadcom, Inc. All Rights Reserved.
        SPDX-License-Identifier: APACHE-2.0
        */}}

        {{- if .Values.ingress.enabled }}
        apiVersion: {{ include "common.capabilities.ingress.apiVersion" . }}
        kind: Ingress
        metadata:
          name: {{ include "common.names.fullname" . }}
          namespace: {{ include "common.names.namespace" . | quote }}
          labels: {{- include "common.labels.standard" (dict "customLabels" .Values.commonLabels "context" .) | nindent 4 }}
            app.kubernetes.io/component: minio
            app.kubernetes.io/part-of: minio
          {{- if or .Values.ingress.annotations .Values.commonAnnotations }}
          {{- $annotations := include "common.tplvalues.merge" (dict "values" (list .Values.ingress.annotations .Values.commonAnnotations) "context" .) }}
          annotations: {{- include "common.tplvalues.render" (dict "value" $annotations "context" .) | nindent 4 }}
          {{- end }}
        spec:
          {{- if .Values.ingress.ingressClassName }}
          ingressClassName: {{ .Values.ingress.ingressClassName | quote }}
          {{- end }}
          rules:
            {{- if .Values.ingress.hostname }}
            - host: {{ tpl .Values.ingress.hostname . }}
              http:
                paths:
                  {{- if .Values.ingress.extraPaths }}
                  {{- toYaml .Values.ingress.extraPaths | nindent 10 }}
                  {{- end }}
                  - path: {{ .Values.ingress.path }}
                    pathType: {{ .Values.ingress.pathType }}
                    backend: {{- include "common.ingress.backend" (dict "serviceName" (include "common.names.fullname" .) "servicePort" "tcp-api" "context" .)  | nindent 14 }}
            {{- end }}
        {{- end }}
    "#};

    let helpers_src = indoc! {r#"
        {{- define "common.capabilities.ingress.apiVersion" -}}
        {{- if .Capabilities.APIVersions.Has "networking.k8s.io/v1/Ingress" -}}
        networking.k8s.io/v1
        {{- else -}}
        networking.k8s.io/v1beta1
        {{- end -}}
        {{- end -}}

        {{- define "common.names.fullname" -}}
        {{- default .Chart.Name .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
        {{- end -}}

        {{- define "common.names.namespace" -}}
        {{- default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-" -}}
        {{- end -}}

        {{- define "common.labels.standard" -}}
        {{- toYaml .customLabels -}}
        {{- end -}}

        {{- define "common.ingress.backend" -}}
        service:
          name: {{ .serviceName }}
          port:
            name: {{ .servicePort }}
        {{- end -}}

        {{- define "common.tplvalues.render" -}}
        {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) -}}
        {{- if contains "{{" (toJson .value) -}}
          {{- tpl $value .context -}}
        {{- else -}}
          {{- $value -}}
        {{- end -}}
        {{- end -}}

        {{- define "common.tplvalues.merge" -}}
        {{- $dst := dict -}}
        {{- range .values -}}
        {{- $dst = include "common.tplvalues.render" (dict "value" . "context" $.context "scope" $.scope) | fromYaml | merge $dst -}}
        {{- end -}}
        {{ $dst | toYaml }}
        {{- end -}}
    "#};

    let defs = index_defines_from_str(helpers_src)?;

    let parsed = helm_schema_template::parse::parse_gotmpl_document(ingress_src)
        .ok_or_eyre("parse ingress")?;
    let uses = VYT::new(ingress_src.to_string())
        .with_defines(defs)
        .run(&parsed.tree);

    assert!(
        uses.iter()
            .any(|u| u.source_expr == "ingress.pathType" && u.kind == VYKind::Scalar),
        "{:#?}",
        uses
    );

    let schema = generate_values_schema_for_ingress_vyt(&uses);

    let path_type = schema
        .pointer("/properties/ingress/properties/pathType")
        .ok_or_eyre("missing ingress.pathType schema")?;
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

    let annotations = schema
        .pointer("/properties/ingress/properties/annotations")
        .ok_or_eyre("missing ingress.annotations schema")?;
    let ap = annotations
        .get("additionalProperties")
        .and_then(|v| v.as_object())
        .ok_or_eyre("missing additionalProperties")?;
    assert_eq!(ap.get("type").and_then(|v| v.as_str()), Some("string"));

    Ok(())
}
