# helm-schema — unify the dual resource detector

Collapse the two parallel resource-identity detectors in `helm-schema-ir` into a single AST-driven
implementation, so the production CLI path and the IR test suite both exercise the same code.

## Why

`crates/helm-schema-ir` currently has two detectors that answer "which Kubernetes resource does
this document represent?":

- **`DefaultResourceDetector`** (`crates/helm-schema-ir/src/lib.rs:104`) — original AST-based
  detector. Given a parsed `HelmAst`, scans top-level `Mapping` items for `apiVersion` / `kind`
  scalar pairs and returns a single `ResourceRef`. Simple, structural. **No notion of multi-document
  files, helper-returned apiVersions, inline `if Capabilities.APIVersions.Has` chains, or `kind:
  List` envelopes.**

- **`SymbolicWalker::ResourceDetector`** (`crates/helm-schema-ir/src/symbolic.rs:1569`) — line-based
  detector used by the production `SymbolicIrGenerator`. Accumulates raw template source via a
  dual-buffer split in `ingest_text_up_to`, tracks `---` document boundaries, evaluates
  helper-returned apiVersions via `helper_evaluate`, captures inline `{{- if -}}`/`{{- else -}}`
  branch chains via a depth-tracked state machine (`InlineIfState`, `ActionDirective` enum,
  `process_action_directive`), and preserves multi-branch alternatives as typed `HelperBranch`
  entries.

Two costs of this split:

1. **Test coverage gap.** Many IR tests under `crates/helm-schema-ir/tests/` construct a `HelmAst`
   and run `DefaultResourceDetector` directly. Those tests pass against the simpler detector and
   tell us nothing about whether the production line-based detector handles the same shape
   correctly. Bugs in the production path can ship even when the test suite is green.

2. **Architectural smell.** The production walker is line-based but has been progressively
   acquiring structural concepts (document boundaries via `---`, nested block depth via
   `inline_block_depth`, action directive classification via `ActionDirective`) that the AST gives
   natively. We are reimplementing a partial AST inside a line-based walker, badly.

`AGENTS.md` already documents this debt and recommends extending the line-based detector "for now"
while signalling that AST-driven logic should accelerate unification. This plan is the unification.

## Goal

One AST-driven `ResourceDetector` (replacing both `DefaultResourceDetector` and
`SymbolicWalker::ResourceDetector`) that:

- finds `apiVersion` / `kind` pairs from `HelmAst::Mapping` items at the YAML document root,
  regardless of source-order;
- handles multi-document streams (`---` separators are already explicit document boundaries in the
  AST — no line scanning required);
- resolves `apiVersion: {{ template "X" . }}` / `{{ include "X" . }}` via `helper_evaluate`
  (already typed; carry the `HelperOutput` through verbatim);
- captures `{{- if .Capabilities.APIVersions.Has "X" -}}` / `{{- else -}}` chains around an
  `apiVersion:` line as typed `HelperBranch` entries by walking the `HelmAst::If` node directly —
  no `inline_block_depth`, no `ActionDirective::parse` keyword matching;
- (sets up the seam for) structural descent into `kind: List` items[*] — see the sibling plan
  `list-envelope-items-descent.md`.

`SymbolicIrGenerator` keeps owning value-use extraction; only the resource-identity detection is
unified.

## How (high-level)

This is a non-trivial refactor. Suggested staging:

1. **Define the new detector contract.** A new trait or struct (working name
   `AstResourceDetector`) that accepts a `&HelmAst` document subtree and returns `Vec<ResourceRef>`
   (multi-document support up-front; the current `Option<ResourceRef>` shape only fits
   single-document templates).

2. **Implement the AST walker** as the structural rewrite of the line-based detector. Pattern-match
   on `HelmAst::Document { items }` to enumerate top-level documents; for each, walk
   `HelmAst::Mapping { items }` looking for `Pair { key: Scalar("apiVersion"|"kind"), value: ... }`
   shapes. Branch handling falls out of the AST: `HelmAst::If { cond, then_branch, else_branch }`
   wrapping an apiVersion pair is exactly the shape `decode_guard` already operates on —
   reuse it.

