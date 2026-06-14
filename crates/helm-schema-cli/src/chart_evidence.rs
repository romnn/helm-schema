use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use helm_schema_ir::required_inference::extract_default_fallback_paths;
use helm_schema_ir::{
    DefineBlock, extract_default_type_hints, extract_define_blocks, extract_helper_calls,
};
use serde_json::{Value, json};

use crate::chart;
use crate::error::CliResult;

/// Template-derived chart evidence that is not part of the manifest contract.
///
/// This contains helper-reachability-dependent evidence that still feeds
/// output policies: default literal type hints and the broader fallback paths
/// used by the optional `--infer-required` pass.
pub(crate) struct ChartTemplateEvidence {
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    pub(crate) default_fallback_paths: BTreeSet<String>,
    #[cfg(test)]
    call_graph: HelperCallGraph,
}

#[derive(Debug, Default)]
struct HelperCallGraph {
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

impl ChartTemplateEvidence {
    #[cfg(test)]
    pub(crate) fn reachable_helpers_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        self.call_graph.reachable_from_chart(prefix)
    }
}

impl HelperCallGraph {
    fn helper_body(&self, name: &str) -> Option<&str> {
        self.helpers.get(name).map(|node| node.body_text.as_str())
    }

    fn chart_direct_body(&self, prefix: &[String]) -> Option<&str> {
        self.chart_direct
            .get(prefix)
            .map(|node| node.body_text.as_str())
    }

    fn reachable_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        let Some(direct) = self.chart_direct.get(prefix) else {
            return BTreeSet::new();
        };
        reachable_helpers(self, &direct.callees)
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_chart_template_evidence(
    charts: &[chart::ChartContext],
    include_tests: bool,
) -> CliResult<ChartTemplateEvidence> {
    let call_graph = build_helper_call_graph(charts, include_tests)?;
    let mut type_hints = BTreeMap::new();
    let mut default_fallback_paths = BTreeSet::new();

    for chart in charts.iter().filter(|chart| !chart.is_library) {
        if let Some(text) = call_graph.chart_direct_body(&chart.values_prefix) {
            apply_template_evidence_to(
                &mut type_hints,
                &mut default_fallback_paths,
                text,
                &chart.values_prefix,
            );
        }
        for helper_name in call_graph.reachable_from_chart(&chart.values_prefix) {
            if let Some(text) = call_graph.helper_body(&helper_name) {
                apply_template_evidence_to(
                    &mut type_hints,
                    &mut default_fallback_paths,
                    text,
                    &chart.values_prefix,
                );
            }
        }
    }
    apply_dependency_activation_type_hints(&mut type_hints, charts);

    Ok(ChartTemplateEvidence {
        type_hints,
        default_fallback_paths,
        #[cfg(test)]
        call_graph,
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
            let mut source = String::new();
            path.open_file()?.read_to_string(&mut source)?;

            let defines = extract_define_blocks(&source);
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
                let direct_text = text_outside_defines(&source, &defines);
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

fn apply_template_evidence_to(
    type_hints: &mut BTreeMap<String, Vec<Value>>,
    default_fallback_paths: &mut BTreeSet<String>,
    body_text: &str,
    prefix: &[String],
) {
    for (path, schema) in extract_default_type_hints(body_text) {
        let scoped = chart::scope_values_path(&path, prefix);
        type_hints.entry(scoped).or_default().push(schema);
    }

    for path in extract_default_fallback_paths(body_text) {
        default_fallback_paths.insert(chart::scope_values_path(&path, prefix));
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

fn text_outside_defines(source: &str, defines: &[DefineBlock]) -> String {
    if defines.is_empty() {
        return source.to_string();
    }
    let mut ranges: Vec<std::ops::Range<usize>> = defines
        .iter()
        .map(|define| define.byte_range.clone())
        .collect();
    ranges.sort_by_key(|range| range.start);

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for range in ranges {
        if cursor < range.start
            && let Some(chunk) = source.get(cursor..range.start)
        {
            out.push_str(chunk);
            out.push('\n');
        }
        cursor = cursor.max(range.end);
    }
    if cursor < source.len()
        && let Some(tail) = source.get(cursor..)
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
    use vfs::VfsPath;

    #[test]
    fn reachable_helper_defaults_are_scoped_as_template_evidence() -> color_eyre::eyre::Result<()> {
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
        test_util::write(&chart_dir.join("charts/child/values.yaml")?, "{}\n")?;
        test_util::write(
            &chart_dir.join("charts/child/templates/_helpers.tpl")?,
            r#"{{- define "child.name" -}}
{{ default "demo" .Values.name }}
{{- end -}}
"#,
        )?;
        test_util::write(
            &chart_dir.join("charts/child/templates/configmap.yaml")?,
            r#"apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include "child.name" . }}
"#,
        )?;

        let discovery = chart::discover_chart_contexts(&chart_dir)?;
        let evidence = collect_chart_template_evidence(&discovery.charts, false)?;

        assert!(
            evidence.type_hints.contains_key("kid.name"),
            "reachable helper default should produce a scoped type hint: {:?}",
            evidence.type_hints
        );
        assert!(
            evidence.default_fallback_paths.contains("kid.name"),
            "reachable helper default should produce a scoped fallback path: {:?}",
            evidence.default_fallback_paths
        );

        Ok(())
    }
}
