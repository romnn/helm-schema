use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helm_schema_core::{
    ConditionalGuard, ContractSchemaSignals, ContractUse, ContractValuePathFacts, MetadataFieldKind,
};
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::{ContractDocument, ContractIr, FinalizedContract};
use helm_schema_k8s::{Diagnostic, DiagnosticSink, LocalSchemaUniverse};
use serde_json::Value;

use crate::analysis::analyze_charts;
use crate::chart;
use crate::error::EngineResult;
use crate::generation::{GenerateOptions, GeneratedSchema, ResolvedContract};
use crate::output_pipeline::{
    OutputPipelineOptions, PolicyInputOptions, PolicyInputs, apply_schema_output_pipeline,
    load_policy_inputs,
};
use crate::provider_builder;
use crate::values_roots;

/// Public analysis artifact produced by [`AnalysisSession`].
///
/// This keeps the core analysis artifact small and non-duplicated:
/// the guarded contract graph plus the chart-local schema universe extracted
/// from sources such as static and template-rendered CRDs. Typed schema
/// lowering evidence stays available as its own memoized session query via
/// [`AnalysisSession::contract_schema_signals`].
#[derive(Debug, Clone)]
pub struct Analysis {
    /// Guarded contract graph recovered from chart templates.
    pub contract: ContractIr,
    /// Resource schemas declared by the chart's CRDs.
    pub local_schemas: LocalSchemaUniverse,
}

/// Session-level explanation for one values path.
#[derive(Debug, Clone, PartialEq)]
pub struct ValuePathExplanation {
    /// Canonical values path described by the explanation.
    pub path: String,
    /// Contract uses that read exactly this path.
    pub exact_uses: Vec<ContractUse>,
    /// Contract uses that read descendants of this path.
    pub descendant_uses: Vec<ContractUse>,
    /// Aggregate behavioral facts for the path, when analysis found evidence.
    pub value_path_facts: Option<ContractValuePathFacts>,
    /// Values-decidable guards attached to the path.
    pub guard_predicates: Vec<ConditionalGuard>,
    /// Kubernetes metadata roles reached from the path.
    pub metadata_fields: Vec<MetadataFieldKind>,
    /// JSON Schema type hints derived from strict consumers.
    pub type_hints: Vec<Value>,
    /// Whether a defaulting operation supplies an absent value.
    pub has_default_fallback: bool,
}

struct PreparedSession {
    analysis: Analysis,
    values_yaml: Option<String>,
    dependency_values_yaml: Option<String>,
    explicit_value_paths: BTreeSet<String>,
    values_descriptions: BTreeMap<String, String>,
    subchart_value_prefixes: Vec<Vec<String>>,
}

impl PreparedSession {
    fn from_generate_options(opts: &GenerateOptions) -> EngineResult<Self> {
        let charts = &chart::discover_chart_contexts(&opts.chart_dir)?;

        let defines = chart::build_define_index(charts, opts.include_tests)?;
        let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;
        // The dependency charts' own declared defaults: schema generation
        // distinguishes parent-owned absence (helm null-deletion, nil at
        // render) from subchart-declared absence (the subchart's default
        // fills at its own coalesce stage, even after a parent-level
        // null-deletion).
        let dependency_values_yaml = if opts.include_subchart_values {
            chart::build_dependency_values_yaml(charts)?
        } else {
            None
        };
        let values_roots = values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref());
        let values_descriptions = chart::build_composed_values_descriptions(
            charts,
            opts.include_subchart_values,
            &opts.values_files,
        )?;
        let kubernetes_version = primary_kubernetes_version(opts);
        let chart_analysis = analyze_charts(
            charts,
            &defines,
            opts.include_tests,
            &values_roots,
            kubernetes_version.as_deref(),
        )?;

        Ok(Self {
            analysis: Analysis {
                contract: chart_analysis.contract,
                local_schemas: chart_analysis.local_schema_universe,
            },
            values_yaml,
            dependency_values_yaml,
            explicit_value_paths: values_roots.explicit_paths,
            values_descriptions,
            subchart_value_prefixes: charts
                .iter()
                .filter(|chart| !chart.values_prefix.is_empty())
                .map(|chart| chart.values_prefix.clone())
                .collect(),
        })
    }
}

/// Memoized facade over chart analysis and schema lowering.
///
/// The session keeps chart loading and analysis results available for later
/// queries without forcing callers to re-run discovery, values composition,
/// contract extraction, and chart-local schema collection manually.
pub struct AnalysisSession {
    opts: GenerateOptions,
    diagnostics: DiagnosticSink,
    prepared: SessionCache<PreparedSession>,
    finalized_contract: SessionCache<FinalizedContract>,
    resolved_contract: SessionCache<ResolvedContract>,
    generated_schema: SessionCache<GeneratedSchema>,
}

struct SessionCache<T> {
    value: Mutex<Option<Arc<T>>>,
}

