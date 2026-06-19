use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helm_schema_engine::contract::{ContractDocument, ContractDocumentUse, ContractProjection};
use helm_schema_engine::{ContractIr, ValuesSchemaInput, generate_values_schema};
use helm_schema_k8s::{DiagnosticSink, LocalSchemaUniverse};
use serde_json::Value;

use crate::analysis::analyze_charts;
use crate::chart;
use crate::error::CliResult;
use crate::generation::{GenerateOptions, GeneratedSchema, ResolvedContract};
use crate::output_pipeline::{
    OutputPipelineOptions, PolicyInputOptions, PolicyInputs, apply_schema_output_pipeline,
    load_policy_inputs,
};
use crate::provider_builder;
use crate::required_inference;

/// Public analysis artifact produced by [`AnalysisSession`].
///
/// This is the current stable facade-level view of chart-local analysis:
/// the guarded contract graph, the typed schema-lowering signals derived from
/// that contract, and the chart-local schema universe extracted from sources
/// such as static and template-rendered CRDs.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub contract: ContractIr,
    pub schema_signals: helm_schema_engine::ContractSchemaSignals,
    pub local_schemas: LocalSchemaUniverse,
}

/// Session-level explanation for one values path.
#[derive(Debug, Clone, PartialEq)]
pub struct ValuePathExplanation {
    pub path: String,
    pub exact_uses: Vec<ContractDocumentUse>,
    pub descendant_uses: Vec<ContractDocumentUse>,
    pub value_path_facts: Option<helm_schema_engine::ContractValuePathFacts>,
    pub guard_constraints: Vec<helm_schema_engine::GuardConstraint>,
    pub metadata_fields: Vec<helm_schema_engine::MetadataFieldKind>,
    pub type_hints: Vec<Value>,
    pub has_default_fallback: bool,
}

pub(crate) struct PreparedSession {
    pub(crate) analysis: Analysis,
    pub(crate) required_inference_signals: helm_schema_engine::RequiredInferenceSignals,
    pub(crate) values_yaml: Option<String>,
    pub(crate) values_descriptions: BTreeMap<String, String>,
    pub(crate) subchart_value_prefixes: Vec<Vec<String>>,
}

impl PreparedSession {
    pub(crate) fn from_generate_options(opts: &GenerateOptions) -> CliResult<Self> {
        let discovery = chart::discover_chart_contexts(&opts.chart_dir)?;
        let charts = &discovery.charts;

        let defines = chart::build_define_index(charts, opts.include_tests)?;
        let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;
        let values_descriptions = chart::build_composed_values_descriptions(
            charts,
            opts.include_subchart_values,
            &opts.values_files,
        )?;
        let chart_analysis =
            analyze_charts(charts, &defines, opts.include_tests, values_yaml.as_deref())?;

        Ok(Self {
            analysis: Analysis {
                contract: chart_analysis.contract,
                schema_signals: chart_analysis.contract_schema_signals,
                local_schemas: chart_analysis.local_schema_universe,
            },
            required_inference_signals: chart_analysis.required_inference_signals,
            values_yaml,
            values_descriptions,
            subchart_value_prefixes: charts
                .iter()
                .filter(|chart| !chart.values_prefix.is_empty())
                .map(|chart| chart.values_prefix.clone())
                .collect(),
        })
    }

    pub(crate) fn analysis(&self) -> Analysis {
        self.analysis.clone()
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
    prepared: Mutex<Option<Arc<PreparedSession>>>,
    resolved_contract: Mutex<Option<Arc<ResolvedContract>>>,
    generated_schema: Mutex<Option<Arc<GeneratedSchema>>>,
    projection: Mutex<Option<Arc<ContractProjection>>>,
}

impl AnalysisSession {
    #[must_use]
    pub fn new(opts: GenerateOptions) -> Self {
        Self::with_diagnostics(opts, DiagnosticSink::new())
    }

