//! CLI-level orchestration for the heuristic `--infer-required`
//! feature. Lives in its own module so the entire feature can be
//! removed by deleting:
//!
//!   - this file
//!   - `helm-schema-gen/src/required_inference.rs`
//!   - the `infer_required` field on [`crate::ChartArgs`] and
//!     [`crate::GenerateOptions`]
//!   - the conditional call in
//!     `generation::generate_values_schema_for_chart_output`
//!   - the six `infer_required_*` / `library_*` integration test
//!     files under `crates/helm-schema-cli/tests/`.
//!
//! Nothing in the core schema-generation pipeline depends on anything here.

use helm_schema_engine::ContractPathSchemaEvidence;
use serde_json::Value;
use tracing::instrument;

use crate::values_roots;

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally.
///
/// `values_yaml` is the composed values.yaml (used to re-derive explicit chart
/// defaults — those must not be marked required).
#[instrument(skip_all)]
pub(crate) fn apply(
    schema: &mut Value,
    schema_evidence_by_value_path: &std::collections::BTreeMap<String, ContractPathSchemaEvidence>,
    values_yaml: Option<&str>,
) {
    let explicit_value_paths = values_roots::explicit_value_paths(values_yaml);
    helm_schema_engine::required_inference::apply_required_inference(
        schema,
        schema_evidence_by_value_path,
        &explicit_value_paths,
    );
}
