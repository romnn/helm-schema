use std::collections::BTreeSet;
use std::io::Read as _;
use std::path::{Component, Path};

use color_eyre::eyre::{self, WrapErr as _};
use flate2::read::GzDecoder;
use helm_schema::AnalysisSession;
use helm_schema::generation::{GenerateOptions, GeneratedSchema, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vfs::VfsPath;

const MAX_PROBES_PER_CHART: usize = 50_000;
const MAX_THIRD_LEVEL_DELETIONS_PER_CHART: usize = 2_048;
const MAX_GUARD_STATE_PAIRS_PER_CHART: usize = 8;
const MAX_GUARD_ARMS_ATTEMPTED_PER_CHART: usize = 24;
const MAX_GUARD_WITNESS_CANDIDATES_PER_CHART: usize = 128;
const MAX_COMPOSITE_STATE_PAIRS_PER_CHART: usize = 8;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) enum GuardSamplingStrategy {
    /// Samples the first guards in deterministic serialized schema order.
    #[default]
    SchemaOrderPrefix,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub(crate) struct ProbeCoverage {
    pub(crate) label: String,
    pub(crate) base_candidates: usize,
    pub(crate) base_emitted: usize,
    pub(crate) base_dropped: usize,
    pub(crate) third_level_candidates: usize,
    pub(crate) third_level_emitted: usize,
    pub(crate) third_level_dropped: usize,
    pub(crate) guards_discovered: usize,
    pub(crate) guard_sampling_strategy: GuardSamplingStrategy,
    pub(crate) guards_attempted: usize,
    pub(crate) guard_pairs_emitted: usize,
    pub(crate) guards_skipped_by_cap: usize,
    pub(crate) guards_without_witness_pair: usize,
    pub(crate) guard_witness_candidates: usize,
    pub(crate) guard_witness_candidates_dropped: usize,
    pub(crate) composite_targets: usize,
    pub(crate) composite_pairs_emitted: usize,
    pub(crate) composite_targets_without_payload: usize,
    pub(crate) composite_pairs_dropped_by_cap: usize,
    pub(crate) total_emitted: usize,
    pub(crate) total_dropped: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContractVerdict {
    Accept,
    Reject(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Transport {
    ValuesFileJson,
    Set,
    SetString,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlCategory {
    RetainedTooth,
    RemovedTooth,
    PositiveControl,
}

#[derive(Clone, Debug)]
pub(crate) enum ProbeInstance {
    Defaults,
    SparseOverride(Value),
    Coalesced(Value),
}

impl ProbeInstance {
    pub(crate) fn helm_values_file(&self) -> Option<Value> {
        match self {
            Self::Defaults => Some(Value::Object(Map::new())),
            Self::SparseOverride(value) => Some(value.clone()),
            Self::Coalesced(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SemanticControl {
    pub(crate) name: &'static str,
    pub(crate) category: ControlCategory,
    pub(crate) instance: ProbeInstance,
    pub(crate) transport: Transport,
    pub(crate) contract: ContractVerdict,
    pub(crate) lean_accepts: bool,
    pub(crate) rationale: &'static str,
}

pub(crate) struct ProfileSchemas {
    full: Validator,
    lean: Validator,
    defaults: Value,
}

impl ProfileSchemas {
    pub(crate) fn compile(
        full_schema: &Value,
        lean_schema: &Value,
        mut defaults: Value,
    ) -> eyre::Result<Self> {
        drop_null_map_entries(&mut defaults);
        let full = jsonschema::validator_for(full_schema)
            .map_err(|error| eyre::eyre!("compile full schema: {error}"))?;
        let lean = jsonschema::validator_for(lean_schema)
            .map_err(|error| eyre::eyre!("compile lean schema: {error}"))?;
        Ok(Self {
            full,
            lean,
            defaults,
        })
    }

    pub(crate) fn verdicts(&self, probe: &ProbeInstance) -> (bool, bool) {
        let instance = self.compose(probe);
        (self.full.is_valid(&instance), self.lean.is_valid(&instance))
    }

    pub(crate) fn candidate_errors(&self, probe: &ProbeInstance) -> Vec<String> {
        let instance = self.compose(probe);
        self.lean
            .iter_errors(&instance)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect()
    }

    pub(crate) fn baseline_errors(&self, probe: &ProbeInstance) -> Vec<String> {
        let instance = self.compose(probe);
        self.full
            .iter_errors(&instance)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect()
    }

    pub(crate) fn assert_monotone<'a>(
        &self,
        probes: impl IntoIterator<Item = (&'a str, &'a ProbeInstance)>,
    ) -> eyre::Result<()> {
        let mut failures = Vec::new();
        for (name, probe) in probes {
            let (full, lean) = self.verdicts(probe);
            if full && !lean {
                failures.push(format!("{name}: full accepts but lean rejects"));
            }
        }
        eyre::ensure!(
            failures.is_empty(),
            "profile monotonicity failures:\n{}",
            failures.join("\n")
        );
        Ok(())
    }

    pub(crate) fn assert_controls(&self, controls: &[SemanticControl]) -> eyre::Result<()> {
        let mut failures = Vec::new();
        for control in controls {
            let instance = self.compose(&control.instance);
            let full_errors = self
                .full
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
            let lean_errors = self
                .lean
                .iter_errors(&instance)
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
            let full = full_errors.is_empty();
            let lean = lean_errors.is_empty();
            let full_matches = match control.contract {
                ContractVerdict::Accept => full,
                ContractVerdict::Reject(_) => !full,
            };
            if !full_matches || lean != control.lean_accepts {
                failures.push(format!(
                    "{} [{:?}, {:?}]: contract={:?}, full={full}, lean={lean}, expected lean={}; {}; full errors={full_errors:?}; lean errors={lean_errors:?}",
                    control.name,
                    control.category,
                    control.transport,
                    control.contract,
                    control.lean_accepts,
                    control.rationale
                ));
            }
        }
        eyre::ensure!(
            failures.is_empty(),
            "semantic control failures:\n{}",
            failures.join("\n")
        );
        Ok(())
    }

    fn compose(&self, probe: &ProbeInstance) -> Value {
        match probe {
            ProbeInstance::Defaults => self.defaults.clone(),
            ProbeInstance::SparseOverride(value) => {
                let mut composed = self.defaults.clone();
                merge_override(&mut composed, value.clone());
                composed
            }
            // A literal `{}` is an explicitly empty coalesced document. It
            // must not be confused with the no-override/defaults case.
            ProbeInstance::Coalesced(value) => value.clone(),
        }
    }
}

pub(crate) fn generate_profile_schemas(chart_relative_path: &str) -> eyre::Result<(Value, Value)> {
    let (full, lean) = generate_profile_outputs(chart_relative_path)?;
    Ok((full.schema, lean.schema))
}

pub(crate) fn generate_profile_outputs(
    chart_relative_path: &str,
) -> eyre::Result<(GeneratedSchema, GeneratedSchema)> {
    let full = generate_schema(chart_relative_path, SchemaProfile::Full)?;
    let lean = generate_schema(chart_relative_path, SchemaProfile::Lean)?;
    Ok((full, lean))
}

pub(crate) fn profile_session(
    chart_relative_path: &str,
    profile: SchemaProfile,
    infer_required: bool,
) -> AnalysisSession {
    AnalysisSession::new(generate_options(
        chart_relative_path,
        profile,
        infer_required,
    ))
}

pub(crate) fn read_root_defaults(chart_relative_path: &str) -> eyre::Result<Value> {
    let path = chart_path(chart_relative_path).join("values.yaml");
    let source =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    serde_yaml::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))
}

pub(crate) fn read_json_fixture(
    chart_relative_path: &str,
    relative_path: &str,
) -> eyre::Result<Value> {
    let path = chart_path(chart_relative_path).join(relative_path);
    let source =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    serde_json::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))
}

pub(crate) fn read_chart_schema_fixture(chart: &str) -> eyre::Result<Value> {
    let path = test_util::workspace_testdata()
        .join("chart-corpus-schemas")
        .join(format!("{chart}.schema.json"));
    let source =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    serde_json::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))
}

pub(crate) fn structural_probe_battery(
    chart_relative_path: &str,
    defaults: &Value,
    schemas: &[&Value],
) -> eyre::Result<Vec<(String, ProbeInstance)>> {
    structural_probe_battery_with_coverage(chart_relative_path, defaults, schemas)
        .map(|(probes, _)| probes)
}

pub(crate) fn structural_probe_battery_with_coverage(
    chart_relative_path: &str,
    defaults: &Value,
    schemas: &[&Value],
) -> eyre::Result<(Vec<(String, ProbeInstance)>, ProbeCoverage)> {
    let dependency_roots = chart_dependency_roots(chart_relative_path)?;
    let retained_dependency_defaults = defaults
        .as_object()
        .map(|defaults| {
            // Helm refills a deleted dependency root from the subchart. The
            // battery preserves the supplied coalesced root, but cannot
            // synthesize child defaults missing from that input document.
            defaults
                .iter()
                .filter(|(key, _)| dependency_roots.contains(*key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut base_probes = vec![
        ("defaults".to_string(), ProbeInstance::Defaults),
        (
            "all declared keys deleted".to_string(),
            ProbeInstance::Coalesced(Value::Object(retained_dependency_defaults)),
        ),
    ];
    let mut paths = Vec::new();
    collect_paths(defaults, &mut Vec::new(), 2, &dependency_roots, &mut paths);
    let replacements = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::from(0),
        Value::from(1.5),
        Value::String(String::new()),
        Value::String("3".to_string()),
        Value::String("not-a-value".to_string()),
        Value::Array(Vec::new()),
        Value::Array(vec![Value::Object(Map::new())]),
        Value::Object(Map::new()),
        Value::Object(Map::from_iter([("unknown".to_string(), Value::Bool(true))])),
    ];
    for path in paths {
        let display = path.join(".");
        for replacement in &replacements {
            let mut patch = Value::Object(Map::new());
            set_path(&mut patch, &path, replacement.clone());
            base_probes.push((
                format!("{display} <- {}", value_shape(replacement)),
                ProbeInstance::SparseOverride(patch),
            ));
        }
    }

    let mut third_level_paths = Vec::new();
    collect_paths_at_depth(
        defaults,
        &mut Vec::new(),
        3,
        &dependency_roots,
        &mut third_level_paths,
    );
    let third_level_candidates = third_level_paths.len();
    let dropped_third_level =
        third_level_candidates.saturating_sub(MAX_THIRD_LEVEL_DELETIONS_PER_CHART);
    third_level_paths.truncate(MAX_THIRD_LEVEL_DELETIONS_PER_CHART);
    let mut third_level_probes = Vec::new();
    for path in third_level_paths {
        let mut patch = Value::Object(Map::new());
        set_path(&mut patch, &path, Value::Null);
        third_level_probes.push((
            format!("{} <- null deletion [depth 3]", path.join(".")),
            ProbeInstance::SparseOverride(patch),
        ));
    }

    let mut coverage = ProbeCoverage {
        label: chart_relative_path.to_string(),
        base_candidates: base_probes.len(),
        third_level_candidates,
        third_level_dropped: dropped_third_level,
        ..ProbeCoverage::default()
    };
    let targeted_probes = guard_state_probes(defaults, schemas, &mut coverage)?;
    let non_target_capacity = MAX_PROBES_PER_CHART.saturating_sub(targeted_probes.len());
    coverage.base_emitted = base_probes.len().min(non_target_capacity);
    coverage.base_dropped = base_probes.len().saturating_sub(coverage.base_emitted);
    let remaining = non_target_capacity.saturating_sub(coverage.base_emitted);
    coverage.third_level_emitted = third_level_probes.len().min(remaining);
    coverage.third_level_dropped += third_level_probes
        .len()
        .saturating_sub(coverage.third_level_emitted);

    let mut probes = Vec::with_capacity(
        coverage.base_emitted + coverage.third_level_emitted + targeted_probes.len(),
    );
    probes.extend(base_probes.into_iter().take(coverage.base_emitted));
    probes.extend(
        third_level_probes
            .into_iter()
            .take(coverage.third_level_emitted),
    );
    probes.extend(targeted_probes);
    coverage.total_emitted = probes.len();
    coverage.total_dropped = coverage.base_dropped
        + coverage.third_level_dropped
        + 2 * coverage.guards_skipped_by_cap
        + 2 * coverage.guards_without_witness_pair
        + 2 * coverage.composite_targets_without_payload
        + 2 * coverage.composite_pairs_dropped_by_cap;
    Ok((probes, coverage))
}

pub(crate) fn guard_state_probes(
    defaults: &Value,
    schemas: &[&Value],
    coverage: &mut ProbeCoverage,
) -> eyre::Result<Vec<(String, ProbeInstance)>> {
    let mut guards = Vec::new();
    let mut seen = BTreeSet::new();
    for schema in schemas {
        for (condition, then_schema) in root_if_arms(schema) {
            let key = serde_json::to_string(&(condition, then_schema))
                .wrap_err("serialize root guard arm")?;
            if seen.insert(key) {
                guards.push((*schema, condition, then_schema));
            }
        }
    }

    coverage.guards_discovered = guards.len();
    let mut normalized_defaults = defaults.clone();
    drop_null_map_entries(&mut normalized_defaults);
    let mut probes = Vec::new();
    for (index, (schema, condition, then_schema)) in guards
        .into_iter()
        .take(MAX_GUARD_ARMS_ATTEMPTED_PER_CHART)
        .enumerate()
    {
        if coverage.guard_pairs_emitted == MAX_GUARD_STATE_PAIRS_PER_CHART {
            break;
        }
        coverage.guards_attempted += 1;
        let guard_validator = compile_root_guard(schema, condition)?;
        let (witness_candidates, omitted_candidates) =
            synthesized_guard_witness_candidates(&normalized_defaults, schema, condition);
        coverage.guard_witness_candidates += witness_candidates.len() + omitted_candidates;
        coverage.guard_witness_candidates_dropped += omitted_candidates;
        let mut satisfied = None;
        let mut violated = None;
        for (name, instance) in witness_candidates {
            if guard_validator.is_valid(&instance) {
                satisfied.get_or_insert((name, instance));
            } else {
                violated.get_or_insert((name, instance));
            }
            if satisfied.is_some() && violated.is_some() {
                break;
            }
        }
        let (Some((satisfied_name, satisfied_probe)), Some((violated_name, violated_probe))) =
            (satisfied, violated)
        else {
            coverage.guards_without_witness_pair += 1;
            continue;
        };
        probes.push((
            format!("root guard {index} satisfied [targeted: {satisfied_name}]"),
            ProbeInstance::Coalesced(satisfied_probe.clone()),
        ));
        probes.push((
            format!("root guard {index} violated [targeted: {violated_name}]"),
            ProbeInstance::Coalesced(violated_probe.clone()),
        ));
        coverage.guard_pairs_emitted += 1;
        append_composite_state_probes(
            index,
            (schema, condition, then_schema),
            &guard_validator,
            (&satisfied_probe, &violated_probe),
            coverage,
            &mut probes,
        )?;
    }

    coverage.guards_skipped_by_cap = coverage
        .guards_discovered
        .saturating_sub(coverage.guards_attempted);
    Ok(probes)
}

fn append_composite_state_probes(
    guard_index: usize,
    (schema, condition, then_schema): (&Value, &Value, &Value),
    guard_validator: &Validator,
    (satisfied, violated): (&Value, &Value),
    coverage: &mut ProbeCoverage,
    probes: &mut Vec<(String, ProbeInstance)>,
) -> eyre::Result<()> {
    let guard_paths = schema_paths(schema, condition);
    let constrained_paths = schema_paths(schema, then_schema)
        .into_iter()
        .filter(|path| {
            !guard_paths
                .iter()
                .any(|guard_path| paths_overlap(path, guard_path))
        })
        .collect::<Vec<_>>();
    coverage.composite_targets += constrained_paths.len();
    let then_validator = compile_root_guard(schema, then_schema)?;
    for (target_index, path) in constrained_paths.into_iter().enumerate() {
        if coverage.composite_pairs_emitted == MAX_COMPOSITE_STATE_PAIRS_PER_CHART {
            coverage.composite_pairs_dropped_by_cap += 1;
            continue;
        }
        let Some((payload, satisfied_composite, violated_composite)) =
            nonconforming_composite_payload(
                guard_validator,
                &then_validator,
                satisfied,
                violated,
                &path,
            )
        else {
            coverage.composite_targets_without_payload += 1;
            continue;
        };
        let path_display = path.join(".");
        probes.push((
            format!(
                "root guard {guard_index} satisfied + {path_display} <- {} [composite {target_index}]",
                value_shape(&payload)
            ),
            ProbeInstance::Coalesced(satisfied_composite),
        ));
        probes.push((
            format!(
                "root guard {guard_index} violated + {path_display} <- {} [composite {target_index}]",
                value_shape(&payload)
            ),
            ProbeInstance::Coalesced(violated_composite),
        ));
        coverage.composite_pairs_emitted += 1;
    }
    Ok(())
}

fn synthesized_guard_witness_candidates(
    defaults: &Value,
    schema: &Value,
    condition: &Value,
) -> (Vec<(String, Value)>, usize) {
    let paths = schema_paths(schema, condition);
    let replacements = [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::from(0),
        Value::String(String::new()),
        Value::String("guard-witness".to_string()),
        Value::Array(Vec::new()),
        Value::Object(Map::new()),
    ];
    let mut candidates = vec![("defaults".to_string(), defaults.clone())];
    let mut total = 1_usize;
    for path in &paths {
        for replacement in &replacements {
            total += 1;
            if candidates.len() < MAX_GUARD_WITNESS_CANDIDATES_PER_CHART {
                candidates.push((
                    format!("{} <- {}", path.join("."), value_shape(replacement)),
                    compose_assignments(defaults, &[(path, replacement)]),
                ));
            }
        }
    }
    let paired_paths = paths.iter().take(8).collect::<Vec<_>>();
    for (left_index, left) in paired_paths.iter().enumerate() {
        for right in paired_paths.iter().skip(left_index + 1) {
            for left_value in [Value::Null, Value::Bool(false), Value::Bool(true)] {
                for right_value in [Value::Null, Value::Bool(false), Value::Bool(true)] {
                    total += 1;
                    if candidates.len() < MAX_GUARD_WITNESS_CANDIDATES_PER_CHART {
                        candidates.push((
                            format!("{} + {}", left.join("."), right.join(".")),
                            compose_assignments(
                                defaults,
                                &[(left, &left_value), (right, &right_value)],
                            ),
                        ));
                    }
                }
            }
        }
    }
    let omitted = total.saturating_sub(candidates.len());
    (candidates, omitted)
}

fn compose_assignments(defaults: &Value, assignments: &[(&Vec<String>, &Value)]) -> Value {
    let mut patch = Value::Object(Map::new());
    for (path, value) in assignments {
        set_path(&mut patch, path, (*value).clone());
    }
    let mut composed = defaults.clone();
    merge_override(&mut composed, patch);
    composed
}

fn root_if_arms(schema: &Value) -> Vec<(&Value, &Value)> {
    let mut arms = Vec::new();
    if let (Some(condition), Some(then_schema)) = (schema.get("if"), schema.get("then")) {
        arms.push((condition, then_schema));
    }
    arms.extend(
        schema
            .get("allOf")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|arm| Some((arm.get("if")?, arm.get("then")?))),
    );
    arms
}

fn schema_paths(root: &Value, schema: &Value) -> Vec<Vec<String>> {
    let mut paths = BTreeSet::new();
    collect_schema_paths(
        root,
        schema,
        &mut Vec::new(),
        &mut BTreeSet::new(),
        &mut paths,
    );
    paths.into_iter().collect()
}

fn collect_schema_paths(
    root: &Value,
    schema: &Value,
    prefix: &mut Vec<String>,
    visited_refs: &mut BTreeSet<String>,
    paths: &mut BTreeSet<Vec<String>>,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str)
        && let Some(pointer) = reference.strip_prefix('#')
        && visited_refs.insert(reference.to_string())
    {
        if let Some(target) = root.pointer(pointer) {
            collect_schema_paths(root, target, prefix, visited_refs, paths);
        }
        visited_refs.remove(reference);
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, child) in properties {
            prefix.push(name.clone());
            paths.insert(prefix.clone());
            collect_schema_paths(root, child, prefix, visited_refs, paths);
            prefix.pop();
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            prefix.push(name.to_string());
            paths.insert(prefix.clone());
            prefix.pop();
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        for child in schema
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_schema_paths(root, child, prefix, visited_refs, paths);
        }
    }
    for keyword in ["not", "if", "then", "else"] {
        if let Some(child) = schema.get(keyword) {
            collect_schema_paths(root, child, prefix, visited_refs, paths);
        }
    }
}

fn paths_overlap(left: &[String], right: &[String]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn nonconforming_composite_payload(
    guard: &Validator,
    then_schema: &Validator,
    satisfied: &Value,
    violated: &Value,
    path: &[String],
) -> Option<(Value, Value, Value)> {
    for payload in [
        Value::Bool(false),
        Value::Bool(true),
        Value::from(0),
        Value::from(1.5),
        Value::String(String::new()),
        Value::String("not-a-value".to_string()),
        Value::Array(Vec::new()),
        Value::Object(Map::new()),
    ] {
        let mut satisfied_composite = satisfied.clone();
        set_path(&mut satisfied_composite, path, payload.clone());
        let mut violated_composite = violated.clone();
        set_path(&mut violated_composite, path, payload.clone());
        if guard.is_valid(&satisfied_composite)
            && !guard.is_valid(&violated_composite)
            && !then_schema.is_valid(&satisfied_composite)
        {
            return Some((payload, satisfied_composite, violated_composite));
        }
    }
    None
}

fn compile_root_guard(schema: &Value, condition: &Value) -> eyre::Result<Validator> {
    let mut wrapper = Map::new();
    for key in ["$schema", "$defs", "definitions"] {
        if let Some(value) = schema.get(key) {
            wrapper.insert(key.to_string(), value.clone());
        }
    }
    wrapper.insert("allOf".to_string(), Value::Array(vec![condition.clone()]));
    jsonschema::validator_for(&Value::Object(wrapper))
        .map_err(|error| eyre::eyre!("compile root guard: {error}"))
}

fn chart_dependency_roots(chart_relative_path: &str) -> eyre::Result<BTreeSet<String>> {
    let chart = chart_path(chart_relative_path);
    let manifest = chart.join("Chart.yaml");
    let path = if manifest.is_file() {
        manifest
    } else {
        chart.join("Chart.template.yaml")
    };
    let source =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    let manifest: serde_yaml::Value =
        serde_yaml::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))?;
    let dependencies = manifest
        .get("dependencies")
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten();
    let mut roots = BTreeSet::new();
    let mut values_keys_by_name = std::collections::BTreeMap::new();
    for dependency in dependencies {
        let Some(name) = dependency.get("name").and_then(serde_yaml::Value::as_str) else {
            continue;
        };
        let values_key = dependency
            .get("alias")
            .and_then(serde_yaml::Value::as_str)
            .unwrap_or(name);
        values_keys_by_name.insert(name.to_string(), values_key.to_string());
    }

    let charts_dir = chart.join("charts");
    if charts_dir.is_dir() {
        let mut entries = std::fs::read_dir(&charts_dir)
            .wrap_err_with(|| format!("read {}", charts_dir.display()))?
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let entry_path = entry.path();
            let name = if entry_path.is_dir() {
                chart_name_from_directory(&entry_path)?
            } else if is_chart_archive(&entry_path) {
                chart_name_from_archive(&entry_path)?
            } else {
                None
            };
            if let Some(name) = name {
                roots.insert(values_keys_by_name.get(&name).cloned().unwrap_or(name));
            }
        }
    }
    Ok(roots)
}

