# 0058. Emitted schemas self-identify their resolved policy

Status: Accepted
Date: 2026-08-02 (plan v2.6; implemented 2026-08-03; vocabulary 2 on 2026-09-29)

## Context

Once profiles, knobs, overrides, and later authoring options could change a
schema, a reader of a shipped `values.schema.json` could no longer tell which
policy produced it.

## Decision

Final emitted documents carry an `x-helm-schema-policy` annotation inserted
at the end of the output pipeline (overwriting any caller-provided key):
annotation-format and policy-vocabulary versions, the requested profile, the
resolved knob values, active narrowing policies (e.g. `infer-required`),
modifiers (override count and digest, reference mode), and a policy
fingerprint. The in-memory `GeneratedSchema` stays unannotated.

## Consequences

- Policy vocabulary version 2 adds the caller authoring settings
  (`--open-root`, `--declared-types`, ADR 0060/0061) beside the emission
  policy, recorded under `x-helm-schema-policy.authoring`.
- Any new caller-visible policy dimension must extend this annotation and
  bump the vocabulary version.

## References

- `plan/schema-emission-profiles.md` "The resolved-policy annotation"
- `crates/helm-schema-gen/src/emission_policy.rs` (`POLICY_VOCABULARY_VERSION = 2`)
- `docs/content/docs/reference/cli.md` (`--open-root` row)
- Commits d67020ff (2026-08-03), 5834fbbd (2026-09-29)
