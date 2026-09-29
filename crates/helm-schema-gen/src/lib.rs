//! JSON Schema lowering from normalized Helm contract signals.

mod base_schema;
#[cfg(feature = "bench-support")]
pub mod bench_support;
mod condition_encoding;
mod emission_plan;
mod emission_policy;
mod emission_report;
mod foreign_schema;
mod generation_decisions;
mod merge;
mod overlay_lowering;
mod path_resolver;
mod path_schema;
mod program_wrapper;
mod provider_definitions;
mod provider_requirement_synthesis;
mod provider_resolution;
mod provider_schema;
mod quoted_serialization;
pub mod required_inference;
mod requirement_domain;
mod resolve_policy;
mod schema_model;
mod schema_node;
mod schema_tree;
mod uncoalesced_root;
mod values_yaml;

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{ContractSchemaSignals, ResourceSchemaOracle};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

pub use base_schema::BaseOwner;
use emission_plan::LoweredEmissionPlan;
pub use emission_policy::{
    ConditionalAnchors, EmissionClassKind, EmissionOrigin, EmissionPolicy, EmissionPolicyDelta,
    EmissionSelection, InvalidEmissionPolicy, POLICY_VOCABULARY_VERSION, ResolvedEmissionPolicy,
    SchemaProfile,
};
pub use emission_report::{
    CanonicalizationCounts, CarrierCounts, EmissionReport, FactCounts, InsertionAbstentionCounts,
    LintDocument, LintOutcome, LintWithdrawal, MandatoryOutcomes,
};
pub use generation_decisions::{
    BaseOwnerDecision, BaseOwnerRule, ChannelAdjustment, ChannelDisposition, ContainmentCheck,
    ContainmentDecision, ContainmentShortCircuit, DropReason, FalsyEscapeReason,
    GenerationDecisions, ImplicationRef, IndependentChannels, IndependentQualification, MergeBase,
    NullAdmissionReason, OverlayResolution, PathGenerationDecision, PathResolution,
    PolicyEvaluation, PolicyRule, QualifiedContract,
};
pub use overlay_lowering::ConditionalBaseEffect;
pub use schema_node::JsonSchemaType;

/// Parsed values documents consumed together during schema lowering.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedValuesDocuments {
    composed: YamlValue,
    dependency_refill: YamlValue,
    uncoalesced_root: Option<YamlValue>,
}

impl PreparedValuesDocuments {
    /// Creates the complete values-document bundle prepared by the caller.
    #[must_use]
    pub fn new(composed: YamlValue, dependency_refill: YamlValue) -> Self {
        Self {
            composed,
            dependency_refill,
            uncoalesced_root: None,
        }
    }

    /// Adds the root chart's `values.yaml` exactly as written, which
    /// `helm lint` validates without coalescing dependency defaults into
    /// it. The generated schema must accept it as well as the coalesced
    /// defaults.
    #[must_use]
    pub fn with_uncoalesced_root(mut self, uncoalesced_root: YamlValue) -> Self {
        self.uncoalesced_root = Some(uncoalesced_root);
        self
    }
}

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
    /// Parsed chart and dependency values documents, when available.
    ///
    /// The composed document is the coalesced defaults every schema is
    /// generated against. A key missing from a validated document reads as
    /// nil whatever this document declares for it, because Helm validates
    /// AFTER coalescing: the declared default is already applied there, so
    /// the key can only be missing because an explicit `null` deleted it.
    ///
    /// The dependency-refill document contains the dependency charts' own
    /// declared defaults: what Helm refills a missing or null dependency
    /// values root with. Deleting the root itself hands the whole subtree
    /// back to the subchart's own defaults, and only the keys they miss
    /// stay gone.
    pub values_documents: Option<&'a PreparedValuesDocuments>,
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
            values_documents: None,
            shadowed_input_paths: None,
            values_descriptions: None,
            emission_policy: SchemaProfile::Full.resolved_policy().policy(),
        }
    }

    /// Attaches the parsed chart and dependency values documents.
    #[must_use]
    pub fn with_values_documents(mut self, values_documents: &'a PreparedValuesDocuments) -> Self {
        self.values_documents = Some(values_documents);
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
pub fn generate_values_schema(input: ValuesSchemaInput<'_>) -> Value {
    generate_values_schema_with_report(input).schema
}

/// One emitter run: the schema, its accounting, and its definition names.
#[derive(Debug, Clone)]
pub struct GeneratedValuesSchema {
    /// The generated JSON Schema.
    /// Shared provider definitions are keyed by private handles.
    pub schema: Value,
    /// Fact-level accounting before caller-owned overrides and
    /// output-pipeline transforms.
    pub emission_report: EmissionReport,
    /// Every private definition handle in `schema`, with the provider
    /// documents and pointers its content came from.
    ///
    /// Name them only after every size-driven extraction decision, as
    /// `helm_schema_json_schema_minify::minimize_schema` does, so no readable
    /// spelling influences those decisions.
    pub definition_origins: BTreeMap<String, Vec<helm_schema_json_schema_minify::DefinitionOrigin>>,
    /// The per-path decisions this run took while resolving, lowering and
    /// materializing the schema.
    pub generation_decisions: GenerationDecisions,
}

/// Generates a JSON Schema and the fact-level accounting from the same emitter run.
#[tracing::instrument(skip_all)]
pub fn generate_values_schema_with_report(input: ValuesSchemaInput<'_>) -> GeneratedValuesSchema {
    let plan = LoweredEmissionPlan::build(&input);
    let mut projected = plan.project(input.emission_policy);
    let base_owners = std::mem::take(&mut projected.base_owners);
    let completed = plan.complete(projected);
    GeneratedValuesSchema {
        schema: completed.schema,
        emission_report: completed.emission_report,
        definition_origins: completed.definition_origins,
        generation_decisions: plan.into_generation_decisions(base_owners),
    }
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
