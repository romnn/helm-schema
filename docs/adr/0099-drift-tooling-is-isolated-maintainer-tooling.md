# 0099. Helm/Kubernetes drift tooling is isolated maintainer tooling under tools/

Status: Accepted
Date: 2026-10-19 (user request and constraints)

## Context

Builders repeatedly re-derived Helm and Kubernetes behaviour in per-track evidence
matrices (quoted-grammar cells and go-yaml productions, values-transport cells,
`hasKey`-null cells and YAML 1.1 decoding, mergo nil, range-over-int, `.helmignore`)
that were lost after landing, and `capability_probe.rs` is hand-maintained debt (CLAUDE.md
"Known architectural debt: capability probe table"). The user asked to keep that
knowledge as permanent tooling, re-runnable every few months or on a version bump.

## Decision

Helm/Kubernetes semantics knowledge lives as drift tooling under `tools/` that is:

- as simple as possible: promote existing tests and harnesses plus a plain directory of
  cases; no over-engineering;
- reproducible: pinned versions, with setup-heavy checks (multiple pinned Helm versions,
  pinned Go toolchains, a Kubernetes apiserver/crdcheck per version, network refresh of
  upstream OpenAPI and CRD bundles) behind one maintainer-only entry point (one container
  image or one script), run infrequently;
- extendable through data: a new Helm or Kubernetes version or a new case is added as
  data/config, not by editing core code;
- fully separate from the core crates people compile, install and contribute to: its own
  directory and build/module manifests, not a workspace member of the core crates, no new
  dependencies or features in core crates, not compiled by `cargo build`/`cargo install`
  of the CLI, not required by the default gates.

## Consequences

- The only coupling into core is plain recorded-case data that the everyday tests replay
  offline, and generated tables (e.g. a capability table) committed as data with a header
  naming the tool and version that produced them.
- The setup-heavy complexity barrier is accepted for maintainers and must not leak into
  the everyday gate, the default build, or contributors' required tooling.

## References

- `/Volumes/T7/dev/round8/q/drift-toolkit-user-constraints.md` (outside the repository)
- Memory `drift-tooling-isolation.md`
- `plan/schema-bug-hunt-v1-progress.md` last entry, 23:45 (Oct 19) "USER REQUEST: keep the
  Helm/Kubernetes semantics knowledge as permanent drift tooling in tools/" (commit 8910ea1e)
