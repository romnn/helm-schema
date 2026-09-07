//! Chart metadata owns template execution names across aliased dependencies.

use std::collections::BTreeSet;

use color_eyre::eyre;
use helm_schema::AnalysisSession;
use helm_schema::generation::GenerateOptions;
use helm_schema::provider::ProviderOptions;
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

#[test]
fn self_includes_keep_root_and_nested_alias_namespaces_distinct() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &root.join("Chart.yaml")?,
        indoc! {"
        apiVersion: v2
        name: execution-root
        version: 1.0.0
        dependencies:
          - name: package
            version: 1.0.0
            alias: left
          - name: package
            version: 1.0.0
            alias: right
    "},
    )?;
    test_util::write(
        &root.join("charts/physical-package/Chart.yaml")?,
        indoc! {"
        apiVersion: v2
        name: package
        version: 1.0.0
        dependencies:
          - name: leaf
            version: 1.0.0
            alias: inside
    "},
    )?;
    test_util::write(
        &root.join("charts/physical-package/charts/physical-leaf/Chart.yaml")?,
        indoc! {"
        apiVersion: v2
        name: leaf
        version: 1.0.0
    "},
    )?;
    for (directory, token) in [
        ("", "parentToken"),
        ("charts/physical-package", "childToken"),
        ("charts/physical-package/charts/physical-leaf", "leafToken"),
    ] {
        let chart = if directory.is_empty() {
            root.clone()
        } else {
            root.join(directory)?
        };
        test_util::write(&chart.join("values.yaml")?, "{}")?;
        test_util::write(
            &chart.join("templates/configmap.yaml")?,
            &formatdoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: token
            data:
              token: {{{{ .Values.{token} | quote }}}}
        "#},
        )?;
        test_util::write(
            &chart.join("templates/consumer.yaml")?,
            indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: consumer
            data:
              token: {{ include (print $.Template.BasePath "/configmap.yaml") . | quote }}
        "#},
        )?;
    }
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir: root,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: helm_schema::generation::SchemaProfile::default().into(),
        provider: ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..ProviderOptions::default()
        },
    });
    let paths: BTreeSet<String> = session
        .contract_document()?
        .uses
        .iter()
        .map(|use_| use_.source_expr.encode())
        .filter(|path| !path.is_empty())
        .collect();
    sim_assert_eq!(have: paths, want: BTreeSet::from([
        "global".to_string(),
        "left".to_string(),
        "parentToken".to_string(),
        "left.childToken".to_string(),
        "left.inside.leafToken".to_string(),
        "right.childToken".to_string(),
        "right".to_string(),
        "right.inside.leafToken".to_string(),
    ]));
    Ok(())
}

#[test]
fn library_non_partials_supply_neither_execution_names_nor_definitions() -> eyre::Result<()> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &root.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: root
            version: 1.0.0
            dependencies:
              - name: library
                version: 1.0.0
        "},
    )?;
    test_util::write(
        &root.join("charts/library/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: library
            version: 1.0.0
            type: library
        "},
    )?;
    test_util::write(
        &root.join("charts/library/templates/nested/_partial.yaml")?,
        r#"{{ .Values.allowed | quote }}{{ define "library.visible" }}{{ .Values.named | quote }}{{ end }}"#,
    )?;
    test_util::write(
        &root.join("charts/library/templates/_directory/configmap.yaml")?,
        r#"{{ .Values.fabricated | quote }}{{ define "library.hidden" }}{{ .Values.hidden | quote }}{{ end }}"#,
    )?;
    test_util::write(
        &root.join("templates/consumer.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: consumer
            data:
              partial: {{ include "root/charts/library/templates/nested/_partial.yaml" . }}
              named: {{ include "library.visible" . }}
              unavailable: {{ include "root/charts/library/templates/_directory/configmap.yaml" . }}
              hidden: {{ include "library.hidden" . }}
        "#},
    )?;
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir: root,
        include_tests: false,
        include_subchart_values: false,
        values_files: Vec::new(),
        infer_required: false,
        emission: helm_schema::generation::SchemaProfile::default().into(),
        provider: ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..ProviderOptions::default()
        },
    });
    let paths: BTreeSet<String> = session
        .contract_document()?
        .uses
        .iter()
        .map(|use_| use_.source_expr.encode())
        .filter(|path| !path.is_empty())
        .collect();
    // Unavailable calls abstain instead of importing reads from bodies Helm never registers.
    // Dependency discovery also admits the root global input for Helm propagation.
    sim_assert_eq!(have: paths, want: BTreeSet::from(["allowed".to_string(), "global".to_string(), "named".to_string()]));
    Ok(())
}