impl<T> SessionCache<T> {
    fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    fn get_or_try_init(&self, init: impl FnOnce() -> EngineResult<T>) -> EngineResult<Arc<T>> {
        {
            let guard = self
                .value
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(value) = guard.as_ref() {
                return Ok(Arc::clone(value));
            }
        }

        let value = Arc::new(init()?);
        let mut guard = self
            .value
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Arc::clone(guard.get_or_insert_with(|| Arc::clone(&value))))
    }
}

impl AnalysisSession {
    /// Creates a memoized session with an internal diagnostic sink.
    #[must_use]
    pub fn new(opts: GenerateOptions) -> Self {
        Self::with_diagnostics(opts, DiagnosticSink::new())
    }

    /// Creates a memoized session that emits diagnostics into `diagnostics`.
    #[must_use]
    pub fn with_diagnostics(opts: GenerateOptions, diagnostics: DiagnosticSink) -> Self {
        Self {
            opts,
            diagnostics,
            prepared: SessionCache::new(),
            finalized_contract: SessionCache::new(),
            resolved_contract: SessionCache::new(),
            generated_schema: SessionCache::new(),
        }
    }

    /// Return the memoized chart analysis artifact.
    ///
    /// # Errors
    ///
    /// Returns an error when chart discovery, source loading, parsing, or
    /// structural analysis fails.
    pub fn analysis(&self) -> EngineResult<Analysis> {
        Ok(self.prepared()?.analysis.clone())
    }

    /// Return typed schema-lowering evidence derived from the guarded contract.
    ///
    /// # Errors
    ///
    /// Returns an error when preparing or finalizing chart analysis fails.
    pub fn contract_schema_signals(&self) -> EngineResult<ContractSchemaSignals> {
        Ok(self.finalized_contract()?.schema_signals().clone())
    }

    /// Return the stable versioned contract export document.
    ///
    /// # Errors
    ///
    /// Returns an error when preparing or finalizing chart analysis fails.
    pub fn contract_document(&self) -> EngineResult<ContractDocument> {
        Ok(self.finalized_contract()?.document())
    }

    /// Return the provider-resolved contract schema prior to optional
    /// required-inference and final output-pipeline transforms.
    ///
    /// This query exposes the stage boundary the architecture document calls
    /// `resolved_contract(policy)`: structural contract facts have already
    /// been resolved against providers, but the later heuristic
    /// `--infer-required` mutation has not yet run.
    ///
    /// # Errors
    ///
    /// Returns an error when chart analysis, values composition, or provider
    /// schema resolution fails.
    pub fn resolved_contract(&self) -> EngineResult<ResolvedContract> {
        Ok((*self.resolved()?).clone())
    }

    /// Return the memoized generated values schema: the resolved contract
    /// schema plus the optional `--infer-required` post-pass.
    ///
    /// # Errors
    ///
    /// Returns an error when resolving the contract or preparing chart values fails.
    pub fn generated_schema(&self) -> EngineResult<GeneratedSchema> {
        Ok((*self.generated_schema.get_or_try_init(|| {
            let resolved = self.resolved()?;
            let mut schema = resolved.schema.clone();
            if self.opts.infer_required {
                helm_schema_gen::required_inference::apply_required_inference(
                    &mut schema,
                    self.finalized_contract()?
                        .schema_signals()
                        .schema_evidence_by_value_path(),
                    &self.prepared()?.explicit_value_paths,
                );
            }
            Ok(GeneratedSchema {
                schema,
                subchart_value_prefixes: resolved.subchart_value_prefixes.clone(),
            })
        })?)
        .clone())
    }

    /// Emit the final JSON Schema document through the output pipeline.
    ///
    /// This is the session-level counterpart to the CLI's final output stage:
    /// it starts from the memoized generated schema, applies override/policy
    /// inputs, mirrors global schema into subcharts, resolves reference mode,
    /// and returns the final document callers would write to disk.
    ///
    /// # Errors
    ///
    /// Returns an error when generated-schema preparation, override merging,
    /// or reference processing fails.
    pub fn emit(
        &self,
        policy_inputs: PolicyInputs,
        output_options: OutputPipelineOptions,
    ) -> EngineResult<Value> {
        let generated = self.generated_schema()?;
        apply_schema_output_pipeline(
            generated.schema,
            policy_inputs,
            &generated.subchart_value_prefixes,
            self.chart_base_dir(),
            output_options,
        )
    }

    /// Load policy inputs from override paths, then emit the final document.
    ///
    /// # Errors
    ///
    /// Returns an error when an override cannot be loaded or prepared, or
    /// when final output transforms fail.
    pub fn emit_with_policy_paths(
        &self,
        override_paths: &[PathBuf],
        policy_input_options: PolicyInputOptions,
        output_options: OutputPipelineOptions,
    ) -> EngineResult<Value> {
        let policy_inputs = load_policy_inputs(override_paths, &policy_input_options)?;
        self.emit(policy_inputs, output_options)
    }

