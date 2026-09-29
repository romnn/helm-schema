# helmsweep: build, identity pins, and moving Helm

`tools/helmsweep/README.md` (commands, sweep contract, `serve` protocol, verdict cache) and
`tools/helmsweep/PATCH.md` (what the one patch changes) are authoritative for behavior. This file
covers building, verifying and re-pinning.

## Why it exists and why it is pinned so hard

Schema acceptance is adjudicated against real Helm. helmsweep runs Helm's own code in-process
(lint, `template --skip-schema-validation`, and a resident `serve` mode for the Rust battery) and
deviates from the CLI only in performance: a schema-compile memo (the patch), a bounded worker
pool, and a fail-closed verdict cache. Every pin exists so a helmsweep verdict equals the pinned
Helm CLI's verdict byte for byte:

- **Module + digest**: `go.sum` verifies the tagged module zip; `stage.sh` additionally checks a
  content digest of the extracted tree (sha256 of `shasum -a 256` over every file, C-sorted paths).
- **Patch**: applied with `git apply --check` then `git apply` (no fuzz, no rejects).
  `TestVendoredHelmIsUpstreamPlusOnePatch` pins the staged tree: N unpatched files hashing to one
  value, plus the patched file's sha256 and the patch's sha256.
- **Build identity** (`main.go` `buildIdentity`): reads Go build info and requires
  `helm.sh/helm/v4 <helmVersion> => <helmReplacement>` and `jsonschema/v6 <jsonschemaVer>` with no
  replace; `helm-build` comes from ldflags (`internal/version.version/gitCommit/gitTreeState`) so
  templates see the release's `.Capabilities.HelmVersion`. Without the ldflags Helm reports a bare
  `v4.x` and `sweep` refuses ("Helm build … is not the pinned release").
- **Requirement set**: Helm computes default `.Capabilities.KubeVersion` from the linked
  `k8s.io/client-go` module version (`internal/version/clientgo.go`) and default
  `.Capabilities.APIVersions` from client-go's scheme (`pkg/chart/common/capabilities.go`). Any
  module version that differs from Helm's release go.mod therefore changes rendering. The release
  binary's `helm version` shows `KubeClientVersion`; helmsweep's linked client-go must match it.
- **Go toolchain**: the release binary's Go version (`helm version` → `GoVersion`), enforced with
  `GOTOOLCHAIN=local` and `mise exec go@<HELMSWEEP_GO>`. `-trimpath -buildvcs=false` make the
  binary's sha256 (the verdict cache's build id) depend only on sources and toolchain.

## Everyday commands

```sh
task build:helmsweep   # stage + build + identity check → <target>/helmsweep
task test:helmsweep    # gofmt -l (source tree), stage, go vet, go test -race (staged copy)
<target>/helmsweep version
```

`<target>` is `$CARGO_TARGET_DIR` or `./target`. The Rust tests look for `helmsweep` in the target
directory their own binary was built into, so build it with the same `CARGO_TARGET_DIR`.

## Checking the requirement set (read-only)

helmsweep's go.mod may list a subset of Helm's requirements (tidy prunes unused modules) but never
a different version and never a module Helm does not require:

```sh
cd tools/helmsweep
V=$(awk '$1=="require" && $2=="helm.sh/helm/v4"{print $3}' go.mod)
HELM_MOD=$(GOTOOLCHAIN=local mise exec go@<HELMSWEEP_GO> -- go mod download -json "helm.sh/helm/v4@$V" \
  | sed -n 's/.*"GoMod": "\(.*\)".*/\1/p')
export LC_ALL=C D=$(mktemp -d)   # or your scratch directory
awk '/^\t[a-z]/{print $1, $2}' "$HELM_MOD" | sort > "$D/helm.req"
awk '/^\t[a-z]/{print $1, $2}' go.mod       | sort > "$D/hs.req"
join "$D/hs.req" "$D/helm.req" | awk '$2!=$3'                          # version drift: must be empty
comm -23 <(cut -d' ' -f1 "$D/hs.req") <(cut -d' ' -f1 "$D/helm.req")   # extra modules: must be empty
```

## Moving to a new Helm release (all steps in one change)

Nothing here is automatic; cooldown deliberately does not touch helmsweep. Find every old pin
first: `git grep -nE '<old version>|<old commit>|<old go version>|<old digest prefix>' -- ':!testdata' ':!plan'`.

1. **Read the release facts** from the official release binary: `helm version` → Version,
   GitCommit, GitTreeState, GoVersion, KubeClientVersion.
2. **Helm CLI pin**: `mise.toml` `helm = "<ver>"` (refresh `mise.lock` with mise). The CLI engine
   (`SCHEMA_HELM_ENGINE=cli`) and helm-version assertions use it.
