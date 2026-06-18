mod extraction;
mod helper_call_graph;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use serde_json::Value;

pub(crate) use extraction::collect_chart_template_evidence;

/// Template-derived chart evidence that is not part of the manifest contract.
///
/// This is the remaining compatibility fallback for default-literal type
/// hints that are not yet represented end-to-end inside the contract artifact.
pub(crate) struct ChartTemplateEvidence {
    pub(crate) type_hints: BTreeMap<String, Vec<Value>>,
    #[cfg(test)]
    call_graph: helper_call_graph::HelperCallGraph,
}

impl ChartTemplateEvidence {
    #[cfg(test)]
    pub(crate) fn reachable_helpers_from_chart(
        &self,
        prefix: &[String],
    ) -> std::collections::BTreeSet<String> {
        self.call_graph.reachable_from_chart(prefix)
    }
}
