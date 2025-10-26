use color_eyre::eyre::OptionExt;
use color_eyre::eyre::{self};
use indoc::indoc;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use test_util::prelude::*;
use vfs::VfsPath;

use helm_schema_mapper::analyze::Role;
use helm_schema_mapper::analyze::analyze_template_file;
use helm_schema_mapper::analyze::{compute_define_closure, index_defines_in_dir};

#[test]
fn parses_signoz_ingress_template_and_maps_values() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::MemoryFS::new());

    // Minimal chart identity
    write(
        &root.join("Chart.yaml")?,
        indoc! {r#"
          apiVersion: v2
          name: signoz
          version: 0.1.0
        "#},
    )?;

    // Chart templates
    write(
        &root.join("templates/_helpers.tpl")?,
        indoc! {r#"
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
          {{- if and (.Capabilities.APIVersions.Has "networking.k8s.io/v1") (semverCompare ">= 1.19-0" .Capabilities.KubeVersion.Version) -}}
              {{- print "networking.k8s.io/v1" -}}
          {{- else if .Capabilities.APIVersions.Has "networking.k8s.io/v1beta1" -}}
            {{- print "networking.k8s.io/v1beta1" -}}
          {{- else -}}
            {{- print "extensions/v1beta1" -}}
          {{- end -}}
        {{- end -}}
        {{- define "ingress.supportsPathType" -}}
          {{- or (eq (include "ingress.isStable" .) "true") (and (eq (include "ingress.apiVersion" .) "networking.k8s.io/v1beta1") (semverCompare ">= 1.18-0" .Capabilities.KubeVersion.Version)) -}}
        {{- end -}}
        {{- define "ingress.isStable" -}}
          {{- eq (include "ingress.apiVersion" .) "networking.k8s.io/v1" -}}
        {{- end -}}
        "#},
    )?;

    // The ingress template (verbatim)
    write(
        &root.join("templates/ingress.yaml")?,
        indoc! {r#"
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
            {{- if and .Values.signoz.ingress.className (semverCompare ">=1.18-0" .Capabilities.KubeVersion.GitVersion) }}
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
        "#},
    )?;

    let tmpl_dir = root.join("templates")?;
    let defs = index_defines_in_dir(&tmpl_dir)?;
    dbg!(&defs);
    let closure = compute_define_closure(&defs);
    dbg!(&closure);

    // signoz.labels should (transitively) reference .Values.signoz.name and .Values.nameOverride
    let label_values: Vec<_> = closure
        .get("signoz.labels")
        .ok_or_eyre("missing define signoz.labels")?
        .iter()
        .map(|s| s.as_str())
        .collect();
    dbg!(&label_values);
    assert_that!(
        label_values,
        unordered_elements_are![&"signoz.name", &"nameOverride"]
    );

    // Analyze the ingress template only (file-by-file analysis); merge across files is higher-level API.
    let uses = analyze_template_file(&root.join("templates/ingress.yaml")?)?;

    // Index by value_path -> (role, optional path)
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Seen {
        role: Role,
        path: Option<String>,
    }
    let mut by: BTreeMap<String, Seen> = BTreeMap::new();
    for u in uses {
        let p = u.yaml_path.as_ref().map(|p| p.to_string());
        by.entry(u.value_path.clone())
            .and_modify(|s| {
                // upgrade role if ScalarValue is seen
                if matches!(u.role, Role::ScalarValue) {
                    s.role = Role::ScalarValue;
                    s.path = p.clone().or(s.path.clone());
                }
            })
            .or_insert(Seen {
                role: u.role.clone(),
                path: p,
            });
    }

    dbg!(&by);

    let by: Vec<_> = by.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();

    // Must see these direct Values keys from ingress.yaml
    assert_that!(
        &by,
        unordered_elements_are![
            (
                eq(&"signoz.ingress.enabled"),
                matches_pattern!(Seen {
                    role: any![eq(&Role::Guard), eq(&Role::Unknown)],
                    path: none(),
                })
            ),
            (
                eq(&"signoz.ingress.annotations"),
                matches_pattern!(Seen {
                    role: any![eq(&Role::Guard), eq(&Role::Unknown)],
                    path: none(),
                })
            ),
            (
                eq(&"signoz.ingress.className"),
                matches_pattern!(Seen {
                    role: eq(&Role::ScalarValue),
                    path: some(eq("spec.ingressClassName")),
                })
            ),
            (
                eq(&"signoz.ingress.tls"),
                matches_pattern!(Seen {
                    role: any![eq(&Role::Guard), eq(&Role::Unknown)],
                    path: none(),
                })
            ),
            (
                eq(&"signoz.ingress.hosts"),
                matches_pattern!(Seen {
                    role: any![eq(&Role::Guard), eq(&Role::Unknown)],
                    path: none(),
                })
            ),
            // Include-inducced keys at call sites
            // labels: {{ include "signoz.labels" . | nindent 4 }} → pulls nameOverride, signoz.name
            (
                eq(&"nameOverride"),
                matches_pattern!(Seen {
                    role: eq(&Role::ScalarValue),
                    path: some(eq("metadata.labels")),
                })
            ),
            (
                eq(&"signoz.name"),
                matches_pattern!(Seen {
                    role: eq(&Role::ScalarValue),
                    path: some(eq("metadata.labels")),
                })
            ),
            // $fullName := include "signoz.fullname" . → pulls fullnameOverride (assignment → Fragment)
            (
                eq(&"fullnameOverride"),
                matches_pattern!(Seen {
                    role: eq(&Role::Fragment),
                    path: none(),
                })
            )
        ]
    );

    let helpers_uses = analyze_template_file(&root.join("templates/_helpers.tpl")?)?;

    let mut helper_values: BTreeSet<String> = BTreeSet::new();
    for u in helpers_uses {
        helper_values.insert(u.value_path);
    }

    // for k in ["signoz.name", "nameOverride"] {
    //     let s = by
    //         .get(k)
    //         .unwrap_or_else(|| panic!("missing include-closure Value key: {k}"));
    //     // This include is placed at `metadata.labels: {{ include "signoz.labels" . | nindent 4 }}`
    //     // Today we model it as a scalar placeholder, so role is ScalarValue and path is metadata.labels.
    //     assert_that!(&s.role, any![eq(&Role::ScalarValue), eq(&Role::Unknown)]);
    //     assert_that!(&s.path, some(eq("metadata.labels")));
    // }

    // Keys inside helpers that the ingress references (indirectly, via includes)
    // (We’ll attach these to call-sites in the closure phase)
    assert_that!(
        &helper_values,
        unordered_elements_are![
            eq(&"fullnameOverride"), // .Values.fullnameOverride (top-level)
            eq(&"nameOverride"),     // .Values.nameOverride (top-level)
            eq(&"signoz.name"),      // used in selectorLabels (default "signoz" otherwise)
        ]
    );

    Ok(())
}
