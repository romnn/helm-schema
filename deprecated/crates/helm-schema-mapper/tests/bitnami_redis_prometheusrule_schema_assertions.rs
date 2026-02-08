use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_mapper::fused_ir::{self, FusedDefineIndex};
use helm_schema_mapper::schema::{
    UpstreamThenDefaultVytSchemaProvider, generate_values_schema_vyt,
};
use helm_schema_mapper::vyt;
use indoc::indoc;
use std::path::PathBuf;
use test_util::prelude::*;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const TEMPLATE_SRC: &str = indoc! {r#"
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

// ---------------------------------------------------------------------------
// Shared helpers for building define indices.
// ---------------------------------------------------------------------------

fn load_fused_defines() -> FusedDefineIndex {
    let mut defs = FusedDefineIndex::default();

    let helpers = std::fs::read_to_string(
        crate_root().join("testdata/charts/bitnami-redis/templates/_helpers.tpl"),
    )
    .expect("read _helpers.tpl");
    defs.add_source(&helpers).expect("parse _helpers.tpl");

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

fn schema_provider() -> UpstreamThenDefaultVytSchemaProvider {
    UpstreamThenDefaultVytSchemaProvider::default()
}

// ---------------------------------------------------------------------------
// Test 1: Fused IR → JSON Schema
// ---------------------------------------------------------------------------

#[test]
fn bitnami_redis_prometheusrule_fused_schema() {
    let defines = load_fused_defines();
    let tree = yaml_rust::parse_fused_yaml_helm(TEMPLATE_SRC).expect("fused parse");
    let uses = fused_ir::generate_fused_ir(&tree, &defines);

    let provider = schema_provider();
    let actual = generate_values_schema_vyt(&uses, &provider);

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "type": "object",
        "properties": {
            "commonAnnotations": {
                "anyOf": [
                    { "additionalProperties": {}, "type": "object" },
                    { "type": "string" }
                ]
            },
            "commonLabels": {
                "additionalProperties": { "type": "string" },
                "type": "object"
            },
            "fullnameOverride": { "type": "string" },
            "metrics": {
                "additionalProperties": false,
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean" },
                    "prometheusRule": {
                        "additionalProperties": false,
                        "type": "object",
                        "properties": {
                            "additionalLabels": {
                                "anyOf": [
                                    { "additionalProperties": {}, "type": "object" },
                                    { "type": "string" }
                                ]
                            },
                            "enabled": { "type": "boolean" },
                            "namespace": { "type": "string" },
                            "rules": { "additionalProperties": {}, "type": "object" }
                        }
                    }
                }
            },
            "nameOverride": { "type": "string" },
            "namespaceOverride": { "type": "string" }
        }
    });

    similar_asserts::assert_eq!(actual, expected);
}

// ---------------------------------------------------------------------------
// Test 2: VYT (tree-sitter) IR → JSON Schema
// ---------------------------------------------------------------------------

#[test]
fn bitnami_redis_prometheusrule_vyt_schema() -> eyre::Result<()> {
    Builder::default().build();

    let defs = build_vyt_define_index()?;
    let parsed = helm_schema_template::parse::parse_gotmpl_document(TEMPLATE_SRC)
        .ok_or_eyre("template parse returned None")?;

    let uses = vyt::VYT::new(TEMPLATE_SRC.to_string())
        .with_defines(std::sync::Arc::clone(&defs))
        .run(&parsed.tree);

    let provider = schema_provider();
    let actual = generate_values_schema_vyt(&uses, &provider);

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "type": "object",
        "properties": {
            "commonAnnotations": {
                "anyOf": [
                    { "additionalProperties": { "type": "string" }, "type": "object" },
                    { "type": "string" }
                ]
            },
            "commonLabels": {
                "additionalProperties": { "type": "string" },
                "type": "object"
            },
            "fullnameOverride": { "type": "string" },
            "metrics": {
                "additionalProperties": false,
                "type": "object",
                "properties": {
                    "enabled": { "type": "boolean" },
                    "prometheusRule": {
                        "additionalProperties": false,
                        "type": "object",
                        "properties": {
                            "additionalLabels": {
                                "anyOf": [
                                    { "additionalProperties": {}, "type": "object" },
                                    { "type": "string" }
                                ]
                            },
                            "enabled": { "type": "boolean" },
                            "namespace": { "type": "string" },
                            "rules": { "additionalProperties": {}, "type": "object" }
                        }
                    }
                }
            },
            "nameOverride": {
                "anyOf": [
                    { "additionalProperties": { "type": "string" }, "type": "object" },
                    { "type": "string" }
                ]
            },
            "namespaceOverride": { "type": "string" }
        }
    });

    similar_asserts::assert_eq!(actual, expected);

    Ok(())
}
