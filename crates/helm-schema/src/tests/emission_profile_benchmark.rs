use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_gen::bench_support::BenchmarkPolicy;
use helm_schema_gen::{EmissionClassKind, EmissionPolicyDelta, EmissionSelection, SchemaProfile};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use vfs::VfsPath;

use crate::AnalysisSession;
use crate::generation::GenerateOptions;
use crate::output_pipeline::{
    EmitRequest, FinalOutputPolicy, JsonOutputFormat, OutputPipelineOptions, PreparedEmitRequest,
    ReferencePolicy, apply_schema_output_pipeline, write_schema_json,
};
use crate::provider_builder::ProviderOptions;

#[test]
#[ignore = "maintenance: records the release emission-profile benchmark"]
fn emission_profile_release_benchmark() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let output_dir = benchmark_output_dir()?;
    std::fs::create_dir_all(&output_dir)
        .wrap_err_with(|| format!("create {}", output_dir.display()))?;
    let runs = benchmark_runs()?;
    let resolved_policies = resolved_policies()?;
    let benchmark_policies = resolved_policies
        .iter()
        .map(|(name, resolved)| BenchmarkPolicy {
            name,
            policy: resolved.policy(),
        })
        .collect::<Vec<_>>();
    let session = temporal_session();
    let generation_started = Instant::now();
    let benchmark = session.benchmark_emission_policies(&benchmark_policies, runs)?;
    let generation_elapsed = generation_started.elapsed();
    let generation_peak_rss_kib = high_water_memory_kib();
    let emit_request = EmitRequest {
        reference_policy: ReferencePolicy::SelfContained,
        output: OutputPipelineOptions {
            strip_descriptions: true,
            minimize: true,
        },
    };
    let chart_dir = temporal_chart_path();
    let (policy_reports, full_raw_schema) = finalize_policy_outputs(
        benchmark.policies,
        &resolved_policies,
        emit_request,
        &chart_dir,
        &output_dir,
    )?;

    let mut scalar_plain_raw = full_raw_schema;
    let scalar_plain_rewrites = remove_scalar_spelling_alternatives(&mut scalar_plain_raw);
    let scalar_plain_schema = apply_schema_output_pipeline(
        scalar_plain_raw,
        PreparedEmitRequest::empty(emit_request),
        &chart_dir,
        FinalOutputPolicy::new(SchemaProfile::Full.resolved_policy(), false),
    )?;
    let scalar_plain_metrics =
        write_schema_file(&output_dir, "scalar-plain", &scalar_plain_schema)?;

    let full_metrics = policy_reports
        .get("full")
        .and_then(|value| value.get("final_output"))
        .ok_or_eyre("full final-output metrics are missing")?;
    let full_bytes = full_metrics
        .get("serialized_bytes")
        .and_then(Value::as_u64)
        .ok_or_eyre("full serialized-byte metric is missing")?;
    let full_objects = full_metrics
        .get("objects")
        .and_then(Value::as_u64)
        .ok_or_eyre("full object metric is missing")?;

    let report = json!({
        "format_version": 1,
        "anchor": {
            "chart": "schema-emission-temporal-wrapper",
            "dependency": "temporal",
            "dependency_version": "0.62.0",
            "archive_sha256": sha256_file(&temporal_chart_path().join("charts/temporal-0.62.0.tgz"))?,
            "lock_sha256": sha256_file(&temporal_chart_path().join("Chart.lock"))?,
        },
        "runs": runs.get(),
        "generation": {
            "end_to_end_elapsed_ms": duration_ms(generation_elapsed),
            "peak_rss_kib": generation_peak_rss_kib,
            "retained_plan_bytes": benchmark.retained_plan_bytes,
            "retained_candidate_bytes": benchmark.retained_candidate_bytes,
            "plan_construction_ms": duration_statistics(&benchmark.plan_construction_times),
            "policies": Value::Object(policy_reports),
        },
        "scalar_spellings_plain": {
            "status": "measurement-only",
            "rewritten_unions": scalar_plain_rewrites,
            "serialized_bytes": scalar_plain_metrics.serialized_bytes,
            "objects": scalar_plain_metrics.objects,
            "bytes_saved": full_bytes.saturating_sub(scalar_plain_metrics.serialized_bytes as u64),
            "objects_saved": full_objects.saturating_sub(scalar_plain_metrics.objects as u64),
        },
    });
    let report_path = output_dir.join("metrics.json");
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    std::fs::write(&report_path, bytes)
        .wrap_err_with(|| format!("write {}", report_path.display()))?;
    eprintln!("benchmark-report={}", report_path.display());
    Ok(())
}

