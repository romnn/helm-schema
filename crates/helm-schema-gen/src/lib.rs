//! JSON Schema lowering from normalized Helm contract signals.

mod base_schema;
mod condition_encoding;
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
use serde_yaml::Value as YamlValue;

use base_schema::{ConditionalTargetIndex, classify_base};
use condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use emission_policy::EmissionPolicy;
use emission_report::FactRecord;
use overlay_lowering::{
    LoweredConjunct, append_selected_constraints, append_terminal_clauses,
    collect_conditional_schemas, prepare_conditional_hosts,
};
use path_resolver::PathSchemaResolver;
use provider_definitions::{
    extract_provider_definitions, extract_repeated_provider_payloads, insert_definitions_into_root,
};
use schema_tree::{SchemaDocument, draft07_root_document};

pub use emission_policy::{EmissionClassKind, EmissionOrigin, SchemaProfile};
pub use emission_report::{
    CanonicalizationCounts, CarrierCounts, EmissionReport, FactCounts, MandatoryOutcomes,
    SelectionDifference, SelectionDifferenceDirection,
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
    /// Amount of analyzed contract evidence emitted into the schema.
    pub profile: SchemaProfile,
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
            profile: SchemaProfile::Full,
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
        self.profile = profile;
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
    let empty_values_descriptions = BTreeMap::new();
    let values_descriptions = input
        .values_descriptions
        .unwrap_or(&empty_values_descriptions);

    let mut values_yaml_doc = input
        .values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);
    values_yaml::apply_values_default_sources(
        &mut values_yaml_doc,
        input.contract_schema_signals.values_default_sources(),
    );
    let mut input_defaults_doc = values_yaml_doc.clone();
    values_yaml::remove_values_paths(
        &mut input_defaults_doc,
        input.shadowed_input_paths.unwrap_or(&BTreeSet::new()),
    );
    let mut subchart_defaults_doc = input
        .dependency_values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);
    // Chart-internal root merges (`set $ "Values" (mustMergeOverwrite
    // defaults .Values)`) fill their defaults at RENDER time, after any
    // null-deletion, so absence at such paths reads as the merged default
    // exactly like a dependency-owned key reads as its subchart default.
    values_yaml::copy_values_default_sources(
        &mut subchart_defaults_doc,
        &values_yaml_doc,
        input.contract_schema_signals.values_default_sources(),
    );

    let mut dependency_refill_doc = input
        .dependency_refill_values_yaml
        .and_then(|s| serde_yaml::from_str::<YamlValue>(s).ok())
        .unwrap_or(YamlValue::Null);
    values_yaml::copy_values_default_sources(
        &mut dependency_refill_doc,
        &values_yaml_doc,
        input.contract_schema_signals.values_default_sources(),
    );

    let (root_schema, report) = build_root_schema(
        input.contract_schema_signals,
        RootValuesDocuments {
            composed: &values_yaml_doc,
            input_defaults: &input_defaults_doc,
            subchart_defaults: &subchart_defaults_doc,
            dependency_refill: &dependency_refill_doc,
        },
        values_descriptions,
        input.provider,
        input.profile,
        completion_pass,
    );

    (draft07_root_document(root_schema), report)
}

/// The domain Go's `range` iterates without aborting: collections and nil
/// render; integer counts iterate through Helm's `--set` int64 channel
/// (JSON Schema cannot separate that from the failing values-file float64
/// spelling, so the renderable channel wins) unless the loop body reads
/// member structure integers cannot provide; strings and non-integral
/// numbers fail in every channel.
pub(crate) fn runtime_iterable_schema(allow_integer: bool) -> serde_json::Value {
    let mut arms = vec![
        serde_json::json!({ "type": "array" }),
        serde_json::json!({ "type": "object" }),
    ];
    if allow_integer {
        arms.push(serde_json::json!({ "type": "integer" }));
    }
    arms.push(serde_json::json!({ "type": "null" }));
    serde_json::json!({ "anyOf": arms })
}

