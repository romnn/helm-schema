# 0043. Declared defaults assert their types by default; `--declared-types=annotate` opts out

Status: Accepted
Date: 2026-09-26 (user decision; register default 2026-09-05)

## Context

Where no template or resource evidence governs a path, helm-schema types a
value from the chart's declared default. F80 showed this collides with the
acceptance flip law: redis-ha renders `haproxy.checkFall` into config text
inside a ConfigMap, Helm renders any value there, yet the defaults `1` and
`1s` typed the leaves `integer` and `string`, and 18 battery probes flipped
to rejected. F43 is the same collision from a `default <literal>` site. The
finding asked for a written decision: exempt declared-shape typing from the
flip law, or withdraw it at opaque-text sinks.

## Decision

- `--declared-types=assert` remains the default. A declared scalar or list
  type applies only at that exact input path, under the execution guard of
  its own positively recorded opaque consumption. It never supplies object
  typing or inherited descendant constraints. This is an authoring assertion,
  not a recovered constraint.
- Known consumption domains govern their own input preimages. A known-total
  formatter such as `quote` is `KnownAny`, not opaque. An absent observation,
  lost input identity, or exhausted analysis budget supplies no policy evidence.
- Each legacy declaration producer remains until its replacement passes ADR 0071's
  deletion gate. Acceptance repairs and independent consumer intersections are separate obligations;
  they cannot be removed merely because a new host requirement exists.
- `--declared-types=annotate` keeps the default as documentation (property
  names, descriptions, `default`) and lets only template and resource
  evidence constrain the value.
- No chart-name or witness-specific exemptions. F80/F43 remain strict under
  their own opaque text consumption guards.

## Consequences

- F80 witnesses are POLICY-EXCEPTION rows (strict verdict by default,
  `Accepts` under the option), reported CLOSED-BY-POLICY.
- The register's earlier default (classify tightenings by proven
  constraint origin, possibly via an analyzer origin tag) was replaced in
  the gate by these explicit POLICY-EXCEPTION rows.
- The setting is recorded in `x-helm-schema-policy.authoring` (ADR [0040](0040-emitted-schemas-self-identify-their-policy.md)).

## References

- `plan/schema-bug-hunt-v1.md` F80 (and F43)
- `plan/schema-bug-hunt-v1-progress.md` Decision register (F80 policy
  default, 3ed93525, 2026-09-05) and 13:00 entry ~3841-3850 (2026-09-26)
- `crates/helm-schema-gen/src/authoring_policy.rs`,
  `crates/helm-schema-cli/src/cli/chart_args.rs`; commit 5834fbbd (2026-09-29)
