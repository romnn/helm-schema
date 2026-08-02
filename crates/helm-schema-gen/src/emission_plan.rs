use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ContractSchemaSignals;
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::base_schema::{ConditionalTargetIndex, classify_base};
use crate::condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use crate::emission_policy::{EmissionClass, EmissionPolicy, SchemaProfile};
use crate::emission_report::{EmissionReport, FactRecord};
use crate::overlay_lowering::{
    ConditionalHostPreparation, LoweredConjunct, append_selected_constraints,
    append_terminal_clauses, collect_conditional_schemas, prepare_conditional_hosts,
};
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_definitions::{
    extract_provider_definitions, extract_repeated_provider_payloads, insert_definitions_into_root,
    prune_unreachable_provider_definitions,
};
use crate::schema_tree::{SchemaDocument, draft07_root_document};
use crate::{ValuesSchemaInput, split_value_path};

pub(crate) struct LoweredEmissionPlan {
    contract_schema_signals: ContractSchemaSignals,
    documents: RootValuesDocuments,
    values_descriptions: BTreeMap<String, String>,
    resolved_paths: Vec<ResolvedPathSchema>,
    conditional_schemas: Vec<LoweredConjunct>,
    terminal_schemas: Vec<LoweredConjunct>,
    support: EmissionSupportPlan,
}

#[derive(Clone)]
struct RootValuesDocuments {
    composed: YamlValue,
    input_defaults: YamlValue,
    subchart_defaults: YamlValue,
    dependency_refill: YamlValue,
}

struct EmissionSupportPlan {
    conditional_targets: ConditionalTargetIndex,
    owning_paths: BTreeSet<Vec<String>>,
    preserving_paths: BTreeSet<Vec<String>>,
    accepted_values_root_paths: Vec<Vec<String>>,
    dependency_roots: BTreeSet<Vec<String>>,
    default_fill_skip_paths: BTreeSet<Vec<String>>,
    conditional_hosts: ConditionalHostPreparation,
}

pub(crate) struct ProjectedTree {
    document: SchemaDocument,
    fact_accounting: FactAccounting,
    provider_definitions: BTreeMap<String, Value>,
}

struct FactAccounting {
    emission_report: EmissionReport,
}

pub(crate) struct CompletedGeneratedSchema {
    pub(crate) schema: Value,
    pub(crate) emission_report: EmissionReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionPass {
    Projected,
    ValuesDefaultBackfill,
    OpenGlobal,
    DeclaredDefaults,
    RepeatedProviderPayloads,
    SharedDefinitions,
    ProgramWrappers,
    Descriptions,
}

#[derive(Clone, Copy)]
enum ProjectionSelector {
    Legacy(SchemaProfile),
    Policy(EmissionPolicy),
}

impl ProjectionSelector {
    fn selected(self, class: &EmissionClass) -> bool {
        match self {
            Self::Legacy(profile) => profile == SchemaProfile::Full,
            Self::Policy(policy) => policy.selects(class),
        }
    }

