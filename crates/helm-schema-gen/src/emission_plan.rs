use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ContractSchemaSignals;
use helm_schema_json_schema_minify::DefinitionOrigin;
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::base_schema::{BaseOwner, ConditionalTargetIndex, classify_base};
use crate::condition_encoding::{
    HELM_TRUTHY_DEFINITION_NAME, helm_truthy_definition_schema, value_references_helm_truthy,
};
use crate::emission_policy::{EmissionClass, EmissionOrigin, EmissionPolicy};
use crate::emission_report::{EmissionReport, FactRecord, InsertionAbstentionCounts};
use crate::generation_decisions::{BaseOwnerDecision, GenerationDecisions};
use crate::overlay_lowering::{
    ConditionalHostPreparation, LoweredConjunct, append_selected_constraints,
    append_terminal_clauses, collect_conditional_schemas, prepare_conditional_hosts,
};
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_definitions::{
    extract_provider_definitions, extract_repeated_provider_payloads, insert_definitions_into_root,
    prune_unreachable_provider_definitions,
};
use crate::schema_tree::{
    CanonicalConstraintApplication, CanonicalConstraintOutcome, SchemaDocument,
    draft07_root_document,
};
use crate::uncoalesced_root::UncoalescedRootGate;
use crate::{AuthoringPolicy, ValuesSchemaInput};

pub(crate) struct LoweredEmissionPlan {
    predicate_memo: helm_schema_core::PredicateMemo,
    contract_schema_signals: ContractSchemaSignals,
    documents: RootValuesDocuments,
    values_descriptions: BTreeMap<String, String>,
    resolved_paths: Vec<ResolvedPathSchema>,
    conditional_schemas: Vec<LoweredConjunct>,
    terminal_schemas: Vec<LoweredConjunct>,
    support: EmissionSupportPlan,
    insertion_abstentions: InsertionAbstentionCounts,
    decisions: GenerationDecisions,
    authoring_policy: AuthoringPolicy,
}

#[derive(Clone)]
pub(crate) struct RootValuesDocuments {
    composed: YamlValue,
    /// The composed defaults with every subchart enabled, before
    /// render-time default merges, as JSON with null-valued keys deleted
    /// the way Helm's coalescing leaves them. Not Helm's defaults-only
    /// coalesce: a subchart a condition disables still contributes, which
    /// is the document the lint gate needs, since an override can enable it.
    coalesced_defaults: Value,
    /// The root `values.yaml` as written, when the caller supplied it.
    uncoalesced_root: Option<Value>,
    input_defaults: YamlValue,
    runtime_defaults: YamlValue,
    dependency_refill: YamlValue,
    guarded: Vec<GuardedRootValuesDocuments>,
}

#[derive(Clone)]
struct GuardedRootValuesDocuments {
    guards: Vec<helm_schema_core::ConditionalGuard>,
    composed: YamlValue,
    runtime_defaults: YamlValue,
    dependency_refill: YamlValue,
}

impl RootValuesDocuments {
    pub(crate) fn condition_context<'a>(
        &'a self,
        guards: &[helm_schema_core::ConditionalGuard],
        dependency_roots: &'a BTreeSet<Vec<String>>,
        predicate_memo: &helm_schema_core::PredicateMemo,
    ) -> (
        Option<usize>,
        &'a YamlValue,
        crate::condition_encoding::AbsenceDefaults<'a>,
    ) {
        let predicate = helm_schema_core::Predicate::all(
            guards
                .iter()
                .map(helm_schema_core::ConditionalGuard::predicate)
                .collect(),
        );
        let guarded = self
            .guarded
            .iter()
            .enumerate()
            .filter(|(_, documents)| {
                predicate_memo.exactly_implies(
                    &predicate,
                    &helm_schema_core::Predicate::all(
                        documents
                            .guards
                            .iter()
                            .map(helm_schema_core::ConditionalGuard::predicate)
                            .collect(),
                    ),
                )
            })
            .max_by_key(|(_, documents)| documents.guards.len());
        let (values_document, composed, runtime_defaults, dependency_refill) = guarded.map_or(
            (
                None,
                &self.composed,
                &self.runtime_defaults,
                &self.dependency_refill,
            ),
            |(index, documents)| {
                (
                    Some(index),
                    &documents.composed,
                    &documents.runtime_defaults,
                    &documents.dependency_refill,
                )
            },
        );
        (
            values_document,
            composed,
            crate::condition_encoding::AbsenceDefaults {
                runtime_defaults,
                dependency_refill,
                dependency_roots,
            },
        )
    }
}

