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

use std::collections::BTreeSet;

use helm_schema_ir::ContractPathSchemaEvidence;
use serde_json::Value;
use tracing::instrument;

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally.
#[instrument(skip_all)]
pub(crate) fn apply(
    schema: &mut Value,
    schema_evidence_by_value_path: &std::collections::BTreeMap<String, ContractPathSchemaEvidence>,
    explicit_value_paths: &BTreeSet<String>,
) {
    helm_schema_gen::required_inference::apply_required_inference(
        schema,
        schema_evidence_by_value_path,
        explicit_value_paths,
    );
}
