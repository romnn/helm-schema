# 0050. Output is self-contained by default; external `$ref`s move into `$defs`

Status: Accepted
Date: 2026-06-11 (`plan/from-scratch-architecture.md`, ab03ed3e; landed in cc6d75de, 2026-06-15)

## Context

Resource and CRD schemas reach the generator with external file/URL `$ref`s.
The earlier pipeline flattened by default, which "inflates output on the hot
path and then re-deduplicates it in the minifier".

## Decision

The output contract defaults to a bundled, self-contained Draft-07 document
with internal `$defs`: external `$ref`s discovered during analysis are
resolved and re-homed into root-level `$defs`, so the schema stands alone
while still sharing referenced subschemas. Full flattening is an explicit
export mode. Repeated subtrees are also interned into root `$defs` by
default (`--no-minimize` opts out): a lossless output-only transform that
never feeds back into inference.

| Flag | Effect |
|---|---|
| *(default)* | Resolve external refs into root-level `$defs`. |
| `--keep-refs` | Leave external `$ref` strings exactly as-is. |
| `--inline-refs` | Fully inline resolved refs instead of writing `$defs`. |

## Consequences

- The from-scratch design's reason: "bundled output is the honest product
  contract"; internal `$defs` with deterministic names keep output small,
  self-contained, offline-validatable and diff-friendly, while inline-by-default
  would re-expand the sharing the lazy `SchemaDoc` tier preserves.
- `--keep-refs` and `--inline-refs` are mutually exclusive.
- Repeated-subtree interning reduced kube-prometheus-stack from 19.7 to
  12.6 MB pretty and cut test time several-fold (53 fixtures re-pinned).
  Lossless sharing cannot guarantee Helm-size compliance ([0047](0047-size-aware-output-layout.md)).
- Interned definitions follow readable naming ([0045](0045-defs-names-are-readable-source-paths.md)).

## References

- `plan/from-scratch-architecture.md` commitment 5 (lines ~59-61) and the
  "Because bundled output is the honest product contract" rationale
  (lines ~1122-1126)
- Commit cc6d75de (2026-06-15) "feat(cli): bundle external schema refs by default"
- `docs/content/docs/reference/output.md` "`$ref` handling", "Minimization"
- 0941a97b (2026-07-15, default repeated-subtree interning);
  `plan/schema-emission-profiles.md` "The public knob matrix" (output options)
