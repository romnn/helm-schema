use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

use helm_schema_ir::{DefineBlock, extract_define_blocks, extract_helper_calls};

use crate::chart;
use crate::error::CliResult;

#[derive(Debug, Default)]
pub(super) struct HelperCallGraph {
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
    pub(super) fn helper_body(&self, name: &str) -> Option<&str> {
        self.helpers.get(name).map(|node| node.body_text.as_str())
    }

    pub(super) fn chart_direct_body(&self, prefix: &[String]) -> Option<&str> {
        self.chart_direct
            .get(prefix)
            .map(|node| node.body_text.as_str())
    }

    pub(super) fn reachable_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        let Some(direct) = self.chart_direct.get(prefix) else {
            return BTreeSet::new();
        };
        self.reachable_helpers(&direct.callees)
    }

    fn reachable_helpers(&self, seeds: &BTreeSet<String>) -> BTreeSet<String> {
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = seeds.iter().cloned().collect();
        while let Some(name) = stack.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(node) = self.helpers.get(&name) {
                for callee in &node.callees {
                    if !visited.contains(callee) {
                        stack.push(callee.clone());
                    }
                }
            }
        }
        visited
    }
}

#[tracing::instrument(skip_all)]
pub(super) fn build_helper_call_graph(
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