fn prepare_guarded_values_documents(
    signals: &ContractSchemaSignals,
    composed: &YamlValue,
    runtime_defaults: &YamlValue,
    dependency_refill: &YamlValue,
    predicate_memo: &helm_schema_core::PredicateMemo,
) -> Vec<GuardedRootValuesDocuments> {
    let guarded_sources = signals
        .guarded_values_default_sources()
        .iter()
        .map(|fact| (fact.outer_guards.clone(), fact.source.clone()))
        .collect::<Vec<_>>();
    guarded_sources
        .iter()
        .map(|(branch_guards, _)| branch_guards)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|branch_guards| {
            let branch_predicate = helm_schema_core::Predicate::all(
                branch_guards
                    .iter()
                    .map(helm_schema_core::ConditionalGuard::predicate)
                    .collect(),
            );
            let sources = guarded_sources
                .iter()
                .filter(|(source_guards, _)| {
                    predicate_memo.exactly_implies(
                        &branch_predicate,
                        &helm_schema_core::Predicate::all(
                            source_guards
                                .iter()
                                .map(helm_schema_core::ConditionalGuard::predicate)
                                .collect(),
                        ),
                    )
                })
                .map(|(_, source)| source.clone())
                .collect::<BTreeSet<_>>();
            let mut branch_composed = composed.clone();
            crate::values_yaml::apply_values_default_sources(&mut branch_composed, &sources);
            let mut branch_runtime_defaults = runtime_defaults.clone();
            crate::values_yaml::copy_values_default_sources(
                &mut branch_runtime_defaults,
                &branch_composed,
                &sources,
            );
            let mut branch_dependency_refill = dependency_refill.clone();
            crate::values_yaml::copy_values_default_sources(
                &mut branch_dependency_refill,
                &branch_composed,
                &sources,
            );
            GuardedRootValuesDocuments {
                guards: branch_guards.clone(),
                composed: branch_composed,
                runtime_defaults: branch_runtime_defaults,
                dependency_refill: branch_dependency_refill,
            }
        })
        .collect()
}

struct EmissionSupportPlan {
    conditional_targets: ConditionalTargetIndex,
    owning_paths: BTreeSet<Vec<String>>,
    preserving_paths: BTreeSet<Vec<String>>,
    accepted_values_root_paths: Vec<Vec<String>>,
    /// Every dependency instance's values root; with the analyzed chart's
    /// empty root, the chart values roots.
    dependency_roots: BTreeSet<Vec<String>>,
    default_fill_skip_paths: BTreeSet<Vec<String>>,
    conditional_hosts: ConditionalHostPreparation,
}

#[derive(Clone)]
pub(crate) struct ProjectedTree {
    pub(crate) document: SchemaDocument,
    pub(crate) emission_report: EmissionReport,
    /// The base owner materialized for each resolved path.
    pub(crate) base_owners: BTreeMap<helm_schema_core::ValuesPath, BaseOwnerDecision>,
    provider_definitions: BTreeMap<String, Value>,
    /// Content origins of the private provider-definition handles.
    definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
}

#[derive(Clone)]
pub(crate) struct MaterializedTree {
    pub(crate) schema: Value,
    pub(crate) emission_report: EmissionReport,
    provider_definitions: BTreeMap<String, Value>,
    definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
}

pub(crate) struct CompletedGeneratedSchema {
    pub(crate) schema: Value,
    pub(crate) emission_report: EmissionReport,
    pub(crate) definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
}

