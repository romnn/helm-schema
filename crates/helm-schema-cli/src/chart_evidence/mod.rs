mod extraction;
mod helper_call_graph;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

pub(crate) use extraction::collect_chart_template_evidence;

/// Template-derived chart evidence that is not part of the manifest contract.
///
/// This contains helper-reachability-dependent evidence that still feeds
/// output policies: default literal type hints and the broader fallback paths
/// used by the optional `--infer-required` pass.
pub(crate) struct ChartTemplateEvidence {
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    pub(crate) default_fallback_paths: BTreeSet<String>,
    #[cfg(test)]
    call_graph: helper_call_graph::HelperCallGraph,
}

impl ChartTemplateEvidence {
    #[cfg(test)]
    pub(crate) fn reachable_helpers_from_chart(&self, prefix: &[String]) -> BTreeSet<String> {
        self.call_graph.reachable_from_chart(prefix)
    }
}
