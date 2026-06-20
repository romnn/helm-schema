#![allow(dead_code)]

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_ast::extract_values_yaml_descriptions;
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use tempfile::TempDir;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

#[derive(Debug, Clone, Copy)]
pub struct HelmValidationSample<'a> {
    pub name: &'a str,
    pub values_yaml: Option<&'a str>,
}

fn chart_dir(chart_relative_path: &str) -> VfsPath {
    let chart_dir = physical_chart_dir(chart_relative_path);
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str))
}

fn physical_chart_dir(chart_relative_path: &str) -> PathBuf {
    test_util::workspace_testdata()
        .join("charts")
        .join(chart_relative_path)
}

fn read_values_yaml(chart_dir: &VfsPath) -> std::result::Result<String, Report> {
    let mut out = String::new();
    chart_dir
        .join("values.yaml")
        .wrap_err("join values.yaml")?
        .open_file()
        .wrap_err("open values.yaml")?
        .read_to_string(&mut out)
        .wrap_err("read values.yaml")?;
    Ok(out)
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

fn validate_json_against_schema(instance: &Value, schema: &Value) -> Vec<String> {
    let validator = match jsonschema::validator_for(schema) {
        Ok(validator) => validator,
        Err(err) => return vec![format!("failed to compile JSON schema: {err}")],
    };

    validator
        .iter_errors(instance)
        .map(|e| format!("{path}: {msg}", path = e.instance_path(), msg = e))
        .collect()
}

pub fn generate_chart_schema_for_path(
    chart_relative_path: &str,
) -> std::result::Result<Value, Report> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = chart_dir(chart_relative_path);

    let opts = GenerateOptions {
        chart_dir: chart_dir.clone(),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            disable_k8s_schemas: false,
            crd_override_dir: Some(test_util::workspace_root().join(".cache/crds-catalog-cache")),
            ..Default::default()
        },
    };

    generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")
}

pub fn generate_chart_schema(chart: &str) -> std::result::Result<Value, Report> {
    generate_chart_schema_for_path(chart)
}

pub fn assert_chart_values_yaml_validates(chart: &str) -> std::result::Result<(), Report> {
    assert_chart_values_yaml_validates_for_path(chart)
}

pub fn assert_chart_values_yaml_validates_for_path(
    chart_relative_path: &str,
) -> std::result::Result<(), Report> {
    let chart_dir = chart_dir(chart_relative_path);
    let schema = generate_chart_schema_for_path(chart_relative_path)?;

    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    let values_json = drop_nulls(&values_json);

    assert_values_json_validates(&values_json, &schema);

    Ok(())
}

pub fn values_yaml_as_json(chart: &str) -> std::result::Result<Value, Report> {
    values_yaml_as_json_for_path(chart)
}

pub fn values_yaml_as_json_for_path(
    chart_relative_path: &str,
) -> std::result::Result<Value, Report> {
    let chart_dir = chart_dir(chart_relative_path);
    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let values_json: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    Ok(drop_nulls(&values_json))
}

pub fn assert_values_json_validates(values_json: &Value, schema: &Value) {
    let errors = validate_json_against_schema(values_json, schema);
    sim_assert_eq!(have: errors, want: Vec::<String>::new());
}

pub fn assert_chart_values_comments_apply_to_existing_schema_paths(
    chart: &str,
    schema: &Value,
    min_applied: usize,
) -> std::result::Result<(), Report> {
    let chart_dir = chart_dir(chart);
    let values_yaml = read_values_yaml(&chart_dir).wrap_err("read values.yaml")?;
    let descriptions =
        extract_values_yaml_descriptions(&values_yaml).wrap_err("extract values comments")?;

    let mut applied = 0;
    for (path, expected_description) in descriptions {
        let Some(schema_node) = schema_node_for_values_path(schema, &path) else {
            continue;
        };
        applied += 1;
        sim_assert_eq!(
            have: schema_node
                .get("description")
                .and_then(serde_json::Value::as_str),
            want: Some(expected_description.as_str()),
            "description mismatch for values path {path}"
        );
    }

    assert!(
        applied >= min_applied,
        "expected at least {min_applied} values comments to apply for {chart}, got {applied}"
    );
    Ok(())
}

fn schema_node_for_values_path<'schema>(
    schema: &'schema Value,
    path: &str,
) -> Option<&'schema Value> {
    let mut current = schema;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = if segment == "*" {
            current.get("items")?
        } else {
            current.get("properties")?.get(segment)?
        };
    }
    Some(current)
}