impl LoweredEmissionPlan {
    #[tracing::instrument(skip_all)]
    pub(crate) fn build(input: &ValuesSchemaInput<'_>) -> Self {
        let predicate_memo = helm_schema_core::PredicateMemo::new();
        let contract_schema_signals = input.contract_schema_signals.clone();
        let mut composed = input
            .values_documents
            .map_or(YamlValue::Null, |documents| documents.composed.clone());
        // Taken before render-time default merges: Helm validates, and
        // `helm lint` compares against, the coalesced values alone.
        let mut coalesced_defaults = serde_json::to_value(&composed).unwrap_or(Value::Null);
        drop_null_members(&mut coalesced_defaults);
        crate::values_yaml::apply_values_default_sources(
            &mut composed,
            contract_schema_signals.values_default_sources(),
        );
        let mut input_defaults = composed.clone();
        crate::values_yaml::remove_values_paths(
            &mut input_defaults,
            input.shadowed_input_paths.unwrap_or(&BTreeSet::new()),
        );
        // A schema validates the COALESCED document, in which every chart's
        // and every subchart's declared defaults have already been applied.
        // A key missing from it was therefore null-deleted, and helm's
        // coalescing carries that deletion through the subchart stage too
        // (verified against helm v4.2.3: `--set kid.grp=null` renders
        // `kid: {flag: true, global: {}}` and aborts on `.Values.grp.enabled`
        // whether the parent, the subchart, or both declared `grp`). Nothing
        // a chart DECLARES can refill such a key; only a render-time merge
        // inside the templates (`set $ "Values" (mustMergeOverwrite defaults
        // .Values)`) can, and those are exactly the `ValuesDefaultSource`
        // facts. So this document starts empty and holds only them.
        let mut runtime_defaults = YamlValue::Null;
        crate::values_yaml::copy_values_default_sources(
            &mut runtime_defaults,
            &composed,
            contract_schema_signals.values_default_sources(),
        );
        let mut dependency_refill = input.values_documents.map_or(YamlValue::Null, |documents| {
            documents.dependency_refill.clone()
        });
        crate::values_yaml::copy_values_default_sources(
            &mut dependency_refill,
            &composed,
            contract_schema_signals.values_default_sources(),
        );
        let guarded = prepare_guarded_values_documents(
            &contract_schema_signals,
            &composed,
            &runtime_defaults,
            &dependency_refill,
            &predicate_memo,
        );
        let uncoalesced_root = input
            .values_documents
            .and_then(|documents| documents.uncoalesced_root.as_ref())
            .and_then(|document| serde_json::to_value(document).ok());
        let documents = RootValuesDocuments {
            composed,
            coalesced_defaults,
            uncoalesced_root,
            input_defaults,
            runtime_defaults,
            dependency_refill,
            guarded,
        };
        let provider_resolutions = crate::provider_resolution::ProviderSchemaResolutions::resolve(
            &contract_schema_signals,
            input.provider,
        );
        let mut decisions = GenerationDecisions::default();
        let dependency_roots = dependency_values_roots(&contract_schema_signals);
        let helm_global_namespaces = helm_global_namespaces(&dependency_roots);
        let resolved_paths = PathSchemaResolver::new(
            &contract_schema_signals,
            &documents.input_defaults,
            &documents.runtime_defaults,
            &helm_global_namespaces,
            input.authoring_policy.declared_types,
            &provider_resolutions,
        )
        .resolve_all(&mut decisions);
        let (conditional_schemas, insertion_abstentions) = collect_conditional_schemas(
            &resolved_paths,
            &contract_schema_signals,
            &documents.composed,
            &documents.runtime_defaults,
            &provider_resolutions,
            input.authoring_policy.declared_types,
            &mut decisions,
        );
        let terminal_schemas = contract_schema_signals
            .terminal_clauses()
            .iter()
            .map(|guards| LoweredConjunct::terminal(guards.clone()))
            .collect::<Vec<_>>();
        let support = EmissionSupportPlan::build(
            &contract_schema_signals,
            &resolved_paths,
            &conditional_schemas,
            dependency_roots,
        );

        Self {
            predicate_memo,
            contract_schema_signals,
            documents,
            values_descriptions: input.values_descriptions.cloned().unwrap_or_default(),
            resolved_paths,
            conditional_schemas,
            terminal_schemas,
            support,
            insertion_abstentions,
            decisions,
            authoring_policy: input.authoring_policy,
        }
    }

