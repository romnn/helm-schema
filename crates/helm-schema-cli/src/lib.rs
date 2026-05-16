mod chart;
mod error;
pub mod flatten;
mod provider;
mod required_inference;
pub mod schema_override;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Args, Parser};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::generate_values_schema_full;
use helm_schema_ir::{
    Guard, IrGenerator, SymbolicIrGenerator, ValueUse, extract_default_type_hints,
    extract_define_blocks, extract_helper_calls,
};
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

    /// Leave file/URL `$ref` strings in the generated schema as-is.
    /// By default, the final output pass walks the merged schema and
    /// inlines every file `$ref` (and, unless `--offline`, every URL
    /// `$ref`), recursively, with cycle detection.
    #[arg(long)]
    pub keep_refs: bool,
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

    /// Mark paths used in unconditional template guards
    /// (`if .Values.X`/`eq .Values.X "..."` with no enclosing guard) as
    /// `required` on their parent object. Paths reachable via any
    /// `default <expr> .Values.X` fallback are excluded — the fallback
    /// expression can be a literal (`default "x" .Values.X`), an
    /// identifier (`default .Chart.Name .Values.X`), or a parenthesized
    /// expression (`default (printf "%s" .Y) .Values.X`).
    #[arg(long)]
    pub infer_required: bool,
}

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
    let chart_dir_str = cli.chart_dir.to_string_lossy().to_string();
    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));

    let opts = GenerateOptions {
        chart_dir,
        include_tests: !cli.chart.exclude_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        infer_required: cli.chart.infer_required,
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

    // Final output pass: inline `$ref`s so the artifact is flat and
    // directly comparable to a fully-resolved schema. Skip when
    // --keep-refs is set (callers who want to bundle later).
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

    let ChartIrCollection {
        mut uses,
        type_hints,
        call_graph,
    } = collect_ir_for_charts(charts, &defines, opts.include_tests)?;
    seed_top_level_values_yaml_keys(&mut uses, values_yaml.as_deref());

    let provider = provider::build_provider(&opts.provider, warning_sink);

    let mut schema = generate_values_schema_full(
        &uses,
        provider.as_ref(),
        values_yaml.as_deref(),
        &type_hints,
    );

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
    /// Per-path JSON Schema fragments inferred from `default <literal>
    /// .Values.X` patterns. Literal-only — feeds nullable-union schema
    /// generation in [`generate_values_schema_full`].
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    /// Cross-chart helper call graph (with raw helper body text) so
    /// downstream consumers — currently only
    /// [`required_inference`] — can run their own text-level
    /// extractors over the same reachability resolution that drives
    /// type-hint scoping in `collect_ir_for_charts`.
    pub(crate) call_graph: HelperCallGraph,
}

/// Inject one synthetic Scalar/empty-path/empty-guards `ValueUse` per
/// top-level `values.yaml` key so the generator materialises a schema
/// property for that key even when no template references it.
///
/// The synthetic uses are indistinguishable from real unconditional
/// `if .Values.X` header uses, which matters only for the heuristic
/// required-inference pass — that module re-derives the seeded key set
/// from values.yaml directly (see
/// [`required_inference::top_level_value_paths`]). Keeping the
/// derivation in the consumer keeps this seeding function's contract
/// narrow.
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

/// Cross-chart helper call graph. Nodes are individual helpers (keyed
/// by the name passed to `{{ define "<name>" }}`) plus per-chart
/// "chart-direct" pseudo-nodes (text outside any define block). Edges
/// go from caller helper / chart-direct context → callee helper.
///
/// Each node carries the *raw body text* of the helper / the
/// concatenated chart-direct text, not pre-extracted signals. That
/// keeps the graph itself feature-agnostic — multiple consumers can
/// run their own text-level extractors over the same nodes without
/// extending the node type. Today's consumers:
///   - core type-hint extraction (this file, in `collect_ir_for_charts`)
///   - heuristic required-inference fallback-path extraction
///     (`required_inference` module)
///
/// **Duplicate define names.** When two `{{ define "X" }}` blocks share
/// a name (e.g. two library subcharts both define `common.name`), Helm
/// — and our [`DefineIndex`] — resolves the call via last-write-wins
/// on iteration order. The graph follows the same rule: the last body
/// seen for a given name fully replaces the previous one. If we kept
/// both bodies (e.g. by concatenating text), text-level extractors
/// would see content from a define that never executes at render time,
/// producing phantom signals for whichever consumer reads the body.
#[derive(Debug, Default)]
pub(crate) struct HelperCallGraph {
    /// All defined helpers from any chart (`name` → body + outgoing
    /// helper-call edges). Last-write-wins on duplicate names so the
    /// stored body matches the one [`DefineIndex`] resolves at render
    /// time.
    helpers: BTreeMap<String, HelperNode>,
    /// Chart-direct context for each non-library chart: body text +
    /// the set of helpers it `include`s outside any `define` block.
    chart_direct: BTreeMap<Vec<String>, ChartDirectNode>,
}

