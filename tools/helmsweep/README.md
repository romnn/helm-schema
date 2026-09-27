# helmsweep

The landing sweep's Helm oracle, in-process: `helm lint` and
`helm template --skip-schema-validation` verdicts from Helm v4.2.3's own code
(the tagged `helm.sh/helm/v4` module, patched at build time: `PATCH.md`).
It deviates from the Helm CLI only in performance:

- a schema compile memo, the one Helm patch (`PATCH.md`);
- a bounded worker pool with one schema compile at a time;
- a fail-closed on-disk verdict cache.

## Build

```sh
task build:helmsweep
```

stages the patched Helm module and this module's sources in
`$CARGO_TARGET_DIR/helmsweep-build` (`stage.sh`, `PATCH.md`) and builds
`$CARGO_TARGET_DIR/helmsweep` (else `./target/…`) there with
`mise exec go@1.26.5`, `GOTOOLCHAIN=local`, `-trimpath` and `-buildvcs=false`
(so its sha256, the cache's build id, depends only on sources and toolchain), then checks
`helmsweep version`: Go 1.26.5, `helm.sh/helm/v4 v4.2.3 => ./third_party/helm-v4.2.3`,
`github.com/santhosh-tekuri/jsonschema/v6 v6.0.2` and the sha256 of
`helm-memo.patch`.

## Commands

| Command | Does |
|---|---|
| `version` | prints `build <sha256 of the executable>`, `go`, the Helm and jsonschema module versions and `patch <sha256>` |
| `lint [--helm-env-clear] --kube-version K --values F CHART` | one `helm lint CHART --kube-version K -f F`: prints the CLI's combined output, exits with its code |
| `template [--helm-env-clear] --kube-version K --values F CHART` | one `helm template t CHART --skip-schema-validation --kube-version K -f F`: prints the CLI's stderr, exits with its code |
| `classify lint\|template RC LOG` | prints the gate's class of a log (a port of `landing.py classify_helm`) |
| `sweep --roster R --roster-sha256 H --work W [--jobs N] --cache C --helm-env-clear` | the sweep, below |
| `serve` | a resident `helm template` server for helm-schema's battery, below |

## Sweep contract (for `run-landing.sh sweep`)

Inputs, as `landing.py sweep-plan` and `sweep-one.sh` lay them out today:

- `R` is the frozen `$S/roster.tsv`; `H` the sha256 `sweep-plan` printed. The
  plan is `$(dirname R)/plan/<chart>/{cand.schema.json,base.schema.json,ov/<id>.json,ov/<id>.name}`.
- `W` is the absolute `$S/w`. For every roster chart the runner has prepared
  `W/<chart>/plain` (`landing.py prep`), `W/<chart>/base` and `W/<chart>/cand`
  (`cp -R plain`, then the planned schema as `values.schema.json`, base only
  when the roster says `baseline`), and nothing else in `W/<chart>`.

Before any cell runs, `sweep` refuses (exit 2, nothing written) unless: the
roster hashes to `H` and every row is well formed (read_roster's checks);
every override file and name and every plan schema matches the roster; `plain`
holds no `values.schema.json` at any depth; `base` and `cand` equal `plain`
(content digest over every path, type, permission bits and file bytes)
plus exactly the planned root schema; no copy holds a symlink or special
file; no chart has outputs yet; and no schema has a `$ref`, `$dynamicRef` or
`$recursiveRef` that does not start with `#`, or a `$schema` jsonschema/v6
does not know (their compile could read files or the network).

`--helm-env-clear` (required for `sweep`, optional for `lint`/`template`)
re-executes the command as a child under exactly `HOME=<fresh empty dir>` and
`PATH=/usr/bin:/bin` (no `HELM_*`, `KUBECONFIG`, proxy or XDG variable) before
any package initializes: client-go reads `HOME` during initialization, so
clearing it later would let the original kubeconfig reach
`settings.Namespace()`. It then runs every roster row's three cells on `N`
workers (default 4), in roster order:

| Cell | Helm call | Log |
|---|---|---|
| `base` | `helm lint W/<chart>/base --kube-version K -f plan/<chart>/ov/<id>.json` | `W/<chart>/base.<id>.log` (stdout+stderr) |
| `cand` | the same on `W/<chart>/cand` | `W/<chart>/cand.<id>.log` |
| `plain` | `helm template t W/<chart>/plain --skip-schema-validation --kube-version K -f …` | `W/<chart>/plain.<id>.log` (stderr) |

Per chart it writes the canonical verdict artifacts — `W/<chart>/rows.tsv`
with exactly `sweep-one.sh`'s ten columns (roster key, then base, cand and
plain exit codes) and the cell logs, byte-deterministic for a deterministic
chart — and, separately, the operational `W/<chart>/cells.tsv` (`cell`, `rc`,
`class`, `cache=hit|miss`, `attributed`, milliseconds, `cacheable=yes|no`). The gate (`landing.py sweep-gate`) reads the
rows and logs as before and stays the classifier and refusal authority.
Exit 0: every cell ran; 1: a harness failure (a log or row file could not be
written); 2: refused. Progress, per-chart wall times and incidental Helm log
records go to stderr (redirect it to a step log).

Each cell uses fresh Helm settings (`cli.New`), values (`values.Options.MergeValues`),
chart load and action (`action.Lint`, or `action.Configuration` plus
`action.Install` with `DryRunClient`), following `pkg/cmd/lint.go`,
`pkg/cmd/template.go` and `pkg/cmd/install.go runInstall` at v4.2.3 with the
CLI's default flags, after the root command's configuration initialization
(`initRootConfig`) that the CLI runs before every command.

### Refused, not reproduced

Where the CLI would take a path helmsweep does not reproduce, helmsweep
refuses (exit 2, or a `serve` error) instead of silently differing: an empty
`--kube-version`, `HELM_DEBUG`, `HELM_DRIVER`, an existing registry
(`registry/config.json`) or repository (`repositories.yaml`) configuration
(the CLI initializes registry clients and checks repository expiry), installed
CLI plugins, stdin (`-`) or remote values, and a Helm build other than the
pinned release (`sweep`). The build links the release's version, Git commit
and tree state (`task build:helmsweep`), so templates see the CLI's
`.Capabilities.HelmVersion`; `helmsweep version` prints them as `helm-build`.
Building natively on Windows (stage.sh, shasum, git apply) is unverified.

### Log records

Helm's library code logs through the process-wide `slog` default, which the
CLI prints to stderr. `helmsweep` installs the CLI's handler once. A record
cannot be attributed to one of several concurrent cells, so a cell's log holds
its command output only, and records go to stderr. A record that could change
any class (it matches a classifier pattern or marker, `sensitiveRecord`) makes
every cell running at the time re-run exclusively, when its records are its
own and are written into its log exactly where the CLI prints them
(`cells.tsv` `attributed=false`). The single-cell `lint` and `template`
commands always run this way, so their output is the CLI's.

Helm lists sibling schema errors in Go map order, so a log's error order can
differ between runs of Helm itself; classes cannot.

### Verdict cache

A speed optimisation, never evidence: `C/v1/<k[:2]>/<k>/` holds `key.json`,
`result.json`, `log` and `MANIFEST` (sha256 of the other three). The key `k`
is the sha256 of the key record: executable sha256, Go, Helm, jsonschema and
patch identity, the environment policy, mode, kube version, chart-copy digest,
schema sha256 (or `none`) and override sha256. A hit requires the exact file
set, manifest and key, rc 0 or 1, class `pass` or `reject`, and the stored log
to reclassify to the stored class before and after its chart path is
rewritten to the current copy's. Anything else is a miss. Only `pass` and
`reject` are stored (temporary directory, then rename); `unresolved:*` never.

A chart whose verdicts identical inputs may not reproduce is never looked up
or stored (`cells.tsv` `cacheable=no`): `chartCacheability` parses every file
under a `templates` directory — the chart's and every dependency's, packaged
at any depth — with `text/template/parse` and refuses calls to clock, random,
key, certificate or salted-hash functions, `keys`, `values`, `tpl` and
`getHostByName` (the battery's list in
`crates/helm-schema/tests/common/helm_cache_policy.rs`) and templates that do
not parse; `lookup` stays cacheable (lint and client-only template answer it
empty).

## Resident template server (`serve`)

The battery's Helm adjudication (`crates/helm-schema/tests/common/helm_invocation.rs`,
engine `helmsweep`, the default; `SCHEMA_HELM_ENGINE=cli` spawns the CLI
instead) keeps `helmsweep serve` processes, one per concurrent render, each
started under exactly the environment and working directory a CLI child
would get; the server does not change them. One JSON request per stdin line,
one answer per stdout line, in order, matched by `id`; the server exits at EOF.

