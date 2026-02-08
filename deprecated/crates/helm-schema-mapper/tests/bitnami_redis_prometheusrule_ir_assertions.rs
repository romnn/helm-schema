use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_mapper::fused_ir::{self, FusedDefineIndex};
use helm_schema_mapper::vyt;
use indoc::indoc;
use std::path::PathBuf;
use test_util::prelude::*;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_defines() -> FusedDefineIndex {
    let mut defs = FusedDefineIndex::default();

    // Redis chart helpers
    let helpers = std::fs::read_to_string(
        crate_root().join("testdata/charts/bitnami-redis/templates/_helpers.tpl"),
    )
    .expect("read _helpers.tpl");
    defs.add_source(&helpers).expect("parse _helpers.tpl");

    // Common chart templates (from signoz testdata – same bitnami common library)
    let common_base = crate_root().join(
        "testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates",
    );
    for name in ["_names.tpl", "_labels.tpl", "_tplvalues.tpl"] {
        let src = std::fs::read_to_string(common_base.join(name))
            .unwrap_or_else(|e| panic!("read {name}: {e}"));
        defs.add_source(&src)
            .unwrap_or_else(|e| panic!("parse {name}: {e}"));
    }

    defs
}

#[test]
fn bitnami_redis_prometheusrule_fused_ir() {
    let template_src = indoc! {r#"
        {{- /*
        Copyright Broadcom, Inc. All Rights Reserved.
        SPDX-License-Identifier: APACHE-2.0
        */}}

        {{- if and .Values.metrics.enabled .Values.metrics.prometheusRule.enabled }}
        apiVersion: monitoring.coreos.com/v1
        kind: PrometheusRule
        metadata:
          name: {{ template "common.names.fullname" . }}
          namespace: {{ default (include "common.names.namespace" .) .Values.metrics.prometheusRule.namespace | quote }}
          labels: {{- include "common.labels.standard" ( dict "customLabels" .Values.commonLabels "context" $ ) | nindent 4 }}
            {{- if .Values.metrics.prometheusRule.additionalLabels }}
            {{- include "common.tplvalues.render" (dict "value" .Values.metrics.prometheusRule.additionalLabels "context" $) | nindent 4 }}
            {{- end }}
          {{- if .Values.commonAnnotations }}
          annotations: {{- include "common.tplvalues.render" ( dict "value" .Values.commonAnnotations "context" $ ) | nindent 4 }}
          {{- end }}
        spec:
          groups:
            - name: {{ include "common.names.fullname" . }}
              rules: {{- include "common.tplvalues.render" ( dict "value" .Values.metrics.prometheusRule.rules "context" $ ) | nindent 8 }}
        {{- end }}
    "#};

    let defines = load_defines();

    let tree = yaml_rust::parse_fused_yaml_helm(template_src).expect("fused parse");

    let uses = fused_ir::generate_fused_ir(&tree, &defines);

    let actual = serde_json::to_value(&uses).unwrap();

    // Hardcoded expected IR – sorted by (source_expr, path, kind, resource, guards).
    //
    // Known limitations of the fused parser approach vs the old VYT:
    //   - annotations path is ["annotations"] not ["metadata", "annotations"]
    //     because the fused parser splits on Helm control flow boundaries.
    //   - additionalLabels include path is [] instead of under metadata/labels
    //     for the same reason.
    let expected = serde_json::json!([
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["annotations"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "commonAnnotations"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "fullnameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "fullnameOverride",
            "path": ["metadata", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "fullnameOverride",
            "path": ["spec", "groups[*]", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": [],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "metrics.prometheusRule.additionalLabels"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled"],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.namespace",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.rules",
            "path": ["spec", "groups[*]", "rules"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["metadata", "labels", "app.kubernetes.io/name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["metadata", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["spec", "groups[*]", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "namespaceOverride",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// Same template, same includes, but using the tree-sitter VYT interpreter.
// ---------------------------------------------------------------------------

fn build_vyt_define_index() -> eyre::Result<std::sync::Arc<vyt::DefineIndex>> {
    let root = VfsPath::new(vfs::PhysicalFS::new(env!("CARGO_MANIFEST_DIR")));
    let mut defs = vyt::DefineIndex::default();

    let redis_helpers = root.join("testdata/charts/bitnami-redis/templates/_helpers.tpl")?;
    let redis_helpers_src = redis_helpers
        .read_to_string()
        .wrap_err_with(|| format!("read template {}", redis_helpers.as_str()))?;
    vyt::extend_define_index_from_str(&mut defs, &redis_helpers_src)
        .wrap_err_with(|| format!("index defines in {}", redis_helpers.as_str()))?;

    let common_dir = root.join(
        "testdata/charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates",
    )?;
    for p in ["_names.tpl", "_labels.tpl", "_tplvalues.tpl"] {
        let path = common_dir.join(p)?;
        let src = path
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", path.as_str()))?;
        vyt::extend_define_index_from_str(&mut defs, &src)
            .wrap_err_with(|| format!("index defines in {}", path.as_str()))?;
    }

    Ok(std::sync::Arc::new(defs))
}

#[test]
fn bitnami_redis_prometheusrule_vyt_ir() -> eyre::Result<()> {
    Builder::default().build();

    let template_src = indoc! {r#"
        {{- /*
        Copyright Broadcom, Inc. All Rights Reserved.
        SPDX-License-Identifier: APACHE-2.0
        */}}

        {{- if and .Values.metrics.enabled .Values.metrics.prometheusRule.enabled }}
        apiVersion: monitoring.coreos.com/v1
        kind: PrometheusRule
        metadata:
          name: {{ template "common.names.fullname" . }}
          namespace: {{ default (include "common.names.namespace" .) .Values.metrics.prometheusRule.namespace | quote }}
          labels: {{- include "common.labels.standard" ( dict "customLabels" .Values.commonLabels "context" $ ) | nindent 4 }}
            {{- if .Values.metrics.prometheusRule.additionalLabels }}
            {{- include "common.tplvalues.render" (dict "value" .Values.metrics.prometheusRule.additionalLabels "context" $) | nindent 4 }}
            {{- end }}
          {{- if .Values.commonAnnotations }}
          annotations: {{- include "common.tplvalues.render" ( dict "value" .Values.commonAnnotations "context" $ ) | nindent 4 }}
          {{- end }}
        spec:
          groups:
            - name: {{ include "common.names.fullname" . }}
              rules: {{- include "common.tplvalues.render" ( dict "value" .Values.metrics.prometheusRule.rules "context" $ ) | nindent 8 }}
        {{- end }}
    "#};

    let defs = build_vyt_define_index()?;

    let parsed = helm_schema_template::parse::parse_gotmpl_document(template_src)
        .ok_or_eyre("template parse returned None")?;

    let uses = vyt::VYT::new(template_src.to_string())
        .with_defines(std::sync::Arc::clone(&defs))
        .run(&parsed.tree);

    let actual = serde_json::to_value(&uses).unwrap();

    // Hardcoded expected IR from the tree-sitter VYT interpreter.
    //
    // Differences vs the fused IR (see bitnami_redis_prometheusrule_fused_ir):
    //   - Guard uses do NOT carry parent guards (guards: [] for all guard entries).
    //   - VYT extracts each level of selector chains separately ("metrics",
    //     "metrics.prometheusRule" in addition to "metrics.prometheusRule.namespace").
    //   - VYT's Shape tracker keeps better context for conditional blocks: annotations
    //     is correctly at ["metadata","annotations"] instead of ["annotations"].
    //   - additionalLabels include is at ["metadata"] instead of [].
    //   - nameOverride from inlined common.names.fullname produces more guard variants.
    //   - nameOverride at labels is ["metadata","labels"] (VYT doesn't descend into
    //     the inlined template's YAML structure for that label key).
    let expected = serde_json::json!([
        {
            "source_expr": "commonAnnotations",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "commonAnnotations",
            "path": ["metadata", "annotations"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "commonAnnotations"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "commonLabels",
            "path": ["metadata", "labels"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "fullnameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "fullnameOverride",
            "path": ["spec", "groups[*]", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.additionalLabels",
            "path": ["metadata"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "metrics.prometheusRule.additionalLabels"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.enabled",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": null
        },
        {
            "source_expr": "metrics.prometheusRule.namespace",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "metrics.prometheusRule.rules",
            "path": ["spec", "groups[*]", "rules"],
            "kind": "Fragment",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": [],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": [],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["metadata", "labels"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["spec", "groups[*]", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "nameOverride",
            "path": ["spec", "groups[*]", "name"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled", "fullnameOverride", "nameOverride"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        },
        {
            "source_expr": "namespaceOverride",
            "path": ["metadata", "namespace"],
            "kind": "Scalar",
            "guards": ["metrics.enabled", "metrics.prometheusRule.enabled"],
            "resource": { "api_version": "monitoring.coreos.com/v1", "kind": "PrometheusRule" }
        }
    ]);

    similar_asserts::assert_eq!(actual, expected);

    Ok(())
}
