pub mod cases;

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_core::YamlPath;
use helm_schema_ir::ResourceRef;
use helm_schema_k8s::K8sSchemaProvider;
use helm_schema_test_support::generate::{self, bundled_crd_provider, bundled_k8s_provider};
use helm_schema_test_support::registry::{ArtifactId, ArtifactTarget, TemplateId};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy)]
pub struct HelmRenderCase<'a> {
    pub name: &'a str,
    pub chart_path: &'a str,
    pub show_only: Option<&'a str>,
    pub extra_args: &'a [&'a str],
}

#[derive(Clone, Copy)]
pub enum RenderedSchemaProviderKind<'a> {
    K8s(&'a str),
    CrdCatalog,
}

#[derive(Clone, Copy)]
pub struct RenderedManifestValidationCase<'a> {
    pub render: HelmRenderCase<'a>,
    pub provider: RenderedSchemaProviderKind<'a>,
}

#[derive(Clone, Copy)]
pub struct SchemaExpectation<'a> {
    pub instance: &'a str,
    pub accepted: bool,
    pub message: &'a str,
}

#[derive(Clone, Copy)]
pub struct SchemaBehaviorCase<'a> {
    pub schema_case: TemplateId,
    pub expectations: &'a [SchemaExpectation<'a>],
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
/// Returns the top-level mapping as a JSON object.
pub fn values_yaml_to_json(values_yaml: &str) -> eyre::Result<Value> {
    serde_yaml::from_str(values_yaml).wrap_err("parse values.yaml as JSON")
}

/// The chart values a template case's schema is generated over.
fn registered_values_yaml(id: TemplateId) -> eyre::Result<String> {
    let recipe = id.case().recipe;
    match recipe.inline_values {
        Some(values_yaml) => Ok(values_yaml.to_string()),
        None => test_util::read_testdata(recipe.values_path),
    }
}

pub fn assert_schema_fixture(id: TemplateId) -> eyre::Result<()> {
    let actual = generate::template_schema(&id.case().recipe)?;
    let spec = ArtifactId::Template(id).spec();
    let ArtifactTarget::Fixture(fixture) = &spec.target else {
        eyre::bail!("{id:?} is registered without a fixture");
    };
    let fixture_path = test_util::workspace_root().join(fixture);
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(&fixture_path)
            .wrap_err_with(|| format!("read {}", fixture_path.display()))?,
    )
    .wrap_err_with(|| format!("parse expected schema fixture {}", fixture_path.display()))?;
    sim_assert_eq!(
        have: actual,
        want: expected,
        "schema fixture mismatch for {}",
        spec.dump_name,
    );
    Ok(())
}

