# Helm v4.2.3, its memo patch and the validator fork

The source tree holds no Helm sources. `go.mod` requires the tagged module
`helm.sh/helm/v4 v4.2.3`, pinned by `go.sum`
(`h1:JEejtPE04+SvyRomOfgRXVxyJ/lude7eShio30oQr0Y=`). `stage.sh` (run by
`task build:helmsweep` and `task test:helmsweep`) builds from a build-local
copy under the target directory, never the source tree:

1. `go mod download helm.sh/helm/v4@v4.2.3` (`GOFLAGS=-mod=mod`; Go verifies the
   zip against `go.sum`);
2. copies the module from `GOMODCACHE` to `<build>/third_party/helm-v4.2.3`,
   writable, next to a copy of this module's sources;
3. refuses unless the copy's content digest (sha256 of `shasum -a 256` over
   every file, paths in C sort order) is the pinned
   `ad77014f…9867305f`;
4. applies `helm-memo.patch` with `git apply --check`, then `git apply`
   (no fuzz; any reject refuses);
5. appends `replace helm.sh/helm/v4 => ./third_party/helm-v4.2.3` to the build
   copy's `go.mod`, where `go build -trimpath -buildvcs=false` and the tests run.

The patch changes only `pkg/chart/common/util/jsonschema.go` (upstream sha256
`7dabf3ef…6bdce6`, patched `80104edd…32cb9f07`); the patch itself has sha256
`1f6767be…43bae4`, which `helmsweep version` prints and the build checks.
`TestVendoredHelmIsUpstreamPlusOnePatch` pins the staged tree: the 1,983
unpatched files and the patched file.

## The change

`ValidateAgainstSingleSchema` keeps upstream's panic recovery, its
`jsonschema.UnmarshalJSON` of the schema bytes, its logging, `Validate` and
the `JSONSchemaValidationError` wrapping on every call. Only the compile moves
into `compileMemoized`:

- The memo is keyed by SHA-256 of exact schema bytes. Sweep retains warmed
  schemas until their chart completes, so overlapping charts do not evict each
  other before validation. Resident `serve` sets a two-entry limit before
  accepting commands. A hit promotes its entry. Concurrent callers
  share a pending compile while its entry remains retained; eviction drops only
  the cache reference, so active validators and waiters remain valid. An evicted
  compile is not reinserted. Capacity bounds entries, not schema size or total
  process memory.
- On a miss, `compileSchema` runs upstream's loader (`file`, `http`, `https`,
  `urn`), `AddResource("file:///values.schema.json", …)` and `Compile`
  sequence unchanged. The loader is wrapped only to record whether it ran.
- Only a successful compile that loaded no external resource is kept: a
  failed compile, or one whose result depends on a file, URL or URN, compiles
  again on every call exactly as upstream does.
- The compiled `*jsonschema.Schema` is shared read-only. jsonschema/v6's
  `Schema.Validate` allocates a fresh validator and error tree per call and
  only reads the compiled schema; `TestConcurrentValidationMatchesFreshCompiles`
  checks this under `go test -race`.
- `DropCompiledSchema` forgets one schema (the sweep drops a chart's schemas
  once its rows are done); `CompiledSchemaMemoStats` reports compiles and hits.

`ResetUnresolvedURNWarnings` clears upstream's process-wide warning history
between resident commands, under the driver's exclusive lock. Each command
therefore emits the warnings a fresh CLI process would emit. Validations
within one command still share upstream's once-per-URN warning behavior.

No `slog` default, `log` output, `URNResolver`, archive limit or environment
setting is touched by the patch.

## Validator queue index

`go.mod` replaces `github.com/santhosh-tekuri/jsonschema/v6 v6.0.2` with
`github.com/romnn/jsonschema/v6 v6.0.3-0.20261018154229-cb4d114ec4c7`
([commit cb4d114ec4c7ffa5a1bbc3ee02cbd0346c477b09](https://github.com/romnn/jsonschema/commit/cb4d114ec4c7ffa5a1bbc3ee02cbd0346c477b09),
branch `perf/indexed-queue`).
This is one commit on upstream v6.0.2: an index keyed by the existing `urlPtr`
sits beside the ordered compile queue, replacing its linear lookup while
preserving traversal order and schema reuse identity.
Helm's required dependency versions and validator options stay pinned.
The fork retains the upstream module declaration; Go accepts a versioned
replacement at the fork's repository path without changing Helm's imports.

The OnCall profile attributed 27.40% of compile/validation CPU samples to
the queue's `slices.IndexFunc` scan, alongside a separate definition-discovery
cost that this override does not address.
The index avoids quadratic lookup work on broad guarded schema graphs.
`helmsweep version` and verdict-cache keys include both the required upstream
version and the effective replacement path/version, and the build refuses an
absent or different replacement.
Go downloads the immutable pseudo-version and verifies `go.sum`; landing
clones do not need a fork checkout.

To drop the override, remove the jsonschema `replace` directive, run
`go mod tidy`, and update `main.go`, `TestBuildIdentityPinsHelmJSONSchemaAndPatch`,
the `build:helmsweep` version check in `taskfile.yaml`, and the README's
identity to the chosen upstream validator.
Re-run the stock-versus-replacement corpus compile/verdict comparison and
`task build:helmsweep` / `task test:helmsweep` before landing.

The parity comparison must include the documents Helm actually validates:
the values lint rule's defaults plus overrides, the template lint rule's
coalesced values, and the template/install action's coalesced values.
Capture `values.AsMap()` at `ValidateAgainstSingleSchema`, before compilation
so failed schemas remain visible, and replay the captured JSON under both
validators with the same schema bytes.
Synthetic merges cannot replace this coverage: Helm deletes null overrides
and coalesces dependency defaults and globals differently at these boundaries.
Report accepted and rejected counts per chart and boundary, identical compile
errors separately, and commands that exit before reaching a boundary.