fn finalize_policy_outputs(
    policies: Vec<helm_schema_gen::bench_support::PolicyProjectionBenchmark>,
    resolved_policies: &[(&str, helm_schema_gen::ResolvedEmissionPolicy)],
    emit_request: EmitRequest,
    chart_dir: &Path,
    output_dir: &Path,
) -> eyre::Result<(serde_json::Map<String, Value>, Value)> {
    let full_raw_schema = policies
        .iter()
        .find(|policy| policy.name == "full")
        .map(|policy| policy.schema.clone())
        .ok_or_eyre("benchmark did not produce the full policy")?;
    let mut reports = serde_json::Map::new();
    for policy in policies {
        let resolved = resolved_policies
            .iter()
            .find(|(name, _)| *name == policy.name)
            .map(|(_, resolved)| *resolved)
            .ok_or_eyre("benchmark policy lost its resolved policy")?;
        let final_schema = apply_schema_output_pipeline(
            policy.schema,
            PreparedEmitRequest::empty(emit_request),
            chart_dir,
            FinalOutputPolicy::new(resolved, false),
        )?;
        let metrics = write_schema_file(output_dir, policy.name, &final_schema)?;
        reports.insert(
            policy.name.to_string(),
            json!({
                "projection_ms": duration_statistics(&policy.projection_times),
                "completion_ms": duration_statistics(&policy.completion_times),
                "emission": emission_report_json(&policy.emission_report),
                "final_output": final_output_metrics_json(metrics),
            }),
        );
    }
    Ok((reports, full_raw_schema))
}

#[test]
#[ignore = "maintenance: records validator compile cost in an isolated process"]
fn emission_profile_validator_benchmark() -> eyre::Result<()> {
    let output_dir = benchmark_output_dir()?;
    let report_path = output_dir.join("metrics.json");
    let source = std::fs::read_to_string(&report_path)
        .wrap_err_with(|| format!("read {}", report_path.display()))?;
    let mut report: Value = serde_json::from_str(&source)
        .wrap_err_with(|| format!("parse {}", report_path.display()))?;
    let mut schemas = BTreeMap::new();
    for name in ["full", "lean", "temporal-fast", "scalar-plain"] {
        let path = output_dir.join(format!("{name}.schema.json"));
        let source =
            std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
        schemas.insert(
            name.to_string(),
            serde_json::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))?,
        );
    }
    let defaults = read_temporal_defaults()?;
    let peak_rss_before_kib = high_water_memory_kib();
    let (policies, peak_rss_after_kib) = benchmark_validators(&schemas, &defaults)?;
    let object = report
        .as_object_mut()
        .ok_or_eyre("benchmark metrics root must be an object")?;
    object.insert(
        "validators".to_string(),
        json!({
            "peak_rss_before_kib": peak_rss_before_kib,
            "peak_rss_after_kib": peak_rss_after_kib,
            "policies": policies,
        }),
    );
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    std::fs::write(&report_path, bytes)
        .wrap_err_with(|| format!("write {}", report_path.display()))?;
    eprintln!("benchmark-report={}", report_path.display());
    Ok(())
}

fn temporal_session() -> AnalysisSession {
    let chart_dir = temporal_chart_path().to_string_lossy().to_string();
    AnalysisSession::new(GenerateOptions {
        chart_dir: VfsPath::new(vfs::PhysicalFS::new(&chart_dir)),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::Full.into(),
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
    })
}

