# 0061. The in-process Helm oracle builds from the pinned Helm module plus one patch; Helm is never vendored

Status: Accepted
Date: 2026-09-26 (helmsweep landed, 193560a1; patch-at-build decision, ledger 23:35)

## Context

The landing sweep ran the Helm CLI per cell, which took hours on heavy charts
(openebs rows 0–3: 81 min through the CLI vs 4.7–11 min in process). An
in-process driver reusing Helm's own code paths was built (`tools/helmsweep`);
its first form carried a vendored Helm tree, whose 68 zero-byte upstream
testdata files hit the runner's zero-byte rule (ledger 23:05 entry).

## Decision

- No vendored Helm sources and no submodules. `tools/helmsweep/go.mod`
  requires the tagged module `helm.sh/helm/v4 v4.2.3`, pinned by `go.sum`.
- `stage.sh` downloads the module, copies it into a build-local directory under
  the target dir, refuses unless the copy's content digest equals the pinned
  `ad77014f…`, applies the single `helm-memo.patch` with `git apply --check`
  (no fuzz), and builds there with a pinned Go and `-trimpath
  -buildvcs=false`.
- The only Helm change is a schema-compile memo in
  `ValidateAgainstSingleSchema`; helmsweep otherwise reproduces `helm lint`
  and `helm template --skip-schema-validation` through Helm's own actions and
  deviates from the CLI only in performance (memo, bounded worker pool,
  verdict cache).
- Its on-disk verdict cache is a speed optimisation, never evidence: keyed by
  executable, Go/Helm/jsonschema/patch identity, environment policy, mode, kube
  version, chart-copy digest, schema and override sha; only `pass`/`reject`
  are stored; charts whose templates call nondeterministic functions are
  never looked up or stored. Cacheability comes from Go template parse trees
  and an explicit per-row `cacheable` column; missing evidence means false.
  Environment policy is fixed before client-go initialization.

Every chart and every sweep row runs through the fast engine. The CLI
differential checks every 100th row per chart, up to three differing rows
per chart (`SWEEP_DIFF_CAP=3`), and every unresolved row. Any disagreement
fails the gate; sampling applies only to this extra CLI check.

## Consequences

- Parity with the CLI is a measured, re-runnable property (0 disagreements on
  639 cells at landing); full parity is re-run when Helm or the driver changes.
- Driver-contract tests guard configuration; row coverage equals charts ×
  overrides. Upgrading Helm re-pins the module, digest and patch.
- Nondeterministic charts must re-render, limiting achievable cache speedups;
  a driver environment-policy change invalidates the cache namespace.

## References

- `plan/campaign-orchestration-playbook.md` "Standing decisions (from the user)" (7b85b2d8, 2026-09-27)
- `plan/schema-bug-hunt-v1-progress.md` ~4158–4172 (cacheability census),
  ~4529–4570 (driver), ~4766–4772 (replay/environment defects),
  04:50 Sep 27 (~4866–4885, CLI cap and explicit cacheability)
- `tools/helmsweep/README.md`, `tools/helmsweep/PATCH.md`;
  `plan/schema-bug-hunt-protocol-v2.md` §6 (shipped bytes/schema-free control)
- 193560a1 `tools/helmsweep: in-process Helm sweep driver (Helm v4.2.3 patched at build time)` (2026-09-26); 90c2b544 helmsweep v2 (2026-09-27)