#[derive(Clone, Copy)]
struct RootValuesDocuments<'a> {
    composed: &'a YamlValue,
    input_defaults: &'a YamlValue,
    subchart_defaults: &'a YamlValue,
    dependency_refill: &'a YamlValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionPass {
    Projected,
    ValuesDefaultBackfill,
    OpenGlobal,
    DeclaredDefaults,
    RepeatedProviderPayloads,
    SharedDefinitions,
    ProgramWrappers,
    Descriptions,
}

#[tracing::instrument(skip_all)]
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
fn build_root_schema(
    contract_schema_signals: &ContractSchemaSignals,
    documents: RootValuesDocuments<'_>,
    values_descriptions: &BTreeMap<String, String>,
    provider: &dyn ResourceSchemaOracle,
    profile: SchemaProfile,
    completion_pass: CompletionPass,
) -> (Value, EmissionReport) {
    let RootValuesDocuments {
        composed: values_yaml_doc,
        input_defaults: input_defaults_doc,
        subchart_defaults: subchart_defaults_doc,
        dependency_refill: dependency_refill_doc,
    } = documents;
    let mut root_schema = SchemaDocument::new_root_object();
    let path_resolver = PathSchemaResolver::new(
        contract_schema_signals,
        input_defaults_doc,
        subchart_defaults_doc,
        provider,
    );
    let mut resolved_paths = path_resolver.resolve_all();
    let all_conditional_schemas = collect_conditional_schemas(
        &resolved_paths,
        contract_schema_signals,
        values_yaml_doc,
        subchart_defaults_doc,
        provider,
    );
    // Keep conditional targets in base classification even when their arms
    // are omitted. This makes the reduced document the full document minus
    // constraints instead of reclassifying guarded evidence as unconditional.
    let conditional_targets = ConditionalTargetIndex::from_conditionals(&all_conditional_schemas);
    let policy = EmissionPolicy::for_profile(profile);
    debug_assert!(policy.is_valid());
    let mut report = EmissionReport::default();
    let mut conditional_schemas = Vec::new();
    let mut fact_index = 0;
    for conjunct in all_conditional_schemas {
        let selected = profile == SchemaProfile::Full;
        let projected_selected = policy.selects(&conjunct.class);
        report.record_fact(FactRecord {
            fact_index,
            class: &conjunct.class,
            origin: conjunct.origin,
            target_value_path: &conjunct.carrier.target_value_path,
            schema: &conjunct.schema,
            selected,
            projected_selected,
        });
        fact_index += 1;
        if selected {
            conditional_schemas.push(conjunct);
        }
    }
    let terminal_schemas = contract_schema_signals
        .terminal_clauses()
        .iter()
        .map(|guards| LoweredConjunct::terminal(guards.clone()))
        .filter(|conjunct| {
            let selected = profile == SchemaProfile::Full;
            let projected_selected = policy.selects(&conjunct.class);
            report.record_fact(FactRecord {
                fact_index,
                class: &conjunct.class,
                origin: conjunct.origin,
                target_value_path: &conjunct.carrier.target_value_path,
                schema: &conjunct.schema,
                selected,
                projected_selected,
            });
            fact_index += 1;
            selected
        })
        .collect::<Vec<_>>();
    let provider_definitions = extract_provider_definitions(
        &mut resolved_paths,
        &mut conditional_schemas,
        values_descriptions,
    );
    let accepted_values_root_paths = contract_schema_signals
        .schema_evidence_by_value_path()
        .values()
        .filter(|evidence| evidence.facts.accepted_values_root_fragment)
        .map(|evidence| split_value_path(&evidence.value_path))
        .collect::<Vec<_>>();
    let dependency_roots = contract_schema_signals
        .schema_evidence_by_value_path()
        .values()
        .filter(|evidence| evidence.facts.accepted_dependency_values_root_fragment)
        .map(|evidence| split_value_path(&evidence.value_path))
        .collect::<BTreeSet<_>>();
    let no_owning_ancestors = BTreeSet::new();
    let no_preserving_ancestors = BTreeSet::new();
    let base_span = tracing::info_span!("base_path_insertion").entered();
    let owning_paths = resolved_paths
        .iter()
        .filter(|resolved_path| {
            classify_base(
                resolved_path,
                &conditional_targets,
                &no_owning_ancestors,
                &no_preserving_ancestors,
            )
            .owns_descendants()
        })
        .map(|resolved_path| resolved_path.path_segments.clone())
        .collect::<BTreeSet<_>>();
    let preserving_paths = resolved_paths
        .iter()
        .filter(|resolved_path| {
            classify_base(
                resolved_path,
                &conditional_targets,
                &no_owning_ancestors,
                &no_preserving_ancestors,
            )
            .preserves_descendants()
        })
        .map(|resolved_path| resolved_path.path_segments.clone())
        .collect::<BTreeSet<_>>();
    for resolved_path in &resolved_paths {
        let owner = classify_base(
            resolved_path,
            &conditional_targets,
            &owning_paths,
            &preserving_paths,
        );
        let Some(schema) = owner.schema(resolved_path) else {
            continue;
        };
        let materialized_member_schema = schema.clone().into_value();
        if owner.replaces() {
            root_schema.replace_path_schema(&resolved_path.path_segments, schema);
        } else {
            root_schema.insert_path_schema(&resolved_path.path_segments, schema);
        }
        let Some((last, parent_segments)) = resolved_path.path_segments.split_last() else {
            continue;
        };
        if last != "*" {
            continue;
        }
        if parent_segments.iter().any(|segment| segment == "*")
            || !contract_schema_signals
                .evidence_for(&resolved_path.value_path)
                .is_some_and(|evidence| evidence.facts.is_direct_ranged_source)
        {
            continue;
        }
        let Some(declared) =
            values_yaml::yaml_value_at_segments(input_defaults_doc, parent_segments)
        else {
            continue;
        };
        root_schema.materialize_declared_member_schema(
            parent_segments,
            declared,
            &materialized_member_schema,
        );
    }
    drop(base_span);
    let absence = condition_encoding::AbsenceDefaults {
        deeper_stage: subchart_defaults_doc,
        dependency_refill: dependency_refill_doc,
        dependency_roots: &dependency_roots,
    };
    prepare_conditional_hosts(&mut root_schema, &mut conditional_schemas, &mut report);
    append_selected_constraints(
        &mut root_schema,
        conditional_schemas,
        values_yaml_doc,
        absence,
        &mut report,
    );
    if !terminal_schemas.is_empty() {
        let terminal_clauses = terminal_schemas
            .iter()
            .filter_map(LoweredConjunct::terminal_guards)
            .map(<[helm_schema_core::ConditionalGuard]>::to_vec)
            .collect::<Vec<_>>();
        append_terminal_clauses(
            &mut root_schema,
            &terminal_clauses,
            contract_schema_signals.values_default_sources(),
            values_yaml_doc,
            absence,
        );
    }
    if completion_pass == CompletionPass::Projected {
        return finish_build(root_schema.into_value(), report);
    }
    // A serialized path's schema is deliberately unconstrained; the
    // declared-default filler must keep the slot present without re-typing
    // it, exactly like a conditional target.
    let mut default_fill_skip_paths = conditional_targets.guarded_only_paths.clone();
    for resolved_path in &resolved_paths {
        if resolved_path.used_as_serialized {
            default_fill_skip_paths.insert(resolved_path.path_segments.clone());
        }
    }
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        if evidence.facts.used_as_yaml_serialized {
            default_fill_skip_paths.insert(split_value_path(value_path));
        }
    }
    // A directly ranged path accepts the runtime iterable domain, which is
    // wider than any declared default; the filler must not re-type it.
    for value_path in contract_schema_signals.direct_ranged_value_paths() {
        default_fill_skip_paths.insert(split_value_path(value_path));
    }
    // A member omitted before every provider sink is governed by its own
    // path evidence. Re-filling its declared default would restore the
    // parent contract that omission removed.
    for value_path in contract_schema_signals.unconditionally_omitted_value_paths() {
        default_fill_skip_paths.insert(split_value_path(value_path));
    }
    let fill_span = tracing::info_span!("default_fill_and_finish").entered();
    {
        let _span = tracing::info_span!("merge_missing_defaults").entered();
        root_schema.merge_missing_values_yaml_defaults_under_roots(
            input_defaults_doc,
            &accepted_values_root_paths,
            &default_fill_skip_paths,
        );
    }
    if completion_pass == CompletionPass::ValuesDefaultBackfill {
        return finish_build(root_schema.into_value(), report);
    }
    root_schema.open_helm_global_namespace();
    if completion_pass == CompletionPass::OpenGlobal {
        return finish_build(root_schema.into_value(), report);
    }

    let mut root_schema = root_schema.into_value();
    if let Ok(declared_defaults) = serde_json::to_value(input_defaults_doc)
        && declared_defaults.is_object()
    {
        let _span = tracing::info_span!("preserve_declared_defaults").entered();
        root_schema =
            resolve_policy::preserve_declared_default_in_schema(root_schema, &declared_defaults);
    }
    if completion_pass == CompletionPass::DeclaredDefaults {
        return finish_build(root_schema, report);
    }
    let mut provider_definitions = provider_definitions;
    {
        let _span = tracing::info_span!("extract_repeated_provider_payloads").entered();
        provider_definitions.extend(extract_repeated_provider_payloads(&mut root_schema));
    }
    if completion_pass == CompletionPass::RepeatedProviderPayloads {
        return finish_build(root_schema, report);
    }
    let truthy_span = tracing::info_span!("helm_truthy_scan").entered();
    if value_references_helm_truthy(&root_schema)
        || provider_definitions
            .values()
            .any(value_references_helm_truthy)
    {
        provider_definitions.insert(
            HELM_TRUTHY_DEFINITION_NAME.to_string(),
            helm_truthy_definition_schema(),
        );
    }
    for style in [
        helm_schema_core::QuotedScalarStyle::Double,
        helm_schema_core::QuotedScalarStyle::Single,
    ] {
        if quoted_serialization::value_references(&root_schema, style)
            || provider_definitions
                .values()
                .any(|definition| quoted_serialization::value_references(definition, style))
        {
            provider_definitions.insert(
                quoted_serialization::definition_name(style).to_string(),
                quoted_serialization::definition_schema(style),
            );
        }
    }
    drop(truthy_span);
    insert_definitions_into_root(&mut root_schema, provider_definitions);
    if completion_pass == CompletionPass::SharedDefinitions {
        return finish_build(root_schema, report);
    }
    {
        let _span = tracing::info_span!("apply_program_wrappers").entered();
        program_wrapper::apply_program_wrapper_alternatives(
            &mut root_schema,
            contract_schema_signals.values_program_wrappers(),
            contract_schema_signals.values_program_wrapper_exclusions(),
        );
    }
    if completion_pass == CompletionPass::ProgramWrappers {
        return finish_build(root_schema, report);
    }
    {
        let _span = tracing::info_span!("apply_values_descriptions").entered();
        schema_tree::apply_values_descriptions(&mut root_schema, values_descriptions);
    }
    drop(fill_span);
    finish_build(root_schema, report)
}

