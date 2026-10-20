# 0025. Reject unquoted YAML 1.1 Boolean-alias keys in chart declarations

Status: Accepted
Date: 2026-08-05 (decision point D1 resolved, option 1; landed 2026-08-25, `1204b0f9`)

## Context

Measured against Helm 4.2.3: unquoted `y/n/yes/no/on/off` mapping keys are
normalized to the string keys `"true"`/`"false"`, quoted spellings and
`--set` paths are preserved, and `.Values.y` is nil after normalization. When
a normalized alias collides with a quoted canonical `"true"`/`"false"` key,
identical invocations have produced different winners. Round 71 therefore
vetoed deterministic normalization. Corpus and luup2 sweeps found no affected
declarations. Options were: (1) reject such declarations with one aggregated
diagnostic, (2) keep current parsing and document the unmodelled
normalization, (3) normalize the non-colliding subset and reject or abstain on
collisions.

## Decision

Option 1, chosen by the user: before analysis, scan every discovered chart's
`values.yaml` and every explicit values file supplied as analysis input, and
reject plain, unquoted mapping keys whose spelling is a legacy YAML 1.1
Boolean alias (`y`, `Y`, `yes`, `Yes`, `YES`, `n`, `N`, `no`, `No`, `NO`,
`on`, `On`, `ON`, `off`, `Off`, `OFF`) with one deterministic aggregated
diagnostic naming every affected file and key location.

## Consequences

- helm-schema never claims a deterministic composition result Helm does not
  have; the known false-rejection lane of option 2 is closed.
- Quoted spellings (`"on"`, `'no'`) remain string keys; canonical
  `true`/`false` keys are unchanged; CLI `--set` paths are not declarations
  and are unaffected.
- No first-error-only lane and no parser normalization.
- Currently non-disruptive: zero affected declarations in the corpus and
  luup2.

## References

- `plan/architecture-review-v3.md` "D1. YAML 1.1 boolean-key disposition",
  Step 11
- `plan/architecture-review-v3-progress.md` "Decision register" (D1),
  "Step 11a — reject YAML 1.1 Boolean-alias declaration keys"
- Commit `1204b0f9` (2026-08-25)