    fn projected(self, class: &EmissionClass) -> bool {
        match self {
            Self::Legacy(profile) => EmissionPolicy::for_profile(profile).selects(class),
            Self::Policy(policy) => policy.selects(class),
        }
    }
}

impl LoweredEmissionPlan {
    #[tracing::instrument(skip_all)]
    pub(crate) fn build(input: &ValuesSchemaInput<'_>) -> Self {
        let contract_schema_signals = input.contract_schema_signals.clone();
        let mut composed = input
            .values_yaml
            .and_then(|source| serde_yaml::from_str::<YamlValue>(source).ok())
            .unwrap_or(YamlValue::Null);
        crate::values_yaml::apply_values_default_sources(
            &mut composed,
            contract_schema_signals.values_default_sources(),
        );
        let mut input_defaults = composed.clone();
        crate::values_yaml::remove_values_paths(
            &mut input_defaults,
            input.shadowed_input_paths.unwrap_or(&BTreeSet::new()),
        );
        let mut subchart_defaults = input
            .dependency_values_yaml
            .and_then(|source| serde_yaml::from_str::<YamlValue>(source).ok())
            .unwrap_or(YamlValue::Null);
        // Chart-internal root merges (`set $ "Values" (mustMergeOverwrite
        // defaults .Values)`) fill their defaults at render time, after any
        // null-deletion, so absence at such paths reads as the merged default
        // exactly like a dependency-owned key reads as its subchart default.
        crate::values_yaml::copy_values_default_sources(
            &mut subchart_defaults,
            &composed,
            contract_schema_signals.values_default_sources(),
        );
        let mut dependency_refill = input
            .dependency_refill_values_yaml
            .and_then(|source| serde_yaml::from_str::<YamlValue>(source).ok())
            .unwrap_or(YamlValue::Null);
        crate::values_yaml::copy_values_default_sources(
            &mut dependency_refill,
            &composed,
            contract_schema_signals.values_default_sources(),
        );
        let documents = RootValuesDocuments {
            composed,
            input_defaults,
            subchart_defaults,
            dependency_refill,
        };
        let resolved_paths = PathSchemaResolver::new(
            &contract_schema_signals,
            &documents.input_defaults,
            &documents.subchart_defaults,
            input.provider,
        )
        .resolve_all();
        let conditional_schemas = collect_conditional_schemas(
            &resolved_paths,
            &contract_schema_signals,
            &documents.composed,
            &documents.subchart_defaults,
            input.provider,
        );
        let terminal_schemas = contract_schema_signals
            .terminal_clauses()
            .iter()
            .map(|guards| LoweredConjunct::terminal(guards.clone()))
            .collect::<Vec<_>>();
        let support = EmissionSupportPlan::build(
            &contract_schema_signals,
            &documents,
            &resolved_paths,
            &conditional_schemas,
        );

        Self {
            contract_schema_signals,
            documents,
            values_descriptions: input.values_descriptions.cloned().unwrap_or_default(),
            resolved_paths,
            conditional_schemas,
            terminal_schemas,
            support,
        }
    }

    pub(crate) fn project(&self, policy: EmissionPolicy) -> ProjectedTree {
        debug_assert!(policy.is_valid());
        self.project_with(ProjectionSelector::Policy(policy))
    }

    pub(crate) fn project_legacy(&self, profile: SchemaProfile) -> ProjectedTree {
        self.project_with(ProjectionSelector::Legacy(profile))
    }

    fn project_with(&self, selector: ProjectionSelector) -> ProjectedTree {
        let mut emission_report = EmissionReport::default();
        let mut selected_indices = Vec::new();
        let mut selected_conditionals = Vec::new();
        let mut fact_index = 0;
        for (conditional_index, conjunct) in self.conditional_schemas.iter().enumerate() {
            let selected = selector.selected(&conjunct.class);
            emission_report.record_fact(FactRecord {
                fact_index,
                class: &conjunct.class,
                origin: conjunct.origin,
                target_value_path: &conjunct.carrier.target_value_path,
                schema: &conjunct.schema,
                selected,
                projected_selected: selector.projected(&conjunct.class),
            });
            fact_index += 1;
            if selected {
                selected_indices.push(conditional_index);
                selected_conditionals.push(conjunct.clone());
            }
        }
        let selected_terminals = self
            .terminal_schemas
            .iter()
            .filter(|conjunct| {
                let selected = selector.selected(&conjunct.class);
                emission_report.record_fact(FactRecord {
                    fact_index,
                    class: &conjunct.class,
                    origin: conjunct.origin,
                    target_value_path: &conjunct.carrier.target_value_path,
                    schema: &conjunct.schema,
                    selected,
                    projected_selected: selector.projected(&conjunct.class),
                });
                fact_index += 1;
                selected
            })
            .cloned()
            .collect::<Vec<_>>();

        // Candidate metadata is consumed only after selection. The shared
        // plan stays immutable, and each projection receives fresh payloads.
        let mut resolved_paths = self.resolved_paths.clone();
        let mut provider_definitions = extract_provider_definitions(
            &mut resolved_paths,
            &mut selected_conditionals,
            &self.values_descriptions,
        );
        let mut document = materialize_base_document(
            &self.contract_schema_signals,
            &self.documents.input_defaults,
            &resolved_paths,
            &self.support,
        );
        let folded_fact_indices = self.support.conditional_hosts.apply(&mut document);
        let mut emitted_conditionals = Vec::new();
        for (conditional_index, conjunct) in selected_indices.into_iter().zip(selected_conditionals)
        {
            if folded_fact_indices.contains(&conditional_index) {
                if matches!(conjunct.class, EmissionClass::Mandatory) {
                    emission_report.mandatory_outcomes.equivalent += 1;
                }
            } else {
                emitted_conditionals.push(conjunct);
            }
        }

        let absence = crate::condition_encoding::AbsenceDefaults {
            deeper_stage: &self.documents.subchart_defaults,
            dependency_refill: &self.documents.dependency_refill,
            dependency_roots: &self.support.dependency_roots,
        };
        append_selected_constraints(
            &mut document,
            emitted_conditionals,
            &self.documents.composed,
            absence,
            &mut emission_report,
        );
        if !selected_terminals.is_empty() {
            let terminal_clauses = selected_terminals
                .iter()
                .filter_map(LoweredConjunct::terminal_guards)
                .map(<[helm_schema_core::ConditionalGuard]>::to_vec)
                .collect::<Vec<_>>();
            append_terminal_clauses(
                &mut document,
                &terminal_clauses,
                self.contract_schema_signals.values_default_sources(),
                &self.documents.composed,
                absence,
            );
        }
        prune_unreachable_provider_definitions(&document, &mut provider_definitions);

        ProjectedTree {
            document,
            fact_accounting: FactAccounting { emission_report },
            provider_definitions,
        }
    }

