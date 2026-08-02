use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use jsonschema::Validator;
use serde_json::{Map, Value};
use vfs::VfsPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContractVerdict {
    Accept,
    Reject(&'static str),
    Unresolved(&'static str),
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
            let (full, lean) = self.verdicts(&control.instance);
            let full_matches = match control.contract {
                ContractVerdict::Accept => full,
                ContractVerdict::Reject(_) => !full,
                ContractVerdict::Unresolved(_) => true,
            };
            if !full_matches || lean != control.lean_accepts {
                failures.push(format!(
                    "{} [{:?}, {:?}]: contract={:?}, full={full}, lean={lean}, expected lean={}; {}",
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
    let full = generate_schema(chart_relative_path, SchemaProfile::Full)?;
    let lean = generate_schema(chart_relative_path, SchemaProfile::Lean)?;
    Ok((full, lean))
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

pub(crate) fn structural_probe_battery(defaults: &Value) -> Vec<(String, ProbeInstance)> {
    let mut probes = vec![
        ("defaults".to_string(), ProbeInstance::Defaults),
        (
            "all declared keys deleted".to_string(),
            ProbeInstance::Coalesced(Value::Object(Map::new())),
        ),
    ];
    let mut paths = Vec::new();
    collect_paths(defaults, &mut Vec::new(), 2, &mut paths);
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
            probes.push((
                format!("{display} <- {}", value_shape(replacement)),
                ProbeInstance::SparseOverride(patch),
            ));
        }
    }
    probes
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

fn generate_schema(chart_relative_path: &str, profile: SchemaProfile) -> eyre::Result<Value> {
    let chart_dir = chart_path(chart_relative_path);
    let chart_dir = chart_dir.to_string_lossy().to_string();
    let options = GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        profile,
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
    };
    AnalysisSession::new(options)
        .generated_schema()
        .map(|generated| helm_schema_json_schema_minify::minimize_schema(generated.schema))
        .map_err(eyre::Report::from)
        .wrap_err_with(|| format!("generate {profile:?} schema for {chart_relative_path}"))
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
    paths: &mut Vec<Vec<String>>,
) {
    let Value::Object(entries) = value else {
        return;
    };
    for (key, child) in entries {
        prefix.push(key.clone());
        paths.push(prefix.clone());
        if depth > 1 {
            collect_paths(child, prefix, depth - 1, paths);
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
