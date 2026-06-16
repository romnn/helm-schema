//! CLI-level orchestration for the heuristic `--infer-required`
//! feature. Lives in its own module so the entire feature can be
//! removed by deleting:
//!
//!   - this file
//!   - `helm-schema-gen/src/required_inference.rs`
//!   - `helm-schema-ir/src/required_inference.rs`
//!   - the `infer_required` field on [`crate::ChartArgs`] and
//!     [`crate::GenerateOptions`]
//!   - the conditional call in
//!     `generation::generate_values_schema_for_chart_output`
//!   - the `default_fallback_paths` field in [`crate::chart_evidence::ChartTemplateEvidence`]
//!   - the six `infer_required_*` / `library_*` integration test
//!     files under `crates/helm-schema-cli/tests/`.
//!
//! Nothing in the core schema-generation pipeline depends on anything here.

use std::collections::BTreeSet;

use helm_schema_engine::RequiredInferenceSignals;
use serde_json::Value;
use tracing::instrument;

use crate::values_roots;

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally.
///
/// `required_inference_signals` comes from the contract schema-signal bundle.
/// `values_yaml` is the composed values.yaml (used to re-derive top-level
/// seeded keys — those must not be marked required). `default_fallback_paths`
/// is template evidence scoped to the consuming chart.
#[instrument(skip_all)]
pub(crate) fn apply(
    schema: &mut Value,
    required_inference_signals: &RequiredInferenceSignals,
    values_yaml: Option<&str>,
    default_fallback_paths: &BTreeSet<String>,
) {
    let synthetic_value_paths = values_roots::top_level_value_paths(values_yaml);
    helm_schema_engine::required_inference::apply_required_inference(
        schema,
        required_inference_signals,
        &synthetic_value_paths,
        default_fallback_paths,
    );
}
