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

#[ignore = "wip"]
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
          {{- define "common.labels.standard" -}}
          {{- /* deliberately reference Values */ -}}
          {{- .Values.commonLabels | toYaml -}}
          {{- end -}}

          {{- define "common.tplvalues.render" -}}
          {{- /* real chart uses the passed dict.value; we keep it simple here */ -}}
          {{- end -}}
        "#},
    )?;

    // Assert helper include closure: common.labels.standard pulls .Values.commonLabels
    let tmpl_dir = root.join("templates")?;
    let defs = index_defines_in_dir(&tmpl_dir)?;
    dbg!(&defs);
    let closure = compute_define_closure(&defs);
    dbg!(&closure);

    let label_values: Vec<_> = closure
        .get("common.labels.standard")
        .ok_or_eyre("missing define common.labels.standard")?
        .iter()
        .map(|s| s.as_str())
        .collect();
    assert_that!(&label_values, unordered_elements_are![&"commonLabels"]);

    // Analyze the ingress template only (file-by-file analysis); merge across files is higher-level API.
    let uses = analyze_template_file(&root.join("templates/ingress.yaml")?)?;
    // let by = canonicalize_uses(&uses);
    let groups = group_uses(&uses);
    dbg!(&groups);

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

    // ---- EXPECTED VALUE KEYS (must all be present) ----
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

    // ---- ROLES we expect (today) ----
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

        // assert_that!(&s.role, eq(&Role::ScalarValue));
        // assert_that!(&s.path, some(eq("spec.ingressClassName")));
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

    Ok(())
}
