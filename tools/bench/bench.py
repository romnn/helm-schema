#!/usr/bin/env python3
"""End-to-end helm-schema benchmark over a fixed set of vendored corpus charts.

Runs a release `helm-schema` binary offline against the committed provider bundle and records,
per chart and configuration:

- CPU time, wall time and peak RSS: the median of a stated number of untraced runs (CASES);
- the output's size: the pretty schema the CLI writes by default, and the compact schema with
  short `$defs` keys that `helm-schema shorten --compact` makes of it (what a chart over Helm's
  5 MiB limit ships), plus the number of `$defs` entries;
- per-phase wall times from the CLI's own tracing spans (`--trace-output`), taken from a separate
  traced run, since tracing slows the run it observes (ADR 0064);
- for the cheap charts (INSTRUCTION_CASES), the instructions one run executes, counted by valgrind's
  cachegrind when valgrind is available: unlike a time, the count does not depend on the CPU a
  hosted runner happens to get.

Every untraced run of one configuration must write the same bytes, or the benchmark fails: the
output is deterministic by contract (ADR 0063).

The results are written as a `github-action-benchmark` `customSmallerIsBetter` history entry list
whose `extra` strings carry benchtrail's typed payload (chart and profile as dimensions, the metric
with its unit and direction, the compiler and the CPU model as the environment), and as Markdown
tables for the job summary. The dashboard splits the times by CPU model, while the instruction
counts, which do not depend on it, stay one series (benchtrail's per-metric settings).

Only the Python standard library is used; the Perfetto trace is decoded by hand, since it holds
nothing but the slice begin/end packets `tracing-perfetto` writes.

Usage: task bench, or python3 tools/bench/bench.py [--chart NAME] [--json PATH] [--summary PATH]
       [--valgrind PATH]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CHARTS_DIR = REPO / "testdata" / "charts"
PROVIDER_BUNDLE = REPO / "testdata" / "provider-bundle"
# The Kubernetes schema version the corpus fixtures are generated against, held in the bundle.
K8S_VERSION = "v1.29.0-standalone-strict"

# The charts, from cheap to the heaviest tail, with the profiles each runs under and the number of
# untraced runs whose median is recorded. A cheap chart gets more runs, since its noise is a larger
# share of its time. Costs are median CPU seconds (full / lean) of a local release build on an
# Apple M3 Pro, from a survey of every corpus chart (2026-10-20; see the continuous-benchmarks
# ADR).
#
# - kubeview (0.06 s): the fixed cost of a run, from opening the provider bundle to writing the
#   output; lean changes nothing here, so it runs once.
# - bitnami-redis (2.4 / 1.6 s): the typical Bitnami chart on the `common` library, where analysis
#   of helper-heavy templates is 40% of the run; lean drops most of the conditional emission.
# - kube-prometheus-stack (20 / 11 s): a heavy tail with subcharts and chart-local CRDs (the local
#   CRD universe and custom-resource instances), where schema generation is three quarters of the
#   run, which lean nearly halves.
# - oncall (lean only, 16 s; full 17 s): the heavy tail lean barely helps. Its full run has the
#   phase proportions of kube-prometheus-stack's, but under lean its selected-constraint emission
#   stays (7 s, against 3 s for kube-prometheus-stack), so it is the lean path's heavy case.
#
# Left out, since they add cost without a new signal: openebs (41 s, the corpus maximum) has the
# phase proportions of kube-prometheus-stack at twice its cost; the CRD-heavy charts
# (external-secrets, kyverno, traefik) spend under 5 ms building their CRD universe, so their cost
# is ordinary analysis and emission, which the charts above cover.
@dataclass(frozen=True)
class Case:
    chart: str
    profiles: tuple[str, ...]
    reps: int


CASES = [
    Case("kubeview", ("full",), 9),
    Case("bitnami-redis", ("full", "lean"), 5),
    Case("kube-prometheus-stack", ("full", "lean"), 3),
    Case("oncall", ("lean",), 3),
]

# The configurations whose executed instructions are counted under cachegrind, which runs a binary
# about 27 times slower than natively (an Ubuntu 24.04 container on an Apple M3 Pro, 2026-10-20):
# kubeview, the fixed cost (5 s, 553 million instructions), and bitnami-redis full, helper-heavy
# analysis (95-110 s, 25.6 billion). bitnami-redis lean (70-80 s, 19.0 billion) would add over a
# minute on a hosted runner for a path its full run mostly covers, and the heavy tails would add
# tens of minutes.
# One counted run per configuration: for a fixed binary and input the count varies by under 0.1%
# between runs (std's per-process random hash seeds change hash-table probing; disabling address
# space randomization changes nothing), against benchtrail's 10% threshold. helm-schema generates
# a schema on one thread, so thread scheduling adds nothing, and valgrind hides AVX-512 from the
# program, so CPU feature dispatch takes the same paths on every runner CPU.
INSTRUCTION_CASES = [("kubeview", "full"), ("bitnami-redis", "full")]

# The tracing spans reported as phases, in pipeline order: the direct children of the CLI's
# `helm_schema_run` span that cost time, and the two heavy stages inside schema generation. A
# span's time is the union of its intervals, so a span that nests in itself is not counted twice.
# Loading the chart (`load`) is left out: at most a few milliseconds of file I/O, it is noise.
PHASES = {
    "analyze_charts": "template analysis",
    "finalize": "contract finalization",
    "generate_values_schema_with_report": "schema generation",
    "collect_conditional_schemas": "conditional schemas",
    "append_selected_constraints": "selected constraints",
    "minimize_schema": "minimization",
}

MIB = 1024 * 1024


@dataclass
class Run:
    """One untraced run: its costs and a digest of what it wrote."""

    wall: float
    cpu: float
    peak_rss: int
    digest: str


def ru_maxrss_bytes(maxrss: int) -> int:
    # Linux reports kibibytes, macOS bytes.
    return maxrss if sys.platform == "darwin" else maxrss * 1024


def cli_args(binary: Path, chart: str, profile: str, output: Path) -> list[str]:
    cache = PROVIDER_BUNDLE
    return [
        str(binary),
        str(CHARTS_DIR / chart),
        "--offline",
        "--k8s-version",
        K8S_VERSION,
        "--k8s-schema-cache-dir",
        str(cache / "kubernetes-json-schema-cache"),
        "--crd-catalog-cache-dir",
        str(cache / "crds-catalog-cache"),
        "--profile",
        profile,
        "--output",
        str(output),
    ]


def run_once(args: list[str]) -> tuple[float, float, int]:
    """Run a command and return its wall seconds, CPU seconds and peak RSS bytes."""
    start = time.perf_counter()
    process = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    assert process.stderr is not None
    stderr = process.stderr.read()
    _, status, usage = os.wait4(process.pid, 0)
    wall = time.perf_counter() - start
    process.returncode = os.waitstatus_to_exitcode(status)
    if process.returncode != 0:
        sys.exit(
            f"{' '.join(args)} exited with {process.returncode}:\n"
            f"{stderr.decode(errors='replace')}"
        )
    return wall, usage.ru_utime + usage.ru_stime, ru_maxrss_bytes(usage.ru_maxrss)


def read_varint(data: bytes, pos: int) -> tuple[int, int]:
    value = 0
    shift = 0
    while True:
        byte = data[pos]
        pos += 1
        value |= (byte & 0x7F) << shift
        if byte < 0x80:
            return value, pos
        shift += 7


def fields(data: bytes):
    """Yield (field number, wire type, value) for one protobuf message."""
    pos = 0
    while pos < len(data):
        key, pos = read_varint(data, pos)
        number, wire = key >> 3, key & 7
        if wire == 0:
            value, pos = read_varint(data, pos)
        elif wire == 1:
            value, pos = data[pos : pos + 8], pos + 8
        elif wire == 2:
            length, pos = read_varint(data, pos)
            value, pos = data[pos : pos + length], pos + length
        elif wire == 5:
            value, pos = data[pos : pos + 4], pos + 4
        else:
            raise ValueError(f"unsupported protobuf wire type {wire}")
        yield number, wire, value


# Perfetto field numbers: Trace.packet, TracePacket.timestamp and .track_event, TrackEvent.type,
# .track_uuid and .name; and the TrackEvent types SLICE_BEGIN and SLICE_END.
TRACE_PACKET, PACKET_TIMESTAMP, PACKET_TRACK_EVENT = 1, 8, 11
EVENT_TYPE, EVENT_TRACK_UUID, EVENT_NAME = 9, 11, 23
SLICE_BEGIN, SLICE_END = 1, 2


def span_intervals(trace: bytes) -> dict[str, list[tuple[int, int]]]:
    """The [begin, end) nanosecond intervals of every span, by name.

    Perfetto pairs a SLICE_END, which carries no name, with the most recent open SLICE_BEGIN on
    the same track, so nested spans close innermost first. A span drawn on several tracks yields
    the same interval once per track; phase times are unions, so the repeats do not count twice.
    """
    intervals: dict[str, list[tuple[int, int]]] = {}
    open_spans: dict[int | None, list[tuple[str, int]]] = {}
    for number, _, packet in fields(trace):
        if number != TRACE_PACKET:
            continue
        timestamp = None
        event = None
        for field, _, value in fields(packet):
            if field == PACKET_TIMESTAMP:
                timestamp = value
            elif field == PACKET_TRACK_EVENT:
                event = value
        if event is None or timestamp is None:
            continue
        kind = None
        track = None
        name = None
        for field, _, value in fields(event):
            if field == EVENT_TYPE:
                kind = value
            elif field == EVENT_TRACK_UUID:
                track = value
            elif field == EVENT_NAME:
                name = value.decode()
        if kind == SLICE_BEGIN:
            open_spans.setdefault(track, []).append((name, timestamp))
        elif kind == SLICE_END:
            stack = open_spans.get(track)
            if not stack:
                raise ValueError(f"span end on track {track} without its begin")
            begin_name, begin = stack.pop()
            intervals.setdefault(begin_name, []).append((begin, timestamp))
    unclosed = sorted(name for stack in open_spans.values() for name, _ in stack)
    if unclosed:
        raise ValueError(f"spans never closed: {unclosed[:5]}")
    return intervals


def union_seconds(intervals: list[tuple[int, int]]) -> float:
    total = 0
    end = None
    for begin, finish in sorted(intervals):
        if end is None or begin > end:
            total += finish - begin
            end = finish
        elif finish > end:
            total += finish - end
            end = finish
    return total / 1e9


def traced_phases(args: list[str], trace: Path) -> dict[str, float]:
    run_once([*args, "--trace-output", str(trace)])
    intervals = span_intervals(trace.read_bytes())
    phases = {}
    for phase in PHASES:
        # A phase that disappears is a renamed span, which must be renamed here too.
        if phase not in intervals:
            sys.exit(f"the trace of {' '.join(args)} has no {phase!r} span")
        phases[phase] = union_seconds(intervals[phase])
    return phases


def instruction_count(valgrind: str, args: list[str], out: Path) -> int:
    """The instructions one run executes, from the summary line of its cachegrind output."""
    # Without the cache simulation, cachegrind counts instructions only, which is what the count
    # needs and the fastest way to run.
    run_once([valgrind, "--tool=cachegrind", "--cache-sim=no", f"--cachegrind-out-file={out}", *args])
    for line in out.read_text().splitlines():
        if line.startswith("summary:"):
            return int(line.split()[1])
    sys.exit(f"the cachegrind output {out} has no summary line")


def bench(
    binary: Path,
    chart: str,
    profile: str,
    reps: int,
    traces: int,
    valgrind: str | None,
    work: Path,
) -> dict:
    output = work / f"{chart}-{profile}.json"
    args = cli_args(binary, chart, profile, output)
    runs = []
    for _ in range(reps):
        wall, cpu, rss = run_once(args)
        digest = hashlib.sha256(output.read_bytes()).hexdigest()
        runs.append(Run(wall, cpu, rss, digest))
    digests = {run.digest for run in runs}
    if len(digests) != 1:
        sys.exit(f"{chart}/{profile} wrote {len(digests)} different schemas in {reps} runs")

    schema = json.loads(output.read_bytes())
    shortened = work / f"{chart}-{profile}.short.json"
    run_once([str(binary), "shorten", str(output), str(shortened), "--compact"])

    trace = work / f"{chart}-{profile}.pftrace"
    phase_runs = [traced_phases(args, trace) for _ in range(traces)]

    instructions = None
    if valgrind and (chart, profile) in INSTRUCTION_CASES:
        instructions = instruction_count(valgrind, args, work / f"{chart}-{profile}.cachegrind")
        # The counted run must write what the untraced runs wrote, or it did not run the same work.
        if hashlib.sha256(output.read_bytes()).hexdigest() not in digests:
            sys.exit(f"{chart}/{profile} wrote a different schema under cachegrind")

    cpus = [run.cpu for run in runs]
    return {
        "chart": chart,
        "profile": profile,
        "reps": reps,
        "wall": statistics.median(run.wall for run in runs),
        "cpu": statistics.median(cpus),
        "cpu_range": (min(cpus), max(cpus)),
        "peak_rss": statistics.median(run.peak_rss for run in runs),
        "pretty": output.stat().st_size,
        "compact": shortened.stat().st_size,
        "defs": len(schema.get("$defs", {})),
        "phases": {
            phase: statistics.median(run[phase] for run in phase_runs) for phase in PHASES
        },
        "instructions": instructions,
    }


def rustc_version() -> str:
    """The compiler on PATH, which built the binary in CI; a new one moves every value."""
    try:
        return subprocess.run(
            ["rustc", "--version"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def cpu_model() -> str:
    """The host's CPU, shown beside each run: runner hardware varies between runs."""
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    if sys.platform == "darwin":
        brand = subprocess.run(
            ["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True
        ).stdout.strip()
        if brand:
            return brand
    return platform.machine()


# The vendor words a CPU model name starts with, which `short_cpu` drops.
CPU_VENDORS = ("AMD", "Intel", "Apple")


def short_cpu(raw: str) -> str:
    """A CPU model name without its noise, which names a series on the dashboard.

    Only noise goes: the vendor word, trademark marks, an Intel name's `CPU @ <frequency>`, an AMD
    name's trailing `<N>-Core Processor` or `Processor` (the host's core count, not the runner's),
    and repeated whitespace. The family word and every model token stay, so different models never
    share a name: "AMD EPYC 7763 64-Core Processor" is "EPYC 7763", "Intel(R) Xeon(R) Platinum
    8370C CPU @ 2.80GHz" is "Xeon Platinum 8370C", "Apple M3 Pro" is "M3 Pro". A name that does
    not start with a known vendor, or that would be left empty, stays as it is, whitespace
    collapsed: nothing is guessed.
    """
    collapsed = " ".join(raw.split())
    marks = re.compile(r"\((?:R|TM|C)\)", re.IGNORECASE)
    words = marks.sub("", collapsed).split()
    if not words or words[0] not in CPU_VENDORS:
        return collapsed
    text = " ".join(words[1:])
    text = re.sub(r" CPU @ [0-9.]+ ?[GM]Hz$", "", text)
    text = re.sub(r" (?:[0-9]+-Core )?Processor$", "", text)
    return text or collapsed


def valgrind_version(valgrind: str) -> str:
    """The valgrind that counts the instructions; a new one may count differently."""
    try:
        return subprocess.run(
            [valgrind, "--version"], capture_output=True, text=True, check=True
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"valgrind {valgrind} does not run: {error}")


RUSTC = rustc_version()
# The model as the host names it, noted beside each value, and its short name, which the
# environment records.
CPU_NAME = cpu_model()
CPU = short_cpu(CPU_NAME)


def entry(
    result: dict,
    metric: str,
    label: str,
    unit: str,
    value,
    direction: str = "lower",
    environment: dict | None = None,
    text: str | None = None,
):
    """One `customSmallerIsBetter` entry with benchtrail's typed payload in `extra`.

    The compiler and the CPU model are the environment. A toolchain update starts a new segment
    of the history, so no change is reported across it. The CPU model changes from run to run on a
    pool of hosted runners, which would leave no segment long enough to judge, so the dashboard
    splits each benchmark into one series per CPU model (benchtrail's `splitEnvironment`): a
    change is a jump within one CPU's series, never a change of runner.
    """
    low, high = result["cpu_range"]
    payload = {
        "benchtrail": {
            "version": 1,
            "dimensions": {"chart": result["chart"], "profile": result["profile"]},
            "metric": {"id": metric, "label": label, "direction": direction},
            "environment": {"rustc": RUSTC, "cpu": CPU, **(environment or {})},
        },
        "text": text
        or f"cpu {low:.3f}-{high:.3f} s over {result['reps']} runs; {CPU_NAME}; {RUSTC}",
    }
    return {
        "name": f"{result['chart']}/{result['profile']} {metric}",
        "unit": unit,
        "value": value,
        "extra": json.dumps(payload, separators=(",", ":")),
    }


def entries(result: dict, valgrind: str | None) -> list[dict]:
    out = instruction_entries(result, valgrind) if valgrind else []
    out += [
        entry(result, "cpu", "CPU time", "s", round(result["cpu"], 4)),
        entry(result, "wall", "Wall time", "s", round(result["wall"], 4)),
        entry(result, "peak-rss", "Peak RSS", "MiB", round(result["peak_rss"] / MIB, 1)),
        entry(result, "schema-size", "Schema size", "KiB", round(result["pretty"] / 1024, 1)),
        entry(
            result,
            "compact-size",
            "Compact schema size (short $defs)",
            "KiB",
            round(result["compact"] / 1024, 1),
        ),
        # More or fewer shared definitions is neither better nor worse in itself.
        entry(result, "defs", "$defs entries", "count", result["defs"], direction="unknown"),
    ]
    for phase, label in PHASES.items():
        out.append(
            entry(
                result,
                f"phase:{phase}",
                f"Phase: {label} ({phase})",
                "s",
                round(result["phases"][phase], 4),
            )
        )
    return out


def instruction_entries(result: dict, valgrind: str) -> list[dict]:
    """The instruction count, with the valgrind that counted it as part of its environment."""
    if result["instructions"] is None:
        return []
    return [
        entry(
            result,
            "instructions",
            "Instructions (cachegrind)",
            "count",
            result["instructions"],
            environment={"valgrind": valgrind},
            text=f"one cachegrind run; {valgrind}; {CPU_NAME}; {RUSTC}",
        )
    ]


def markdown(results: list[dict], traces: int) -> str:
    header = ["chart", "profile", "runs", "cpu s", "wall s", "peak RSS MiB", "schema KiB"]
    header += ["compact KiB", "$defs", *PHASES]
    lines = [
        f"helm-schema benchmark: medians of untraced runs; phases (wall s) from {traces} traced "
        f"run(s) per configuration. Host: {CPU_NAME}, {os.cpu_count()} CPUs; {RUSTC}.",
        "",
        "| " + " | ".join(header) + " |",
        "| --- | --- |" + " ---: |" * (len(header) - 2),
    ]
    for r in results:
        row = [r["chart"], r["profile"], str(r["reps"]), f"{r['cpu']:.3f}", f"{r['wall']:.3f}"]
        row += [f"{r['peak_rss'] / MIB:.0f}", f"{r['pretty'] / 1024:.0f}"]
        row += [f"{r['compact'] / 1024:.0f}", str(r["defs"])]
        row += [f"{r['phases'][phase]:.3f}" for phase in PHASES]
        lines.append("| " + " | ".join(row) + " |")
    counted = [r for r in results if r["instructions"] is not None]
    if counted:
        lines += [
            "",
            "Executed instructions, counted by one cachegrind run per configuration:",
            "",
            "| chart | profile | instructions |",
            "| --- | --- | ---: |",
        ]
        lines += [f"| {r['chart']} | {r['profile']} | {r['instructions']:,} |" for r in counted]
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=Path, default=REPO / "target" / "release" / "helm-schema")
    parser.add_argument("--reps", type=int, help="untraced runs per configuration, for every chart")
    parser.add_argument("--traces", type=int, default=1, help="traced runs per configuration")
    parser.add_argument("--chart", action="append", help="only these charts (repeatable)")
    parser.add_argument("--json", type=Path, help="write the github-action-benchmark entries here")
    parser.add_argument("--summary", type=Path, help="append the Markdown table here")
    parser.add_argument(
        "--valgrind",
        help="the valgrind that counts instructions, which must run (default: the one on PATH; "
        "without one, instructions are not counted)",
    )
    args = parser.parse_args()
    if args.reps is not None and args.reps < 3:
        parser.error("--reps must be at least 3, so that the median ignores an outlier")
    if args.traces < 1:
        parser.error("--traces must be at least 1")
    cases = [case for case in CASES if not args.chart or case.chart in args.chart]
    if args.chart and len(cases) != len(set(args.chart)):
        parser.error(f"--chart must name charts of the benchmark: {[c.chart for c in CASES]}")
    valgrind_path = args.valgrind or shutil.which("valgrind")
    valgrind = valgrind_version(valgrind_path) if valgrind_path else None
    if not valgrind:
        print("valgrind not found: instructions are not counted", file=sys.stderr)

    results = []
    with tempfile.TemporaryDirectory(prefix="helm-schema-bench-") as work:
        for case in cases:
            for profile in case.profiles:
                started = time.perf_counter()
                reps = args.reps or case.reps
                result = bench(
                    args.binary,
                    case.chart,
                    profile,
                    reps,
                    args.traces,
                    valgrind_path,
                    Path(work),
                )
                results.append(result)
                counted = result["instructions"] is not None
                print(
                    f"{case.chart}/{profile}: cpu {result['cpu']:.3f} s, "
                    f"wall {result['wall']:.3f} s, rss {result['peak_rss'] / MIB:.0f} MiB"
                    + (f", {result['instructions']:,} instructions" if counted else "")
                    + f" ({time.perf_counter() - started:.1f} s for {reps} + {args.traces}"
                    + (" + 1 cachegrind" if counted else "")
                    + " runs)",
                    file=sys.stderr,
                )

    table = markdown(results, args.traces)
    print(table)
    if args.summary:
        with args.summary.open("a") as summary:
            summary.write(table)
    if args.json:
        history = [item for result in results for item in entries(result, valgrind)]
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps(history, indent=2) + "\n")


if __name__ == "__main__":
    main()