pub fn assert_generated_schema_accepts_helm_samples_for_path(
    chart_relative_path: &str,
    schema: &Value,
    samples: &[HelmValidationSample<'_>],
) -> std::result::Result<(), Report> {
    let temp_chart = GeneratedSchemaHelmChart::new(chart_relative_path, schema)?;

    for sample in samples {
        let values_file = temp_chart.write_sample_values(sample)?;
        run_helm_lint(temp_chart.chart_dir(), values_file.as_deref()).wrap_err_with(|| {
            format!(
                "helm lint must accept sample '{}' for {}",
                sample.name, chart_relative_path
            )
        })?;
        run_helm_template(temp_chart.chart_dir(), values_file.as_deref()).wrap_err_with(|| {
            format!(
                "helm template must render sample '{}' for {}",
                sample.name, chart_relative_path
            )
        })?;
    }

    Ok(())
}

pub fn assert_generated_schema_accepts_helm_samples(
    chart: &str,
    schema: &Value,
    samples: &[HelmValidationSample<'_>],
) -> std::result::Result<(), Report> {
    assert_generated_schema_accepts_helm_samples_for_path(chart, schema, samples)
}

struct GeneratedSchemaHelmChart {
    _temp_dir: TempDir,
    chart_dir: PathBuf,
}

impl GeneratedSchemaHelmChart {
    fn new(chart_relative_path: &str, schema: &Value) -> std::result::Result<Self, Report> {
        let temp_dir = tempfile::tempdir().wrap_err("create temp dir for helm validation")?;
        let chart_dir = temp_dir.path().join("chart");
        copy_chart_tree(&physical_chart_dir(chart_relative_path), &chart_dir)
            .wrap_err("copy chart fixture into temp dir")?;
        fs::write(
            chart_dir.join("values.schema.json"),
            serde_json::to_vec_pretty(schema).wrap_err("serialize generated schema")?,
        )
        .wrap_err("write generated values.schema.json")?;
        Ok(Self {
            _temp_dir: temp_dir,
            chart_dir,
        })
    }

    fn chart_dir(&self) -> &Path {
        &self.chart_dir
    }

    fn write_sample_values(
        &self,
        sample: &HelmValidationSample<'_>,
    ) -> std::result::Result<Option<PathBuf>, Report> {
        let Some(values_yaml) = sample.values_yaml else {
            return Ok(None);
        };

        let sample_file = self.chart_dir.join(format!(
            "sample-{}.values.yaml",
            sanitize_sample_name(sample.name)
        ));
        fs::write(&sample_file, values_yaml).wrap_err("write helm validation sample values")?;
        Ok(Some(sample_file))
    }
}

fn copy_chart_tree(from: &Path, to: &Path) -> std::result::Result<(), Report> {
    let metadata =
        fs::metadata(from).wrap_err_with(|| format!("read metadata for {}", from.display()))?;
    if metadata.is_dir() {
        fs::create_dir_all(to).wrap_err_with(|| format!("create directory {}", to.display()))?;
        for entry in
            fs::read_dir(from).wrap_err_with(|| format!("read directory {}", from.display()))?
        {
            let entry = entry.wrap_err("read chart directory entry")?;
            copy_chart_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)
                .wrap_err_with(|| format!("create directory {}", parent.display()))?;
        }
        fs::copy(from, to)
            .wrap_err_with(|| format!("copy {} -> {}", from.display(), to.display()))?;
    }

    Ok(())
}

fn sanitize_sample_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches('-');
    if sanitized.is_empty() {
        "sample".to_string()
    } else {
        sanitized.to_string()
    }
}

fn run_helm_lint(chart_dir: &Path, values_file: Option<&Path>) -> std::result::Result<(), Report> {
    let mut command = Command::new("helm");
    command.arg("lint").arg(chart_dir);
    if let Some(values_file) = values_file {
        command.arg("-f").arg(values_file);
    }
    run_helm_command(command, "helm lint")
}

fn run_helm_template(
    chart_dir: &Path,
    values_file: Option<&Path>,
) -> std::result::Result<(), Report> {
    let mut command = Command::new("helm");
    command.arg("template").arg("test-release").arg(chart_dir);
    if let Some(values_file) = values_file {
        command.arg("-f").arg(values_file);
    }
    run_helm_command(command, "helm template")
}

fn run_helm_command(mut command: Command, label: &str) -> std::result::Result<(), Report> {
    let output = command
        .output()
        .wrap_err_with(|| format!("run {label} command"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(Report::msg(format!(
        "{label} failed with status {}:\nstderr: {stderr}\nstdout: {stdout}",
        output.status
    )))
}
