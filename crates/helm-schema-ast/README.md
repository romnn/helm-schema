I’ll explain what makes the YAML-path problem inherently ambiguous in the current pipeline (even with a “structured” AST), which parts of our AST/parsers are missing information needed for a fully principled solution, and then outline 2-3 alternative designs that can make this 100% reliable (including what to borrow from the old deprecated approach). I’ll quickly re-open the relevant parts of [walker.rs](cci:7://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:0:0-0:0) to ground the discussion in what the code actually knows today.

I’m going to (1) pinpoint the *exact* information the current `HelmAst` and parsers do/don’t preserve (especially indentation/byte offsets), (2) explain why that makes YAML-path attachment ambiguous without heuristics, and (3) outline a couple of fully-principled alternatives (template-aware YAML parser state machine; symbolic rendering + YAML reparse; or enriching the AST with indentation/continuation markers). I’ll also quickly search the repo for any older “shape/indent” tracker implementations we can reuse.



## Why we *currently* need heuristics in [walker.rs](cci:7://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:0:0-0:0)

Your premise is right *in principle*: if we had the **exact output indentation / line structure** of the rendered YAML (or a faithful symbolic equivalent), then YAML-path attachment is not guesswork.

The reason we’re doing “brittle stitching” today is that our current IR pipeline is walking a **purely structural** `HelmAst` that has **lost the key piece of information that disambiguates these cases**:

- **The original template’s layout** (newlines, indentation, and the `-` sequence-item markers as they appear in the source stream around Helm directives)
- **The exact placement of Helm actions within a line** (e.g. after `key:` vs at column 0 vs after `- `)
- **Whitespace-trimming semantics** of `{{-` and `-}}` (which can delete the newline/indent you think exists)
- **Indent-/nindent-filter effects at the call site** (our AST contains `HelmExpr { text: String }`, but does not encode “this expression inserts a newline and indents by N” as a structured fact)

`HelmAst` today is:

```rust
pub enum HelmAst { Mapping{..}, Pair{..}, Sequence{..}, If{..}, HelmExpr{ text }, ... }
```

It contains **no spans, no column info, no indent levels, no raw “text nodes”**.

### The ambiguity that forces heuristics
When Helm control-flow splits what is *semantically one YAML sequence item* into multiple AST siblings, the structure alone doesn’t tell you whether a `Mapping` sibling is:

- **A continuation of the previous list item**, or
- **A brand new list item**, or
- **A mapping at the parent level** that happens to be adjacent in the AST.

The only reliable discriminator is: **what would the actual YAML tokens look like after whitespace trimming and indentation?**

That’s why the current walker ends up using heuristics like “if a mapping contains `to/from/ports` and the last path had `egress[*]`, stitch it under that”. That’s not because the AST is “unstructured”, but because the AST is **not layout-aware**, and the ambiguity is fundamentally about **layout**.

## Why “expand includes + apply `indent/nindent`” doesn’t magically fix it (today)

There are two separate issues:

### 1) We don’t actually model indentation operations as semantics
In [walker.rs](cci:7://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:0:0-0:0) we detect “fragment-ness” with substring checks:

```rust
pub fn is_fragment_expr(text: &str) -> bool {
    text.contains("toYaml") || text.contains("nindent") || ...
}
```

That’s classification, not execution.

Even if we inline a define body ([parse_include_name](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:416:0-420:1) + [defines.get()](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-ast/src/lib.rs:250:4-253:5)), we currently walk that AST at the **same** `yaml_path`, without a principled “this include is emitted *under* `spec.egress[-].to` at indent 6” model.

### 2) Even a perfect include expander doesn’t solve runtime-dependent output
Some constructs are inherently not statically reconstructible:

- **`tpl`**: executes a template contained in a string value. That string might contain YAML, might not, might include `{{ }}`.
- **`toYaml .Values.x`**: the resulting YAML structure depends on the runtime shape of `.Values.x` (map vs list vs scalar). If you want “100%”, you must either:
  - assume a shape (guess), or
  - treat it as an opaque fragment attached to the current key (principled conservative behavior), or
  - parameterize over possible shapes (state explosion).

So: yes, you *can* get rid of heuristic stitching for the “egress/from/to” cases, but it requires changing what information the walker has access to.

## The real underlying reason (in one sentence)

**YAML path attachment across Helm directives is a text-layout problem; our current `HelmAst` is a structure-only representation that discards the layout needed to decide attachment deterministically.**

It’s not “one parser is insufficient” so much as “our shared AST abstraction is insufficient for this goal”.

## Principled alternatives that can be 100% reliable (no guesswork)

Below are options that eliminate “content-based stitching” and replace it with deterministic rules derived from layout or a formal output model.

### Option A (most practical): Reintroduce a layout-aware “Shape tracker” (like deprecated `VYT.Shape`), but done properly
Your deprecated code actually hints at the right direction. In `deprecated/.../vyt.rs`, there’s a `Shape` that:

- ingests literal text,
- tracks indentation + mapping/sequence stack,
- handles “action appears after `key:` on the same line” via `(indent, col)` sync,
- and can therefore produce a stable YAML path without guessing.

That approach is principled because it derives the path from **the template’s concrete syntax**, not from the content of later nodes.

**What we’d need to make it first-class in the new architecture:**
- **Preserve source spans / positions** (start byte, end byte; line/col) in the AST, *or* run the IR generator directly over the **tree-sitter CST** (like VYT did).
- For `If/With/Range`: **save/restore Shape state per branch** (so the then-branch’s text doesn’t “leak” into else-branch shape).
- For `include/template`:
  - inline the define body **and** apply *call-site indentation semantics* (`indent/nindent`) deterministically when ingesting its text.
- For `tpl` / non-constant indent amounts:
  - treat as an **opaque fragment hole** at the call site (not a heuristic; a correctness boundary).

This can get you “100% reliable” **with respect to the template text semantics** (including `{{-` trimming), which is what you actually want for YAML placement.

### Option B: Enrich `HelmAst` with layout metadata so the current structural walker can be deterministic
If you want to keep the high-level `HelmAst` walk (instead of a text-ingesting shape tracker), you need to augment nodes with enough layout info to make attachment rules deterministic:

- **indent level** of each emitted YAML token
- whether a mapping entry / node begins with a **sequence dash** (`-`)
- **line/column of HelmExpr**
- span info so you can derive “is this on the same line as `key:`?” and whitespace trimming effects

Then “stitching” becomes a strict rule:
- if a node starts at indent > current mapping indent and the last thing was a pending key → it belongs under that key
- if it starts with `-` at the list indent → it’s a new list item
- etc.

No content sniffing.

Downside: you end up re-deriving “shape” implicitly anyway—so Option A is usually simpler/safer.

### Option C (most correct but biggest effort): A symbolic renderer that emits a YAML token/event stream + provenance
Think “execute the Helm template”, but instead of producing a single string YAML, produce:

- a stream of YAML-ish events (`BeginMap`, `Key("spec")`, `BeginSeqItem`, …)
- plus “holes” for expressions (`ScalarHole(.Values.x)`, `FragmentHole(include "x")`)
- plus guards attached to branches

Then the YAML path at each `.Values` use is just the current event stack.

This is extremely principled, but it’s basically writing a Helm interpreter (including whitespace trimming, pipelines, dot rebinding, range scopes, etc.). It’s doable, but much more work than Option A.

## What I’d recommend if you want “no heuristics” soon

- **Adopt Option A** (shape tracker) using tree-sitter positional data, because it already existed in the old codebase and directly targets the missing invariant: *layout*.
- Extend `HelmAst` or IR generator inputs to carry:
  - the original source string
  - node spans (or run on CST)
- Make the fallback behavior for `tpl`/dynamic indent explicit and conservative (attach as fragment at call-site, or empty path).

That will eliminate the current “key-content stitching” ([contains_scalar_key("to")](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:424:0-472:1) etc.) entirely for the cases that are actually determined by indentation/structure.

## One key nuance: “100% reliable” has a boundary
You can be 100% reliable **about YAML placement implied by the template’s literal text + Helm whitespace rules**, *without runtime values*.

You cannot be 100% reliable about YAML structure produced by:
- `tpl` (runtime template string),
- `toYaml` of unknown-shaped values,
- `indent/nindent` whose indent amount is computed dynamically,

unless you model multiple possibilities or assume a shape (which is guesswork).

If you want, I can sketch a concrete design for integrating the deprecated `Shape` approach into `helm-schema-ir` (what types to add, what the traversal looks like, how to do branch save/restore, how to do include indentation) so we can replace [stitch_path](cci:1://file:///home/roman/dev/helm-schema/crates/helm-schema-ir/src/walker.rs:267:4-346:5) entirely.