pub fn assert_values_yaml_validates(id: TemplateId) -> eyre::Result<()> {
    let recipe = id.case().recipe;
    let values_yaml = test_util::read_testdata(recipe.values_path)?;
    let schema = generate::template_schema_with_values(&recipe, &values_yaml)?;
    let errors = validate_values_yaml(&values_yaml, &schema)?;
    color_eyre::eyre::ensure!(
        errors.is_empty(),
        "values.yaml failed schema validation with {} error(s):\n{}",
        errors.len(),
        errors.join("\n")
    );
    Ok(())
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
pub fn validate_values_yaml(values_yaml: &str, schema: &Value) -> eyre::Result<Vec<String>> {
    let json_values = drop_nulls(&values_yaml_to_json(values_yaml)?);
    let relaxed = relax_schema(schema);
    Ok(validate_json_against_schema(&json_values, &relaxed))
}

pub fn helm_template_render_with_args(
    chart_dir: &Path,
    show_only: Option<&str>,
    extra_args: &[&str],
) -> eyre::Result<String> {
    let mut cmd = Command::new("helm");
    cmd.envs(test_util::scratch::temp_env()?);
    cmd.arg("template").arg("test-release").arg(chart_dir);

    if let Some(template) = show_only {
        cmd.arg("--show-only").arg(template);
    }
    for arg in extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().wrap_err("run helm template")?;

    if output.status.success() {
        String::from_utf8(output.stdout).wrap_err("decode helm output as UTF-8")
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // helm template prints errors to stdout sometimes
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(eyre::eyre!(
            "helm template failed:\nstderr: {stderr}\nstdout: {stdout}"
        ))
    }
}

pub fn render_helm_case(case: &HelmRenderCase<'_>) -> eyre::Result<String> {
    let chart_dir = test_util::workspace_testdata().join(case.chart_path);
    helm_template_render_with_args(&chart_dir, case.show_only, case.extra_args)
}

pub fn assert_helm_render_case(case: &HelmRenderCase<'_>) -> eyre::Result<()> {
    let rendered = render_helm_case(case)
        .wrap_err_with(|| format!("render Helm corpus case {}", case.name))?;
    color_eyre::eyre::ensure!(
        !rendered.is_empty(),
        "helm render produced empty YAML for {}",
        case.name
    );
    Ok(())
}

pub fn assert_schema_behavior_case(case: &SchemaBehaviorCase<'_>) -> eyre::Result<()> {
    let schema = generate::template_schema(&case.schema_case.case().recipe)?;
    // Expectations are sparse OVERRIDES over the chart's declared defaults:
    // helm validates the coalesced document, so a template that navigates
    // `.Values.x.y` aborts on a document missing `x` — the state a user's
    // `null` deletion produces, which is pinned separately.
    let defaults = registered_values_yaml(case.schema_case)?;
    let defaults: Value = serde_yaml::from_str::<serde_yaml::Value>(&defaults)
        .ok()
        .and_then(|doc| serde_json::to_value(doc).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    for expectation in case.expectations {
        let overrides: Value = serde_json::from_str(expectation.instance)
            .wrap_err_with(|| format!("parse behavior JSON: {}", expectation.message))?;
        let mut instance = defaults.clone();
        merge_composed_override(&mut instance, overrides);
        let accepted = schema_accepts_instance(&schema, &instance);
        sim_assert_eq!(
            have: accepted,
            want: expectation.accepted,
            "{}: {}. schema={schema}",
            case.schema_case.case().dump_stem, expectation.message
        );
    }
    Ok(())
}

pub fn parse_yaml_documents(yaml: &str) -> eyre::Result<Vec<Value>> {
    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        let value = Value::deserialize(doc).wrap_err("parse rendered YAML document as JSON")?;
        if value.is_null() {
            continue;
        }
        out.push(value);
    }
    Ok(out)
}

pub fn assert_rendered_manifest_validation_case(
    case: &RenderedManifestValidationCase<'_>,
) -> eyre::Result<()> {
    let rendered_yaml = render_helm_case(&case.render)
        .wrap_err_with(|| format!("render Helm corpus case {}", case.render.name))?;
    let docs = parse_yaml_documents(&rendered_yaml)?;
    color_eyre::eyre::ensure!(
        !docs.is_empty(),
        "rendered YAML contained no documents for {}",
        case.render.name
    );

    for doc in docs {
        let api_version = doc
            .get("apiVersion")
            .and_then(|value| value.as_str())
            .ok_or_eyre("rendered manifest missing apiVersion")?;
        let kind = doc
            .get("kind")
            .and_then(|value| value.as_str())
            .ok_or_eyre("rendered manifest missing kind")?;
        let resource = ResourceRef::concrete(api_version.to_string(), kind.to_string());
        let schema = materialized_schema_for_rendered_resource(case.provider, &resource)
            .ok_or_eyre(format!("load schema for rendered {api_version}/{kind}"))?;
        let errors = validate_json_against_schema(&doc, &schema);
        color_eyre::eyre::ensure!(
            errors.is_empty(),
            "rendered {api_version}/{kind} for {} failed schema validation with {} error(s):\n{}",
            case.render.name,
            errors.len(),
            errors.join("\n")
        );
    }
    Ok(())
}

fn materialized_schema_for_rendered_resource(
    provider: RenderedSchemaProviderKind<'_>,
    resource: &ResourceRef,
) -> Option<Value> {
    let schema = match provider {
        RenderedSchemaProviderKind::K8s(version) => {
            materialize_provider_root_schema(&bundled_k8s_provider(version), resource)?
        }
        RenderedSchemaProviderKind::CrdCatalog => {
            materialize_provider_root_schema(&bundled_crd_provider(), resource)?
        }
    };

    Some(match schema {
        Value::Object(mut object) => {
            let _ = object.remove("$schema");
            Value::Object(object)
        }
        other => other,
    })
}

fn materialize_provider_root_schema(
    provider: &impl K8sSchemaProvider,
    resource: &ResourceRef,
) -> Option<Value> {
    provider
        .lookup(resource, &YamlPath(Vec::new()))
        .into_schema_fragment()
        .map(helm_schema_core::ProviderSchemaFragment::into_schema)
}

/// Merge `overrides` into `base` the way helm coalesces user values: map
/// keys merge member-wise and a null override DELETES its key.
pub fn merge_composed_override(base: &mut Value, overrides: Value) {
    let (Some(base), Value::Object(overrides)) = (base.as_object_mut(), overrides) else {
        return;
    };
    for (key, value) in overrides {
        if value.is_null() {
            base.remove(&key);
        } else if base.get(&key).is_some_and(Value::is_object) && value.is_object() {
            if let Some(existing) = base.get_mut(&key) {
                merge_composed_override(existing, value);
            }
        } else {
            base.insert(key, value);
        }
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
