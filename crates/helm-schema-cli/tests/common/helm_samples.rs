use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Report, WrapErr};
use serde_json::Value;
use tempfile::TempDir;

#[derive(Debug, Clone, Copy)]
pub struct HelmValidationSample<'a> {
    pub name: &'a str,
    pub values_yaml: Option<&'a str>,
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

struct GeneratedSchemaHelmChart {
    _temp_dir: TempDir,
    chart_dir: PathBuf,
}

impl GeneratedSchemaHelmChart {
    fn new(chart_relative_path: &str, schema: &Value) -> std::result::Result<Self, Report> {
        let temp_dir = tempfile::tempdir().wrap_err("create temp dir for helm validation")?;
        let chart_dir = temp_dir.path().join("chart");
        copy_chart_tree(
            &crate::schema_roundtrip::physical_chart_dir(chart_relative_path),
            &chart_dir,
        )
        .wrap_err("copy chart fixture into temp dir")?;
        // Mirror the CLI's default output policy (pretty, degrading to
        // compact past Helm's chart-file limit) so `helm lint` sees the
        // same file a real generation run would ship.
        let mut bytes = Vec::new();
        helm_schema::output::write_schema_json(
            &mut bytes,
            schema,
            helm_schema::output::JsonOutputFormat::Pretty,
        )
        .wrap_err("serialize generated schema")?;
        fs::write(chart_dir.join("values.schema.json"), bytes)
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
        let path = self
            .chart_dir
            .join(format!("{}.values.yaml", sanitize_sample_name(sample.name)));
        fs::write(&path, values_yaml).wrap_err("write sample values file")?;
        Ok(Some(path))
    }
}

fn copy_chart_tree(from: &Path, to: &Path) -> std::result::Result<(), Report> {
    fs::create_dir_all(to).wrap_err("create chart destination dir")?;
    for entry in fs::read_dir(from).wrap_err("read chart source dir")? {
        let entry = entry.wrap_err("read chart source entry")?;
        let file_type = entry.file_type().wrap_err("read chart source file type")?;
        let source = entry.path();
        let dest = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_chart_tree(&source, &dest)?;
        } else if file_type.is_file() {
            fs::copy(&source, &dest).wrap_err("copy chart file")?;
        }
    }
    Ok(())
}

fn sanitize_sample_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
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
        .wrap_err_with(|| format!("spawn {label}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(Report::msg(format!(
        "{label} failed with status {}:\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )))
}