fn resolved_policies() -> eyre::Result<Vec<(&'static str, helm_schema_gen::ResolvedEmissionPolicy)>>
{
    let temporal_fast = EmissionSelection::Preset {
        profile: SchemaProfile::Lean,
        delta: EmissionPolicyDelta::new(None, Some(false), None, None),
    }
    .resolve()?;
    Ok(vec![
        ("full", SchemaProfile::Full.resolved_policy()),
        ("lean", SchemaProfile::Lean.resolved_policy()),
        ("temporal-fast", temporal_fast),
    ])
}

fn benchmark_output_dir() -> eyre::Result<PathBuf> {
    std::env::var_os("HELM_SCHEMA_BENCH_DIR")
        .map(PathBuf::from)
        .ok_or_eyre("HELM_SCHEMA_BENCH_DIR must name a persistent benchmark directory")
}

fn benchmark_runs() -> eyre::Result<NonZeroUsize> {
    let runs = std::env::var("HELM_SCHEMA_BENCH_RUNS")
        .unwrap_or_else(|_| "3".to_string())
        .parse::<usize>()
        .wrap_err("parse HELM_SCHEMA_BENCH_RUNS")?;
    NonZeroUsize::new(runs).ok_or_eyre("HELM_SCHEMA_BENCH_RUNS must be greater than zero")
}

fn temporal_chart_path() -> PathBuf {
    test_util::workspace_testdata().join("charts/schema-emission-temporal-wrapper")
}

fn read_temporal_defaults() -> eyre::Result<Value> {
    let path = temporal_chart_path().join("values.yaml");
    let source =
        std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?;
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&source).wrap_err_with(|| format!("parse {}", path.display()))?;
    let mut defaults = serde_json::to_value(yaml)?;
    drop_null_map_entries(&mut defaults);
    Ok(defaults)
}

fn drop_null_map_entries(value: &mut Value) {
    if let Value::Object(entries) = value {
        entries.retain(|_, value| !value.is_null());
        for value in entries.values_mut() {
            drop_null_map_entries(value);
        }
    }
}

fn write_schema_file(
    output_dir: &Path,
    name: &str,
    schema: &Value,
) -> eyre::Result<crate::output_pipeline::FinalOutputMetrics> {
    let path = output_dir.join(format!("{name}.schema.json"));
    let mut bytes = Vec::new();
    let metrics = write_schema_json(&mut bytes, schema, JsonOutputFormat::Compact)?;
    std::fs::write(&path, bytes).wrap_err_with(|| format!("write {}", path.display()))?;
    Ok(metrics)
}

fn benchmark_validators(
    schemas: &BTreeMap<String, Value>,
    defaults: &Value,
) -> eyre::Result<(Value, Option<u64>)> {
    let full_schema = schemas.get("full").ok_or_eyre("full schema is missing")?;
    let started = Instant::now();
    let full = jsonschema::validator_for(full_schema)
        .map_err(|error| eyre::eyre!("compile full validator: {error}"))?;
    eyre::ensure!(full.is_valid(defaults), "full validator rejects defaults");
    let mut reports = serde_json::Map::from_iter([(
        "full".to_string(),
        json!({ "compile_ms": duration_ms(started.elapsed()) }),
    )]);

    for name in ["lean", "temporal-fast", "scalar-plain"] {
        let schema = schemas
            .get(name)
            .ok_or_else(|| eyre::eyre!("{name} schema is missing"))?;
        let started = Instant::now();
        let validator = jsonschema::validator_for(schema)
            .map_err(|error| eyre::eyre!("compile {name} validator: {error}"))?;
        eyre::ensure!(
            validator.is_valid(defaults),
            "{name} validator rejects defaults"
        );
        reports.insert(
            name.to_string(),
            json!({ "compile_ms": duration_ms(started.elapsed()) }),
        );
        drop(validator);
    }
    drop(full);
    Ok((Value::Object(reports), high_water_memory_kib()))
}

fn emission_report_json(report: &helm_schema_gen::EmissionReport) -> Value {
    let mut classes = serde_json::Map::new();
    for (name, class) in [
        ("mandatory", EmissionClassKind::Mandatory),
        ("ordinary-root", EmissionClassKind::OrdinaryRoot),
        ("ordinary-local", EmissionClassKind::OrdinaryLocal),
        ("kind-partition-root", EmissionClassKind::KindPartitionRoot),
        (
            "kind-partition-local",
            EmissionClassKind::KindPartitionLocal,
        ),
        ("terminal-always", EmissionClassKind::TerminalAlways),
        ("terminal-guarded", EmissionClassKind::TerminalGuarded),
    ] {
        let counts = report.counts_for_class(class);
        classes.insert(
            name.to_string(),
            json!({
                "lowered": counts.lowered,
                "selected": counts.selected,
                "dropped": counts.dropped,
            }),
        );
    }
    json!({
        "facts": {
            "lowered": report.facts.lowered,
            "selected": report.facts.selected,
            "dropped": report.facts.dropped,
            "by_class": classes,
        },
        "mandatory_outcomes": {
            "emitted": report.mandatory_outcomes.emitted,
            "equivalent": report.mandatory_outcomes.equivalent,
            "redundant": report.mandatory_outcomes.redundant,
            "fallback": report.mandatory_outcomes.fallback,
        },
        "carriers": {
            "root": report.carriers.root,
            "local": report.carriers.local,
            "condition_nodes": report.carriers.condition_nodes,
            "grouping_fan_in": report.carriers.grouping_fan_in,
        },
        "canonicalization": {
            "applied": report.canonicalization.applied,
            "redundant": report.canonicalization.redundant,
            "fallback": report.canonicalization.fallback,
        },
    })
}

