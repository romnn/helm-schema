#![allow(dead_code)]

pub mod cases;

use helm_schema_ast::{DefineIndex, HelmParser};
use helm_schema_core::ResourceSchemaOracle;
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::ContractIr;
use helm_schema_k8s::{Chain, CrdsCatalogSchemaProvider, KubernetesJsonSchemaProvider};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use test_util::prelude::sim_assert_eq;

pub fn build_define_index(
    parser: &dyn HelmParser,
    spec: test_util::DefineSourceSpec<'_>,
    helper_parse_mode: HelperParseMode,
) -> DefineIndex {
    let loaded = spec.load();
    let mut idx = DefineIndex::new();
    for source in loaded.helper_templates {
        let result = idx.add_source(parser, &source);
        if helper_parse_mode == HelperParseMode::Strict {
            result.expect("helper source should parse");
        }
    }
    for (name, source) in loaded.file_sources {
        idx.add_file_source(&name, &source);
    }
    idx
}

#[derive(Clone, Copy)]
pub enum ProviderKind<'a> {
    K8s(&'a str),
    CrdK8s(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperParseMode {
    Lenient,
    Strict,
}

#[derive(Clone, Copy)]
pub struct SchemaCorpusCase<'a> {
    pub template_path: &'a str,
    pub values_path: &'a str,
    pub fixture_values_yaml: Option<&'a str>,
    pub expected_fixture: &'a str,
    pub define_sources: test_util::DefineSourceSpec<'a>,
    pub provider: ProviderKind<'a>,
    pub helper_parse_mode: HelperParseMode,
    pub dump_stem: &'a str,
}

/// Production-like K8s provider path for chart-level generator tests.
///
/// These tests are meant to approximate what end users run through the CLI,
/// so they use the chain layer plus apiVersion inference instead of the older
/// single-provider shortcut.
pub fn production_k8s_chain(version: &str) -> Chain {
    let k8s_provider = KubernetesJsonSchemaProvider::new(version.to_string())
        .with_allow_download(true)
        .with_api_version_guess(true);
    Chain::new(vec![Box::new(k8s_provider)]).with_inference_enabled(true)
}

/// Production-like CRD + K8s provider path for chart-level generator tests.
///
/// This keeps real-world CRD-consuming chart tests on the same resolution path
/// as the CLI while leaving lower-layer provider-specific tests free to pin a
/// single provider when that is the actual subject under test.
pub fn production_crd_k8s_chain(version: &str) -> Chain {
    let crds = CrdsCatalogSchemaProvider::new().with_allow_download(true);
    let k8s_provider = KubernetesJsonSchemaProvider::new(version.to_string())
        .with_allow_download(true)
        .with_api_version_guess(true);
    Chain::new(vec![Box::new(crds), Box::new(k8s_provider)]).with_inference_enabled(true)
}

/// Recursively remove `"additionalProperties": false` from a JSON schema.
///
/// Our generated schemas are per-template and use `additionalProperties: false`
/// to flag unknown keys. However, the chart's `values.yaml` contains values for
/// ALL templates, so a per-template schema will reject keys it doesn't cover.
/// Relaxing the schema lets us validate that the types/structure of the values
/// we *do* cover are correct without false positives from unrelated keys.
pub fn relax_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if k == "additionalProperties" && *v == Value::Bool(false) {
                    continue;
                }
                out.insert(k.clone(), relax_schema(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(relax_schema).collect()),
        other => other.clone(),
    }
}

/// Parse a `values.yaml` string into a [`serde_json::Value`].
///
/// Returns the top-level mapping as a JSON object, or panics on parse failure.
pub fn values_yaml_to_json(values_yaml: &str) -> Value {
    let yaml: Value = serde_yaml::from_str(values_yaml).expect("parse values.yaml as JSON");
    yaml
}

pub fn generate_schema_with_values_yaml(
    contract: ContractIr,
    provider: &dyn ResourceSchemaOracle,
    values_yaml: Option<&str>,
) -> Value {
    let schema_signals = contract.into_schema_signals();
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, provider).with_values_yaml(values_yaml),
    )
}

pub fn render_schema_case(case: &SchemaCorpusCase<'_>) -> Value {
    match case.fixture_values_yaml {
        Some(values_yaml) => render_schema_case_with_values(case, values_yaml),
        None => {
            let values_yaml = test_util::read_testdata(case.values_path);
            render_schema_case_with_values(case, &values_yaml)
        }
    }
}

