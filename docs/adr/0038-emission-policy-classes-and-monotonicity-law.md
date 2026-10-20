# 0038. Emission policies are classed, and only widening policies are selectable presets

Status: Accepted
Date: 2026-08-02 (plan v2.6 frozen; implemented 2026-08-03)

## Context

Live validation cost was dominated by `helm lint` recompiling the large
generated `values.schema.json`, and an earlier opaque `full|lean` switch
gave no account of what a lighter schema gave up. The emission-profiles plan
replaced it with named, individually reasoned emission policies, under the
rule that default behavior must not change: full-fidelity schemas stay the
default.

## Decision

Every emission policy belongs to one class:

- **VE (validation-equivalent):** accepts exactly the same instances.
- **W (widen):** accepts the same or more, `accepts(full) ⊆ accepts(config)`.
  W is the only class profiles may toggle.
- **X (narrow):** beyond proven facts. Never in a profile and never
  activatable from discovered config; `infer-required` is X and stays an
  explicit CLI opt-in with its own policy type.

The monotonicity law (`accepts(full) ⊆ accepts(config)`, equality for
VE-only deltas) binds every VE and W policy. Built-in profiles are exactly
two presets: `full` (default, every W knob on), and `lean`, which keeps all
Mandatory facts and local conditionals while disabling root-anchored
conditionals, kind partitions and terminal clauses. Deliberately not
exposed: presence/not-null, values-default backfill, declared-default
preservation, falsy escapes, program wrappers, normalization, and
`assume-typed-scalars` (fidelity charter).

## Consequences

- A profile or config can only make a schema more permissive than `full`,
  never stricter; narrowing requires an explicit caller flag.
- The public knob matrix is four W knobs (`root-anchored-conditionals`,
  `local-conditionals`, `terminal-clauses`, `kind-partitions`); turning
  `kind-partitions` off deletes the partition refinements (a pure widening).
- Output-side options (`refs`, `descriptions`, `minify`) stay separate from
  emission policy; one top-level CLI/config resolver owns all of them.
- Lean is the measured middle point: temporal-wrapper retained the `"three"`
  control at 17.3 s versus 110 s; the sub-second variant lost typing. Sizes
  (3.84/4.19 MB) fit Helm but left little headroom. Turning local conditionals
  off remains an explicit config choice, as in the temporal chart migration.
- Preset membership is pinned by config version ([0039](0039-versioned-chart-local-policy-config.md)); changing lean needs
  a version bump. The lint floor is a release criterion, not a general law.

## References

- `plan/schema-emission-profiles.md` intro (v2.6), "Policy classes and the
  laws", "The public knob matrix", "Profiles are presets"
- `plan/schema-emission-profiles-progress.md` "Open decision points",
  "Lean contract", "Temporal migration"; d2e0717f (v2.4, 2026-08-02)
- Commits d9cb4a2d (plan, 2026-07-17), 20e31e83 and d67020ff (2026-08-03)
- `docs/content/docs/reference/cli.md` (`--profile`, knob flags, `--infer-required`)