fn final_output_metrics_json(metrics: crate::output_pipeline::FinalOutputMetrics) -> Value {
    json!({
        "serialized_bytes": metrics.serialized_bytes,
        "objects": metrics.objects,
        "condition_nodes": metrics.condition_nodes,
        "unique_conditions": metrics.unique_conditions,
        "unique_then_payloads": metrics.unique_then_payloads,
    })
}

fn duration_statistics(durations: &[Duration]) -> Value {
    let values = durations
        .iter()
        .map(|value| duration_ms(*value))
        .collect::<Vec<_>>();
    let Some((&cold, remaining)) = values.split_first() else {
        return Value::Null;
    };
    let warm = if remaining.is_empty() {
        values.as_slice()
    } else {
        remaining
    };
    let mut sorted = warm.to_vec();
    sorted.sort_by(f64::total_cmp);
    let Some((&minimum, rest)) = sorted.split_first() else {
        return Value::Null;
    };
    let maximum = rest.last().copied().unwrap_or(minimum);
    let middle = sorted.len() / 2;
    let median = if sorted.len() % 2 == 0 {
        let lower = sorted
            .get(middle.saturating_sub(1))
            .copied()
            .unwrap_or(minimum);
        let upper = sorted.get(middle).copied().unwrap_or(maximum);
        f64::midpoint(lower, upper)
    } else {
        sorted.get(middle).copied().unwrap_or(minimum)
    };
    json!({
        "cold": cold,
        "warm_median": median,
        "warm_min": minimum,
        "warm_max": maximum,
        "samples": values,
    })
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn remove_scalar_spelling_alternatives(value: &mut Value) -> usize {
    let mut rewrites = 0;
    match value {
        Value::Object(object) => {
            for child in object.values_mut() {
                rewrites += remove_scalar_spelling_alternatives(child);
            }
            if object.len() == 1 {
                for keyword in ["anyOf", "oneOf"] {
                    let replacement = object
                        .get(keyword)
                        .and_then(Value::as_array)
                        .and_then(|variants| native_scalar_without_spelling_arm(variants));
                    if let Some(replacement) = replacement {
                        *value = replacement;
                        rewrites += 1;
                        break;
                    }
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrites += remove_scalar_spelling_alternatives(item);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    rewrites
}

fn native_scalar_without_spelling_arm(variants: &[Value]) -> Option<Value> {
    let [first, second] = variants else {
        return None;
    };
    for (native, spelling) in [(first, second), (second, first)] {
        let native_type = native.get("type").and_then(Value::as_str);
        if matches!(native_type, Some("integer" | "number" | "boolean" | "null"))
            && is_scalar_spelling_schema(spelling)
        {
            return Some(native.clone());
        }
    }
    None
}

fn is_scalar_spelling_schema(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("type").and_then(Value::as_str) == Some("string") {
        return object.contains_key("pattern")
            || object.contains_key("enum")
            || object.contains_key("const");
    }
    object
        .get("anyOf")
        .and_then(Value::as_array)
        .is_some_and(|variants| {
            !variants.is_empty() && variants.iter().all(is_scalar_spelling_schema)
        })
}

fn sha256_file(path: &Path) -> eyre::Result<String> {
    let bytes = std::fs::read(path).wrap_err_with(|| format!("read {}", path.display()))?;
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}")?;
    }
    Ok(encoded)
}

#[cfg(target_os = "linux")]
fn high_water_memory_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    line.split_ascii_whitespace().nth(1)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn high_water_memory_kib() -> Option<u64> {
    None
}
