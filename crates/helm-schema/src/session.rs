use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use helm_schema_engine::compatibility::{
    ContractDocument, ContractDocumentUse, ContractProjection,
};
use helm_schema_engine::{ContractIr, ValuesSchemaInput, generate_values_schema};
use helm_schema_k8s::{DiagnosticSink, LocalSchemaUniverse};
use serde_json::{Map, Value};

use crate::analysis::analyze_charts;
use crate::chart;
use crate::chart_evidence::ChartTemplateEvidence;
use crate::error::CliResult;
use crate::fetch_policy::FetchPolicy;
use crate::generation::{GenerateOptions, GeneratedSchema, ResolvedContract};
use crate::load_budget::LoadBudget;
use crate::output_pipeline::{
    OutputPipelineOptions, PolicyInputOptions, PolicyInputs, apply_schema_output_pipeline,
    load_policy_inputs,
};
use crate::provider_builder;
use crate::required_inference;

/// Public analysis artifact produced by [`AnalysisSession`].
///
/// This is the current stable facade-level view of chart-local analysis:
/// the guarded contract graph plus the chart-local schema universe extracted
/// from sources such as static and template-rendered CRDs.
#[derive(Debug, Clone)]
pub struct Analysis {
    pub contract: ContractIr,
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
    pub(crate) contract: ContractIr,
    pub(crate) contract_schema_signals: helm_schema_engine::ContractSchemaSignals,
    pub(crate) template_evidence: ChartTemplateEvidence,
    pub(crate) local_schema_universe: LocalSchemaUniverse,
    pub(crate) values_yaml: Option<String>,
    pub(crate) values_descriptions: BTreeMap<String, String>,
    pub(crate) subchart_value_prefixes: Vec<Vec<String>>,
    pub(crate) shipped_values_schema_constraints: Vec<chart::ScopedValuesSchemaConstraint>,
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
        let shipped_values_schema_constraints = chart::load_shipped_values_schema_constraints(
            charts,
            FetchPolicy::input_assembly(opts.provider.allow_net),
            LoadBudget::default(),
        )?;

        Ok(Self {
            contract: chart_analysis.contract,
            contract_schema_signals: chart_analysis.contract_schema_signals,
            template_evidence: chart_analysis.template_evidence,
            local_schema_universe: chart_analysis.local_schema_universe,
            values_yaml,
            values_descriptions,
            subchart_value_prefixes: charts
                .iter()
                .filter(|chart| !chart.values_prefix.is_empty())
                .map(|chart| chart.values_prefix.clone())
                .collect(),
            shipped_values_schema_constraints,
        })
    }

    pub(crate) fn analysis(&self) -> Analysis {
        Analysis {
            contract: self.contract.clone(),
            local_schemas: self.local_schema_universe.clone(),
        }
    }
}

/// Memoized facade over the current stage pipeline.
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
        Ok(self.prepared()?.contract.clone())
    }

    /// Return the stable versioned contract export document.
    pub fn contract_document(&self) -> CliResult<ContractDocument> {
        Ok(self.contract_projection()?.as_ref().clone().into_document())
    }

    /// Return the chart-local schema universe extracted from the chart tree.
    pub fn local_schema_universe(&self) -> CliResult<LocalSchemaUniverse> {
        Ok(self.prepared()?.local_schema_universe.clone())
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
    /// been resolved against providers and chart-authored shipped
    /// `values.schema.json` constraints have been intersected, but the later
    /// heuristic `--infer-required` mutation has not yet run.
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
        let value_path_facts = prepared
            .contract_schema_signals
            .value_path_facts
            .get(&normalized_path)
            .copied();
        let guard_constraints = prepared
            .contract_schema_signals
            .path_signals
            .guard_constraints_by_value_path
            .get(&normalized_path)
            .cloned()
            .unwrap_or_default();
        let metadata_fields = prepared
            .contract_schema_signals
            .path_signals
            .metadata_fields_by_value_path
            .get(&normalized_path)
            .map(|fields| fields.iter().copied().collect())
            .unwrap_or_default();
        let type_hints = prepared
            .template_evidence
            .type_hints
            .get(&normalized_path)
            .cloned()
            .unwrap_or_default();
        let has_default_fallback = prepared
            .template_evidence
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
        let projection = Arc::new(prepared.contract.clone().project());
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
    provider_options.local_schema_universe = prepared.local_schema_universe.clone();
    let provider = provider_builder::build_provider(&provider_options, diagnostic_sink);

    let schema = generate_values_schema(
        ValuesSchemaInput::new(&prepared.contract_schema_signals, &provider)
            .with_values_yaml(prepared.values_yaml.as_deref())
            .with_type_hints(&prepared.template_evidence.type_hints)
            .with_values_descriptions(&prepared.values_descriptions),
    );
    let schema = apply_shipped_values_schema_constraints(
        schema,
        &prepared.shipped_values_schema_constraints,
    );
    validate_composed_defaults_against_schema(prepared.values_yaml.as_deref(), &schema)?;

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
            &prepared.contract_schema_signals.required_inference_signals,
            prepared.values_yaml.as_deref(),
            &prepared.template_evidence.default_fallback_paths,
        );
    }

    GeneratedSchema {
        schema,
        subchart_value_prefixes: resolved.subchart_value_prefixes.clone(),
    }
}

