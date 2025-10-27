#![allow(warnings)]
use color_eyre::eyre::{self, OptionExt};
use helm_schema_mapper::ValueUse;
use indoc::indoc;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::*;
use vfs::VfsPath;

use helm_schema_mapper::analyze::{Occurrence, analyze_template_file};
use helm_schema_mapper::analyze::{Role, group_uses};
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

    // Analyze the ingress template only (file-by-file analysis); merge across files is higher-level API.
    let uses = analyze_template_file(&root.join("templates/ingress.yaml")?)?;
    dbg!(&uses);

    let groups = group_uses(&uses);
    dbg!(&groups);

    // NEGATIVE sanity: no Capabilities-derived keys & no guard-prefix rebinding
    let bad_keys: Vec<_> = groups
        .keys()
        .filter(|k| {
            k.contains(".Capabilities")
                || (k.starts_with("signoz.ingress.enabled.")
                    && k.as_str() != "signoz.ingress.enabled")
        })
        .cloned()
        .collect();
    assert_that!(&bad_keys, unordered_elements_are![]);

    // --- USE-LEVEL spot checks for the direct Values in the template
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"signoz.ingress.className"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.ingressClassName"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"signoz.ingress.tls.hosts"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.tls[0].hosts[0]"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"signoz.ingress.tls.secretName"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.tls[0].secretName"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"signoz.ingress.hosts.host"),
            role: eq(&Role::ScalarValue),
            yaml_path: some(displays_as(eq("spec.rules[0].host"))),
            ..
        }))
    );
    assert_that!(
        &uses,
        contains(matches_pattern!(ValueUse {
            value_path: eq(&"signoz.ingress.annotations"),
            role: eq(&Role::Fragment), // toYaml block
            yaml_path: some(displays_as(eq("metadata.annotations"))),
            ..
        }))
    );

    // GROUP-LEVEL expectations from the template only
    let by: Vec<_> = groups
        .clone()
        .into_iter()
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                // guards
                "signoz.ingress.enabled"
                    | "signoz.ingress.tls"
                    | "signoz.ingress.hosts"
                    // scalar placements
                    | "signoz.ingress.className"
                    | "signoz.ingress.tls.hosts"
                    | "signoz.ingress.tls.secretName"
                    | "signoz.ingress.hosts.host"
                    | "signoz.ingress.hosts.paths.path"
                    | "signoz.ingress.hosts.paths.pathType"
                    | "signoz.ingress.hosts.paths.port"
                    // fragment block
                    | "signoz.ingress.annotations"
            )
        })
        .collect();

    assert_that!(
        &by,
        unordered_elements_are![
            // top-level template guard
            (
                eq(&"signoz.ingress.enabled"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::Guard),
                    path: none(),
                    ..
                }))
            ),
            // annotations: guard + fragment at metadata.annotations
            (
                eq(&"signoz.ingress.annotations"),
                unordered_elements_are![
                    matches_pattern!(Occurrence {
                        role: eq(&Role::Guard),
                        path: none(),
                        ..
                    }),
                    matches_pattern!(Occurrence {
                        role: eq(&Role::Fragment),
                        path: some(displays_as(eq("metadata.annotations"))),
                        ..
                    }),
                ]
            ),
            // className: guard + scalar render
            (
                eq(&"signoz.ingress.className"),
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
            ),
            // tls appears as guards (outer if + range)
            (
                eq(&"signoz.ingress.tls"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::Guard),
                    path: none(),
                    ..
                }))
            ),
            // concrete tls scalars
            (
                eq(&"signoz.ingress.tls.hosts"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.tls[0].hosts[0]"))),
                    ..
                }))
            ),
            (
                eq(&"signoz.ingress.tls.secretName"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.tls[0].secretName"))),
                    ..
                }))
            ),
            // hosts range guard
            (
                eq(&"signoz.ingress.hosts"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::Guard),
                    path: none(),
                    ..
                }))
            ),
            // concrete host scalar
            (
                eq(&"signoz.ingress.hosts.host"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].host"))),
                    ..
                }))
            ),
            // path & pathType scalars inside paths[]
            (
                eq(&"signoz.ingress.hosts.paths.path"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].http.paths[0].path"))),
                    ..
                }))
            ),
            (
                eq(&"signoz.ingress.hosts.paths.pathType"),
                contains(matches_pattern!(Occurrence {
                    role: eq(&Role::ScalarValue),
                    path: some(displays_as(eq("spec.rules[0].http.paths[0].pathType"))),
                    ..
                }))
            ),
            // port scalar appears in both stable/legacy branches
            (
                eq(&"signoz.ingress.hosts.paths.port"),
                unordered_elements_are![
                    matches_pattern!(Occurrence {
                        role: eq(&Role::ScalarValue),
                        path: some(displays_as(eq(
                            "spec.rules[0].http.paths[0].backend.service.port.number"
                        ))),
                        ..
                    }),
                    matches_pattern!(Occurrence {
                        role: eq(&Role::ScalarValue),
                        path: some(displays_as(eq(
                            "spec.rules[0].http.paths[0].backend.servicePort"
                        ))),
                        ..
                    }),
                ]
            )
        ]
    );

    Ok(())
}

