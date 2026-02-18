mod chart;
mod error;
mod provider;
mod schema_override;

use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{Guard, IrGenerator, SymbolicIrGenerator, ValueUse};
use helm_schema_k8s::WarningSink;
use serde_json::Value;
use serde_yaml::Value as YamlValue;
use vfs::VfsPath;

use crate::error::CliResult;

pub use error::CliError;
pub use provider::ProviderOptions;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "helm-schema",
    about = "Generate JSON schema for Helm values.yaml"
)]
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
    pub offline: bool,

    #[arg(long)]
    pub no_k8s_schemas: bool,

    #[arg(long)]
    pub crd_catalog_dir: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct ChartArgs {
    #[arg(long)]
    pub exclude_tests: bool,

    #[arg(long)]
    pub no_subchart_values: bool,
}

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: VfsPath,
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
pub fn run(cli: Cli) -> CliResult<()> {
    let chart_dir_str = cli.chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let opts = GenerateOptions {
        chart_dir,
        include_tests: !cli.chart.exclude_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        provider: ProviderOptions {
            k8s_version: cli.k8s.k8s_version,
            k8s_schema_cache_dir: cli.k8s.k8s_schema_cache_dir,
            allow_net: !cli.k8s.offline,
            disable_k8s_schemas: cli.k8s.no_k8s_schemas,
            crd_catalog_dir: cli.k8s.crd_catalog_dir,
        },
    };

    let warnings: WarningSink = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut schema = generate_values_schema_for_chart_with_warnings(&opts, Some(&warnings))?;

    if let Some(path) = cli.override_schema {
        let override_schema = load_json_file(&path)?;
        schema = schema_override::apply_schema_override(schema, override_schema);
    }

    let json = if cli.output.compact {
        serde_json::to_vec(&schema)?
    } else {
        serde_json::to_vec_pretty(&schema)?
    };

    if let Ok(guard) = warnings.lock() {
        for w in guard.iter() {
            eprintln!("warning: {w}");
        }
    }

    if let Some(path) = cli.output.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::CreateOutputDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        std::fs::write(&path, json).map_err(|e| CliError::WriteOutput {
            path: path.clone(),
            source: e,
        })?;
    } else {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        out.write_all(&json)?;
        out.write_all(b"\n")?;
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
pub fn generate_values_schema_for_chart(opts: &GenerateOptions) -> CliResult<Value> {
    generate_values_schema_for_chart_with_warnings(opts, None)
}

/// Generate a values JSON schema for a full Helm chart, collecting warnings.
///
/// This walks the root chart and all vendored dependencies under `charts/`,
/// collects `.Values.*` usages across all manifest templates, and produces a
/// single JSON schema.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart_with_warnings(
    opts: &GenerateOptions,
    warning_sink: Option<&WarningSink>,
) -> CliResult<Value> {
    let discovery = chart::discover_chart_contexts(&opts.chart_dir)?;
    let charts = &discovery.charts;

    let defines = chart::build_define_index(charts, opts.include_tests)?;

    let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;

    let mut uses = collect_ir_for_charts(charts, &defines, opts.include_tests)?;
    seed_top_level_values_yaml_keys(&mut uses, values_yaml.as_deref());

    let provider = provider::build_provider(&opts.provider, warning_sink);

    Ok(generate_values_schema_with_values_yaml(
        &uses,
        provider.as_ref(),
        values_yaml.as_deref(),
    ))
}

fn seed_top_level_values_yaml_keys(uses: &mut Vec<ValueUse>, values_yaml: Option<&str>) {
    let Some(values_yaml) = values_yaml else {
        return;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
        return;
    };
    let YamlValue::Mapping(m) = doc else {
        return;
    };

    for (k, _) in m {
        let Some(key) = k.as_str() else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        uses.push(ValueUse {
            source_expr: key.to_string(),
            path: helm_schema_ir::YamlPath(Vec::new()),
            kind: helm_schema_ir::ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
        });
    }
}

fn collect_ir_for_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
) -> CliResult<Vec<ValueUse>> {
    let mut out: Vec<ValueUse> = Vec::new();

    for c in charts {
        if c.is_library {
            continue;
        }

        let manifests = chart::list_manifest_templates(&c.chart_dir, include_tests)?;

        for path in manifests {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;

            let ast = TreeSitterParser.parse(&src)?;

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

fn load_json_file(path: &Path) -> CliResult<Value> {
    let bytes = std::fs::read(path)?;
    let v: Value = serde_json::from_slice(&bytes)?;
    Ok(v)
}
