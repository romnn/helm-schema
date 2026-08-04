//! JSON Schema lowering from normalized Helm contract signals.

mod base_schema;
#[cfg(feature = "bench-support")]
pub mod bench_support;
mod condition_encoding;
mod emission_plan;
mod emission_policy;
mod emission_report;
mod foreign_schema;
mod merge;
mod overlay_lowering;
mod path_resolver;
mod path_schema;
mod program_wrapper;
mod provider_definitions;
mod provider_schema;
mod quoted_serialization;
pub mod required_inference;
mod required_source_backprojection;
mod resolve_policy;
mod schema_model;
mod schema_node;
mod schema_tree;
mod values_yaml;

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{ContractSchemaSignals, ResourceSchemaOracle};
use serde_json::Value;

pub(crate) use emission_plan::CompletionPass;
use emission_plan::LoweredEmissionPlan;
pub use emission_policy::{
    ConditionalAnchors, EmissionClassKind, EmissionOrigin, EmissionPolicy, EmissionPolicyDelta,
    EmissionSelection, InvalidEmissionPolicy, POLICY_VOCABULARY_VERSION, ResolvedEmissionPolicy,
    SchemaProfile,
};
pub use emission_report::{
    CanonicalizationCounts, CarrierCounts, EmissionReport, FactCounts, InsertionAbstentionCounts,
    MandatoryOutcomes,
};

/// Inputs for JSON Schema generation from the current contract schema signals.
///
/// The generated schema is derived from the contract-layer signal bundle plus
/// optional structural signals collected by earlier analysis phases.
/// Values-file descriptions are metadata only: they are applied only to schema
/// nodes that already exist from template or values evidence.
#[derive(Clone, Copy)]
pub struct ValuesSchemaInput<'a> {
    /// Path-local static-analysis facts prepared by contract finalization.
    pub contract_schema_signals: &'a ContractSchemaSignals,
    /// Resource-schema oracle used to constrain rendered Kubernetes fields.
    pub provider: &'a dyn ResourceSchemaOracle,
    /// Composed chart values defaults, when available.
    pub values_yaml: Option<&'a str>,
    /// ONLY the dependency charts' declared defaults, composed under
    /// their value prefixes. A key present here fills at the SUBCHART's
    /// coalesce stage even when the parent-level document misses it —
    /// including after a parent-level null-deletion — so absence at such
    /// paths reads as the subchart default instead of nil. When absent,
    /// every missing key reads as nil.
    pub dependency_values_yaml: Option<&'a str>,
    /// The same dependency defaults WITHOUT the parent-declared
    /// subtraction: what helm refills a missing or null dependency values
    /// root with. Absence below such a root reads as nil only while the
    /// root survives — deleting the root itself hands the whole subtree
    /// back to the subchart's own defaults, and only the keys they miss
    /// stay gone.
    pub dependency_refill_values_yaml: Option<&'a str>,
    /// Descendant `global.*` input paths hidden by an ancestor chart's
    /// declared global value. Helm accepts these paths but never exposes
    /// them to the descendant consumer.
    pub shadowed_input_paths: Option<&'a BTreeSet<String>>,
    /// Documentation strings keyed by canonical values path.
    pub values_descriptions: Option<&'a BTreeMap<String, String>>,
    /// Complete valid policy selecting analyzed contract evidence.
    pub emission_policy: EmissionPolicy,
}

impl<'a> ValuesSchemaInput<'a> {
    /// Creates schema input with contract signals and a resource provider.
    pub fn new(
        contract_schema_signals: &'a ContractSchemaSignals,
        provider: &'a dyn ResourceSchemaOracle,
    ) -> Self {
        Self {
            contract_schema_signals,
            provider,
            values_yaml: None,
            dependency_values_yaml: None,
            dependency_refill_values_yaml: None,
            shadowed_input_paths: None,
            values_descriptions: None,
            emission_policy: SchemaProfile::Full.resolved_policy().policy(),
        }
    }