    #[must_use]
    pub fn with_diagnostics(opts: GenerateOptions, diagnostics: DiagnosticSink) -> Self {
        Self {
            opts,
            diagnostics,
            prepared: Mutex::new(None),
            resolved_contract: Mutex::new(None),
            generated_schema: Mutex::new(None),
            projection: Mutex::new(None),
        }
    }

    /// Return the memoized chart analysis artifact.
    pub fn analysis(&self) -> CliResult<Analysis> {
        Ok(self.prepared()?.analysis())
    }

    /// Return the guarded contract graph for the chart tree.
    pub fn contract(&self) -> CliResult<ContractIr> {
        Ok(self.prepared()?.analysis.contract.clone())
    }

    /// Return the stable versioned contract export document.
    pub fn contract_document(&self) -> CliResult<ContractDocument> {
        Ok(self.contract_projection()?.as_ref().clone().into_document())
    }

    /// Return the chart-local schema universe extracted from the chart tree.
    pub fn local_schema_universe(&self) -> CliResult<LocalSchemaUniverse> {
        Ok(self.prepared()?.analysis.local_schemas.clone())
    }

    #[must_use]
    pub fn diagnostics(&self) -> DiagnosticSink {
        self.diagnostics.clone()
    }

    /// Return the provider-resolved contract schema prior to optional
    /// required-inference and final output-pipeline transforms.
    ///
    /// This query exposes the stage boundary the architecture document calls
    /// `resolved_contract(policy)`: structural contract facts have already
    /// been resolved against providers, but the later heuristic
    /// `--infer-required` mutation has not yet run.
    pub fn resolved_contract(&self) -> CliResult<ResolvedContract> {
        Ok((*self.resolved()?).clone())
    }

    /// Return the memoized generated values schema.
    pub fn generated_schema(&self) -> CliResult<GeneratedSchema> {
        {
            let guard = self
                .generated_schema
                .lock()
                .expect("generated schema mutex");
            if let Some(generated) = guard.as_ref() {
                return Ok((**generated).clone());
            }
        }

        let prepared = self.prepared()?;
        let resolved = self.resolved()?;
        let generated = Arc::new(generate_schema_from_resolved_contract(
            &resolved, &prepared, &self.opts,
        ));
        let mut guard = self
            .generated_schema
            .lock()
            .expect("generated schema mutex");
        let generated = Arc::clone(guard.get_or_insert_with(|| Arc::clone(&generated)));
        Ok((*generated).clone())
    }

