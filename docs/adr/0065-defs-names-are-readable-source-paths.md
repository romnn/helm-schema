# 0065. `$defs` entries are named by readable source paths

Status: Accepted
Date: 2026-09-27

## Context

Fixture diffs are the only cheap review signal for a landing. Ordinal names
churned on every dump and the content-hash keys of ADR 0064 were
unreadable. An unreadable verification artifact is a defect to fix before
building more adjudication around it.

## Decision

- Every `$defs` entry helm-schema creates is named by a deterministic,
  human-interpretable schema path. By default (`--defs-names source`) the
  name is the definition's SOURCE: provider document + JSON pointer for
  Kubernetes/CRD content (`k8s/...`, `crd/...`), `values/<path>` for
  values-derived content, the helper or meaning for other fragments
  (`when/<if paths>/then/<then paths>`, `constrains/<paths>`, typed leaf
  names). When deduplication merges several sources, the smallest path wins.
- `@N` suffixes appear only on exact name collisions, ordered by content so
  an unrelated fragment never renames them.
- Destination-path naming is an option (`--defs-names destination`).
- Fixtures are always readable and pretty, at any size; there is no
  size-triggered fallback for fixtures. Inlining all references is not an
  alternative (about 60k refs).

## Consequences

- Source paths are stable (unlike first-use destination paths, which churn
  when an earlier use appears), so fixture diffs show real changes; 160
  fixtures were renamed with a naming-only proof.
- `$ref` building and parsing go through `referencing` and `fluent-uri`
  (names contain `/`).
- Short keys exist only for the Helm-shipped file (ADR 0066, 0068).
- Size or churn arguments against paths must separate source from
  destination naming.

## References

- Session memory `fixture-defs-naming.md` (agreed policy and scheme,
  2026-09-27); `fix-uninterpretable-output-first.md`
- `docs/content/docs/reference/output.md` (`$defs` naming)
- Commits 0c21000f (Land readable $defs names, 2026-09-27), 1ec44c73,
  8678808d, bbef70d5
