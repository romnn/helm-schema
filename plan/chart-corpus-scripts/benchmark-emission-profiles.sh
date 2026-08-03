#!/usr/bin/env bash

set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
run_id="$(date -u +%Y%m%dT%H%M%SZ)"
output_dir="${HELM_SCHEMA_BENCH_DIR:-$root_dir/target/schema-emission-benchmark/$run_id}"
benchmark_runs="${HELM_SCHEMA_BENCH_RUNS:-3}"
lint_warm_runs="${HELM_SCHEMA_LINT_WARM_RUNS:-3}"
anchor="$root_dir/testdata/charts/schema-emission-temporal-wrapper"

if [[ -e "$output_dir" ]]; then
  echo "benchmark output already exists: $output_dir" >&2
  exit 2
fi
mkdir -p "$output_dir/charts"

export TMPDIR="$root_dir/target/schema-emission-benchmark/tmp"
mkdir -p "$TMPDIR"
export HELM_SCHEMA_BENCH_DIR="$output_dir"
export HELM_SCHEMA_BENCH_RUNS="$benchmark_runs"

cargo nextest run -p helm-schema --features bench-support \
  -E 'test(emission_profile_release_benchmark)' \
  --run-ignored ignored-only --nocapture
cargo nextest run -p helm-schema --features bench-support \
  -E 'test(emission_profile_validator_benchmark)' \
  --run-ignored ignored-only --nocapture

for name in baseline full lean temporal-fast scalar-plain; do
  chart_dir="$output_dir/charts/$name"
  mkdir -p "$chart_dir"
  cp -a "$anchor/." "$chart_dir/"
  mkdir -p "$chart_dir/templates"
  if [[ "$name" != baseline ]]; then
    cp "$output_dir/$name.schema.json" "$chart_dir/values.schema.json"
  fi
done

time_lint() {
  local name="$1"
  local chart_dir="$output_dir/charts/$name"
  local samples="$output_dir/$name.helm-lint.samples"
  local log="$output_dir/$name.helm-lint.log"
  local total_runs=$((lint_warm_runs + 1))

  : >"$samples"
  : >"$log"
  for ((run = 1; run <= total_runs; run += 1)); do
    /usr/bin/time -f '%e %U %S %M' -o "$output_dir/time.current" \
      helm lint "$chart_dir" --strict >>"$log" 2>&1
    tr '\n' ' ' <"$output_dir/time.current" | sed 's/[[:space:]]*$//' >>"$samples"
    printf '\n' >>"$samples"
  done

  jq -Rn --arg name "$name" '
    def median:
      sort as $s
      | if length == 0 then null
        elif length % 2 == 1 then $s[length / 2 | floor]
        else (($s[length / 2 - 1] + $s[length / 2]) / 2)
        end;
    [inputs | split(" ") | {
      elapsed_seconds: (.[0] | tonumber),
      user_seconds: (.[1] | tonumber),
      system_seconds: (.[2] | tonumber),
      max_rss_kib: (.[3] | tonumber)
    }] as $samples
    | ($samples[1:] | map(.elapsed_seconds)) as $warm
    | {
        name: $name,
        cold: $samples[0],
        warm: {
          runs: ($warm | length),
          median_seconds: ($warm | median),
          min_seconds: ($warm | min),
          max_seconds: ($warm | max),
          samples: $samples[1:]
        }
      }
  ' <"$samples" >"$output_dir/$name.helm-lint.json"
}

for name in baseline full lean temporal-fast scalar-plain; do
  time_lint "$name"
done

jq -s '
  map({key: .name, value: del(.name)}) | from_entries
' "$output_dir"/*.helm-lint.json >"$output_dir/helm-lint.json"

cpu_model="$(awk -F: '/model name/ {sub(/^[ \t]+/, "", $2); print $2; exit}' /proc/cpuinfo 2>/dev/null || true)"
memory_kib="$(awk '/MemTotal/ {print $2; exit}' /proc/meminfo 2>/dev/null || true)"
jv_version="$(jv --version 2>/dev/null || true)"
jq -n \
  --arg timestamp_utc "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --arg git_commit "$(git -C "$root_dir" rev-parse HEAD)" \
  --arg uname "$(uname -a)" \
  --arg cpu_model "$cpu_model" \
  --arg memory_kib "$memory_kib" \
  --arg rustc "$(rustc --version)" \
  --arg cargo "$(cargo --version)" \
  --arg helm "$(helm version --short)" \
  --arg jq "$(jq --version)" \
  --arg jv "$jv_version" \
  '{
    timestamp_utc: $timestamp_utc,
    git_commit: $git_commit,
    machine: {uname: $uname, cpu_model: $cpu_model, memory_kib: $memory_kib},
    tools: {rustc: $rustc, cargo: $cargo, helm: $helm, jq: $jq, jv: $jv}
  }' >"$output_dir/environment.json"

jq --slurpfile lint "$output_dir/helm-lint.json" \
  --slurpfile environment "$output_dir/environment.json" \
  '. + {helm_lint: $lint[0], environment: $environment[0]}' \
  "$output_dir/metrics.json" >"$output_dir/metrics.final.json"

echo "benchmark report: $output_dir/metrics.final.json"
