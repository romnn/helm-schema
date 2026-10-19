# 0033. Values paths are a segmented type, not strings

Status: Accepted
Date: 2026-08-25 (architecture review v4, A3/B4); carrier landed 2026-08-26 (`e5aaf250`)

## Context

Values paths travelled as escaped strings. The core codec escapes `.` and
`\` inside segments, but about 15 sites re-encoded or split paths with raw
`join(".")`/`split('.')`/`format!`. Any dotted key
(`podAnnotations."prometheus.io/scrape"`, `"config.yaml"`) could land
abort-grade requirements on phantom paths or silently drop overlay-guard
collapses. A literal `*` key was indistinguishable from the range-member
wildcard. v4 listed "untyped currencies" as a root cause.

## Decision

`ValuesPath` in `helm-schema-core` is the only semantic path currency: a
private segmented representation with explicit `parse`/`from_segments`
constructors, structural navigation (`segments`, `parent`, `push`,
`is_descendant_of`, `item_parent`) and no `Deref<Target=str>`, `AsRef<str>`
or `Display`. Encoding happens only at diagnostics and wire boundaries.
Segments are typed `Segment::{Literal(String), EachMember}`, so a literal
`*` key stays an object member and only `EachMember` means array/map members.

## Consequences

- `split('.')` and `format!("{a}.{b}")` on paths stop compiling.
- Custom serde and a manual `Ord` preserve the legacy encoded-string spelling
  and order, so fixture bytes and BTree iteration did not move (B4a,
  migrated carrier by carrier in the byte-exact B4a.1-B4a.22 rounds).
- The dot/backslash repair (A3) and the literal-star distinction (B4b) were
  behavior-bearing fixes; the carrier migration itself was representation-only.
- Earlier, v3 deferred a `ValuesPath` newtype until the fact carrier was
  singular (v3 "Deliberately unscheduled debt"); v4 sequenced it before the
  transform and predicate work.

## References

- `plan/architecture-review-v4.md` "Verdict" (root cause 3), A3, B4
  (`bb61a78f`, 2026-08-25)
- `plan/architecture-review-v4-progress.md` "Round A3", "B4a.1"-"B4a.22",
  "B4b — distinguish literal `*` keys from ranged members"
- `plan/architecture-review-v3.md` "Deliberately unscheduled debt"
- Commits `e5aaf250`, `054f7ed8` (2026-08-26), `45606b5b` (2026-08-29); code:
  `crates/helm-schema-core/src/value_path.rs`
