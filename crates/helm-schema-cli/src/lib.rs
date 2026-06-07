mod chart;
pub mod cli;
mod diag_emit;
mod error;
pub mod flatten;
mod provider_builder;
mod required_inference;
pub mod schema_override;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_full;
use helm_schema_ir::{
    Guard, IrGenerator, SymbolicIrGenerator, ValueUse, extract_default_type_hints,
    extract_define_blocks, extract_helper_calls,
};
use helm_schema_k8s::DiagnosticSink;
use serde_json::Value;
use serde_yaml::Value as YamlValue;
use vfs::VfsPath;

use crate::error::CliResult;

pub use cli::Cli;
pub use error::CliError;
pub use provider_builder::ProviderOptions;

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: VfsPath,
    pub include_tests: bool,
    pub include_subchart_values: bool,
    pub infer_required: bool,
    pub provider: ProviderOptions,
}

/// Run the CLI.
///
/// # Errors
///
/// Returns an error if chart discovery fails, a template/values file cannot be
/// read/parsed, the schema cannot be generated, or output cannot be written.
pub fn run(cli: Cli) -> CliResult<()> {
    cli.crd.validate().map_err(CliError::CliValidation)?;
    let fallback_window = cli
        .k8s
        .resolved_fallback_window()
        .map_err(CliError::CliValidation)?;

    let chart_dir_str = cli.chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let provider_options = ProviderOptions {
        k8s_versions: cli.k8s.k8s_version.clone(),
        k8s_version_fallback_window: fallback_window,
        k8s_schema_mirrors: cli.k8s.k8s_schema_mirror.clone(),
        k8s_schema_cache_dir: cli.k8s.k8s_schema_cache_dir.clone(),
        allow_net: !cli.k8s.offline,
        disable_k8s_schemas: cli.k8s.no_k8s_schemas,
        crd_lookup_loose: matches!(cli.crd.lookup_mode(), cli::CrdVersionLookup::Loose),
        crd_catalog_mirrors: cli.crd.crd_catalog_mirror.clone(),
        crd_catalog_cache_dir: cli.crd.crd_catalog_cache_dir.clone(),
        crd_override_dir: cli.crd.crd_override_dir.clone(),
        crd_cache_record_source: cli.crd.crd_cache_record_source,
        api_version_guess: cli.inference.enabled(),
    };

    let opts = GenerateOptions {
        chart_dir,
        include_tests: !cli.chart.exclude_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        infer_required: cli.chart.infer_required,
        provider: provider_options,
    };

    let diagnostics = DiagnosticSink::new();
    let mut schema = generate_values_schema_for_chart_with_diagnostics(&opts, Some(&diagnostics))?;

    if let Some(path) = cli.override_schema {
        let override_schema = load_json_file(&path)?;
        schema = schema_override::apply_schema_override(schema, override_schema);
    }

    if !cli.output.keep_refs {
        let allow_net = !cli.k8s.offline;
        schema = flatten::flatten_refs(
            schema,
            &cli.chart_dir,
            &flatten::FlattenOptions { allow_net },
        )?;
    }

    let json = if cli.output.compact {
        serde_json::to_vec(&schema)?
    } else {
        serde_json::to_vec_pretty(&schema)?
    };

    diag_emit::emit_to_stderr(&diagnostics, cli.diag.diag_format);

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
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart(opts: &GenerateOptions) -> CliResult<Value> {
    generate_values_schema_for_chart_with_diagnostics(opts, None)
}

/// Generate a values JSON schema for a full Helm chart, collecting diagnostics.
///
/// # Errors
///
/// Returns an error if charts cannot be discovered, files cannot be read, or
/// templates/values cannot be parsed.
pub fn generate_values_schema_for_chart_with_diagnostics(
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<Value> {
    let discovery = chart::discover_chart_contexts(&opts.chart_dir)?;
    let charts = &discovery.charts;

    let defines = chart::build_define_index(charts, opts.include_tests)?;

    let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;

    let ChartIrCollection {
        mut uses,
        type_hints,
        call_graph,
    } = collect_ir_for_charts(charts, &defines, opts.include_tests)?;
    seed_top_level_values_yaml_keys(&mut uses, values_yaml.as_deref());

    let provider = provider_builder::build_provider(&opts.provider, diagnostic_sink);

    let mut schema =
        generate_values_schema_full(&uses, &provider, values_yaml.as_deref(), &type_hints);

    if opts.infer_required {
        required_inference::apply(
            &mut schema,
            &uses,
            values_yaml.as_deref(),
            charts,
            &call_graph,
        );
    }

    Ok(schema)
}

/// IR + auxiliary signals collected from a chart's templates.
pub(crate) struct ChartIrCollection {
    pub(crate) uses: Vec<ValueUse>,
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    pub(crate) call_graph: HelperCallGraph,
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

#[derive(Debug, Default)]
pub(crate) struct HelperCallGraph {
    helpers: BTreeMap<String, HelperNode>,
    chart_direct: BTreeMap<Vec<String>, ChartDirectNode>,
}

#[derive(Debug, Default)]
pub(crate) struct HelperNode {
    body_text: String,
    callees: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ChartDirectNode {
    body_text: String,
    callees: BTreeSet<String>,
}

impl HelperCallGraph {
    pub(crate) fn helper_body(&self, name: &str) -> Option<&str> {
        self.helpers.get(name).map(|n| n.body_text.as_str())
    }

    pub(crate) fn chart_direct_body(&self, prefix: &[String]) -> Option<&str> {
        self.chart_direct.get(prefix).map(|n| n.body_text.as_str())
    }

    pub(crate) fn reachable_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        let Some(direct) = self.chart_direct.get(prefix) else {
            return BTreeSet::new();
        };
        reachable_helpers(self, &direct.callees)
    }
}

pub(crate) fn build_helper_call_graph(
    charts: &[chart::ChartContext],
    include_tests: bool,
) -> CliResult<HelperCallGraph> {
    let mut graph = HelperCallGraph::default();

    for c in charts {
        let sources = chart::list_template_sources_for_define_index(&c.chart_dir, include_tests)?;
        for path in sources {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;

            let defines = extract_define_blocks(&src);
            for block in &defines {
                let callees = extract_helper_calls(&block.body).into_iter().collect();
                graph.helpers.insert(
                    block.name.clone(),
                    HelperNode {
                        body_text: block.body.clone(),
                        callees,
                    },
                );
            }

            if !c.is_library {
                let direct_text = text_outside_defines(&src, &defines);
                let direct_callees = extract_helper_calls(&direct_text);
                let node = graph
                    .chart_direct
                    .entry(c.values_prefix.clone())
                    .or_default();
                push_body_text(&mut node.body_text, &direct_text);
                for callee in direct_callees {
                    node.callees.insert(callee);
                }
            }
        }
    }

    Ok(graph)
}

fn push_body_text(body: &mut String, chunk: &str) {
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(chunk);
}

fn text_outside_defines(src: &str, defines: &[helm_schema_ir::DefineBlock]) -> String {
    if defines.is_empty() {
        return src.to_string();
    }
    let mut ranges: Vec<std::ops::Range<usize>> =
        defines.iter().map(|d| d.byte_range.clone()).collect();
    ranges.sort_by_key(|r| r.start);

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for r in ranges {
        if cursor < r.start
            && let Some(chunk) = src.get(cursor..r.start)
        {
            out.push_str(chunk);
            out.push('\n');
        }
        cursor = cursor.max(r.end);
    }
    if cursor < src.len()
        && let Some(tail) = src.get(cursor..)
    {
        out.push_str(tail);
    }
    out
}

fn reachable_helpers(graph: &HelperCallGraph, seeds: &BTreeSet<String>) -> BTreeSet<String> {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = seeds.iter().cloned().collect();
    while let Some(name) = stack.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(node) = graph.helpers.get(&name) {
            for callee in &node.callees {
                if !visited.contains(callee) {
                    stack.push(callee.clone());
                }
            }
        }
    }
    visited
}

fn collect_ir_for_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
) -> CliResult<ChartIrCollection> {
    let mut uses: Vec<ValueUse> = Vec::new();
    let mut type_hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();

    for c in charts {
        if c.is_library {
            continue;
        }
        let manifests = chart::list_manifest_templates(&c.chart_dir, include_tests)?;
        for path in manifests {
            let mut src = String::new();
            path.open_file()?.read_to_string(&mut src)?;
            let ast = TreeSitterParser.parse(&src)?;
            let manifest_uses = SymbolicIrGenerator.generate(&src, &ast, defines);
            for u in manifest_uses {
                uses.push(scope_value_use(u, &c.values_prefix));
            }
        }
    }

    let call_graph = build_helper_call_graph(charts, include_tests)?;

    for c in charts.iter().filter(|c| !c.is_library) {
        if let Some(text) = call_graph.chart_direct_body(&c.values_prefix) {
            apply_type_hints_to(&mut type_hints, text, &c.values_prefix);
        }
        for helper_name in call_graph.reachable_from_chart(&c.values_prefix) {
            if let Some(text) = call_graph.helper_body(&helper_name) {
                apply_type_hints_to(&mut type_hints, text, &c.values_prefix);
            }
        }
    }

    Ok(ChartIrCollection {
        uses,
        type_hints,
        call_graph,
    })
}

fn apply_type_hints_to(
    type_hints: &mut BTreeMap<String, Vec<Value>>,
    body_text: &str,
    prefix: &[String],
) {
    for (path, schema) in extract_default_type_hints(body_text) {
        let scoped = scope_values_path(&path, prefix);
        type_hints.entry(scoped).or_default().push(schema);
    }
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
        Guard::Range { path } => Guard::Range {
            path: scope_values_path(&path, prefix),
        },
        Guard::With { path } => Guard::With {
            path: scope_values_path(&path, prefix),
        },
        Guard::Default { path } => Guard::Default {
            path: scope_values_path(&path, prefix),
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

    if prefix.is_empty() {
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