// let tmpl_dir = root.join("templates")?;
// let defs = index_defines_in_dir(&tmpl_dir)?;
// dbg!(&defs);
// let closure = compute_define_closure(&defs);
// dbg!(&closure);
//
// // signoz.labels should (transitively) reference .Values.signoz.name and .Values.nameOverride
// let label_values: Vec<_> = closure
//     .get("signoz.labels")
//     .ok_or_eyre("missing define signoz.labels")?
//     .iter()
//     .map(|s| s.as_str())
//     .collect();
// dbg!(&label_values);
// assert_that!(
//     label_values,
//     unordered_elements_are![&"signoz.name", &"nameOverride"]
// );

// // Must see these direct Values keys from ingress.yaml
// assert_that!(
//     &groups,
//     unordered_elements_are![
//         (
//             eq(&"signoz.ingress.enabled"),
//             unordered_elements_are![matches_pattern!(Occurrence {
//                 // role: any![eq(&Role::Guard), eq(&Role::Unknown)],
//                 role: eq(&Role::Guard),
//                 path: none(),
//                 ..
//             })]
//         ),
//         (
//             eq(&"signoz.ingress.annotations"),
//             unordered_elements_are![matches_pattern!(Occurrence {
//                 role: eq(&Role::Guard),
//                 // role: any![eq(&Role::Guard), eq(&Role::Unknown)],
//                 path: none(),
//                 ..
//             })]
//         ),
//         (
//             eq(&"signoz.ingress.className"),
//             unordered_elements_are![
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Guard),
//                     path: none(),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq("spec.ingressClassName"))),
//                     ..
//                 }),
//             ]
//         ),
//         (
//             eq(&"signoz.ingress.tls"),
//             unordered_elements_are![
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Guard),
//                     path: none(),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Guard),
//                     path: none(),
//                     ..
//                 }),
//             ]
//         ),
//         (
//             eq(&"signoz.ingress.hosts"),
//             unordered_elements_are![matches_pattern!(Occurrence {
//                 role: eq(&Role::Guard),
//                 // role: any![eq(&Role::Guard), eq(&Role::Unknown)],
//                 path: none(),
//                 ..
//             })]
//         ),
//         // Include-inducced keys at call sites
//         // labels: {{ include "signoz.labels" . | nindent 4 }} → pulls nameOverride, signoz.name
//         (
//             eq(&"nameOverride"),
//             unordered_elements_are![
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Fragment),
//                     path: none(),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Fragment),
//                     path: none(),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq("metadata.name"))),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq(
//                         "spec.rules[0].http.paths[0].backend.service.name"
//                     ))), // stable
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq(
//                         "spec.rules[0].http.paths[0].backend.serviceName"
//                     ))), // legacy
//                     ..
//                 }),
//             ]
//         ),
//         (
//             eq(&"signoz.name"),
//             unordered_elements_are![matches_pattern!(Occurrence {
//                 role: eq(&Role::Fragment),
//                 path: none(),
//                 // path: some(displays_as(eq("metadata.labels"))),
//                 ..
//             })]
//         ),
//         // $fullName := include "signoz.fullname" . → pulls fullnameOverride (assignment → Fragment)
//         (
//             eq(&"fullnameOverride"),
//             unordered_elements_are![
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::Fragment),
//                     path: none(),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq("metadata.name"))),
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq(
//                         "spec.rules[0].http.paths[0].backend.service.name"
//                     ))), // stable
//                     ..
//                 }),
//                 matches_pattern!(Occurrence {
//                     role: eq(&Role::ScalarValue),
//                     path: some(displays_as(eq(
//                         "spec.rules[0].http.paths[0].backend.serviceName"
//                     ))), // legacy
//                     ..
//                 }),
//             ]
//         )
//     ]
// );
//
// let helpers_uses = analyze_template_file(&root.join("templates/_helpers.tpl")?)?;
//
// let mut helper_values: BTreeSet<String> = BTreeSet::new();
// for u in helpers_uses {
//     helper_values.insert(u.value_path);
// }
//
// // Keys inside helpers that the ingress references (indirectly, via includes)
// // (We’ll attach these to call-sites in the closure phase)
// assert_that!(
//     &helper_values,
//     unordered_elements_are![
//         eq(&"fullnameOverride"), // .Values.fullnameOverride (top-level)
//         eq(&"nameOverride"),     // .Values.nameOverride (top-level)
//         eq(&"signoz.name"),      // used in selectorLabels (default "signoz" otherwise)
//     ]
// );
