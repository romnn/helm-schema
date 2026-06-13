mod chart;
pub mod cli;
mod diag_emit;
mod error;
pub mod flatten;
mod output_pipeline;
mod provider_builder;
mod required_inference;
pub mod schema_override;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::{
    ChartFacts, ContractIr, ContractProjection, SymbolicIrContext, derive_chart_facts_from_ast,
    extract_default_type_hints, extract_define_blocks, extract_helper_calls,
};
use helm_schema_k8s::DiagnosticSink;
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;
use tracing_subscriber::Layer as _;
use tracing_subscriber::layer::SubscriberExt as _;
use vfs::VfsPath;

use crate::error::CliResult;
use crate::output_pipeline::OutputPipelineOptions;

pub use cli::Cli;
pub use error::CliError;
pub use provider_builder::ProviderOptions;

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: VfsPath,
    pub include_tests: bool,
    pub include_subchart_values: bool,
    pub values_files: Vec<PathBuf>,
    pub infer_required: bool,
    pub provider: ProviderOptions,
}

#[derive(Debug, Clone)]
struct GeneratedSchema {
    schema: Value,
    subchart_value_prefixes: Vec<Vec<String>>,
}

/// Run the CLI.
///
/// # Errors
///
/// Returns an error if chart discovery fails, a template/values file cannot be
/// read/parsed, the schema cannot be generated, or output cannot be written.
pub fn run(cli: Cli) -> CliResult<()> {
    let trace_output = cli.perf.trace_output.clone();
    if let Some(trace_output) = trace_output {
        if let Some(parent) = trace_output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| CliError::CreateOutputDir {
                path: parent.to_path_buf(),
                source: err,
            })?;
        }
        let trace_file =
            std::fs::File::create(&trace_output).map_err(|err| CliError::WriteOutput {
                path: trace_output.clone(),
                source: err,
            })?;
        let perfetto_layer =
            tracing_perfetto::PerfettoLayer::new(std::sync::Mutex::new(trace_file))
                .with_debug_annotations(true)
                .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                    metadata.target().starts_with("helm_schema")
                        || metadata.target().starts_with("json_schema_minify")
                }));
        let subscriber = tracing_subscriber::registry().with(perfetto_layer);
        let dispatch = tracing::Dispatch::new(subscriber);
        return tracing::dispatcher::with_default(&dispatch, || run_inner(cli));
    }

    run_inner(cli)
}