    /// The plan's decisions completed by the base owners one projection
    /// materialized.
    pub(crate) fn into_generation_decisions(
        self,
        base_owners: BTreeMap<helm_schema_core::ValuesPath, BaseOwnerDecision>,
    ) -> GenerationDecisions {
        let mut decisions = self.decisions;
        for (path, base_owner) in base_owners {
            decisions.path_mut(&path).base_owner = Some(base_owner);
        }
        decisions
    }

    pub(crate) fn project(&self, policy: EmissionPolicy) -> ProjectedTree {
        debug_assert!(policy.is_valid());
        let mut emission_report = EmissionReport::default();
        emission_report.insertion_abstentions = self.insertion_abstentions;
        let mut selected_conditionals = Vec::new();
        for conjunct in &self.conditional_schemas {
            let selected = policy.selects(&conjunct.class);
            emission_report.record_fact(FactRecord {
                class: &conjunct.class,
                selected,
            });
            if selected {
                selected_conditionals.push(conjunct.clone());
            }
        }
        let selected_terminals = self
            .terminal_schemas
            .iter()
            .filter(|conjunct| {
                let selected = policy.selects(&conjunct.class);
                emission_report.record_fact(FactRecord {
                    class: &conjunct.class,
                    selected,
                });
                selected
            })
            .cloned()
            .collect::<Vec<_>>();

        // Candidate metadata is consumed only after selection. The shared
        // plan stays immutable, and each projection receives fresh payloads.
        let mut resolved_paths = self.resolved_paths.clone();
        let mut definition_origins = BTreeMap::new();
        let mut provider_definitions = extract_provider_definitions(
            &mut resolved_paths,
            &mut selected_conditionals,
            &self.values_descriptions,
            &mut definition_origins,
        );
        let (mut document, base_document_abstentions, base_owners) = materialize_base_document(
            &self.contract_schema_signals,
            &self.documents.input_defaults,
            &resolved_paths,
            &self.support,
            self.authoring_policy,
        );
        emission_report.insertion_abstentions.base_document += base_document_abstentions;
        self.support.conditional_hosts.apply(&mut document);
        let fallback_conditionals = canonicalize_mandatory_constraints(
            &mut document,
            selected_conditionals,
            &mut emission_report,
        );
        let mut uncoalesced_root_gate = UncoalescedRootGate::new(
            self.documents.uncoalesced_root.as_ref(),
            &self.documents.coalesced_defaults,
            &provider_definitions,
        );
        append_selected_constraints(
            &mut document,
            fallback_conditionals,
            &self.documents,
            &self.support.dependency_roots,
            &mut emission_report,
            &self.predicate_memo,
            &mut uncoalesced_root_gate,
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
                &self.contract_schema_signals,
                &self.documents,
                &self.support.dependency_roots,
                &self.predicate_memo,
                &mut uncoalesced_root_gate,
            );
        }
        emission_report.lint_withdrawals = uncoalesced_root_gate.into_relaxed();
        prune_unreachable_provider_definitions(&document, &mut provider_definitions);