#[derive(Debug, Default)]
pub(crate) struct HelperNode {
    body_text: String,
    /// Helper names this helper transitively *calls* via `include` or
    /// `template` from inside its own define body.
    callees: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub(crate) struct ChartDirectNode {
    body_text: String,
    /// Helper names called from outside any `define` block in this
    /// chart's templates — i.e. directly from the chart's manifest
    /// templates and any top-level helper file content.
    callees: BTreeSet<String>,
}

impl HelperCallGraph {
    /// Read access to a helper's body text (for consumers that run
    /// their own text-level extractors).
    pub(crate) fn helper_body(&self, name: &str) -> Option<&str> {
        self.helpers.get(name).map(|n| n.body_text.as_str())
    }

    /// Read access to a chart's chart-direct body text.
    pub(crate) fn chart_direct_body(&self, prefix: &[String]) -> Option<&str> {
        self.chart_direct.get(prefix).map(|n| n.body_text.as_str())
    }

    /// Transitive closure of helper names reachable from `prefix`'s
    /// chart-direct includes.
    pub(crate) fn reachable_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        let Some(direct) = self.chart_direct.get(prefix) else {
            return BTreeSet::new();
        };
        reachable_helpers(self, &direct.callees)
    }
}

/// Build the global helper call graph from every chart's templates.
///
/// For each template source we scan:
///   - All `{{ define "name" }}…{{ end }}` blocks. Body text feeds
///     `HelperNode` for that name (signals + outgoing helper calls).
///   - Whatever text lies outside any define block — that's chart-direct
///     code. For non-library charts it feeds a `ChartDirectNode` keyed
///     on the chart's value prefix.
///
/// Library charts' chart-direct text is ignored: library charts have no
/// value scope of their own, so any `default <…> .Values.X` in their
/// top-level (non-define) template positions doesn't apply to anyone —
/// in practice library charts only have `_helpers.tpl` files with
/// nothing but defines anyway.
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

            // Slice the source into (in-define, chart-direct) text.
            let defines = extract_define_blocks(&src);
            for block in &defines {
                // Last-write-wins, matching `DefineIndex` (which uses
                // `BTreeMap::insert`). If two charts both define
                // `common.name`, only the body of the later one is
                // what Helm actually renders — so only that body's
                // callees and text feed downstream extractors.
                let callees = extract_helper_calls(&block.body).into_iter().collect();
                graph.helpers.insert(
                    block.name.clone(),
                    HelperNode {
                        body_text: block.body.clone(),
                        callees,
                    },
                );
            }

            // Chart-direct text only contributes for non-library charts.
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

/// Append `chunk` to `body` separated by a newline. Used to fuse the
/// chart-direct text of a single chart across multiple template files
/// (all of which render at the same scope). Helper bodies use plain
/// `insert` instead — see the duplicate-name note on
/// [`HelperCallGraph`].
fn push_body_text(body: &mut String, chunk: &str) {
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(chunk);
}

/// Returns the concatenation of `src` slices that lie *outside* every
/// define block, joined by newlines so cross-region patterns can't
/// accidentally span the gap.
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

/// Resolve the transitive closure of helper names reachable from
/// `seeds` via the graph's helper→helper edges.
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

    // IR collection (uses) from non-library manifest templates. Library
    // charts don't render their own manifests — they only export helpers
    // — so they contribute no `uses`. The IR generator inlines helper
    // definitions via `defines` when it processes a manifest, so
    // library-helper content reaches `uses` through the normal symbolic
    // walk; no extra scan needed here.
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

    // Helper-granular type-hint scoping. The regex-based extractor sees
    // raw text only, so we run it against each helper-graph node's body
    // text and apply the resulting hints at the prefix of every chart
    // that transitively reaches that node. A type-hint declared inside
    // helper H reaches chart C if and only if H is in the transitive
    // closure of C's directly-called helpers — handling:
    //   1. A library with a USED helper that ALSO defines an unused one
    //      no longer leaks the unused helper's hints to the caller.
    //   2. Transitive include chains (`app → libA.X → libB.Y`) propagate
    //      libB.Y's hints to `app`.
    //   3. Unused libraries still contribute nothing.
    //
    // The graph is also handed to required-inference (via
    // [`ChartIrCollection::call_graph`]) so its fallback-path
    // extraction can ride the same reachability resolution without
    // re-deriving the graph.
    let call_graph = build_helper_call_graph(charts, include_tests)?;

    for c in charts.iter().filter(|c| !c.is_library) {
        // Apply chart-direct type hints at this chart's prefix.
        if let Some(text) = call_graph.chart_direct_body(&c.values_prefix) {
            apply_type_hints_to(&mut type_hints, text, &c.values_prefix);
        }
        // Apply type hints from every transitively reachable helper.
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
        Guard::With { path } => Guard::With {
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
