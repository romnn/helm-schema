use std::collections::BTreeMap;

use helm_schema_engine::extract_default_type_hints;
use serde_json::Value;

use super::ChartTemplateEvidence;
use super::helper_call_graph::build_helper_call_graph;
use crate::chart;
use crate::error::CliResult;

#[tracing::instrument(skip_all)]
pub(crate) fn collect_chart_template_evidence(
    charts: &[chart::ChartContext],
    include_tests: bool,
) -> CliResult<ChartTemplateEvidence> {
    let call_graph = build_helper_call_graph(charts, include_tests)?;
    let mut type_hints = BTreeMap::new();

    for chart in charts.iter().filter(|chart| !chart.is_library) {
        if let Some(text) = call_graph.chart_direct_body(&chart.values_prefix) {
            apply_template_evidence_to(&mut type_hints, text, &chart.values_prefix);
        }
        for helper_name in call_graph.reachable_from_chart(&chart.values_prefix) {
            if let Some(text) = call_graph.helper_body(&helper_name) {
                apply_template_evidence_to(&mut type_hints, text, &chart.values_prefix);
            }
        }
    }
    Ok(ChartTemplateEvidence {
        type_hints,
        #[cfg(test)]
        call_graph,
    })
}

fn apply_template_evidence_to(
    type_hints: &mut BTreeMap<String, Vec<Value>>,
    body_text: &str,
    prefix: &[String],
) {
    for (path, schema) in extract_default_type_hints(body_text) {
        let scoped = chart::scope_values_path(&path, prefix);
        type_hints.entry(scoped).or_default().push(schema);
    }
}
