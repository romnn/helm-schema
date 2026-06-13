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
//!     [`crate::generate_values_schema_for_chart_with_warnings`]
//!   - the six `infer_required_*` / `library_*` integration test
//!     files under `crates/helm-schema-cli/tests/`.
//!
//! Nothing in the core schema-generation pipeline depends on anything here.

use std::collections::BTreeSet;

use helm_schema_ir::ContractProjection;
use helm_schema_ir::required_inference::extract_default_fallback_paths;
use serde_json::Value;
use serde_yaml::Value as YamlValue;
use tracing::instrument;

use crate::HelperCallGraph;
use crate::chart::ChartContext;
use crate::scope_values_path;

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally.
///
/// `contract_projection` comes from the IR collection. `values_yaml` is the
/// composed values.yaml (used to re-derive top-level seeded keys — those must
/// not be marked required). `charts` and `call_graph` come from the IR
/// collection too and drive per-prefix fallback-path scoping.
#[instrument(skip_all)]
pub(crate) fn apply(
    schema: &mut Value,
    contract_projection: &ContractProjection,
    values_yaml: Option<&str>,
    charts: &[ChartContext],
    call_graph: &HelperCallGraph,
) {
    let synthetic_value_paths = top_level_value_paths(values_yaml);
    let default_fallback_paths = collect_fallback_paths(charts, call_graph);
    helm_schema_gen::required_inference::apply_required_inference(
        schema,
        contract_projection,
        &synthetic_value_paths,
        &default_fallback_paths,
    );
}

/// Re-derive the set of top-level `values.yaml` keys that the CLI
/// seeded as synthetic uses. Kept here (not exposed from
/// `seed_top_level_values_yaml_keys` in `lib.rs`) so the core seeding
/// function's contract stays narrow — only required-inference needs
/// to know which uses are synthetic.
fn top_level_value_paths(values_yaml: Option<&str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Some(yaml) = values_yaml else {
        return out;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(yaml) else {
        return out;
    };
    let YamlValue::Mapping(m) = doc else {
        return out;
    };
    for (k, _) in m {
        if let Some(key) = k.as_str() {
            let key = key.trim();
            if !key.is_empty() {
                out.insert(key.to_string());
            }
        }
    }
    out
}

/// Walk every non-library chart's reachable-helper closure (via the
/// shared call graph) and union the default-fallback paths extracted
/// from each reached body's text, scoped to the consuming chart's
/// value prefix.
///
/// Mirrors the shape of the type-hint extraction loop in
/// `collect_ir_for_charts` (lib.rs) but with the broader fallback-
/// path regex instead of the literal-only type-hint regex. Sharing
/// the call graph keeps scoping consistent between the two consumers
/// without re-deriving graph edges.
fn collect_fallback_paths(
    charts: &[ChartContext],
    call_graph: &HelperCallGraph,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for c in charts.iter().filter(|c| !c.is_library) {
        let prefix = &c.values_prefix;
        if let Some(text) = call_graph.chart_direct_body(prefix) {
            apply_fallback_paths_to(&mut out, text, prefix);
        }
        for helper_name in call_graph.reachable_from_chart(prefix) {
            if let Some(text) = call_graph.helper_body(&helper_name) {
                apply_fallback_paths_to(&mut out, text, prefix);
            }
        }
    }
    out
}

fn apply_fallback_paths_to(out: &mut BTreeSet<String>, body_text: &str, prefix: &[String]) {
    for path in extract_default_fallback_paths(body_text) {
        out.insert(scope_values_path(&path, prefix));
    }
}