    pub(crate) fn complete(
        &self,
        projected: ProjectedTree,
        completion_pass: CompletionPass,
    ) -> CompletedGeneratedSchema {
        let ProjectedTree {
            mut document,
            fact_accounting,
            mut provider_definitions,
        } = projected;
        let emission_report = fact_accounting.emission_report;
        if completion_pass == CompletionPass::Projected {
            return finish_generated(document.into_value(), emission_report);
        }

        let fill_span = tracing::info_span!("default_fill_and_finish").entered();
        {
            let _span = tracing::info_span!("merge_missing_defaults").entered();
            document.merge_missing_values_yaml_defaults_under_roots(
                &self.documents.input_defaults,
                &self.support.accepted_values_root_paths,
                &self.support.default_fill_skip_paths,
            );
        }
        if completion_pass == CompletionPass::ValuesDefaultBackfill {
            return finish_generated(document.into_value(), emission_report);
        }
        document.open_helm_global_namespace();
        if completion_pass == CompletionPass::OpenGlobal {
            return finish_generated(document.into_value(), emission_report);
        }

        let mut schema = document.into_value();
        if let Ok(declared_defaults) = serde_json::to_value(&self.documents.input_defaults)
            && declared_defaults.is_object()
        {
            let _span = tracing::info_span!("preserve_declared_defaults").entered();
            schema = crate::resolve_policy::preserve_declared_default_in_schema(
                schema,
                &declared_defaults,
            );
        }
        if completion_pass == CompletionPass::DeclaredDefaults {
            return finish_generated(schema, emission_report);
        }
        {
            let _span = tracing::info_span!("extract_repeated_provider_payloads").entered();
            provider_definitions.extend(extract_repeated_provider_payloads(&mut schema));
        }
        if completion_pass == CompletionPass::RepeatedProviderPayloads {
            return finish_generated(schema, emission_report);
        }
        let truthy_span = tracing::info_span!("helm_truthy_scan").entered();
        if value_references_helm_truthy(&schema)
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
            if crate::quoted_serialization::value_references(&schema, style)
                || provider_definitions.values().any(|definition| {
                    crate::quoted_serialization::value_references(definition, style)
                })
            {
                provider_definitions.insert(
                    crate::quoted_serialization::definition_name(style).to_string(),
                    crate::quoted_serialization::definition_schema(style),
                );
            }
        }
        drop(truthy_span);
        insert_definitions_into_root(&mut schema, provider_definitions);
        if completion_pass == CompletionPass::SharedDefinitions {
            return finish_generated(schema, emission_report);
        }
        {
            let _span = tracing::info_span!("apply_program_wrappers").entered();
            crate::program_wrapper::apply_program_wrapper_alternatives(
                &mut schema,
                self.contract_schema_signals.values_program_wrappers(),
                self.contract_schema_signals
                    .values_program_wrapper_exclusions(),
            );
        }
        if completion_pass == CompletionPass::ProgramWrappers {
            return finish_generated(schema, emission_report);
        }
        {
            let _span = tracing::info_span!("apply_values_descriptions").entered();
            crate::schema_tree::apply_values_descriptions(&mut schema, &self.values_descriptions);
        }
        drop(fill_span);
        finish_generated(schema, emission_report)
    }
}