    /// Explain one values path using the current contract and chart evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when chart analysis or contract finalization fails.
    pub fn explain(&self, path: &str) -> EngineResult<ValuePathExplanation> {
        let normalized_path = normalize_values_path(path);
        let finalized_contract = self.finalized_contract()?;
        let uses = finalized_contract.uses();
        let schema_signals = finalized_contract.schema_signals();
        let evidence = schema_signals.evidence_for(&normalized_path);

        let exact_uses = uses
            .iter()
            .filter(|use_| use_.source_expr == normalized_path)
            .cloned()
            .collect();
        let descendant_uses = uses
            .iter()
            .filter(|use_| {
                use_.source_expr
                    .strip_prefix(&normalized_path)
                    .is_some_and(|suffix| suffix.starts_with('.'))
            })
            .cloned()
            .collect();
        let value_path_facts = evidence.map(|evidence| evidence.facts);
        let guard_predicates = evidence
            .map(|evidence| evidence.guard_predicates.clone())
            .unwrap_or_default();
        let metadata_fields = evidence
            .map(|evidence| evidence.metadata_field_kinds.iter().copied().collect())
            .unwrap_or_default();
        let type_hints: Vec<serde_json::Value> = evidence
            .map(|evidence| {
                let schema_types = &evidence.type_hints;
                schema_types
                    .iter()
                    .map(|schema_type| serde_json::json!({ "type": schema_type }))
                    .collect()
            })
            .unwrap_or_default();
        let has_default_fallback =
            evidence.is_some_and(|evidence| evidence.requiredness.has_default_fallback);

        Ok(ValuePathExplanation {
            path: normalized_path,
            exact_uses,
            descendant_uses,
            value_path_facts,
            guard_predicates,
            metadata_fields,
            type_hints,
            has_default_fallback,
        })
    }

    fn prepared(&self) -> EngineResult<Arc<PreparedSession>> {
        self.prepared
            .get_or_try_init(|| PreparedSession::from_generate_options(&self.opts))
    }

    fn chart_base_dir(&self) -> &Path {
        Path::new(self.opts.chart_dir.as_str())
    }

    fn finalized_contract(&self) -> EngineResult<Arc<FinalizedContract>> {
        self.finalized_contract.get_or_try_init(|| {
            let prepared = self.prepared()?;
            let finalized = prepared.analysis.contract.clone().finalize();
            emit_input_channel_diagnostics(finalized.schema_signals(), &self.diagnostics);
            Ok(finalized)
        })
    }

    fn resolved(&self) -> EngineResult<Arc<ResolvedContract>> {
        self.resolved_contract.get_or_try_init(|| {
            let prepared = self.prepared()?;
            let finalized_contract = self.finalized_contract()?;
            let mut provider_options = self.opts.provider.clone();
            provider_options.local_schema_universe = prepared.analysis.local_schemas.clone();
            let provider =
                provider_builder::build_provider(&provider_options, Some(&self.diagnostics));

            let schema = generate_values_schema(
                ValuesSchemaInput::new(finalized_contract.schema_signals(), &provider)
                    .with_values_yaml(prepared.values_yaml.as_deref())
                    .with_dependency_values_yaml(prepared.dependency_values_yaml.as_deref())
                    .with_values_descriptions(&prepared.values_descriptions),
            );

            Ok(ResolvedContract {
                schema,
                subchart_value_prefixes: prepared.subchart_value_prefixes.clone(),
            })
        })
    }
}

pub(crate) fn emit_input_channel_diagnostics(
    signals: &ContractSchemaSignals,
    diagnostics: &DiagnosticSink,
) {
    for (value_path, evidence) in signals.schema_evidence_by_value_path() {
        let base_is_ambiguous = evidence.facts.is_direct_ranged_source
            && !evidence.facts.has_destructured_range_use
            && !evidence.facts.has_json_decoded_range_use;
        let guarded_is_ambiguous = evidence.conditional_overlays.iter().any(|overlay| {
            overlay.evidence.facts.is_direct_ranged_source
                && !overlay.evidence.facts.has_destructured_range_use
                && !overlay.evidence.facts.has_json_decoded_range_use
        });
        if base_is_ambiguous || guarded_is_ambiguous {
            diagnostics.push(Diagnostic::InputChannelNumericRangeAmbiguity {
                value_path: value_path.clone(),
            });
        }
    }
}

fn normalize_values_path(path: &str) -> String {
    let path = path.trim();
    if let Some(stripped) = path.strip_prefix(".Values.") {
        stripped.to_string()
    } else if path == ".Values" {
        String::new()
    } else {
        path.to_string()
    }
}

/// The normalized numeric core of the primary configured Kubernetes
/// version (`v1.29.0-standalone-strict` → `1.29.0`): the value
/// `.Capabilities.KubeVersion` conditions evaluate against under this
/// run's provider policy. `None` when no version is configured — the
/// capabilities lanes then abstain instead of guessing a cluster.
fn primary_kubernetes_version(opts: &GenerateOptions) -> Option<String> {
    let token = opts.provider.k8s_versions.first()?;
    let token = token.trim().strip_prefix('v').unwrap_or(token.trim());
    let core: String = token
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let parts: Vec<&str> = core.split('.').collect();
    if parts.is_empty()
        || parts.len() > 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some(core)
}
