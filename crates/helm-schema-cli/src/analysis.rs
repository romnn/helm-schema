use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{
    ChartFacts, ContractIr, ContractProjection, SymbolicIrContext, derive_chart_facts_from_ast,
    extract_default_type_hints, extract_define_blocks, extract_helper_calls,
};
use helm_schema_k8s::LocalSchemaUniverse;
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use vfs::VfsPath;

use crate::chart;
use crate::error::CliResult;

/// Contract and auxiliary signals collected from a chart tree.
pub(crate) struct ChartAnalysis {
    pub(crate) contract_projection: ContractProjection,
    pub(crate) chart_facts: ChartFacts,
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    pub(crate) call_graph: HelperCallGraph,
    pub(crate) local_schema_universe: LocalSchemaUniverse,
}

#[derive(Debug, Default)]
pub(crate) struct HelperCallGraph {
    helpers: BTreeMap<String, HelperNode>,
    chart_direct: BTreeMap<Vec<String>, ChartDirectNode>,
}

#[derive(Debug, Default)]
struct HelperNode {
    body_text: String,
    callees: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct ChartDirectNode {
    body_text: String,
    callees: BTreeSet<String>,
}

impl HelperCallGraph {
    pub(crate) fn helper_body(&self, name: &str) -> Option<&str> {
        self.helpers.get(name).map(|node| node.body_text.as_str())
    }

    pub(crate) fn chart_direct_body(&self, prefix: &[String]) -> Option<&str> {
        self.chart_direct
            .get(prefix)
            .map(|node| node.body_text.as_str())
    }

    pub(crate) fn reachable_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        let Some(direct) = self.chart_direct.get(prefix) else {
            return BTreeSet::new();
        };
        reachable_helpers(self, &direct.callees)
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn analyze_charts(
    charts: &[chart::ChartContext],
    defines: &DefineIndex,
    include_tests: bool,
    values_yaml: Option<&str>,
) -> CliResult<ChartAnalysis> {
    let mut contract = ContractIr::default();
    let mut chart_facts = ChartFacts::default();
    let mut type_hints: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let local_schema_universe = chart::collect_static_crd_universe(charts)?;
    let symbolic_context = SymbolicIrContext::new(defines);

    for chart in charts {
        if chart.is_library {
            continue;
        }
        collect_manifest_ir_for_chart(
            chart,
            defines,
            &symbolic_context,
            include_tests,
            &mut contract,
            &mut chart_facts,
        )?;
    }

    let call_graph = build_helper_call_graph(charts, include_tests)?;

    for chart in charts.iter().filter(|chart| !chart.is_library) {
        if let Some(text) = call_graph.chart_direct_body(&chart.values_prefix) {
            apply_type_hints_to(&mut type_hints, text, &chart.values_prefix);
        }
        for helper_name in call_graph.reachable_from_chart(&chart.values_prefix) {
            if let Some(text) = call_graph.helper_body(&helper_name) {
                apply_type_hints_to(&mut type_hints, text, &chart.values_prefix);
            }
        }
    }
    apply_dependency_activation_type_hints(&mut type_hints, charts);

    seed_top_level_values_yaml_keys(&mut contract, values_yaml);

    Ok(ChartAnalysis {
        contract_projection: contract.project(),
        chart_facts,
        type_hints,
        call_graph,
        local_schema_universe,
    })
}

#[tracing::instrument(skip_all)]
fn build_helper_call_graph(
    charts: &[chart::ChartContext],
    include_tests: bool,
) -> CliResult<HelperCallGraph> {
    let mut graph = HelperCallGraph::default();

    for chart in charts {
        let sources =
            chart::list_template_sources_for_define_index(&chart.chart_dir, include_tests)?;
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

            if !chart.is_library {
                let direct_text = text_outside_defines(&src, &defines);
                let direct_callees = extract_helper_calls(&direct_text);
                let node = graph
                    .chart_direct
                    .entry(chart.values_prefix.clone())
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

fn seed_top_level_values_yaml_keys(contract: &mut ContractIr, values_yaml: Option<&str>) {
    let Some(values_yaml) = values_yaml else {
        return;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
        return;
    };
    let YamlValue::Mapping(mapping) = doc else {
        return;
    };

    for (key, _) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        contract.push_pathless_scalar(key.to_string());
    }
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
        manifest_contract
            .map_value_paths(|path| chart::scope_values_path(path, &chart.values_prefix));
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
            .map(|(path, fact)| (chart::scope_values_path(&path, prefix), fact))
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
        let scoped = chart::scope_values_path(&path, prefix);
        type_hints.entry(scoped).or_default().push(schema);
    }
}

fn apply_dependency_activation_type_hints(
    type_hints: &mut BTreeMap<String, Vec<Value>>,
    charts: &[chart::ChartContext],
) {
    let paths = charts
        .iter()
        .flat_map(|chart| {
            chart
                .dependency_activation
                .condition_paths
                .iter()
                .chain(chart.dependency_activation.tag_paths.iter())
        })
        .cloned()
        .collect::<BTreeSet<_>>();

    for path in paths {
        type_hints
            .entry(path)
            .or_default()
            .push(json!({ "type": "boolean" }));
    }
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
    let mut ranges: Vec<std::ops::Range<usize>> = defines
        .iter()
        .map(|define| define.byte_range.clone())
        .collect();
    ranges.sort_by_key(|range| range.start);

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for range in ranges {
        if cursor < range.start
            && let Some(chunk) = src.get(cursor..range.start)
        {
            out.push_str(chunk);
            out.push('\n');
        }
        cursor = cursor.max(range.end);
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

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_ir::PathFact;

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
        let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
        let path = "kid.controller.ingressClassResource.parameters";

        let uses = collection.contract_projection.uses();
        let ir_facts = collection.contract_projection.chart_facts();
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
    fn signoz_root_service_account_helper_is_reachable_for_type_hints()
    -> color_eyre::eyre::Result<()> {
        let chart_dir = test_util::workspace_testdata()
            .join("charts")
            .join("signoz-signoz");
        let chart_dir_str = chart_dir.to_string_lossy().to_string();
        let chart_dir = VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str));
        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let defines = chart::build_define_index(&discovery.charts, false)?;
        let collection = analyze_charts(&discovery.charts, &defines, false, None)?;
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