impl EmissionSupportPlan {
    fn build(
        contract_schema_signals: &ContractSchemaSignals,
        documents: &RootValuesDocuments,
        resolved_paths: &[ResolvedPathSchema],
        conditional_schemas: &[LoweredConjunct],
    ) -> Self {
        // Keep conditional targets in base classification even when their
        // arms are omitted. A reduced document is the full document minus
        // constraints, not a reclassification of guarded evidence.
        let conditional_targets = ConditionalTargetIndex::from_conditionals(conditional_schemas);
        let no_owning_ancestors = BTreeSet::new();
        let no_preserving_ancestors = BTreeSet::new();
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
        // A serialized path's schema is deliberately unconstrained; the
        // declared-default filler keeps the slot without re-typing it,
        // exactly like a conditional target.
        let mut default_fill_skip_paths = conditional_targets.guarded_only_paths.clone();
        for resolved_path in resolved_paths {
            if resolved_path.used_as_serialized {
                default_fill_skip_paths.insert(resolved_path.path_segments.clone());
            }
        }
        for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
            if evidence.facts.used_as_yaml_serialized {
                default_fill_skip_paths.insert(split_value_path(value_path));
            }
        }
        // A directly ranged path accepts the runtime iterable domain, which
        // is wider than any declared default; the filler must not re-type it.
        for value_path in contract_schema_signals.direct_ranged_value_paths() {
            default_fill_skip_paths.insert(split_value_path(value_path));
        }
        // A member omitted before every provider sink is governed by its own
        // evidence. Refilling its default would restore the removed parent
        // contract.
        for value_path in contract_schema_signals.unconditionally_omitted_value_paths() {
            default_fill_skip_paths.insert(split_value_path(value_path));
        }
        let mut support = Self {
            conditional_targets,
            owning_paths,
            preserving_paths,
            accepted_values_root_paths,
            dependency_roots,
            default_fill_skip_paths,
            conditional_hosts: ConditionalHostPreparation::default(),
        };
        let mut support_document = materialize_base_document(
            contract_schema_signals,
            &documents.input_defaults,
            resolved_paths,
            &support,
        );
        support.conditional_hosts =
            prepare_conditional_hosts(&mut support_document, conditional_schemas);
        support
    }
}

fn materialize_base_document(
    contract_schema_signals: &ContractSchemaSignals,
    input_defaults: &YamlValue,
    resolved_paths: &[ResolvedPathSchema],
    support: &EmissionSupportPlan,
) -> SchemaDocument {
    let mut document = SchemaDocument::new_root_object();
    let base_span = tracing::info_span!("base_path_insertion").entered();
    for resolved_path in resolved_paths {
        let owner = classify_base(
            resolved_path,
            &support.conditional_targets,
            &support.owning_paths,
            &support.preserving_paths,
        );
        let Some(schema) = owner.schema(resolved_path) else {
            continue;
        };
        let materialized_member_schema = schema.clone().into_value();
        if owner.replaces() {
            document.replace_path_schema(&resolved_path.path_segments, schema);
        } else {
            document.insert_path_schema(&resolved_path.path_segments, schema);
        }
        let Some((last, parent_segments)) = resolved_path.path_segments.split_last() else {
            continue;
        };
        if last != "*"
            || parent_segments.iter().any(|segment| segment == "*")
            || !contract_schema_signals
                .evidence_for(&resolved_path.value_path)
                .is_some_and(|evidence| evidence.facts.is_direct_ranged_source)
        {
            continue;
        }
        let Some(declared) =
            crate::values_yaml::yaml_value_at_segments(input_defaults, parent_segments)
        else {
            continue;
        };
        document.materialize_declared_member_schema(
            parent_segments,
            declared,
            &materialized_member_schema,
        );
    }
    drop(base_span);
    document
}

fn finish_generated(
    schema: Value,
    mut emission_report: EmissionReport,
) -> CompletedGeneratedSchema {
    emission_report.carriers =
        count_emitted_carriers(&schema, emission_report.carriers.grouping_fan_in);
    CompletedGeneratedSchema {
        schema: draft07_root_document(schema),
        emission_report,
    }
}

fn count_emitted_carriers(
    schema: &Value,
    grouping_fan_in: usize,
) -> crate::emission_report::CarrierCounts {
    fn visit(
        schema: &Value,
        path_depth: usize,
        counts: &mut crate::emission_report::CarrierCounts,
    ) {
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

    let mut counts = crate::emission_report::CarrierCounts {
        grouping_fan_in,
        ..crate::emission_report::CarrierCounts::default()
    };
    visit(schema, 0, &mut counts);
    counts
}
