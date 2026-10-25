# Native custom-resource validation

This isolated maintainer tool uses Kubernetes v0.29.0 for the battery's fresh
Kubernetes 1.29.0 reference environment. Controllers and readiness are outside
that contract. Unsupported apply metadata and any supplied status abstain, both on selected
CRD declarations and on resource instances.
Helm uses SSA for its CRD Create calls too; both paths share one request boundary.

Build with `mise exec go@1.26.5 -- tools/crdcheck/provision.sh`.
Go 1.26.5 is exact; `go.mod` and `go.sum` pin dependency versions and hashes.
On first use the script downloads modules into the maintainer's Go module cache,
with `GOFLAGS=-mod=readonly` and `GONOSUMDB` unset, then verifies that cache.
This maintainer-only download may use the network; ordinary gates never provision Go.
The build runs offline, disables toolchain downloads and writes the binary and build
cache under `target/`. `GOMODCACHE` may select a private module cache.

The binary is `target/crdcheck`; `HELM_SCHEMA_CRDCHECK` can select another binary.
The Rust caller hashes its bytes into every original-input request identity.
Only explicit maintainer live adjudication may invoke it; ordinary gates replay
complete recorded requests/responses and neither build nor require this tool.
Missing, failed or mismatched tooling is Uncertain.
Live results default to `.cache/crdcheck`; `HELM_SCHEMA_CRDCHECK_RECORDS` selects a
recording directory. Binary changes recompute under a new complete-input key;
older records remain available without supplying authority to the new binary.
Copy deliberately reviewed complete receipts to `testdata/crdcheck`
for ordinary replay; normal live runs never mutate committed replay cases.

Inputs are three JSON files: complete v1 CRD, original resource, request context.
Protocol `helm-schema/crdcheck/v3` requires the shared declaration/instance apply
boundary and native SSA typed-patch validation of custom resources before
pruning/defaulting.
The pinned OpenAPI builder and managed-fields converter supply the same structured
merge validation Kubernetes applies, including set/map lists and atomic maps.
Older binaries and receipts abstain in the Rust caller.
The response echoes the length-framed SHA-256 of those original bytes.
Effective defaulted JSON and its digest are response evidence only.
Native rejection diagnostics are not battery allowance identities; the Rust
oracle abstains unless it can prove an exact existing assertion correspondence.

The pinned catalog keeps its pre-existing validation behavior (ADR 0101).
Catalog schemas take precedence, including when chart declarations differ;
missing sidecars, install hooks, rendered CRDs/admission, and native SSA limits
add no veto to catalog decisions. The catalog path does not prove that chart
installation, admission, conversion, or SSA metadata/status agree with its schema.
That limitation is an inherited residual. Install-scope and native request checks
apply only when chart-sourced CRDs supply authority absent from the catalog.

Chart validation adds an acceptance only when native proof succeeds. Otherwise
the legacy verdict and exact uncertainty text remain, preserving existing battery
roster matches as well as decided catalog outcomes.