pub fn render_schema_case_with_values(case: &SchemaCorpusCase<'_>, values_yaml: &str) -> Value {
    let src = test_util::read_testdata(case.template_path);
    let idx = build_define_index(
        &helm_schema_ast::TreeSitterParser,
        case.define_sources,
        case.helper_parse_mode,
    );
    let ir = helm_schema_ir::SymbolicIrContext::new(&idx).generate_contract_ir(&src, &idx);
    let provider = match case.provider {
        ProviderKind::K8s(version) => production_k8s_chain(version),
        ProviderKind::CrdK8s(version) => production_crd_k8s_chain(version),
    };
    let schema = generate_schema_with_values_yaml(ir, &provider, Some(values_yaml));

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&schema).expect("pretty json")
        );
        let path = std::env::temp_dir().join(format!("helm-schema.{}.schema.json", case.dump_stem));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&schema).expect("json bytes"),
        )
        .expect("write schema dump");
    }

    schema
}

pub fn assert_schema_fixture(case: &SchemaCorpusCase<'_>) {
    let actual = render_schema_case(case);
    let expected: Value =
        serde_json::from_str(case.expected_fixture).expect("expected schema json");
    sim_assert_eq!(
        have: actual,
        want: expected,
        "schema fixture mismatch for {}",
        case.dump_stem,
    );
}

pub fn assert_values_yaml_validates(case: &SchemaCorpusCase<'_>) {
    let values_yaml = test_util::read_testdata(case.values_path);
    let schema = render_schema_case_with_values(case, &values_yaml);
    let errors = validate_values_yaml(&values_yaml, &schema);
    assert!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
}

fn drop_nulls(v: &Value) -> Value {
    match v {
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => v.clone(),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .filter(|x| !x.is_null())
                .map(drop_nulls)
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if v.is_null() {
                    continue;
                }
                out.insert(k.clone(), drop_nulls(v));
            }
            Value::Object(out)
        }
    }
}

/// Validate a JSON value against a JSON schema.
///
/// Returns a list of human-readable validation error strings.
/// An empty list means validation passed.
pub fn validate_json_against_schema(instance: &Value, schema: &Value) -> Vec<String> {
    let Ok(validator) = jsonschema::validator_for(schema) else {
        return vec!["failed to compile JSON schema".to_string()];
    };
    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path(), msg = e))
        .collect()
}

pub fn schema_accepts_instance(schema: &Value, instance: &Value) -> bool {
    validate_json_against_schema(instance, schema).is_empty()
}

/// Validate a `values.yaml` string against a generated JSON schema.
///
/// The schema is first relaxed (removing `additionalProperties: false`) so that
/// values for other templates don't cause false positives. Returns a list of
/// validation errors (empty = pass).
pub fn validate_values_yaml(values_yaml: &str, schema: &Value) -> Vec<String> {
    let json_values = drop_nulls(&values_yaml_to_json(values_yaml));
    let relaxed = relax_schema(schema);
    validate_json_against_schema(&json_values, &relaxed)
}

/// Run `helm template` on a chart directory, optionally showing only a specific template.
///
/// Returns `Ok(rendered_yaml)` on success, or `Err(stderr)` on failure.
/// If `helm` is not installed or the chart can't be rendered, returns an error.
pub fn helm_template_render(chart_dir: &Path, show_only: Option<&str>) -> Result<String, String> {
    helm_template_render_with_args(chart_dir, show_only, &[])
}

pub fn helm_template_render_with_args(
    chart_dir: &Path,
    show_only: Option<&str>,
    extra_args: &[&str],
) -> Result<String, String> {
    let mut cmd = Command::new("helm");
    cmd.arg("template").arg("test-release").arg(chart_dir);

    if let Some(template) = show_only {
        cmd.arg("--show-only").arg(template);
    }
    for arg in extra_args {
        cmd.arg(arg);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("failed to run helm: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout).map_err(|e| format!("helm output is not valid UTF-8: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // helm template prints errors to stdout sometimes
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "helm template failed:\nstderr: {stderr}\nstdout: {stdout}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn relax_removes_additional_properties_false() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "foo": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "bar": { "type": "string" }
                    }
                }
            }
        });
        let relaxed = relax_schema(&schema);
        sim_assert_eq!(
            have: relaxed,
            want: serde_json::json!({
                "type": "object",
                "properties": {
                    "foo": {
                        "type": "object",
                        "properties": {
                            "bar": { "type": "string" }
                        }
                    }
                }
            })
        );
    }

    #[test]
    fn relax_keeps_additional_properties_object() {
        let schema = serde_json::json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        });
        let relaxed = relax_schema(&schema);
        sim_assert_eq!(have: relaxed, want: schema);
    }
}
