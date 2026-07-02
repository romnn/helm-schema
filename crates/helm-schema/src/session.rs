use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::{
    ConditionalGuard, ContractDocument, ContractIr, ContractSchemaSignals, ContractUse,
    ContractValuePathFacts, FinalizedContract, MetadataFieldKind,
};
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
    pub contract: ContractIr,
    pub local_schemas: LocalSchemaUniverse,
}

/// Session-level explanation for one values path.
#[derive(Debug, Clone, PartialEq)]
pub struct ValuePathExplanation {
    pub path: String,
    pub exact_uses: Vec<ContractUse>,
    pub descendant_uses: Vec<ContractUse>,
    pub value_path_facts: Option<ContractValuePathFacts>,
    pub guard_predicates: Vec<ConditionalGuard>,
    pub metadata_fields: Vec<MetadataFieldKind>,
    pub type_hints: Vec<Value>,
    pub has_default_fallback: bool,
}

pub(crate) struct PreparedSession {
    pub(crate) analysis: Analysis,
    pub(crate) values_yaml: Option<String>,
    pub(crate) explicit_value_paths: BTreeSet<String>,
    pub(crate) values_descriptions: BTreeMap<String, String>,
    pub(crate) subchart_value_prefixes: Vec<Vec<String>>,
}

impl PreparedSession {
    pub(crate) fn from_generate_options(opts: &GenerateOptions) -> CliResult<Self> {
        let charts = &chart::discover_chart_contexts(&opts.chart_dir)?;

        let defines = chart::build_define_index(charts, opts.include_tests)?;
        let values_yaml = chart::build_composed_values_yaml(charts, opts.include_subchart_values)?;
        let values_roots = values_roots::ValuesRoots::from_values_yaml(values_yaml.as_deref());
        let values_descriptions = chart::build_composed_values_descriptions(
            charts,
            opts.include_subchart_values,
            &opts.values_files,
        )?;
        let chart_analysis = analyze_charts(charts, &defines, opts.include_tests, &values_roots)?;

        Ok(Self {
            analysis: Analysis {
                contract: chart_analysis.contract,
                local_schemas: chart_analysis.local_schema_universe,
            },
            values_yaml,
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

    fn get_or_try_init(&self, init: impl FnOnce() -> CliResult<T>) -> CliResult<Arc<T>> {
        {
            let guard = self.value.lock().expect("session cache mutex");
            if let Some(value) = guard.as_ref() {
                return Ok(Arc::clone(value));
            }
        }

        let value = Arc::new(init()?);
        let mut guard = self.value.lock().expect("session cache mutex");
        Ok(Arc::clone(guard.get_or_insert_with(|| Arc::clone(&value))))
    }
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
            prepared: SessionCache::new(),
            finalized_contract: SessionCache::new(),
            resolved_contract: SessionCache::new(),
            generated_schema: SessionCache::new(),
        }
    }

    /// Return the memoized chart analysis artifact.
    pub fn analysis(&self) -> CliResult<Analysis> {
        Ok(self.prepared()?.analysis.clone())
    }

    /// Return typed schema-lowering evidence derived from the guarded contract.
    pub fn contract_schema_signals(&self) -> CliResult<ContractSchemaSignals> {
        Ok(self.finalized_contract()?.schema_signals().clone())
    }

    /// Return the stable versioned contract export document.
    pub fn contract_document(&self) -> CliResult<ContractDocument> {
        Ok(self.finalized_contract()?.document())
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
        Ok((*self.generated_schema.get_or_try_init(|| {
            let prepared = self.prepared()?;
            let resolved = self.resolved()?;
            let finalized_contract = self.finalized_contract()?;
            Ok(generate_schema_from_resolved_contract(
                &resolved,
                &prepared,
                finalized_contract.schema_signals(),
                &self.opts,
            ))
        })?)
        .clone())
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

    fn prepared(&self) -> CliResult<Arc<PreparedSession>> {
        self.prepared
            .get_or_try_init(|| PreparedSession::from_generate_options(&self.opts))
    }

    fn chart_base_dir(&self) -> &Path {
        Path::new(self.opts.chart_dir.as_str())
    }

    fn finalized_contract(&self) -> CliResult<Arc<FinalizedContract>> {
        self.finalized_contract.get_or_try_init(|| {
            let prepared = self.prepared()?;
            Ok(prepared.analysis.contract.clone().finalize())
        })
    }

    fn resolved(&self) -> CliResult<Arc<ResolvedContract>> {
        self.resolved_contract.get_or_try_init(|| {
            let prepared = self.prepared()?;
            let finalized_contract = self.finalized_contract()?;
            resolve_contract_from_prepared(
                &prepared,
                finalized_contract.schema_signals(),
                &self.opts,
                Some(&self.diagnostics),
            )
        })
    }
}

pub(crate) fn resolve_contract_from_prepared(
    prepared: &PreparedSession,
    contract_schema_signals: &ContractSchemaSignals,
    opts: &GenerateOptions,
    diagnostic_sink: Option<&DiagnosticSink>,
) -> CliResult<ResolvedContract> {
    let mut provider_options = opts.provider.clone();
    provider_options.local_schema_universe = prepared.analysis.local_schemas.clone();
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(contract_schema_signals, &provider)
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
    contract_schema_signals: &ContractSchemaSignals,
    opts: &GenerateOptions,
) -> GeneratedSchema {
    let mut schema = resolved.schema.clone();

    if opts.infer_required {
        helm_schema_gen::required_inference::apply_required_inference(
            &mut schema,
            contract_schema_signals.schema_evidence_by_value_path(),
            &prepared.explicit_value_paths,
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