fn chart_name_from_directory(chart: &Path) -> eyre::Result<Option<String>> {
    for manifest_name in ["Chart.yaml", "Chart.template.yaml"] {
        let manifest = chart.join(manifest_name);
        if manifest.is_file() {
            return chart_name_from_manifest_bytes(
                &std::fs::read(&manifest)
                    .wrap_err_with(|| format!("read {}", manifest.display()))?,
                &manifest.display().to_string(),
            );
        }
    }
    Ok(None)
}

fn chart_name_from_archive(path: &Path) -> eyre::Result<Option<String>> {
    let file = std::fs::File::open(path).wrap_err_with(|| format!("read {}", path.display()))?;
    let mut archive = tar::Archive::new(GzDecoder::new(file));
    let mut manifests = Vec::new();
    for entry in archive.entries().wrap_err("read chart archive entries")? {
        let mut entry = entry.wrap_err("read chart archive entry")?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let entry_path = entry
            .path()
            .wrap_err("read chart archive entry path")?
            .into_owned();
        let file_name = entry_path.file_name().and_then(|name| name.to_str());
        if !matches!(file_name, Some("Chart.yaml" | "Chart.template.yaml")) {
            continue;
        }
        let depth = archive_entry_depth(&entry_path);
        if depth > 2 {
            continue;
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .wrap_err("read chart manifest from archive")?;
        let chart_root = entry_path
            .parent()
            .map(|parent| parent.to_string_lossy().to_string())
            .unwrap_or_default();
        let manifest_priority = usize::from(file_name != Some("Chart.yaml"));
        manifests.push((
            depth,
            chart_root,
            manifest_priority,
            entry_path.to_string_lossy().to_string(),
            bytes,
        ));
    }
    manifests.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
    });
    let Some((_, _, _, manifest_path, bytes)) = manifests.into_iter().next() else {
        return Ok(None);
    };
    chart_name_from_manifest_bytes(&bytes, &format!("{}:{manifest_path}", path.display()))
}