    /// Attaches composed chart values defaults.
    #[must_use]
    pub fn with_values_yaml(mut self, values_yaml: Option<&'a str>) -> Self {
        self.values_yaml = values_yaml;
        self
    }

    /// Attaches dependency defaults composed beneath subchart prefixes.
    #[must_use]
    pub fn with_dependency_values_yaml(mut self, dependency_values_yaml: Option<&'a str>) -> Self {
        self.dependency_values_yaml = dependency_values_yaml;
        self
    }

    /// Attaches the defaults a deleted dependency values root refills with.
    #[must_use]
    pub fn with_dependency_refill_values_yaml(
        mut self,
        dependency_refill_values_yaml: Option<&'a str>,
    ) -> Self {
        self.dependency_refill_values_yaml = dependency_refill_values_yaml;
        self
    }

    /// Marks accepted values paths that Helm shadows before template evaluation.
    #[must_use]
    pub fn with_shadowed_input_paths(mut self, shadowed_input_paths: &'a BTreeSet<String>) -> Self {
        self.shadowed_input_paths = Some(shadowed_input_paths);
        self
    }

    /// Attaches values-file descriptions as output metadata.
    #[must_use]
    pub fn with_values_descriptions(
        mut self,
        values_descriptions: &'a BTreeMap<String, String>,
    ) -> Self {
        self.values_descriptions = Some(values_descriptions);
        self
    }

    /// Selects the schema emission profile.
    #[must_use]
    pub fn with_profile(mut self, profile: SchemaProfile) -> Self {
        self.emission_policy = profile.resolved_policy().policy();
        self
    }

    /// Selects an already validated emission policy.
    #[must_use]
    pub fn with_emission_policy(mut self, policy: EmissionPolicy) -> Self {
        self.emission_policy = policy;
        self
    }
}

/// Generate a JSON Schema with chart-authored values-file descriptions.
///
/// The output schema has no `required` arrays inferred by helm-schema; callers
/// that want that behaviour layer [`required_inference::apply_required_inference`]
/// on top of the returned schema. Keeping required-inference outside this
/// function isolates a heuristic feature from the core schema-generation
/// pipeline.
#[tracing::instrument(skip_all)]
#[expect(
    clippy::large_types_passed_by_value,
    reason = "the input is a Copy bundle of borrows built by chained `with_*` calls, and generation runs once per chart"
)]
pub fn generate_values_schema(input: ValuesSchemaInput<'_>) -> Value {
    generate_values_schema_with_report(input).0
}

/// Generates a JSON Schema and the fact-level accounting from the same emitter run.
///
/// The report describes generator emission before caller-owned overrides and
/// output-pipeline transforms.
#[tracing::instrument(skip_all)]
#[expect(
    clippy::large_types_passed_by_value,
    reason = "the input is a Copy bundle of borrows built by chained `with_*` calls, and generation runs once per chart"
)]
pub fn generate_values_schema_with_report(input: ValuesSchemaInput<'_>) -> (Value, EmissionReport) {
    generate_values_schema_through(&input, CompletionPass::Descriptions)
}

fn generate_values_schema_through(
    input: &ValuesSchemaInput<'_>,
    completion_pass: CompletionPass,
) -> (Value, EmissionReport) {
    let plan = LoweredEmissionPlan::build(input);
    let projected = plan.project(input.emission_policy);
    let completed = plan.complete(projected, completion_pass);
    (completed.schema, completed.emission_report)
}

/// The domain Go's `range` iterates without aborting: collections and nil
/// render; integer counts iterate through Helm's `--set` int64 channel
/// (JSON Schema cannot separate that from the failing values-file float64
/// spelling, so the renderable channel wins) unless the loop body reads
/// member structure integers cannot provide; strings and non-integral
/// numbers fail in every channel.
pub(crate) fn runtime_iterable_schema(allow_integer: bool) -> serde_json::Value {
    let mut types = vec!["array", "object"];
    if allow_integer {
        types.push("integer");
    }
    types.push("null");
    crate::schema_model::type_union_schema(types)
}

pub(crate) use helm_schema_core::split_value_path;

fn common_prefix_len(left: &[String], right: &[String]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
