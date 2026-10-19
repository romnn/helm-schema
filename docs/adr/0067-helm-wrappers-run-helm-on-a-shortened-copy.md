# 0067. `helm-schema lint` / `template` run Helm on a shortened chart copy and translate diagnostics

Status: Accepted
Date: 2026-09-28

## Context

With readable names in the shipped schema and shortening kept separate
(ADR 0065, 0066), a user validating with Helm faced either an oversized
file or Helm errors that cite opaque short keys. Landing 3 first offered a
text-translation subcommand, `helm-schema expand-defs`, for Helm error text.

## Decision

`helm-schema lint|template [--helm PATH] <chart> [helm args...]` copies the
whole chart tree under TMPDIR, writes a freshly shortened root
`values.schema.json`, runs the pinned Helm, and translates every
`#/$defs/<token>` in Helm's diagnostics back to the readable name byte for
byte. `template` stdout passes through unchanged; signals are forwarded to
the Helm child. `expand-defs` is deleted.

## Consequences

- The chart on disk keeps the readable schema; only the throwaway copy Helm
  reads is shortened.
- One translation path replaces the separate `expand-defs` command.
- The Helm executable is `--helm`, else `$HELM`, else `helm` on `PATH`.

## References

- Commits a06dc075 (expand-defs, 2026-09-27), dca51bd4 and d42a1f33
  (Land helm-schema lint and template wrappers; expand-defs removed,
  2026-09-28)
- `docs/content/docs/reference/cli.md` (`--helm`)