3. **Go toolchain**: taskfile `HELMSWEEP_GO`; `tools/helmsweep/go.mod` `toolchain`; both
   `go@…` installs in `.github/workflows/test.yaml`; helmsweep README.
4. **go.mod**: set `require helm.sh/helm/v4 v<ver>`, delete the old indirect block, then
   `go get helm.sh/helm/v4@v<ver>` and `go mod tidy` (with `GOTOOLCHAIN=local` and the pinned Go).
   Never `go get -u`. Run the requirement-set check above; both outputs must be empty.
5. **stage.sh**: version in the download spec, the `third_party/helm-v<ver>` path and messages, and
   `pinned` — computed with stage.sh's own pipeline over the go.sum-verified module directory:
   `cd "$(go list -m -f '{{.Dir}}' helm.sh/helm/v4)" && find . -type f | LC_ALL=C sort | tr '\n' '\0' | xargs -0 shasum -a 256 | shasum -a 256`.
   Only record a digest from a download that go.sum verified.
6. **helm-memo.patch**: re-port the change in PATCH.md onto the new
   `pkg/chart/common/util/jsonschema.go` (paths `a/pkg/…` relative to the module root). It must
   apply with `git apply --check`. If upstream changed that file, re-check that the memo still
   preserves upstream semantics (panic recovery, loader, `AddResource`, error wrapping).
7. **PATCH.md**: go.sum `h1:` hash, module digest, upstream/patched file sha256, patch sha256.
8. **Go tests**: `memo_test.go` `TestVendoredHelmIsUpstreamPlusOnePatch` (root path, unpatched
   file count and hash, patched sha256, patch sha256) and `TestBuildIdentityPinsHelmJSONSchemaAndPatch`;
   fixture strings in `cache_test.go`.
9. **main.go**: `helmVersion`, `helmReplacement`, `jsonschemaVer` (if Helm's jsonschema/v6
   requirement moved), `helmReleaseBuild` (`"<Version> <GitCommit> <GitTreeState> <GoVersion>"`).
10. **taskfile**: `HELMSWEEP_HELM_COMMIT`, `HELMSWEEP_LDFLAGS` (version + commit; confirm the
    `internal/version` symbol names still exist in the new release), the task descriptions, and the
    hard-coded lines in `build:helmsweep`'s identity loop (Helm replace line, `helm-build`,
    jsonschema version).
11. **Rust test pins**: `crates/helm-schema-test-support/src/helm/invocation.rs`
    (`PINNED_HELM_VERSION`, `HELMSWEEP_HELM_LINES`), fake `version` outputs in
    `crates/helm-schema/tests/helm_invocation.rs`, the version assertions in
    `schema_emission_profiles.rs` and `schema_emission_profile_live.rs`.
12. **Behavioral comments**: many Rust comments say "Helm v<old> does X" (coalescing, kubeVersion
    constraints, metadata). Each is a claim about Helm source; re-verify against the new release
    before rewording, and fix code where behavior changed.
13. **cooldown.toml** comment naming the embedded version.
14. **Verify**: `task build:helmsweep` (identity check), `task test:helmsweep`; in
    `<target>/helmsweep-build`, `go list -m k8s.io/client-go` must match the release's
    `KubeClientVersion` minor. Then the full gate ladder. Helm-invocation and verdict caches key on
    the engine identity, so expect cold caches, not stale hits. Any verdict change is a corpus flip
    to adjudicate per `acceptance-battery`; fixture regeneration per `corpus-fixtures`.

## Failure symptoms

| Message | Meaning |
|---|---|
| `helmsweep needs go<ver>` | pinned Go missing: `mise install go@<ver>` |
| `helm v… module copy has digest X, not the pinned Y` | version moved without re-pinning, or the module dir is not the verified download. Never paste X in without step 5's provenance |
| `git apply --check` error | patch does not fit this release: re-port it (step 6) |
| `helmsweep version lacks: <line>` | a taskfile identity line, `main.go` constant, ldflag or go.mod disagrees |
| `binary is not built from helm.sh/helm/v4 … => ./third_party/…` | built outside `stage.sh` (no replace) or jsonschema/v6 version/replace differs |
| `Helm build "…" is not the pinned release` (sweep) | built without `HELMSWEEP_LDFLAGS`; use `task build:helmsweep` |
| `undefined: util.DropCompiledSchema` | compiling the source tree directly; only the staged copy compiles |
| Rust: `no helmsweep at …` / unpinned engine program | not built into this target dir, or Rust pins (step 11) disagree with the binary |
