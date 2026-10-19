# 0069. Repeated subtrees are interned into `$defs` by default, as an output-only transform

Status: Accepted
Date: 2026-07-15

## Context

Branch-scoped guard arms repeat large fragments. kube-prometheus-stack
measured 19.7 MB pretty, far above Helm's 5 MiB chart-file limit, and large
schemas slow every downstream validator.

## Decision

Minimization is on by default: repeated subtrees are interned into
root-level `$defs` and referenced with `$ref`; `--no-minimize` opts out. It
is an output-only, lossless transform that never affects inference.

## Consequences

- kube-prometheus-stack dropped 19.7 → 12.6 MB pretty and test wall time
  dropped several-fold; 53 chart fixtures were re-pinned with the corpus
  harness mirroring the default.
- Being lossless, minimization cannot guarantee that a schema fits Helm's
  limit; size is handled by ADR 0068.
- Interned definitions are named per ADR 0065.

## References

- Commit 0941a97b (2026-07-15, feat(cli): intern repeated schema subtrees
  into $defs by default)
- `docs/content/docs/reference/output.md` "Minimization"
- `plan/schema-emission-profiles.md` "The public knob matrix" (output-side
  options stay separate from emission policy)