pub(crate) fn archive_entry_depth(path: &Path) -> usize {
    path.components()
        .filter(|component| !matches!(component, Component::CurDir))
        .count()
}

fn chart_name_from_manifest_bytes(bytes: &[u8], source: &str) -> eyre::Result<Option<String>> {
    let manifest: serde_yaml::Value =
        serde_yaml::from_slice(bytes).wrap_err_with(|| format!("parse {source}"))?;
    Ok(manifest
        .get("name")
        .and_then(serde_yaml::Value::as_str)
        .map(str::to_string))
}

fn is_chart_archive(path: &Path) -> bool {
    let is_tgz = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tgz"));
    let is_tar_gz = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gz"))
        && path
            .file_stem()
            .and_then(|stem| Path::new(stem).extension())
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tar"));

    is_tgz || is_tar_gz
}

pub(crate) fn sparse_override(path: &[&str], value: Value) -> ProbeInstance {
    let mut override_doc = Value::Object(Map::new());
    let path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    set_path(&mut override_doc, &path, value);
    ProbeInstance::SparseOverride(override_doc)
}

fn generate_schema(
    chart_relative_path: &str,
    profile: SchemaProfile,
) -> eyre::Result<GeneratedSchema> {
    profile_session(chart_relative_path, profile, false)
        .generated_schema()
        .map(|mut generated| {
            generated.schema = helm_schema_json_schema_minify::minimize_schema(generated.schema);
            generated
        })
        .map_err(eyre::Report::from)
        .wrap_err_with(|| format!("generate {profile:?} schema for {chart_relative_path}"))
}

