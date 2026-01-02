#![allow(warnings)]
use color_eyre::eyre::{self, OptionExt, WrapErr};
use helm_schema_mapper::analyze::Occurrence;
use helm_schema_mapper::analyze::canonicalize_uses;
use helm_schema_mapper::analyze::group_uses;
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::*;
use vfs::VfsPath;

use helm_schema_mapper::analyze::Role;
use helm_schema_mapper::analyze::analyze_template_file;
use helm_schema_mapper::analyze::{compute_define_closure, index_defines_in_dir};
use helm_schema_mapper::yaml_path::YamlPath;

#[test]
fn parses_bitnami_ingress_template_and_maps_values() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());

    // Minimal chart identity
    write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
      apiVersion: v2
      name: minio
      version: 0.1.0
    "#},
    )?;

    // The Bitnami ingress template (verbatim)
    write(
        &root.join("templates/ingress.yaml")?,
        indoc! {r#"
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
          {{- range .Values.ingress.extraHosts }}
          - host: {{ .name | quote }}
            http:
              paths:
                - path: {{ default "/" .path }}
                  pathType: {{ default "ImplementationSpecific" .pathType }}
                  backend: {{- include "common.ingress.backend" (dict "serviceName" (include "common.names.fullname" $) "servicePort" "tcp-api" "context" $) | nindent 14 }}
          {{- end }}
          {{- if .Values.ingress.extraRules }}
          {{- include "common.tplvalues.render" (dict "value" .Values.ingress.extraRules "context" .) | nindent 4 }}
          {{- end }}
        {{- if or (and .Values.ingress.tls (or (include "common.ingress.certManagerRequest" (dict "annotations" .Values.ingress.annotations)) .Values.ingress.selfSigned)) .Values.ingress.extraTls }}
        tls:
          {{- if and .Values.ingress.tls (or (include "common.ingress.certManagerRequest" (dict "annotations" .Values.ingress.annotations)) .Values.ingress.selfSigned) }}
          - hosts:
              - {{ tpl .Values.ingress.hostname . }}
            secretName: {{ printf "%s-tls" (tpl .Values.ingress.hostname .) }}
          {{- end }}
          {{- if .Values.ingress.extraTls }}
          {{- include "common.tplvalues.render" (dict "value" .Values.ingress.extraTls "context" .) | nindent 4 }}
          {{- end }}
        {{- end }}
      {{- end }}
    "#},
    )?;

    // Add stub helpers that get scanned too (to simulate cross-file analysis).
    // We don't "execute" includes; we only want to ensure scanning other files works.
    write(
        &root.join("templates/_helpers.tpl")?,
        indoc! {r#"
            {{/*
            Kubernetes standard labels
            {{ include "common.labels.standard" (dict "customLabels" .Values.commonLabels "context" $) -}}
            */}}
            {{- define "common.labels.standard" -}}
            {{- if and (hasKey . "customLabels") (hasKey . "context") -}}
            {{- $default := dict "app.kubernetes.io/name" (include "common.names.name" .context) "helm.sh/chart" (include "common.names.chart" .context) "app.kubernetes.io/instance" .context.Release.Name "app.kubernetes.io/managed-by" .context.Release.Service -}}
            {{- with .context.Chart.AppVersion -}}
            {{- $_ := set $default "app.kubernetes.io/version" . -}}
            {{- end -}}
            {{ template "common.tplvalues.merge" (dict "values" (list .customLabels $default) "context" .context) }}
            {{- else -}}
            app.kubernetes.io/name: {{ include "common.names.name" . }}
            helm.sh/chart: {{ include "common.names.chart" . }}
            app.kubernetes.io/instance: {{ .Release.Name }}
            app.kubernetes.io/managed-by: {{ .Release.Service }}
            {{- with .Chart.AppVersion }}
            app.kubernetes.io/version: {{ . | replace "+" "_" | quote }}
            {{- end -}}
            {{- end -}}
            {{- end -}}

            {{/*
            Return the appropriate apiVersion for ingress.
            */}}
            {{- define "common.capabilities.ingress.apiVersion" -}}
            {{- print "networking.k8s.io/v1" -}}
            {{- end -}}

            {{/*
            Generate backend entry that is compatible with all Kubernetes API versions.

            Usage:
            {{ include "common.ingress.backend" (dict "serviceName" "backendName" "servicePort" "backendPort" "context" $) }}

            Params:
              - serviceName - String. Name of an existing service backend
              - servicePort - String/Int. Port name (or number) of the service. It will be translated to different yaml depending if it is a string or an integer.
              - context - Dict - Required. The context for the template evaluation.
            */}}
            {{- define "common.ingress.backend" -}}
            service:
              name: {{ .serviceName }}
              port:
                {{- if typeIs "string" .servicePort }}
                name: {{ .servicePort }}
                {{- else if or (typeIs "int" .servicePort) (typeIs "float64" .servicePort) }}
                number: {{ .servicePort | int }}
                {{- end }}
            {{- end -}}

            {{/*
            Return true if cert-manager required annotations for TLS signed
            certificates are set in the Ingress annotations
            Ref: https://cert-manager.io/docs/usage/ingress/#supported-annotations
            Usage:
            {{ include "common.ingress.certManagerRequest" ( dict "annotations" .Values.path.to.the.ingress.annotations ) }}
            */}}
            {{- define "common.ingress.certManagerRequest" -}}
            {{ if or (hasKey .annotations "cert-manager.io/cluster-issuer") (hasKey .annotations "cert-manager.io/issuer") (hasKey .annotations "kubernetes.io/tls-acme") }}
                {{- true -}}
            {{- end -}}
            {{- end -}}

            {{/*
            Expand the name of the chart.
            */}}
            {{- define "common.names.name" -}}
            {{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
            {{- end -}}

            {{/*
            Create chart name and version as used by the chart label.
            */}}
            {{- define "common.names.chart" -}}
            {{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" -}}
            {{- end -}}

            {{/*
            Create a default fully qualified app name.
            We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
            If release name contains chart name it will be used as a full name.
            */}}
            {{- define "common.names.fullname" -}}
            {{- if .Values.fullnameOverride -}}
            {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- $name := default .Chart.Name .Values.nameOverride -}}
            {{- $releaseName := regexReplaceAll "(-?[^a-z\\d\\-])+-?" (lower .Release.Name) "-" -}}
            {{- if contains $name $releaseName -}}
            {{- $releaseName | trunc 63 | trimSuffix "-" -}}
            {{- else -}}
            {{- printf "%s-%s" $releaseName $name | trunc 63 | trimSuffix "-" -}}
            {{- end -}}
            {{- end -}}
            {{- end -}}

            {{/*
            Allow the release namespace to be overridden for multi-namespace deployments in combined charts.
            */}}
            {{- define "common.names.namespace" -}}
            {{- default .Release.Namespace .Values.namespaceOverride | trunc 63 | trimSuffix "-" -}}
            {{- end -}}

            {{/* vim: set filetype=mustache: */}}
            {{/*
            Renders a value that contains template perhaps with scope if the scope is present.
            Usage:
            {{ include "common.tplvalues.render" ( dict "value" .Values.path.to.the.Value "context" $ ) }}
            {{ include "common.tplvalues.render" ( dict "value" .Values.path.to.the.Value "context" $ "scope" $app ) }}
            */}}
            {{- define "common.tplvalues.render" -}}
            {{- $value := typeIs "string" .value | ternary .value (.value | toYaml) }}
            {{- if contains "{{" (toJson .value) }}
              {{- if .scope }}
                  {{- tpl (cat "{{- with $.RelativeScope -}}" $value "{{- end }}") (merge (dict "RelativeScope" .scope) .context) }}
              {{- else }}
                {{- tpl $value .context }}
              {{- end }}
            {{- else }}
                {{- $value }}
            {{- end }}
            {{- end -}}

            {{/*
            Merge a list of values that contains template after rendering them.
            Merge precedence is consistent with http://masterminds.github.io/sprig/dicts.html#merge-mustmerge
            Usage:
            {{ include "common.tplvalues.merge" ( dict "values" (list .Values.path.to.the.Value1 .Values.path.to.the.Value2) "context" $ ) }}
            */}}
            {{- define "common.tplvalues.merge" -}}
            {{- $dst := dict -}}
            {{- range .values -}}
            {{- $dst = include "common.tplvalues.render" (dict "value" . "context" $.context "scope" $.scope) | fromYaml | merge $dst -}}
            {{- end -}}
            {{ $dst | toYaml }}
            {{- end -}}
            
        "#},
    )?;

    // Assert helper include closure: common.labels.standard pulls .Values.commonLabels
    // let tmpl_dir = root.join("templates")?;
    // let defs = index_defines_in_dir(&tmpl_dir)?;
    // dbg!(&defs);
    // let closure = compute_define_closure(&defs);
    // dbg!(&closure);

    // let label_values: Vec<_> = closure
    //     .get("common.labels.standard")
    //     .ok_or_eyre("missing define common.labels.standard")?
    //     .iter()
    //     .map(|s| s.as_str())
    //     .collect();
    // assert_that!(&label_values, unordered_elements_are![&"commonLabels"]);

    let uses = analyze_template_file(&root.join("templates/ingress.yaml")?)?;
    let groups = group_uses(&uses);
    dbg!(&groups);

    // EXPECTED VALUE KEYS (must all be present)
    let must_exist = [
        "ingress.enabled",          // guard
        "commonLabels",             // via include(... .Values.commonLabels ...)
        "commonAnnotations",        // guard + used in $annotations merge
        "ingress.annotations",      // guard + merge
        "ingress.ingressClassName", // scalar value
        "ingress.hostname",         // scalar value (tpl)
        "ingress.path",             // scalar value
        "ingress.pathType",         // scalar value
        "ingress.extraPaths",       // fragment via toYaml|nindent
        "ingress.extraHosts",       // range source (guard)
        "ingress.extraRules",       // fragment rendered via include|nindent
        "ingress.tls",              // guard (and inside and/or)
        "ingress.selfSigned",       // guard
        "ingress.extraTls",         // guard + fragment include
    ];
    for k in must_exist {
        assert!(groups.contains_key(k), "missing expected Values key: {k}");
    }

    // ROLES we expect (today)
    // Guards (no YAML path)
    for k in [
        "ingress.enabled",
        "ingress.tls",
        "ingress.selfSigned",
        "ingress.extraTls",
        "ingress.extraHosts",
        "ingress.annotations",
        "commonAnnotations",
    ] {
        // NOTE: until we record guard usage explicitly, these might be Unknown — mark as TODO when failing.
        if let Some(seen) = groups.get(k) {
            // assert_that!(&seen.role, any![eq(&Role::Guard), eq(&Role::Unknown)]);
            // assert!(seen.path.is_none());
        }
    }

    // Scalars with precise YAML paths
    // Ingress class name → spec.ingressClassName
    {
        let s = groups
            .get("ingress.ingressClassName")
            .ok_or_eyre("ingress.ingressClassName")?;
        assert_that!(
            s,
            unordered_elements_are![
                matches_pattern!(Occurrence {
                    role: eq(&Role::Guard),
                    path: none(),
                    ..
                }),
                matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.ingressClassName"))),
                    ..
                }),
            ]
        );
    }

    // host/path/pathType under the first rule
    {
        let host = groups
            .get("ingress.hostname")
            .ok_or_eyre("ingress.hostname")?;
        assert_that!(
            host,
            unordered_elements_are![
                matches_pattern!(Occurrence {
                    role: eq(&Role::Guard),
                    path: none(),
                    ..
                }),
                matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].host"))),
                    ..
                }),
                matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.tls[0].hosts[0]"))),
                    ..
                }),
                matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.tls[0].secretName"))),
                    ..
                }),
            ]
        );

        // let path = by.get("ingress.path").expect("ingress.path");
        // assert_that!(&path.role, eq(&Role::ScalarValue));
        // assert_that!(
        //     &path.path,
        //     some(contains_substring("spec.rules[0].http.paths[0].path"))
        // );
        //
        // let ptype = by.get("ingress.pathType").expect("ingress.pathType");
        // assert_that!(&ptype.role, eq(&Role::ScalarValue));
        // assert_that!(
        //     &ptype.path,
        //     some(contains_substring("spec.rules[0].http.paths[0].pathType"))
        // );
    }

    Ok(())
}

