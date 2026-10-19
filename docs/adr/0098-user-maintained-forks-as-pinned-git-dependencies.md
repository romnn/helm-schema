# 0098. Fix upstream defects in user-maintained forks pinned as git dependencies

Status: Accepted
Date: 2026-10-18 (user: fork santhosh-tekuri/jsonschema, ledger 17:44)

## Context

Two third-party libraries limited the verification tooling. The Go JSON Schema
validator used by `tools/helmsweep` (santhosh-tekuri/jsonschema v6.0.2) had a queue
slowdown (`queue []*Schema` scanned with `slices.IndexFunc`). The Rust
`tracing-perfetto` 0.1.5 layer produced thousands of misplaced span ends, which made
Perfetto self times untrustworthy (ADR 0097).

## Decision

Upstream defects that block the project are fixed in forks under the user's account
and consumed as pinned git dependencies, with upstreaming decided later ("we can later
decide if we want to upstream it"):

- `romnn/jsonschema` (branch `perf/indexed-queue` on tag v6.0.2: an ordered slice plus a
  `map[urlPtr]*Schema`, traversal order unchanged) for helmsweep's Go module;
- `romnn/tracing-perfetto` (git only, 0.2.0, rev d4c54ab: slice begins streamed as they
  happen, a monotonic per-sequence clock, one track per root span) for the tracing layer.

Issues and fixes for these forks go on `fix/` branches in the fork repositories; nothing
is pushed to helm-schema for them.

## Consequences

- Switching to a fork requires a parity proof: the jsonschema fork passed `go vet`,
  `go test ./...` with the JSON-Schema-Test-Suite populated, and helmsweep parity; the
  tracing fork showed 0 misplaced ends and byte-identical fixtures with tracing off.
- The project now carries two forks to keep in sync with upstream.
- As of 2026-10-19 the switch is not yet on `main`; landing 10a carries it
  (`plan/schema-bug-hunt-v1-round8-handoff.md` §6 "Landing 10a").

## References

- `plan/schema-bug-hunt-v1-progress.md` ~line 8321, entry 17:44 (Oct 18) (commit
  c9e6116e, 2026-10-18); ~8546 (00:29, Oct 19, tracing fork); ~8557 (01:20, Oct 19,
  clean traces)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §6 "Landing 10a" and "Standing decisions
  made this session" ("Forks: romnn/jsonschema (helmsweep) and romnn/tracing-perfetto")