fn generate_options(
    chart_relative_path: &str,
    profile: SchemaProfile,
    infer_required: bool,
) -> GenerateOptions {
    let chart_dir = chart_path(chart_relative_path);
    let chart_dir = chart_dir.to_string_lossy().to_string();
    GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required,
        emission: profile.into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_testdata()
                    .join("provider-bundle/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            crd_catalog_cache_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            disable_k8s_schemas: false,
            crd_override_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            ..Default::default()
        },
    }
}

fn chart_path(chart_relative_path: &str) -> std::path::PathBuf {
    test_util::workspace_testdata()
        .join("charts")
        .join(Path::new(chart_relative_path))
}

fn merge_override(base: &mut Value, override_value: Value) {
    let overrides = match override_value {
        Value::Object(overrides) => overrides,
        mut value => {
            drop_null_map_entries(&mut value);
            *base = value;
            return;
        }
    };
    if !base.is_object() {
        *base = Value::Object(Map::new());
    }
    let Some(base) = base.as_object_mut() else {
        return;
    };
    for (key, mut value) in overrides {
        if value.is_null() {
            base.remove(&key);
        } else if let Some(existing) = base.get_mut(&key) {
            merge_override(existing, value);
        } else {
            drop_null_map_entries(&mut value);
            base.insert(key, value);
        }
    }
}