fn finish_build(schema: Value, mut report: EmissionReport) -> (Value, EmissionReport) {
    report.carriers = count_emitted_carriers(&schema, report.carriers.grouping_fan_in);
    (schema, report)
}

fn count_emitted_carriers(
    schema: &Value,
    grouping_fan_in: usize,
) -> emission_report::CarrierCounts {
    fn visit(schema: &Value, path_depth: usize, counts: &mut emission_report::CarrierCounts) {
        let Some(object) = schema.as_object() else {
            if let Some(items) = schema.as_array() {
                for item in items {
                    visit(item, path_depth, counts);
                }
            }
            return;
        };
        if object.contains_key("if") && object.contains_key("then") {
            counts.condition_nodes += 1;
            if path_depth == 0 {
                counts.root += 1;
            } else {
                counts.local += 1;
            }
        }
        for (key, child) in object {
            let child_depth = if matches!(
                key.as_str(),
                "properties" | "items" | "additionalProperties"
            ) {
                path_depth + 1
            } else {
                path_depth
            };
            visit(child, child_depth, counts);
        }
    }

    let mut counts = emission_report::CarrierCounts {
        grouping_fan_in,
        ..emission_report::CarrierCounts::default()
    };
    visit(schema, 0, &mut counts);
    counts
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