fn run_inner(cli: Cli) -> CliResult<()> {
    let run_span = tracing::info_span!(
        "helm_schema_run",
        chart_dir = %cli.chart_dir.display()
    );
    let _entered = run_span.enter();

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
        no_cache: cli.k8s.no_cache,
        allow_net: !cli.k8s.offline,
        disable_k8s_schemas: cli.k8s.no_k8s_schemas,
        crd_lookup_loose: matches!(cli.crd.lookup_mode(), cli::CrdVersionLookup::Loose),
        crd_catalog_mirrors: cli.crd.crd_catalog_mirror.clone(),
        crd_catalog_cache_dir: cli.crd.crd_catalog_cache_dir.clone(),
        crd_override_dir: cli.crd.crd_override_dir.clone(),
        chart_local_crds: Vec::new(),
        crd_cache_record_source: cli.crd.crd_cache_record_source,
        api_version_guess: cli.inference.enabled(),
    };

    let opts = GenerateOptions {
        chart_dir,
        include_tests: !cli.chart.exclude_tests,
        include_subchart_values: !cli.chart.no_subchart_values,
        values_files: cli.chart.values_files.clone(),
        infer_required: cli.chart.infer_required,
        provider: provider_options,
    };

    let diagnostics = DiagnosticSink::new();
    let GeneratedSchema {
        mut schema,
        subchart_value_prefixes,
    } = generate_values_schema_for_chart_with_diagnostics_inner(&opts, Some(&diagnostics))?;
    let output_options = OutputPipelineOptions {
        keep_refs: cli.output.keep_refs,
        allow_net: !cli.k8s.offline,
        strip_descriptions: cli.output.strip_descriptions,
        minimize: cli.output.minimize,
    };

    for path in cli.override_schema {
        let mut override_schema = load_json_file(&path)?;

        // Tag every subtree that carries `$ref` with an internal
        // "replace on merge" marker. The marker rides through the
        // pre-flatten dereference pass below and tells
        // `apply_schema_override` to swap the resolved content into the
        // base instead of deep-merging it with whatever helm-schema's
        // inference produced for the same path. Without this, an
        // inferred `cloud: {type: [boolean, string]}` would end up
        // alongside the override's resolved `enum: [null, "azure",
        // "minikube"]`, leaving the schema impossible to satisfy.
        schema_override::mark_refs_for_replacement(&mut override_schema);

        // Resolve `$ref`s in the override against the override file's
        // own directory, not the chart directory. This lets a shared
        // override (e.g. `deployment/charts/schemas/foo.override.json`
        // pulled in across many charts) carry refs to its siblings
        // (`./bar.json`) and have them resolve to the same physical
        // file regardless of where the chart being generated lives.
        // Without this, every override would have to use a chart-dir-
        // relative path that only works at one tree depth.
        //
        // `--keep-refs` opts out of the final flatten pass over the
        // merged schema (so chart-inference refs remain literal). We
        // honour the same flag here for symmetry — without it, an
        // override that wants to ship literal `$ref` strings can't.
        let override_schema =
            output_pipeline::prepare_override_schema(override_schema, &path, &output_options)?;

        schema = schema_override::apply_schema_override(schema, override_schema);
    }
    mirror_global_schema_into_subcharts(&mut schema, &subchart_value_prefixes);

    schema = output_pipeline::apply_output_transforms(schema, &cli.chart_dir, &output_options)?;

    diag_emit::emit_to_stderr(&diagnostics, cli.diag.diag_format);

    if let Some(path) = cli.output.output {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CliError::CreateOutputDir {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        let file = std::fs::File::create(&path).map_err(|err| CliError::WriteOutput {
            path: path.clone(),
            source: err,
        })?;
        let mut out = BufWriter::new(file);
        output_pipeline::write_schema_json(&mut out, &schema, cli.output.compact)
            .map_err(|err| write_output_error_with_path(err, &path))?;
        out.flush().map_err(|err| CliError::WriteOutput {
            path: path.clone(),
            source: err,
        })?;
    } else {
        let stdout = std::io::stdout();
        let mut out = BufWriter::new(stdout.lock());
        output_pipeline::write_schema_json(&mut out, &schema, cli.output.compact)?;
        out.flush()?;
    }

    Ok(())
}

fn write_output_error_with_path(err: CliError, path: &Path) -> CliError {
    match err {
        CliError::Io(source) => CliError::WriteOutput {
            path: path.to_path_buf(),
            source,
        },
        err => err,
    }
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
    let generated = generate_values_schema_for_chart_with_diagnostics_inner(opts, diagnostic_sink)?;
    Ok(generated.schema)
}

#[tracing::instrument(skip_all)]
fn generate_values_schema_for_chart_with_diagnostics_inner(
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<GeneratedSchema> {
    let discovery = chart::discover_chart_contexts(&opts.chart_dir)?;
    let charts = &discovery.charts;

    let defines = chart::build_define_index(charts, opts.include_tests)?;

    let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;
    let values_descriptions = chart::build_composed_values_descriptions(
        charts,
        opts.include_subchart_values,
        &opts.values_files,
    )?;

    let ChartIrCollection {
        contract_projection,
        chart_facts,
        type_hints,
        call_graph,
    } = collect_ir_for_charts(charts, &defines, opts.include_tests, values_yaml.as_deref())?;

    let mut provider_options = opts.provider.clone();
    provider_options.chart_local_crds = chart::collect_static_crd_sources(charts)?;
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let mut schema = generate_values_schema(
        ValuesSchemaInput::new(&contract_projection, &provider)
            .with_values_yaml(values_yaml.as_deref())
            .with_type_hints(&type_hints)
            .with_chart_facts(&chart_facts)
            .with_values_descriptions(&values_descriptions),
    );

    if opts.infer_required {
        required_inference::apply(
            &mut schema,
            &contract_projection,
            values_yaml.as_deref(),
            charts,
            &call_graph,
        );
    }

    Ok(GeneratedSchema {
        schema,
        subchart_value_prefixes: charts
            .iter()
            .filter(|chart| !chart.values_prefix.is_empty())
            .map(|chart| chart.values_prefix.clone())
            .collect(),
    })
}

fn mirror_global_schema_into_subcharts(schema: &mut Value, subchart_prefixes: &[Vec<String>]) {
    let Some(root_global_schema) = schema.pointer("/properties/global").cloned() else {
        return;
    };

    for prefix in subchart_prefixes {
        let subchart_schema = schema_object_at_values_prefix(schema, prefix);
        let subchart_global_schema = schema_property_mut(subchart_schema, "global");
        let existing = std::mem::take(subchart_global_schema);
        *subchart_global_schema =
            schema_override::apply_schema_override(existing, root_global_schema.clone());
    }
}

fn schema_object_at_values_prefix<'a>(schema: &'a mut Value, prefix: &[String]) -> &'a mut Value {
    let mut current = schema;
    for segment in prefix {
        current = schema_property_mut(current, segment);
    }
    current
}