3. **Make `SymbolicIrGenerator` consume the new detector.** Replace the line-based
   `ResourceDetector` machinery in `crates/helm-schema-ir/src/symbolic.rs` with calls to the new
   AST walker, threading detected `ResourceRef`s through the existing `ValueUse` attribution path.

4. **Migrate IR tests off `DefaultResourceDetector`.** Each test under
   `crates/helm-schema-ir/tests/` either:
   - parses a real template and asserts on the IR `SymbolicIrGenerator` produces (preferred — this
     is what we wanted all along), or
   - exercises the new AST detector directly for the few cases that want a focused unit shape.

5. **Delete `DefaultResourceDetector`, the `ResourceDetector` trait in `lib.rs`, and
   `scan_for_resource`.** The simple AST scanner becomes the new detector's special case for
   single-document, no-branch shapes; no separate type needed.

6. **Delete the line-based machinery in `symbolic.rs`:** `InlineIfState`, `ActionDirective`,
   `ActionDirective::parse`, `strip_keyword`, `process_action_directive`,
   `attribute_api_version_literal`, `parse_literal_value`, `parse_helper_call_value`, the
   `inline_block_depth` field, the dual-buffer split in `ingest_text_up_to` (`shape_buf` vs
   `detector_buf`), and `ResourceDetector::ingest` / `process_line` themselves.

## What this removes / cleans up

When this plan lands:

**Deleted code (`crates/helm-schema-ir/src/`)**

- `lib.rs`: `DefaultResourceDetector` struct + impl, `ResourceDetector` trait, `scan_for_resource`
  helper.
- `symbolic.rs`: `ResourceDetector` struct, `InlineIfState`, `ActionDirective` enum + `parse`,
  `strip_keyword`, `process_action_directive`, `attribute_api_version_literal`, `parse_literal_value`,
  `parse_helper_call_value`, `inline_block_depth` field, dual-buffer ingest logic, all
  `detector_unit_tests` module entries that test the line-based shape.

**Deleted dependencies (`AGENTS.md`)**

- The "Known architectural debt: dual resource detector" section — replaced by a brief reference
  ("this section was retired when `unify-resource-detector.md` shipped; see git history for context")
  or removed outright after a grace period.

**Removed workarounds**

- The line-based detector's `header_done` flag, the indent-zero gate (`if indent != 0 { return }`),
  and the `if trimmed.starts_with("{{") { process_action_directive(...); return }` switch — all
  exist because line-based scanning has no native sense of "where in the YAML tree am I". The AST
  walker has that natively from `HelmAst::Mapping` / `HelmAst::Pair` structure.

**Improved guarantees**

- Every IR test exercises the production code path. No more "the simple detector passes; the
  production one may or may not".
- New detectors features (e.g. List items[*] descent — see sibling plan) land in one place, not
  two.

## Relationship to AGENTS.md

The existing `AGENTS.md` "Known architectural debt: dual resource detector" section should
**coexist** with this plan until the plan is implemented. The AGENTS.md section is the in-tree
hazard warning for agents touching the area; this plan is the design for the cleanup. When the
plan lands, the AGENTS.md section is deleted in the same PR — leaving the warning after the debt
is gone would be misleading.

A one-line cross-reference can be added to the AGENTS.md section now ("Design for the cleanup
lives in `plan/unify-resource-detector.md`") so anyone reading the warning knows where to find the
follow-up. Not required for the plan to be useful.

## Out of scope

- Restructuring the rest of `SymbolicIrGenerator` (value-use extraction, guard accumulation,
  range-domain tracking). Only resource-identity detection unifies here.
- Migrating `helper_evaluate`'s typed branch output to a different shape. The existing
  `HelperOutput`/`HelperBranch` types stay; the new detector just consumes them more directly.
- `kind: List` items[*] structural descent. Tracked separately in
  `plan/list-envelope-items-descent.md` — depends on this plan but is its own ship vehicle.