fn apply_shipped_values_schema_constraints(
    mut schema: Value,
    constraints: &[chart::ScopedValuesSchemaConstraint],
) -> Value {
    if constraints.is_empty() {
        return schema;
    }

    let prepared_constraints = constraints
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, constraint)| prepare_values_schema_constraint(index, constraint))
        .collect::<Vec<_>>();

    let Value::Object(root) = &mut schema else {
        return schema;
    };
    if let Some(hoisted_defs) = collect_hoisted_constraint_definitions(&prepared_constraints) {
        let definitions = root
            .entry("$defs".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        let Value::Object(existing_defs) = definitions else {
            return schema;
        };
        existing_defs.extend(hoisted_defs);
    }
    let wrapped_constraints = prepared_constraints
        .into_iter()
        .map(|constraint| wrap_values_schema_constraint(constraint))
        .collect::<Vec<_>>();
    let all_of = root
        .entry("allOf".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(entries) = all_of else {
        root.insert("allOf".to_string(), Value::Array(wrapped_constraints));
        return schema;
    };
    entries.extend(wrapped_constraints);
    schema
}

fn validate_composed_defaults_against_schema(
    values_yaml: Option<&str>,
    schema: &Value,
) -> CliResult<()> {
    let Some(values_yaml) = values_yaml else {
        return Ok(());
    };

    let values_json: Value = serde_yaml::from_str(values_yaml)?;
    let values_json = coalesced_values_json(&values_json);
    let validator = jsonschema::validator_for(schema).map_err(|err| {
        crate::error::CliError::SchemaPostconditionCompile {
            reason: err.to_string(),
        }
    })?;
    let errors = validator
        .iter_errors(&values_json)
        .map(|err| format!("{path}: {err}", path = err.instance_path()))
        .collect::<Vec<_>>();

    if errors.is_empty() {
        Ok(())
    } else {
        Err(crate::error::CliError::SchemaPostconditionViolated { errors })
    }
}

fn coalesced_values_json(value: &Value) -> Value {
    match value {
        Value::Null => Value::Null,
        Value::Bool(_) | Value::Number(_) | Value::String(_) => value.clone(),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .filter(|item| !item.is_null())
                .map(coalesced_values_json)
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, value) in map {
                if value.is_null() {
                    continue;
                }
                out.insert(key.clone(), coalesced_values_json(value));
            }
            Value::Object(out)
        }
    }
}

struct PreparedValuesSchemaConstraint {
    values_prefix: Vec<String>,
    schema: Value,
    hoisted_defs: Map<String, Value>,
}

fn prepare_values_schema_constraint(
    index: usize,
    constraint: chart::ScopedValuesSchemaConstraint,
) -> PreparedValuesSchemaConstraint {
    let namespace = format!("shippedValuesSchema{index}");
    let mut schema = constraint.schema;
    strip_root_schema_keyword(&mut schema);
    strip_root_id_keyword(&mut schema);
    let mut hoisted_defs = Map::new();
    hoist_constraint_defs(&mut schema, &namespace, &mut hoisted_defs);

    PreparedValuesSchemaConstraint {
        values_prefix: constraint.values_prefix,
        schema,
        hoisted_defs,
    }
}

fn wrap_values_schema_constraint(constraint: PreparedValuesSchemaConstraint) -> Value {
    constraint
        .values_prefix
        .iter()
        .rev()
        .fold(constraint.schema, |inner, segment| {
            Value::Object(
                [
                    ("type".to_string(), Value::String("object".to_string())),
                    (
                        "properties".to_string(),
                        Value::Object(
                            [(segment.clone(), inner)]
                                .into_iter()
                                .collect::<Map<String, Value>>(),
                        ),
                    ),
                ]
                .into_iter()
                .collect(),
            )
        })
}

fn strip_root_schema_keyword(schema: &mut Value) {
    if let Value::Object(object) = schema {
        object.remove("$schema");
    }
}

fn strip_root_id_keyword(schema: &mut Value) {
    if let Value::Object(object) = schema {
        object.remove("$id");
    }
}

fn collect_hoisted_constraint_definitions(
    constraints: &[PreparedValuesSchemaConstraint],
) -> Option<Map<String, Value>> {
    let mut definitions = Map::new();
    for constraint in constraints {
        definitions.extend(constraint.hoisted_defs.clone());
    }
    (!definitions.is_empty()).then_some(definitions)
}

fn hoist_constraint_defs(
    schema: &mut Value,
    namespace: &str,
    hoisted_defs: &mut Map<String, Value>,
) {
    rewrite_constraint_refs(schema, namespace);

    let Value::Object(object) = schema else {
        return;
    };

    for defs_key in ["$defs", "definitions"] {
        let Some(Value::Object(definitions)) = object.remove(defs_key) else {
            continue;
        };

        for (name, mut definition) in definitions {
            rewrite_constraint_refs(&mut definition, namespace);
            hoisted_defs.insert(namespaced_definition_name(namespace, &name), definition);
        }
    }
}

fn rewrite_constraint_refs(schema: &mut Value, namespace: &str) {
    match schema {
        Value::Object(object) => {
            if let Some(Value::String(reference)) = object.get_mut("$ref") {
                if let Some(def_name) = reference.strip_prefix("#/$defs/")
                    && !def_name.starts_with(&format!("{namespace}."))
                {
                    *reference = format!(
                        "#/$defs/{}",
                        namespaced_definition_name(namespace, def_name)
                    );
                } else if let Some(def_name) = reference.strip_prefix("#/definitions/")
                    && !def_name.starts_with(&format!("{namespace}."))
                {
                    *reference = format!(
                        "#/$defs/{}",
                        namespaced_definition_name(namespace, def_name)
                    );
                }
            }
            for value in object.values_mut() {
                rewrite_constraint_refs(value, namespace);
            }
        }
        Value::Array(values) => {
            for value in values {
                rewrite_constraint_refs(value, namespace);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn namespaced_definition_name(namespace: &str, name: &str) -> String {
    format!("{namespace}.{name}")
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