| Request | Answer |
|---|---|
| `{"id":N,"op":"template","release":R,"chart":C,"kube_version":K,"values":[F…],"skip_schema_validation":true,"stdout_path":P,"stderr_path":Q}` | `{"id":N,"exit_code":0\|1,"peak_bytes":…,"held_bytes":…,"max_rss_bytes":…}` after writing the CLI's stdout to `P` and stderr to `Q`; or `{"id":N,"error":…}` |

The render is `helm template R C --kube-version K [--skip-schema-validation]
-f F…` (`templateRun`: `pkg/cmd/template.go` and `install.go runInstall` with
default flags), stdout composed as the CLI prints it (manifest, then hooks).
A server renders one request at a time, so every log record written during a
render is that render's own and goes into its stderr where the CLI prints it:
stdout, stderr and exit code are the CLI's byte for byte. `error` is a
harness failure, never a verdict: a malformed or unsupported request,
unwritable outputs, or an abnormal end (a panic, where the CLI would exit 2).
Chart, values and outputs must be absolute local paths.

Memory: `peak_bytes` is the most memory the server held while this render
ran (sampled every millisecond) — the per-render figure the battery's pool
reservations use; `held_bytes` is what it holds idle afterwards, having
returned free memory to the OS above 64 MiB; `max_rss_bytes` is the server's
lifetime peak, telemetry only. The client retires a server holding more than
128 MiB after a render, kills and reaps any server that fails or does not
answer within its timeout (600 s), and never runs more servers than
concurrent renders, so idle servers stay small beside the pool's budget.

## CLI adjudication

Run the pinned Helm binary under the same environment:
`env -i HOME=$(mktemp -d) PATH=/usr/bin:/bin $HELM lint …`, and compare
`(rc, class)` per cell with the gate's classifier. Never use this cache for the
CLI side.

## Tests

```sh
task test:helmsweep
```

runs `gofmt -l`, then `go vet` and `go test -race` in the staged build copy
(the source tree alone does not compile: it needs the patched Helm).
