# 0057. Chart-local policy lives in a strict, versioned `helm-schema.yaml`

Status: Accepted
Date: 2026-08-02 (plan v2.6; implemented 2026-08-03)

## Context

Emission policy needed a per-chart home so a packaged chart could carry its
choice (the temporal chart's lean variant) without CLI flags, while a chart
on disk must not be able to change the caller's I/O or enroll it in
narrowing.

## Decision

- Policy config is `helm-schema.yaml` in the **root chart directory only**,
  discovered through the root chart source, or given with `--config`.
  Dependency configs are ignored; `--no-config` ignores discovered policy.
- The `version` field pins the complete interpretation (syntax, policy
  vocabulary, preset membership). A binary honors exactly the versions it
  ships support for and fails with a diagnostic on any other; it never
  silently reinterprets a packaged chart's policy.
- Discovered config controls presets and W-class emission knobs only. X
  policies, override paths, and reference transport stay CLI/library-only.
- Precedence: explicit CLI > config file > profile preset > built-in default;
  an explicit CLI `--profile` resets file-level knob deltas.
- Unknown keys, invalid combinations, vocabulary mismatches, and malformed
  discovered config are hard errors.

## Consequences

- Evolving `lean` or a knob's meaning requires bumping the config version,
  the stated price of drift-proof packaged configs.
- One aggregated diagnostic reports when discovered config weakens relative
  to `full`; `--print-effective-config` shows each field's source and runs no
  analysis, provider construction, or network.

## References

- `plan/schema-emission-profiles.md` "Configuration surface and trust"
- `docs/content/docs/reference/configuration.md`; `cli.md` (`--config`,
  `--no-config`, `--print-effective-config`)
- Commit 20e31e83 (2026-08-03, feat(schema): add emission policy configuration)