        ProjectedTree {
            document,
            emission_report,
            base_owners,
            provider_definitions,
            definition_origins,
        }
    }

    pub(crate) fn complete(&self, projected: ProjectedTree) -> CompletedGeneratedSchema {
        let projected = self.merge_missing_defaults(projected);
        let projected = self.reserve_helm_global_namespaces(projected);
        let materialized = self.preserve_declared_defaults(projected);
        let materialized = Self::extract_repeated_payloads(materialized);
        let materialized = Self::insert_shared_definitions(materialized);
        let materialized = self.apply_program_wrappers(materialized);
        let materialized = self.apply_values_descriptions(materialized);
        finish_generated(
            materialized.schema,
            materialized.emission_report,
            materialized.definition_origins,
            self.authoring_policy.root,
        )
    }

    pub(crate) fn merge_missing_defaults(&self, mut projected: ProjectedTree) -> ProjectedTree {
        let _span = tracing::info_span!("merge_missing_defaults").entered();
        projected
            .emission_report
            .canonicalization
            .default_backfill_abstentions += projected
            .document
            .merge_missing_values_yaml_defaults_under_roots(
                &self.documents.input_defaults,
                &self.support.accepted_values_root_paths,
                &self.support.default_fill_skip_paths,
                self.authoring_policy.declared_types,
            );
        projected
    }

    /// Reserves Helm's `global` namespace at every chart values root, after
    /// default backfill so no declared default can re-type it.
    ///
    /// A namespace no contract reaches stays unconstrained. Every evidence
    /// row at or below a namespace is a template use (a direct read, or a
    /// dependency's read projected onto each ancestor namespace Helm merges
    /// into it) or a dependency values-root registration, which carries
    /// Helm's table assertion on an instance aliased `global`. Values-file
    /// seeds never reach a namespace, and `is_referenced_value_path` /
    /// `has_referenced_descendants` are not consulted: seeds and
    /// registrations set both, so they prove no read.
    pub(crate) fn reserve_helm_global_namespaces(
        &self,
        mut projected: ProjectedTree,
    ) -> ProjectedTree {
        let evidence = self.contract_schema_signals.schema_evidence_by_value_path();
        for chart_root in chart_values_roots(&self.support.dependency_roots) {
            let namespace = helm_global_namespace(chart_root);
            let consumed = evidence
                .keys()
                .any(|path| *path == namespace || path.is_descendant_of(&namespace));
            projected
                .document
                .reserve_helm_global_namespace(chart_root, consumed);
        }
        projected
    }

    pub(crate) fn preserve_declared_defaults(&self, projected: ProjectedTree) -> MaterializedTree {
        let ProjectedTree {
            document,
            emission_report,
            base_owners: _,
            provider_definitions,
            definition_origins,
        } = projected;
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
        MaterializedTree {
            schema,
            emission_report,
            provider_definitions,
            definition_origins,
        }
    }

    pub(crate) fn extract_repeated_payloads(
        mut materialized: MaterializedTree,
    ) -> MaterializedTree {
        {
            let _span = tracing::info_span!("extract_repeated_provider_payloads").entered();
            materialized
                .provider_definitions
                .extend(extract_repeated_provider_payloads(
                    &mut materialized.schema,
                    &mut materialized.definition_origins,
                ));
        }
        materialized
    }

    pub(crate) fn insert_shared_definitions(
        mut materialized: MaterializedTree,
    ) -> MaterializedTree {
        let truthy_span = tracing::info_span!("helm_truthy_scan").entered();
        if value_references_helm_truthy(&materialized.schema)
            || materialized
                .provider_definitions
                .values()
                .any(value_references_helm_truthy)
        {
            materialized.provider_definitions.insert(
                HELM_TRUTHY_DEFINITION_NAME.to_string(),
                helm_truthy_definition_schema(),
            );
        }
        for style in [
            helm_schema_core::QuotedScalarStyle::Double,
            helm_schema_core::QuotedScalarStyle::Single,
        ] {
            if crate::quoted_serialization::value_references(&materialized.schema, style)
                || materialized
                    .provider_definitions
                    .values()
                    .any(|definition| {
                        crate::quoted_serialization::value_references(definition, style)
                    })
            {
                materialized.provider_definitions.insert(
                    crate::quoted_serialization::definition_name(style).to_string(),
                    crate::quoted_serialization::definition_schema(style),
                );
            }
        }
        drop(truthy_span);
        insert_definitions_into_root(
            &mut materialized.schema,
            std::mem::take(&mut materialized.provider_definitions),
        );
        materialized
    }

    pub(crate) fn apply_program_wrappers(
        &self,
        mut materialized: MaterializedTree,
    ) -> MaterializedTree {
        {
            let _span = tracing::info_span!("apply_program_wrappers").entered();
            crate::program_wrapper::apply_program_wrapper_alternatives(
                &mut materialized.schema,
                self.contract_schema_signals.values_program_wrappers(),
                self.contract_schema_signals
                    .values_program_wrapper_exclusions(),
            );
        }
        materialized
    }

    pub(crate) fn apply_values_descriptions(
        &self,
        mut materialized: MaterializedTree,
    ) -> MaterializedTree {
        {
            let _span = tracing::info_span!("apply_values_descriptions").entered();
            crate::schema_tree::apply_values_descriptions(
                &mut materialized.schema,
                &self.values_descriptions,
            );
        }
        materialized
    }

    #[cfg(feature = "bench-support")]
    pub(crate) fn benchmark_retained_candidate_bytes(&self) -> usize {
        fn record_candidate(
            candidate: Option<&crate::provider_schema::ProviderSchemaCandidate>,
            payloads: &mut BTreeSet<String>,
        ) {
            let Some(candidate) = candidate else {
                return;
            };
            payloads.insert(candidate.key().to_string());
            if let Some(definition) = candidate.source_definition_schema() {
                payloads.insert(helm_schema_json_schema_walk::canonical_json_string(
                    definition,
                ));
            }
        }

        let mut payloads = BTreeSet::new();
        for resolved in &self.resolved_paths {
            record_candidate(resolved.provider_schema_candidate.as_ref(), &mut payloads);
        }
        for conjunct in &self.conditional_schemas {
            record_candidate(conjunct.provider_candidate.as_ref(), &mut payloads);
        }
        payloads.iter().map(String::len).sum()
    }
}