fn drop_null_map_entries(value: &mut Value) {
    if let Value::Object(entries) = value {
        entries.retain(|_, value| !value.is_null());
        for value in entries.values_mut() {
            drop_null_map_entries(value);
        }
    }
}

fn collect_paths(
    value: &Value,
    prefix: &mut Vec<String>,
    depth: usize,
    protected_roots: &BTreeSet<String>,
    paths: &mut Vec<Vec<String>>,
) {
    let Value::Object(entries) = value else {
        return;
    };
    for (key, child) in entries {
        if prefix.is_empty() && protected_roots.contains(key) {
            continue;
        }
        prefix.push(key.clone());
        paths.push(prefix.clone());
        if depth > 1 {
            collect_paths(child, prefix, depth - 1, protected_roots, paths);
        }
        prefix.pop();
    }
}

fn collect_paths_at_depth(
    value: &Value,
    prefix: &mut Vec<String>,
    depth: usize,
    protected_roots: &BTreeSet<String>,
    paths: &mut Vec<Vec<String>>,
) {
    let Value::Object(entries) = value else {
        return;
    };
    for (key, child) in entries {
        if prefix.is_empty() && protected_roots.contains(key) {
            continue;
        }
        prefix.push(key.clone());
        if prefix.len() == depth {
            paths.push(prefix.clone());
        } else {
            collect_paths_at_depth(child, prefix, depth, protected_roots, paths);
        }
        prefix.pop();
    }
}

fn set_path(target: &mut Value, path: &[String], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        *target = value;
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let Some(entries) = target.as_object_mut() else {
        return;
    };
    if tail.is_empty() {
        entries.insert(head.clone(), value);
        return;
    }
    let child = entries
        .entry(head.clone())
        .or_insert_with(|| Value::Object(Map::new()));
    set_path(child, tail, value);
}

fn value_shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null deletion",
        Value::Bool(false) => "false",
        Value::Bool(true) => "true",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(value) if value.is_empty() => "empty string",
        Value::String(value) if value == "3" => "coercible string",
        Value::String(_) => "non-coercible string",
        Value::Array(value) if value.is_empty() => "empty array",
        Value::Array(_) => "empty object item",
        Value::Object(value) if value.is_empty() => "empty object",
        Value::Object(_) => "unknown object member",
    }
}
