# 0066. `$defs` shortening only on explicit request; never change output to fit Helm

Status: Superseded by 0068
Date: 2026-09-27

## Context

Helm refuses a chart file larger than 5 MiB (5,242,880 bytes); F74 found 18
of 156 corpus schemas over that limit. Readable source-path `$defs` names
(ADR 0065) make output larger. The user ruled that there is no size-triggered
fallback: shortening is a separate single-pass transform applied only right
before handing a schema to Helm, with a mapping that translates Helm error
messages back to readable names.

## Decision

- `helm-schema shorten`, or `--shorten-defs` with `--defs-map`, writes a
  Helm-ready schema with every `$defs` entry renamed to a short base-62 key
  (most referenced definition shortest) plus the key-to-name map.
- Shortening is never applied automatically. helm-schema never changes the
  format or names of its output to fit: when the written schema exceeds the
  limit it warns (`SchemaExceedsHelmFileLimit`) and names the fix.

## Consequences

- Callers had to opt into compact/short output to ship large charts.
- Superseded for the CLI's default output on 2026-10-19 by the size-aware
  `--output-layout auto` (ADR 0068), which switches to compact JSON with
  short handles only when pretty output would exceed Helm's limit. The
  fixtures rule (always readable and pretty) is unchanged.

## References

- Session memory `fixture-defs-naming.md` ("No size fallback", 2026-09-27)
- `plan/schema-bug-hunt-v1.md` F74
- `docs/content/docs/reference/output.md` "Size limit"; `cli.md`
  (`--shorten-defs`: "Never applied automatically")
- Commits c7cd9fc5 (shorten only on request), 0c21000f, 1ec44c73 (2026-09-27)
