# helm-schema — `kind: List` items[*] structural descent

Replace the chain-layer suppression of `MissingSchema(kind=List, ...)` with real structural
descent into `items[*]`: each inner resource gets its own apiVersion/kind identity, and uses
inside it are validated against that identity's actual schema.

## Why

Some vendored Helm templates emit the standard Kubernetes `List` envelope to ship multiple
resources from a single template file:

```yaml
apiVersion: v1
kind: List
items:
  - apiVersion: networking.k8s.io/v1
    kind: Ingress
    metadata: { ... }
    spec: { ... }
  - apiVersion: v1
    kind: Service
    metadata: { ... }
    spec: { ... }
```

The canonical examples in the corpus are alertmanager's `ingressperreplica.yaml` and
`serviceperreplica.yaml` — both emit `kind: List` wrapping per-replica resources.

**Today's behaviour.** The line-based detector attributes every `.Values.*` use inside `items[*]`
to the wrapper's identity (`kind: List, apiVersion: v1`). The chain layer then has two suppression
short-circuits:

- `Chain::schema_for_use` (`crates/helm-schema-k8s/src/lookup/chain.rs:61`): returns `None` early
  if `resource.kind == "List"`.
- `Chain::commit_missing_schema` (`crates/helm-schema-k8s/src/lookup/chain.rs:263`): returns early
  if `resource.kind == "List"`.

This keeps the live gate clean (no useless `MissingSchema(kind=List, api_version=v1)` noise) but
it also means **every `.Values.*` use inside `items[*]` is validated against nothing**. If the
inner Ingress's `.spec.rules[*].host` is misspelled as `.spec.rules[*].hots`, no schema check
fires. The List wrapper acts as an unintentional validation black hole.

This is exactly the kind of structural recovery helm-schema is supposed to do: the AST shows us
each `items[*]` entry is a real K8s resource with its own apiVersion and kind. Treating that
information as opaque after the IR runs is a regression in precision.

## Goal

When the detector encounters a `kind: List` document with an `items: [...]` sequence:

- recursively detect each item's `(apiVersion, kind)` — using exactly the same detector that
  handles top-level resources;
- emit each `ValueUse` inside `items[i]` attributed to the item's own `ResourceRef`, with its
  YAML path rebased so that `items[0].spec.rules[*].host` becomes the inner resource's
  `spec.rules[*].host`;
- the wrapper itself produces no schema lookup and no diagnostic — not because we suppress it,
  but because the IR no longer attributes anything to the List identity.

The chain stops needing to know about `kind: List` at all. The two suppression short-circuits are
deleted; the chain just sees inner resources with their own identities and resolves them
normally.

## How (high-level)

**Depends on `plan/unify-resource-detector.md`.** Structural descent into nested mappings inside
a YAML sequence requires walking the AST, not a line buffer. The line-based detector fundamentally
can't do this: indent-based scanning doesn't carry the structural fact that "this mapping is the
i-th item of a `items:` sequence inside a `kind: List` document". The unified AST detector has
that information natively from `HelmAst::Pair { key, value: Sequence { items } }`.

Sketch, on top of the AST-driven detector:

1. **Recognise the envelope.** When the detector identifies a document with `kind: List` *and*
   discovers a `Pair { key: Scalar("items"), value: Sequence { items } }` in the same top-level
   mapping, mark the document as a List envelope.

2. **Recursive identity detection.** For each entry in the `items` sequence that is itself a
   `Mapping`, run the AST detector recursively on that mapping. The result is one `ResourceRef`
   per item.

3. **YAML path rebasing for contract attribution.** `SymbolicIrContext` produces contract claims
   whose document paths are currently rooted at `["items", "0", "spec", "rules"]` and attributed to
   the `List` envelope. After descent, it produces claims rooted at `["spec", "rules"]` and
   attributed to the inner `Ingress` item — the `items.[i]` prefix is stripped before compatibility
   `ValueUse` fixture projection.

4. **Drop the envelope entirely from the IR.** The List wrapper has no contributed value uses of
   its own (its only field is `items`, which is structural plumbing). After descent, the IR for a
   List envelope file contains exactly the union of the IRs each inner item would have produced
   as its own top-level document. The wrapper's `ResourceRef` never appears in any projected
   contract claim.

5. **Delete the chain suppressions** in `Chain::schema_for_use` and `Chain::commit_missing_schema`.
   They become dead code.

6. **Update the existing regression test.** `kind_list_envelope_emits_no_missing_schema_diagnostic`
   in `crates/helm-schema-gen/tests/` currently asserts "no MissingSchema(List, ...) is emitted".
   After descent, the stronger assertion is "the inner Ingress is validated against
   `networking.k8s.io/v1/Ingress` schema (its real attribution), AND no MissingSchema(List, ...)
   is emitted as a side effect". The test still pins the same surface contract for the user but
   now also pins that inner-resource validation actually happened.

## What this removes / cleans up

When this plan lands:

**Deleted code (`crates/helm-schema-k8s/src/lookup/chain.rs`)**

- Round-5 Finding 3 short-circuit in `Chain::schema_for_use` (`chain.rs:61` block).
- Round-6 Finding 3 short-circuit in `Chain::commit_missing_schema` (`chain.rs:263` block).
- Their long doc comments explaining why the wrapper is suppressed.

**Deleted dependencies (`AGENTS.md`)**

- The portion of the "Known architectural debt: dual resource detector" section that calls out
  `kind: List` items[*] as the canonical example of structural recursion the unified detector
  enables. That sentence becomes stale once both plans ship.

**Deleted regression tests**

- `chain_schema_for_use_kind_list_envelope_emits_no_diagnostic` (round-5)
- `chain_resolve_against_chain_kind_list_envelope_emits_no_diagnostic` (round-6)

These tested the chain's suppression contract specifically. With suppression gone, the contract
they pin no longer exists — the corresponding behaviour is now "the IR doesn't produce a List
attribution in the first place", which the gen-level
`kind_list_envelope_emits_no_missing_schema_diagnostic` test (upgraded as in step 6 above)
covers end-to-end.

**Removed silent failure mode**

- Uses inside `items[*]` are no longer validated against nothing. Real chart bugs in inner
  resources now surface as `MissingSchema(kind=<inner>, …)` or as proper schema validation
  errors during values.yaml validation. Currently they're invisible.

## Relationship to AGENTS.md

Same handling as the sibling plan. The AGENTS.md note about partial `kind: List` handling stays
in place until this plan lands, then deleted in the same PR. A one-line cross-reference from
AGENTS.md to this plan ("Design for full structural descent in
`plan/list-envelope-items-descent.md`") is fine to add now.

## Out of scope

- Validating that `apiVersion` and `kind` are present on every item. The Kubernetes API server
  enforces this; helm-schema treats items lacking identity the same way it would treat a
  top-level document lacking identity (the existing IR semantics — no detected resource).
- Nested List envelopes (a `kind: List` whose items include another `kind: List`). Possible in
  theory; not observed in any vendored chart. If it shows up, the recursive detector handles it
  by construction, but write a regression test then, not now.
- Other "transparent envelope" kinds beyond `List` (e.g. `kind: ConfigMapList`). The Kubernetes
  convention is that any `Kind` ending in `List` with an `items` array is an envelope, but real
  templates only emit `kind: List`. If we discover others in the corpus later, generalise then.