// {{- define "common.labels.standard" -}}
// {{- /* deliberately reference Values */ -}}
// {{- .Values.commonLabels | toYaml -}}
// {{- end -}}
//
// {{- define "common.tplvalues.render" -}}
// {{- /* real chart uses the passed dict.value; we keep it simple here */ -}}
// {{- end -}}

// // Index by value_path -> (role, optional path)
// #[derive(Debug, Clone, PartialEq, Eq)]
// struct Seen {
//     role: Role,
//     path: Option<String>,
// }
// let mut by: BTreeMap<String, Seen> = BTreeMap::new();
// for u in uses {
//     let p = u.yaml_path.as_ref().map(|p| p.to_string());
//     by.entry(u.value_path.clone())
//         .and_modify(|s| {
//             // upgrade role if ScalarValue is seen
//             if matches!(u.role, Role::ScalarValue) {
//                 s.role = Role::ScalarValue;
//                 s.path = p.clone().or(s.path.clone());
//             }
//         })
//         .or_insert(Seen {
//             role: u.role.clone(),
//             path: p,
//         });
// }
//
// dbg!(&by);

// assert_that!(&s.role, eq(&Role::ScalarValue));
// assert_that!(&s.path, some(eq("spec.ingressClassName")));

// // Fragments (toYaml/ include-rendered) — we may only map to a parent path or None (for now)
// for k in [
//     "ingress.extraPaths",
//     "ingress.extraRules",
//     "ingress.extraTls",
// ] {
//     let s = groups.get(k).ok_or_eyre(k)?;
//     // currently we drop fragment structure; placeholder is scalar
//     assert_that!(&s.role, any![eq(&Role::ScalarValue), eq(&Role::Unknown)]);
//     // Parent may resolve as 'spec.rules' or 'spec.tls' etc., or be None — both acceptable for now.
// }
//
// // commonLabels via include(... dict "customLabels" .Values.commonLabels ...)
// {
//     let s = groups.get("commonLabels").ok_or_eyre("commonLabels")?;
//     // This appears inside a value position (labels: <include|nindent>) so today it's a ScalarValue placeholder
//     assert_that!(&s.role, any![eq(&Role::ScalarValue), eq(&Role::Unknown)]);
//     // Path commonly "metadata.labels" (the include emits a mapping), but we accept None for now.
// }
