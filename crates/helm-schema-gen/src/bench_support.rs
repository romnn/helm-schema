//! Feature-gated measurements for the repository's release benchmark.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::emission_plan::{CompletionPass, LoweredEmissionPlan};
use crate::{EmissionPolicy, EmissionReport, ValuesSchemaInput};

/// One named policy projected from the shared benchmark plan.
#[derive(Debug, Clone, Copy)]
pub struct BenchmarkPolicy {
    /// Stable report label.
    pub name: &'static str,
    /// Checked policy projected by the benchmark.
    pub policy: EmissionPolicy,
}

/// Repeated timings and final artifacts for one benchmark policy.
#[derive(Debug)]
pub struct PolicyProjectionBenchmark {
    /// Stable report label.
    pub name: &'static str,
    /// Time spent selecting and materializing the projected tree per run.
    pub projection_times: Vec<Duration>,
    /// Time spent in policy-free completion passes per run.
    pub completion_times: Vec<Duration>,
    /// Completed schema from the final run.
    pub schema: Value,
    /// Emission accounting from the final run.
    pub emission_report: EmissionReport,
}

/// Measurements from repeatedly building one plan and projecting every policy.
#[derive(Debug)]
pub struct MultiPolicyBenchmark {
    /// Time spent constructing the policy-free plan per run.
    pub plan_construction_times: Vec<Duration>,
    /// Process-resident byte increase while retaining the first plan.
    pub retained_plan_bytes: Option<u64>,
    /// Unique canonical provider-candidate payload bytes retained by the plan.
    pub retained_candidate_bytes: usize,
    /// Per-policy projection and completion measurements.
    pub policies: Vec<PolicyProjectionBenchmark>,
}

/// Builds one immutable plan per run and projects all policies from that plan.
#[must_use]
pub fn benchmark_policies(
    input: &ValuesSchemaInput<'_>,
    policies: &[BenchmarkPolicy],
    runs: NonZeroUsize,
) -> MultiPolicyBenchmark {
    let mut plan_construction_times = Vec::with_capacity(runs.get());
    let mut retained_plan_bytes = None;
    let mut retained_candidate_bytes = 0;
    let mut outputs = policies
        .iter()
        .map(|policy| PolicyProjectionBenchmark {
            name: policy.name,
            projection_times: Vec::with_capacity(runs.get()),
            completion_times: Vec::with_capacity(runs.get()),
            schema: Value::Null,
            emission_report: EmissionReport::default(),
        })
        .collect::<Vec<_>>();

    for run in 0..runs.get() {
        let resident_before = resident_memory_kib();
        let started = Instant::now();
        let plan = LoweredEmissionPlan::build(input);
        plan_construction_times.push(started.elapsed());
        if run == 0 {
            retained_plan_bytes = resident_memory_kib()
                .zip(resident_before)
                .map(|(after, before)| after.saturating_sub(before) * 1024);
            retained_candidate_bytes = plan.benchmark_retained_candidate_bytes();
        }

        for (policy, output) in policies.iter().zip(&mut outputs) {
            let started = Instant::now();
            let projected = plan.project(policy.policy);
            output.projection_times.push(started.elapsed());

            let started = Instant::now();
            let completed = plan.complete(projected, CompletionPass::Descriptions);
            output.completion_times.push(started.elapsed());
            output.schema = completed.schema;
            output.emission_report = completed.emission_report;
        }
    }

    MultiPolicyBenchmark {
        plan_construction_times,
        retained_plan_bytes,
        retained_candidate_bytes,
        policies: outputs,
    }
}

#[cfg(target_os = "linux")]
fn resident_memory_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    line.split_ascii_whitespace().nth(1)?.parse().ok()
}

#[cfg(not(target_os = "linux"))]
fn resident_memory_kib() -> Option<u64> {
    None
}