fn canonicalize_mandatory_constraints(
    document: &mut SchemaDocument,
    conditionals: Vec<LoweredConjunct>,
    report: &mut EmissionReport,
) -> Vec<LoweredConjunct> {
    let mut fallback = Vec::new();
    for conjunct in conditionals {
        if !matches!(conjunct.class, EmissionClass::Mandatory) {
            fallback.push(conjunct);
            continue;
        }
        let mut target_segments = conjunct.carrier.ancestor_segments.clone();
        target_segments.extend(conjunct.carrier.relative_target_segments.iter().cloned());
        let is_object_host = conjunct.schema.is_exact_object_type_schema();
        let canonical_object_host = is_object_host
            && conjunct.carrier.ancestor_segments.is_empty()
            && matches!(
                conjunct.origin,
                EmissionOrigin::RequirementImplication | EmissionOrigin::Backprojection
            );
        let outcome = if !is_object_host || canonical_object_host {
            document.canonicalize_constraint_at_path(&target_segments, &conjunct.schema)
        } else {
            CanonicalConstraintOutcome::NotApplicable
        };
        match outcome {
            CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted) => {
                report.canonicalization.applied += 1;
                report.mandatory_outcomes.emitted += 1;
            }
            CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant) => {
                report.canonicalization.redundant += 1;
                report.mandatory_outcomes.redundant += 1;
            }
            CanonicalConstraintOutcome::NotApplicable => {
                report.canonicalization.fallback += 1;
                fallback.push(conjunct);
            }
        }
    }
    fallback
}

impl EmissionSupportPlan {
    fn build(
        contract_schema_signals: &ContractSchemaSignals,
        resolved_paths: &[ResolvedPathSchema],
        conditional_schemas: &[LoweredConjunct],
        dependency_roots: BTreeSet<Vec<String>>,
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
                .owner
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
                .owner
                .preserves_descendants()
            })
            .map(|resolved_path| resolved_path.path_segments.clone())
            .collect::<BTreeSet<_>>();
        let accepted_values_root_paths = contract_schema_signals
            .schema_evidence_by_value_path()
            .iter()
            .filter(|(_, evidence)| evidence.facts.accepted_values_root_fragment)
            .map(|(path, _)| path.segments().map(tree_segment_spelling).collect())
            .collect::<Vec<_>>();
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
                default_fill_skip_paths
                    .insert(value_path.segments().map(tree_segment_spelling).collect());
            }
        }
        // A directly ranged path accepts the runtime iterable domain, which
        // is wider than any declared default; the filler must not re-type it.
        for value_path in contract_schema_signals.direct_ranged_value_paths() {
            default_fill_skip_paths
                .insert(value_path.segments().map(tree_segment_spelling).collect());
        }
        // A member omitted before every provider sink is governed by its own
        // evidence. Refilling its default would restore the removed parent
        // contract.
        for value_path in contract_schema_signals.unconditionally_omitted_value_paths() {
            default_fill_skip_paths
                .insert(value_path.segments().map(tree_segment_spelling).collect());
        }
        // Helm's `global` namespace is reserved without a declared shape.
        for chart_root in chart_values_roots(&dependency_roots) {
            let mut namespace = chart_root.to_vec();
            namespace.push("global".to_string());
            default_fill_skip_paths.insert(namespace);
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
        support.conditional_hosts = prepare_conditional_hosts(conditional_schemas);
        support
    }
}

