# 0056. The lean profile keeps mandatory facts and local conditionals

Status: Accepted
Date: 2026-08-02 (plan v2.4; adopted in code 2026-08-03)

## Context

`lean` exists to make `helm lint` cheaper on large schemas. The measured
options for the temporal wrapper chart were a sub-second variant that lost
real typing ("tooth loss", the motivating regression) and a middle point
(17.3 s vs 110 s for the approximation) that kept the `"three"` control
verdict. Size was not binding (3.84 vs 4.19 MB, both under Helm's 5 MiB
limit) but had limited headroom.

## Decision

`lean` is the measured middle point, defined as a retention contract:

```text
lean: root-anchored-conditionals off, kind-partitions off,
      terminal-clauses off, local-conditionals ON.
```

It keeps every Mandatory fact plus every locally anchored conditional
refinement. The faster point stays one config line away
(`local-conditionals: off`).

## Consequences

- Wrapper charts keep most typing value through local conditionals.
- The veto window closed with the Step 3 fixture commit; changing the preset
  now needs an explicit follow-up and, because the config version pins
  preset membership (ADR 0057), a config version bump.
- The downstream temporal chart moved from `--profile lean` to a chart-local
  `helm-schema.yaml` (`profile: lean`, `emission.local-conditionals: off`),
  pinned by a precedence integration test.

## References

- `plan/schema-emission-profiles.md` "Profiles are presets — and the lean decision"
- `plan/schema-emission-profiles-progress.md` "Lean contract", "Temporal migration"
- Commits d2e0717f (plan v2.4, 2026-08-02), d67020ff (2026-08-03)
