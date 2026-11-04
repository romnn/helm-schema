# Saved corpus replay and harness benchmarks

Build the tools with the repository's normal debug configuration:

```sh
CARGO_TARGET_DIR="$PWD/target" RUSTC_WRAPPER=sccache \
  /Volumes/T7/dev/round8/memcap.sh 7 \
  cargo build -p helm-schema-test-support --bins
```

Replay an entire saved preview against one complete producer dump:

```sh
/Volumes/T7/dev/round8/memcap.sh 8 target/debug/corpus_replay \
  --candidate /path/to/verified-dump \
  --baseline-ref d23159c7 \
  --evidence /path/to/preview-evidence \
  --census /path/to/placement-census.json \
  --jobs 4 --out target/replay
```

`--baseline-dump /path/to/previous-dump` replaces `--baseline-ref`.
Git refs resolve once to immutable commits. Historical baseline patterns receive
exact dialect normalization, as in the battery. Candidate schemas use their
recorded spelling. Both use the workspace-pinned validator, explicitly Draft 7.
Invalid schemas, missing probe documents, and invalid producer artifacts fail the
run; none becomes a validation rejection.

Dump content verification checks the complete registry, recipes, current recipe
input digests, artifact hashes and sizes, and any Helm-ready derivations. It
rejects a producer lock. It preserves the recorded source/build provenance and
binds it through the manifest digest; it does not require the replay binary to
have been built from the candidate's source tree. The generation/adoption
verifier remains `corpus_generation --verify` in the producer checkout.

Outputs are `replay.json` and `replay.md`. Chart totals count source document
occurrences, including embedded verdict documents and saved successful controls.
Distinct document counts deduplicate canonical JSON within a chart; each schema
is compiled once and each distinct document validated once per schema.
Class totals count the census's target/document memberships, so repeated or
multiple memberships deliberately count more than once. Unmapped occurrences
are `unclassified`. Every changed occurrence retains its source path in JSON.
A chart with no saved documents has an explicit zero row.

FA closed/reopened counts require a saved Helm or Kubernetes rejection. New FR
candidates require a saved successful Helm result and no saved Kubernetes
rejection; they still need investigation, since the saved oracle can be weaker
than a new live adjudication. Unknown oracle outcomes remain separate.
Replay reports changes and exits successfully even when candidates exist; callers
may impose their own gate on the JSON counts. It never runs Helm or recomposes a
saved document.

Freeze a `schema_emission_profiles` test executable before editing the harness.
Run the benchmark against the same producer dump, context roster and Helm build:

```sh
python3 tools/harness/benchmark.py \
  --binary before=target/harness/before-schema-emission-profiles \
  --dump /path/to/verified-dump --preview /path/to/preview-evidence \
  --cache "$PWD/helm-cache" --out target/harness/battery \
  --memcap /Volumes/T7/dev/round8/memcap.sh
```

The default charts are kube-prometheus-stack, alertmanager and argocd-apps.
Each chart gets a cold-cache run at six workers and warm shared-cache runs at
four, six and eight workers. Each battery subprocess has an 8 GiB cap and uses
6144 MiB of Helm reservations. The measurements record effective elapsed time,
exit status, censoring and frozen binary hash. Receipts make completed runs
resumable. `--warm-only` skips cold runs; repeated `--chart` narrows the set.
Perfetto traces use the existing tracing layer, not custom phase timers.
A nonzero battery exit can represent inherited defects; inspect coverage and
verdict reports, not the exit status alone, when comparing harness performance.

Use `compare_reports.py BEFORE/coverage.json AFTER/coverage.json` to compare all
coverage and verdict fields while ignoring evidence directory identities. Cost
reports and traces are operational measurements and are compared separately.

For a fair warm comparison, use `--reuse-cold-cache` after a `before` cold run.
The replay key intentionally binds the absolute store root: copying a shared
store to a new path does not make its root-bound request keys match. This option
keeps each chart's cold-run root, seeds immutable entries from `--cache` without
overwriting its home or inputs, and reuses the exact primed root across binaries
and worker counts. Use `--warm-only --workers 6` for one comparable baseline,
then the default four/six/eight worker matrix for the changed binary. The copied
root-independent content identities remain diagnostic; they never replace the
complete replay key.

Cache writes publish complete, hashed files atomically without forcing each
file and directory to stable storage. Cache durability is optional: readers
verify the complete request and every output byte, quarantine damaged or
incomplete entries, and execute Helm again on a miss. Input publications also
compare exact bytes before reuse. Chart snapshots, defaults/context caches,
worker limits, rendering policies, and verdict rules are unchanged.

`trace_summary.sql` selects thread-execution tracks only. The Perfetto layer
also emits lifetime tracks; summing both would double-count work. Inclusive
span work overlaps across workers and nested spans, so it is not additive wall
time. `summarize.py` retains those measurements separately from pool wall time,
stage execution counters, and process startup/teardown overhead.

Evidence discovery follows the preservation layout: `charts/` and `inputs/`
contain render inputs and are excluded from the probe walk. Saved control JSON
and status files are read once per exact path within each chart's immutable
preview, then document validation is deduplicated by canonical content.
Legacy index paths are rebased by their chart/adjudication/probe suffix, while
relative control references resolve from the owning verdict directory.
Dangling index links are listed per chart and in the Markdown summary; they do
not create documents or oracle verdicts. Every census document must still be
replayed, and actual saved documents, schemas, and manifests remain fail-closed.

For evidence on a slow volume, make a compact byte-verified copy once:

```sh
/Volumes/T7/dev/round8/memcap.sh 4 python3 tools/harness/stage_evidence.py \
  --source /path/to/preview-evidence --out target/replay-evidence --jobs 4
```

The staging helper copies saved documents, verdicts, relevant status files, and
chart indexes without modifying their bytes. It excludes preserved render input
trees and writes `snapshot-manifest.json` last, recording each source-relative
path, size and SHA-256. Point `--evidence` at that output. Keep staging time
separate from replay time; the source need only be copied once per immutable
preview. The replay still checks all census documents and producer artifacts.

Benchmark receipts include a canonical `verdicts.json` multiset captured before
the next run can sweep scratch directories. It retains input hashes, invocation
content keys and complete schema verdicts; only operational evidence identities
are normalized. Compare these bytes as well as coverage. Payload fields such as
`document`, `values` and `overlay` are preserved, including keys named
`evidence`. Each run uses an isolated scratch root, and retains its trace, raw
log, environment, invocation costs and binary hash.