fn schema_property_mut<'a>(schema: &'a mut Value, property: &str) -> &'a mut Value {
    let object = ensure_json_object(schema);
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let properties = ensure_json_object(properties);
    properties
        .entry(property.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(object) => object,
        _ => unreachable!("json value was just replaced with an object"),
    }
}

/// IR + auxiliary signals collected from a chart's templates.
pub(crate) struct ChartIrCollection {
    pub(crate) contract_projection: ContractProjection,
    pub(crate) chart_facts: ChartFacts,
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    pub(crate) call_graph: HelperCallGraph,
}

fn seed_top_level_values_yaml_keys(contract: &mut ContractIr, values_yaml: Option<&str>) {
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

        contract.push_pathless_scalar(key.to_string());
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

#[tracing::instrument(skip_all)]
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

#[tracing::instrument(skip_all)]
fn collect_ir_for_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
    values_yaml: Option<&str>,
) -> CliResult<ChartIrCollection> {
    let mut contract = ContractIr::default();
    let mut chart_facts = ChartFacts::default();
    let mut type_hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let symbolic_context = SymbolicIrContext::new(defines);

    for c in charts {
        if c.is_library {
            continue;
        }
        collect_manifest_ir_for_chart(
            c,
            defines,
            &symbolic_context,
            include_tests,
            &mut contract,
            &mut chart_facts,
        )?;
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

    seed_top_level_values_yaml_keys(&mut contract, values_yaml);

    Ok(ChartIrCollection {
        contract_projection: contract.project(),
        chart_facts,
        type_hints,
        call_graph,
    })
}

#[tracing::instrument(skip_all, fields(prefix_len = chart.values_prefix.len()))]
fn collect_manifest_ir_for_chart(
    chart: &chart::ChartContext,
    defines: &DefineIndex,
    symbolic_context: &SymbolicIrContext,
    include_tests: bool,
    contract: &mut ContractIr,
    chart_facts: &mut ChartFacts,
) -> CliResult<()> {
    let manifests = chart::list_manifest_templates(&chart.chart_dir, include_tests)?;
    for path in manifests {
        let ManifestIr {
            chart_facts: manifest_facts,
            contract: mut manifest_contract,
        } = collect_manifest_ir_for_template(&path, defines, symbolic_context)?;
        merge_chart_facts(
            chart_facts,
            scope_chart_facts(manifest_facts, &chart.values_prefix),
        );
        manifest_contract.map_value_paths(|path| scope_values_path(path, &chart.values_prefix));
        contract.append(manifest_contract);
    }
    Ok(())
}

struct ManifestIr {
    chart_facts: ChartFacts,
    contract: ContractIr,
}

#[tracing::instrument(skip_all)]
fn collect_manifest_ir_for_template(
    path: &VfsPath,
    defines: &DefineIndex,
    symbolic_context: &SymbolicIrContext,
) -> CliResult<ManifestIr> {
    let mut src = String::new();
    path.open_file()?.read_to_string(&mut src)?;
    let ast = TreeSitterParser.parse(&src)?;
    let chart_facts = derive_chart_facts_from_ast(&ast);
    let contract = symbolic_context.generate_contract_ir(&src, &ast, defines);
    Ok(ManifestIr {
        chart_facts,
        contract,
    })
}

fn scope_chart_facts(chart_facts: ChartFacts, prefix: &[String]) -> ChartFacts {
    ChartFacts {
        path_facts: chart_facts
            .path_facts
            .into_iter()
            .map(|(path, fact)| (scope_values_path(&path, prefix), fact))
            .collect(),
    }
}

fn merge_chart_facts(dst: &mut ChartFacts, src: ChartFacts) {
    for (path, fact) in src.path_facts {
        let entry = dst.path_facts.entry(path).or_default();
        let had_render_use = entry.has_render_use;
        if fact.has_render_use {
            entry.all_render_uses_self_guarded = if had_render_use {
                entry.all_render_uses_self_guarded && fact.all_render_uses_self_guarded
            } else {
                fact.all_render_uses_self_guarded
            };
        }
        entry.has_render_use |= fact.has_render_use;
        entry.has_fragment_render |= fact.has_fragment_render;
        entry.descendant_accessed |= fact.descendant_accessed;
        entry.has_self_range_guard_render_use |= fact.has_self_range_guard_render_use;
    }
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

pub(crate) fn scope_values_path(path: &str, prefix: &[String]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_ir::PathFact;
    use vfs::VfsPath;

    fn chart_facts_for(path: &str, all_render_uses_self_guarded: bool) -> ChartFacts {
        let mut chart_facts = ChartFacts::default();
        chart_facts.path_facts.insert(
            path.to_string(),
            PathFact {
                has_render_use: true,
                all_render_uses_self_guarded,
                ..PathFact::default()
            },
        );
        chart_facts
    }

    #[test]
    fn merge_chart_facts_initializes_self_guarded_state_from_first_render_use() {
        let mut merged = ChartFacts::default();

        merge_chart_facts(&mut merged, chart_facts_for("annotations", true));

        assert_eq!(
            merged
                .path_facts
                .get("annotations")
                .map(|fact| fact.all_render_uses_self_guarded),
            Some(true),
        );
    }

    #[test]
    fn merge_chart_facts_conjoins_self_guarded_state_across_render_uses() {
        let mut merged = ChartFacts::default();

        merge_chart_facts(&mut merged, chart_facts_for("annotations", true));
        merge_chart_facts(&mut merged, chart_facts_for("annotations", false));

        assert_eq!(
            merged
                .path_facts
                .get("annotations")
                .map(|fact| fact.all_render_uses_self_guarded),
            Some(false),
        );
    }

    #[test]
    fn subchart_helper_render_with_guard_surfaces_scoped_self_guarded_fact()
    -> color_eyre::eyre::Result<()> {
        let chart_dir = VfsPath::new(vfs::MemoryFS::new());

        test_util::write(
            &chart_dir.join("Chart.yaml")?,
            "apiVersion: v2\nname: root\nversion: 0.1.0\ndependencies:\n  - name: child\n    alias: kid\n    version: 0.1.0\n",
        )?;
        test_util::write(&chart_dir.join("values.yaml")?, "{}\n")?;
        test_util::write(
            &chart_dir.join("charts/child/Chart.yaml")?,
            "apiVersion: v2\nname: child\nversion: 0.1.0\n",
        )?;
        test_util::write(
            &chart_dir.join("charts/child/values.yaml")?,
            "controller:\n  ingressClassResource:\n    parameters: {}\n",
        )?;
        test_util::write(
            &chart_dir.join("charts/child/templates/_helpers.tpl")?,
            r#"{{- define "common.tplvalues.render" -}}
{{- .value | toYaml -}}
{{- end -}}
"#,
        )?;
        test_util::write(
            &chart_dir.join("charts/child/templates/ingressclass.yaml")?,
            r#"apiVersion: networking.k8s.io/v1
kind: IngressClass
spec:
  {{- with .Values.controller.ingressClassResource.parameters }}
  parameters: {{ include "common.tplvalues.render" (dict "value" . "context" $) | nindent 4 }}
  {{- end }}
"#,
        )?;

        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = collect_ir_for_charts(&discovery.charts, &defines, false, None)?;
        let path = "kid.controller.ingressClassResource.parameters";

        let uses = collection.contract_projection.uses();
        let ir_facts = helm_schema_ir::derive_chart_facts(&collection.contract_projection);
        let ir_fact = ir_facts
            .path_facts
            .get(path)
            .unwrap_or_else(|| panic!("missing IR-derived fact for {path}: {uses:#?}"));
        assert!(
            ir_fact.all_render_uses_self_guarded,
            "IR-derived chart fact should stay self-guarded: {ir_fact:#?}; uses={:#?}",
            uses
        );

        Ok(())
    }

    #[test]
    fn shared_global_override_schema_is_mirrored_into_nested_subcharts() {
        let mut schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {
                    "additionalProperties": true,
                    "properties": {
                        "kube-score/ignore": {
                            "type": "string"
                        }
                    },
                    "type": "object"
                },
                "oauth2-proxy": {
                    "additionalProperties": false,
                    "properties": {
                        "global": {
                            "additionalProperties": false,
                            "properties": {
                                "imageRegistry": {
                                    "type": "string"
                                }
                            },
                            "type": "object"
                        },
                        "redis": {
                            "additionalProperties": false,
                            "properties": {
                                "global": {
                                    "additionalProperties": false,
                                    "properties": {
                                        "storageClass": {
                                            "type": "string"
                                        }
                                    },
                                    "type": "object"
                                }
                            },
                            "type": "object"
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        });

        mirror_global_schema_into_subcharts(
            &mut schema,
            &[
                vec!["oauth2-proxy".to_string()],
                vec!["oauth2-proxy".to_string(), "redis".to_string()],
            ],
        );

        let child_global = schema
            .pointer("/properties/oauth2-proxy/properties/global")
            .expect("child global schema");
        assert_eq!(
            child_global
                .pointer("/properties/kube-score~1ignore/type")
                .and_then(Value::as_str),
            Some("string"),
            "shared global property should be mirrored into child global: {child_global}"
        );
        assert_eq!(
            child_global
                .pointer("/properties/imageRegistry/type")
                .and_then(Value::as_str),
            Some("string"),
            "child global-specific properties should be preserved: {child_global}"
        );
        assert_eq!(
            child_global
                .get("additionalProperties")
                .and_then(Value::as_bool),
            Some(true),
            "shared open-global policy should be mirrored into child global: {child_global}"
        );

        let nested_global = schema
            .pointer("/properties/oauth2-proxy/properties/redis/properties/global")
            .expect("nested global schema");
        assert_eq!(
            nested_global
                .pointer("/properties/kube-score~1ignore/type")
                .and_then(Value::as_str),
            Some("string"),
            "shared global property should be mirrored into nested child global: {nested_global}"
        );
        assert_eq!(
            nested_global
                .pointer("/properties/storageClass/type")
                .and_then(Value::as_str),
            Some("string"),
            "nested child global-specific properties should be preserved: {nested_global}"
        );
        assert_eq!(
            nested_global
                .get("additionalProperties")
                .and_then(Value::as_bool),
            Some(true),
            "shared open-global policy should be mirrored into nested child global: {nested_global}"
        );
    }

    #[test]
    fn signoz_root_service_account_helper_is_reachable_for_type_hints()
    -> color_eyre::eyre::Result<()> {
        let chart_dir = test_util::workspace_testdata()
            .join("charts")
            .join("signoz-signoz");
        let chart_dir_str = chart_dir.to_string_lossy().to_string();
        let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = collect_ir_for_charts(&discovery.charts, &defines, false, None)?;
        let path = "alertmanager.serviceAccount.name";

        assert!(
            collection.type_hints.contains_key(path),
            "expected type hint for {path}; reachable={:?}; hints={:?}",
            collection
                .call_graph
                .reachable_from_chart(&Vec::<String>::new()),
            collection.type_hints.keys().collect::<Vec<_>>()
        );

        Ok(())
    }
}
