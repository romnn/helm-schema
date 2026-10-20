# 0048. Emit JSON Schema Draft-07, the dialect Helm validates against

Status: Accepted
Date: 2026-01-03 (earliest emitted `draft-07/schema#` in the crates, ba086477)

## Context

The generated file is consumed by Helm, which validates the coalesced values
document against a chart's `values.schema.json`. The output reference states
that Draft-07 "is the dialect Helm validates against". Constraints the chart
expresses through Go regexps must also survive the validators that read the
file: `jsonschema` 0.56 checks `format: "regex"` with a strict ECMA-262
`u`-mode parser, so one provider pattern such as zalando's `^\ *(...)\ *$`
made a whole emitted schema fail metaschema validation.

## Decision

helm-schema emits a single JSON Schema Draft-07 document rooted at the values
object (`"$schema": "http://json-schema.org/draft-07/schema#"`). Every emitted
`pattern` and `patternProperties` key must be a valid ECMA-262 `u`-mode
regular expression: superfluous RE2 escapes are respelled (located with
`regex-syntax`), class `&`/`~` become hex escapes and `\<`/`\>` become
literals, while colliding `patternProperties` keys keep their original
spellings.

## Consequences

- Draft-07's limits are representability boundaries, not analyzer gaps: a
  constraint with no Draft-07 encoding stays open (ADR [0049](0049-runtime-tpl-programs-stay-open.md)). Recorded
  examples: Go's `(?m)` flag has no Draft-07 ECMA pattern encoding; Draft-07
  cannot correlate one dynamic property name across two independent maps;
  general substring preimages of provider enums are not faithfully encodable.
- The acceptance battery normalizes baselines read from older commits with the
  same respelling so they still compile.

## References

- `docs/content/docs/reference/output.md` "Anatomy" (`$schema` always
  Draft-07), first in 5e59ea17 (2026-07-23)
- `plan/from-scratch-architecture.md` (lowering "to draft-07", bundled
  draft-07 output)
- Commit 430ba851 (2026-09-27) "fix(patterns): respell regex escapes that
  ecma-262 u-mode rejects"
- `plan/chart-corpus-status.md` line ~1618 (vault `(?m)` stays open), "Rejected
  (invalid or won't fix by design)" F84 remainder and F93 remainder (~2710-2720)
