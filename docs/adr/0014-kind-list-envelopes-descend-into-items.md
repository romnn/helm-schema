# 0014. `kind: List` envelopes: descend into `items`

Status: Accepted
Date: 2026-05-18 (plan `plan/list-envelope-items-descent.md`, commit cc2662e6); implemented 2026-06-10 (c942b0b5)

## Context

Some templates emit the Kubernetes `List` envelope with real resources under
`items:` (alertmanager's `ingressperreplica.yaml`, `serviceperreplica.yaml`).
The line-based detector attributed every `.Values` use inside `items[*]` to
the wrapper, and the lookup chain suppressed `kind: List` lookups, so every
use inside the items was validated against nothing.

## Decision

The detector descends structurally into `items[*]` of the exact
Kubernetes `v1` `List` envelope. Each item gets its own `(apiVersion, kind)`
identity from the same detector, and uses inside it are attributed to that
identity with their YAML path rebased (`items[0].spec.rules[*].host` becomes
the inner resource's `spec.rules[*].host`). The wrapper produces no lookup
and no diagnostic because nothing is attributed to it.

## Consequences

- The two `kind == "List"` short-circuits in the lookup chain are deleted;
  the chain no longer knows about `List`.
- Custom resources named `List` remain regular resources; other `*List`
  kinds are not treated as envelopes until a real chart needs it.
- Nested List envelopes are handled by construction but not specially
  tested; missing identity on an item is treated like a top-level document
  without identity.
- After the frontend redesign, List items are read from the CST (ece942cf).

## References

- `plan/list-envelope-items-descent.md` ("Why", "Goal", "Out of scope")
- CLAUDE.md "Design goal: structural static analysis first" (`kind: List`
  example)
- Commits c942b0b5 (2026-06-10), ece942cf (2026-07-03)