fn materialize_base_document(
    contract_schema_signals: &ContractSchemaSignals,
    input_defaults: &YamlValue,
    resolved_paths: &[ResolvedPathSchema],
    support: &EmissionSupportPlan,
    authoring_policy: AuthoringPolicy,
) -> (
    SchemaDocument,
    usize,
    BTreeMap<helm_schema_core::ValuesPath, BaseOwnerDecision>,
) {
    let mut document = SchemaDocument::new_root_object(authoring_policy.root);
    let mut insertion_abstentions = 0;
    let mut base_owners = BTreeMap::new();
    let base_span = tracing::info_span!("base_path_insertion").entered();
    for resolved_path in resolved_paths {
        let decision = classify_base(
            resolved_path,
            &support.conditional_targets,
            &support.owning_paths,
            &support.preserving_paths,
        );
        base_owners.insert(resolved_path.value_path.clone(), decision);
        let owner = decision.owner;
        let Some(schema) = owner.schema(resolved_path) else {
            continue;
        };
        if owner == BaseOwner::IndependentContract
            && let Some(contract) = &resolved_path.independent_base_contract
        {
            document.conjoin_literal_path_schema(contract.literal_path(), schema);
            continue;
        }
        let materialized_member_schema = schema.clone();
        if owner.replaces() {
            insertion_abstentions +=
                document.replace_values_path_schema(&resolved_path.value_path, schema);
        } else {
            insertion_abstentions +=
                document.insert_values_path_schema(&resolved_path.value_path, schema);
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
    (document, insertion_abstentions, base_owners)
}

/// Every dependency instance's values root, as tree segments.
fn dependency_values_roots(signals: &ContractSchemaSignals) -> BTreeSet<Vec<String>> {
    signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.accepted_dependency_values_root_fragment)
        .map(|(path, _)| path.segments().map(tree_segment_spelling).collect())
        .collect()
}

/// The analyzed chart's empty root followed by every dependency root.
fn chart_values_roots(dependency_roots: &BTreeSet<Vec<String>>) -> impl Iterator<Item = &[String]> {
    std::iter::once(&[][..]).chain(dependency_roots.iter().map(Vec::as_slice))
}

/// Helm's `global` namespace beneath every chart values root.
fn helm_global_namespaces(
    dependency_roots: &BTreeSet<Vec<String>>,
) -> BTreeSet<helm_schema_core::ValuesPath> {
    chart_values_roots(dependency_roots)
        .map(helm_global_namespace)
        .collect()
}

/// Helm's `global` namespace beneath a chart values root.
fn helm_global_namespace(chart_root: &[String]) -> helm_schema_core::ValuesPath {
    helm_schema_core::ValuesPath::from_segments(
        chart_root
            .iter()
            .cloned()
            .chain(std::iter::once("global".to_string()))
            .map(helm_schema_core::Segment::from),
    )
}

fn tree_segment_spelling(segment: &helm_schema_core::Segment) -> String {
    segment.literal().unwrap_or("*").to_owned()
}

pub(crate) fn finish_generated(
    schema: Value,
    mut emission_report: EmissionReport,
    definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
    root_policy: crate::RootPolicy,
) -> CompletedGeneratedSchema {
    emission_report.carriers =
        count_emitted_carriers(&schema, emission_report.carriers.grouping_fan_in);
    CompletedGeneratedSchema {
        schema: draft07_root_document(schema, root_policy),
        emission_report,
        definition_origins,
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

/// Deletes null-valued object members along object chains; lists are
/// replaced wholesale by Helm's coalescing and keep their members.
fn drop_null_members(value: &mut Value) {
    if let Value::Object(members) = value {
        members.retain(|_, member| !member.is_null());
        for member in members.values_mut() {
            drop_null_members(member);
        }
    }
}