    /// Emit the final JSON Schema document through the output pipeline.
    ///
    /// This is the session-level counterpart to the CLI's final output stage:
    /// it starts from the memoized generated schema, applies override/policy
    /// inputs, mirrors global schema into subcharts, resolves reference mode,
    /// and returns the final document callers would write to disk.
    pub fn emit(
        &self,
        policy_inputs: PolicyInputs,
        output_options: &OutputPipelineOptions,
    ) -> CliResult<Value> {
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
    pub fn emit_with_policy_paths(
        &self,
        override_paths: &[PathBuf],
        policy_input_options: &PolicyInputOptions,
        output_options: &OutputPipelineOptions,
    ) -> CliResult<Value> {
        let policy_inputs = load_policy_inputs(override_paths, policy_input_options)?;
        self.emit(policy_inputs, output_options)
    }

    /// Explain one values path using the current contract and chart evidence.
    pub fn explain(&self, path: &str) -> CliResult<ValuePathExplanation> {
        let normalized_path = normalize_values_path(path);
        let projection = self.contract_projection()?;
        let prepared = self.prepared()?;
        let evidence = prepared
            .analysis
            .schema_signals
            .schema_evidence_by_value_path
            .get(&normalized_path);

        let exact_uses = projection
            .uses()
            .iter()
            .filter(|use_| use_.source_expr == normalized_path)
            .cloned()
            .map(ContractDocumentUse::from)
            .collect();
        let descendant_uses = projection
            .uses()
            .iter()
            .filter(|use_| {
                use_.source_expr
                    .strip_prefix(&normalized_path)
                    .is_some_and(|suffix| suffix.starts_with('.'))
            })
            .cloned()
            .map(ContractDocumentUse::from)
            .collect();
        let value_path_facts = evidence.map(|evidence| evidence.facts);
        let guard_constraints = evidence
            .map(|evidence| evidence.guard_constraints.clone())
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
        let has_default_fallback = prepared
            .required_inference_signals
            .default_fallback_paths
            .contains(&normalized_path);

        Ok(ValuePathExplanation {
            path: normalized_path,
            exact_uses,
            descendant_uses,
            value_path_facts,
            guard_constraints,
            metadata_fields,
            type_hints,
            has_default_fallback,
        })
    }

    fn prepared(&self) -> CliResult<Arc<PreparedSession>> {
        {
            let guard = self.prepared.lock().expect("prepared session mutex");
            if let Some(prepared) = guard.as_ref() {
                return Ok(Arc::clone(prepared));
            }
        }

        let prepared = Arc::new(PreparedSession::from_generate_options(&self.opts)?);
        let mut guard = self.prepared.lock().expect("prepared session mutex");
        let prepared = Arc::clone(guard.get_or_insert_with(|| Arc::clone(&prepared)));
        Ok(prepared)
    }

    fn chart_base_dir(&self) -> &Path {
        Path::new(self.opts.chart_dir.as_str())
    }

    fn resolved(&self) -> CliResult<Arc<ResolvedContract>> {
        {
            let guard = self
                .resolved_contract
                .lock()
                .expect("resolved contract mutex");
            if let Some(resolved) = guard.as_ref() {
                return Ok(Arc::clone(resolved));
            }
        }

        let prepared = self.prepared()?;
        let resolved = Arc::new(resolve_contract_from_prepared(
            &prepared,
            &self.opts,
            Some(&self.diagnostics),
        )?);
        let mut guard = self
            .resolved_contract
            .lock()
            .expect("resolved contract mutex");
        let resolved = Arc::clone(guard.get_or_insert_with(|| Arc::clone(&resolved)));
        Ok(resolved)
    }

    fn contract_projection(&self) -> CliResult<Arc<ContractProjection>> {
        {
            let guard = self.projection.lock().expect("projection mutex");
            if let Some(projection) = guard.as_ref() {
                return Ok(Arc::clone(projection));
            }
        }

        let prepared = self.prepared()?;
        let projection = Arc::new(prepared.analysis.contract.clone().project());
        let mut guard = self.projection.lock().expect("projection mutex");
        let projection = Arc::clone(guard.get_or_insert_with(|| Arc::clone(&projection)));
        Ok(projection)
    }
}

pub(crate) fn resolve_contract_from_prepared(
    prepared: &PreparedSession,
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<ResolvedContract> {
    let mut provider_options = opts.provider.clone();
    provider_options.local_schema_universe = prepared.analysis.local_schemas.clone();
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&prepared.analysis.schema_signals, &provider)
            .with_values_yaml(prepared.values_yaml.as_deref())
            .with_values_descriptions(&prepared.values_descriptions),
    );

    Ok(ResolvedContract {
        schema,
        subchart_value_prefixes: prepared.subchart_value_prefixes.clone(),
    })
}

pub(crate) fn generate_schema_from_resolved_contract(
    resolved: &ResolvedContract,
    prepared: &PreparedSession,
    opts: &GenerateOptions,
) -> GeneratedSchema {
    let mut schema = resolved.schema.clone();

    if opts.infer_required {
        required_inference::apply(
            &mut schema,
            &prepared.required_inference_signals,
            &prepared.analysis.schema_signals.value_path_facts,
            prepared.values_yaml.as_deref(),
        );
    }

    GeneratedSchema {
        schema,
        subchart_value_prefixes: resolved.subchart_value_prefixes.clone(),
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
