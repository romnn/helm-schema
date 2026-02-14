mod chart;
mod provider;
mod schema_override;

use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use color_eyre::eyre::{Result, WrapErr, eyre};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{Guard, IrGenerator, SymbolicIrGenerator, ValueUse};
use serde_json::Value;

pub use provider::ProviderOptions;

#[derive(Parser, Debug, Clone)]
#[command(name = "helm-schema", about = "Generate JSON schema for Helm values.yaml")]
pub struct Cli {
    #[arg(value_name = "CHART_DIR")]
    pub chart_dir: PathBuf,

    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub k8s: K8sArgs,

    #[command(flatten)]
    pub chart: ChartArgs,

    #[arg(long)]
    pub override_schema: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct OutputArgs {
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub compact: bool,
}

#[derive(Args, Debug, Clone)]
pub struct K8sArgs {
    #[arg(long, default_value = "v1.35.0")]
    pub k8s_version: String,

    #[arg(long)]
    pub k8s_schema_cache_dir: Option<PathBuf>,

    #[arg(long)]
    pub allow_net: bool,

    #[arg(long)]
    pub disable_k8s_schemas: bool,

    #[arg(long)]
    pub crd_catalog_dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct ChartArgs {
    #[arg(long)]
    pub include_tests: bool,

    #[arg(long)]
    pub no_subchart_values: bool,
}

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: PathBuf,
    pub include_tests: bool,
    pub include_subchart_values: bool,
    pub provider: ProviderOptions,
}

/// Run the CLI.
///
/// # Errors
///
/// Returns an error if chart discovery fails, a template/values file cannot be
/// read/parsed, the schema cannot be generated, or output cannot be written.
pub fn run(cli: Cli) -> Result<()> {
    let opts = GenerateOptions {
        chart_dir: cli.chart_dir.clone(),
        include_tests: cli.chart.include_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        provider: ProviderOptions {
            k8s_version: cli.k8s.k8s_version,
            k8s_schema_cache_dir: cli.k8s.k8s_schema_cache_dir,
            allow_net: cli.k8s.allow_net,
            disable_k8s_schemas: cli.k8s.disable_k8s_schemas,
            crd_catalog_dir: cli.k8s.crd_catalog_dir,
        },
    };

    let mut schema = generate_values_schema_for_chart(&opts)?;

    if let Some(path) = cli.override_schema {
        let override_schema = load_json_file(&path)
            .wrap_err_with(|| format!("read override schema: {}", path.display()))?;
        schema = schema_override::apply_schema_override(schema, override_schema);
    }

    let json = if cli.output.compact {
        serde_json::to_vec(&schema).wrap_err("serialize schema json")?
    } else {
        serde_json::to_vec_pretty(&schema).wrap_err("serialize schema json")?
    };

    if let Some(path) = cli.output.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("create output directory: {}", parent.display()))?;
        }
        std::fs::write(&path, json).wrap_err_with(|| format!("write output: {}", path.display()))?;
    } else {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        out.write_all(&json).wrap_err("write stdout")?;
        out.write_all(b"\n").wrap_err("write stdout")?;
    }

    Ok(())
}

/// Generate a values JSON schema for a full Helm chart.
///
/// This walks the root chart and all vendored dependencies under `charts/`,
/// collects `.Values.*` usages across all manifest templates, and produces a
/// single JSON schema.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart(opts: &GenerateOptions) -> Result<Value> {
    let discovery = chart::discover_chart_contexts(&opts.chart_dir)
        .wrap_err_with(|| format!("discover charts: {}", opts.chart_dir.display()))?;
    let charts = &discovery.charts;

    let defines = chart::build_define_index(charts, opts.include_tests)
        .wrap_err("build define index")?;

    let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)
        .wrap_err("read values.yaml")?;

    let uses = collect_ir_for_charts(charts, &defines, opts.include_tests)
        .wrap_err("collect IR")?;

    let provider = provider::build_provider(&opts.provider).wrap_err("build k8s provider")?;

    Ok(generate_values_schema_with_values_yaml(
        &uses,
        provider.as_ref(),
        values_yaml.as_deref(),
    ))
}

fn collect_ir_for_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
) -> Result<Vec<ValueUse>> {
    let mut out: Vec<ValueUse> = Vec::new();

    for c in charts {
        if c.is_library {
            continue;
        }

        let manifests = chart::list_manifest_templates(&c.chart_dir, include_tests)
            .wrap_err_with(|| format!("list templates: {}", c.chart_dir.display()))?;

        for path in manifests {
            let src = std::fs::read_to_string(&path)
                .wrap_err_with(|| format!("read template: {}", path.display()))?;

            let ast = TreeSitterParser
                .parse(&src)
                .map_err(|e| eyre!("parse {}: {e}", path.display()))?;

            let uses = SymbolicIrGenerator.generate(&src, &ast, defines);
            for u in uses {
                out.push(scope_value_use(u, &c.values_prefix));
            }
        }
    }

    Ok(out)
}

fn scope_value_use(mut u: ValueUse, prefix: &[String]) -> ValueUse {
    if prefix.is_empty() {
        return u;
    }

    u.source_expr = scope_values_path(&u.source_expr, prefix);
    u.guards = u
        .guards
        .into_iter()
        .map(|g| scope_guard(g, prefix))
        .collect();

    u
}

fn scope_guard(g: Guard, prefix: &[String]) -> Guard {
    match g {
        Guard::Truthy { path } => Guard::Truthy {
            path: scope_values_path(&path, prefix),
        },
        Guard::Not { path } => Guard::Not {
            path: scope_values_path(&path, prefix),
        },
        Guard::Eq { path, value } => Guard::Eq {
            path: scope_values_path(&path, prefix),
            value,
        },
        Guard::Or { paths } => Guard::Or {
            paths: paths
                .into_iter()
                .map(|p| scope_values_path(&p, prefix))
                .collect(),
        },
    }
}

fn scope_values_path(path: &str, prefix: &[String]) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }

    if path == "global" || path.starts_with("global.") {
        return path.to_string();
    }

    let pfx = prefix.join(".");
    format!("{pfx}.{path}")
}

fn load_json_file(path: &Path) -> Result<Value> {
    let bytes = std::fs::read(path).wrap_err("read file")?;
    let v: Value = serde_json::from_slice(&bytes).wrap_err("parse json")?;
    Ok(v)
}
