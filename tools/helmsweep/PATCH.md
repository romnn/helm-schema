# Helm v4.2.3 and its one patch

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
`7dabf3ef…6bdce6`, patched `3afeb41d…748da0e`); the patch itself has sha256
`3c2d5c38…039155f`, which `helmsweep version` prints and the build checks.
`TestVendoredHelmIsUpstreamPlusOnePatch` pins the staged tree: the 1,983
unpatched files and the patched file.

## The change

`ValidateAgainstSingleSchema` keeps upstream's panic recovery, its
`jsonschema.UnmarshalJSON` of the schema bytes, its logging, `Validate` and
the `JSONSchemaValidationError` wrapping on every call. Only the compile moves
into `compileMemoized`:

- The memo is keyed by the SHA-256 of the schema bytes. Concurrent callers of
  the same bytes wait for one compile (single flight).
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

Nothing else changes: no `slog` default, `log` output, `URNResolver`, archive
limit or environment setting is touched by the patch.
