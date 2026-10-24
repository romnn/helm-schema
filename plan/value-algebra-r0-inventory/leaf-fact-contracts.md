# R1b binding-fact derivation contracts

All thirteen `LocalSlot` facts remain scheduled for R3b. Their classes and
producer mappings retain the R1a definitions. No leaf-fact transfer is claimed.

| Fact | Required derivation contract | Why it remains a slot fact |
| --- | --- | --- |
| `binding` | Preserve the stored value tree, evaluation boundary, leaf metadata and branch selection through lexical scope and control exits. | Removal needs the replacement frame model. |
| `default_paths` | A whole-local or selector read uses the union of exit sets only when **every** exit carries `Some(paths)`; one `None` drops the fact. A proven alternative reads its own leaf metadata. | A range binding can exist without a slot default fact. A union of leaf sets erases that presence distinction. |
| `output_meta` | Reproduce omission-exclusion retains, branch joins and metadata-only assignments, including mutations after binding creation. | A leaf union omits later slot mutations. |
| `scalar_dispatch` | Match guarded alternatives, completeness, the unchanged-entry shortcut and the 128-arm cap. | Scalar joins have independent bounded control semantics. |
| `typeof_source` | Retain the descriptor source map only when every exit retains the same map, including metadata-only bindings. | Leaf programs do not establish equality of exit provenance. |
| `int_cast_source` | Retain the total-cast source and literal fallback only by equality across exits; preserve self-assignment resolution order. | Source resolution occurs after binding displacement and depends on that read frame. |
| `truthy_reduction` | Preserve existential accumulation over monotone range exits, last-write behavior for falsy-capable writes and bounded branch stamps. | This is a control-exit accumulator. |
| `truthiness_abstained` | Preserve sticky reduction-budget abstention across exits and reset it on declaration. | A final value does not encode earlier budget exhaustion. |
| `truthiness_cleared` | Preserve sticky non-monotone write history, union it across exits and reset it on declaration. | Identical final leaves can have different write histories. |
| `range_domain` | Union literal key domains only when every exit carries a domain; restore declaration scope. | A key domain can exist without a fragment binding. |
| `get_binding` | Preserve the computed-key base and key variable only when all exits retain equal facts. | Applying this provenance clears the fragment binding; replacement requires member identity. |
| `range_member_value` | Join member identities by value choice only when every exit retains one; preserve the lazy direct-binding fallback. | Member identity differs from rendered placement. |
| `definite_range_member_value` | Retain a definitely iterated overlay member only when every exit retains the same identity. | Universal iteration identity is stronger than a selected-value alternative. |

## Default-path counterexample

```gotemplate
{{ range $x := list .Values.item }}{{ if $.Values.flag }}{{ $x = default "d" $.Values.other }}{{ end }}{{ $x }}{{ end }}
```

Range activation installs a direct binding without assigning a default-path
fact. The conditional assignment records `Some({other})`, while the untouched
exit still has `None`. `LocalSlot::join` therefore drops the fact through
`join_field`. The attempted leaf union retains `{other}`. An ordinary local
declaration instead records `Some(empty)`, so its untouched exit participates
in the union. These cases must remain distinct.

The bare template and its non-range and nested-if variants emit identical full
schemas at 054aba56 and 43eacf58: the bare read introduces no schema obligation
that observes the changed default provenance. This does **not** make their
internal joins equal. With a ConfigMap header and `{{ $x | toYaml }}` as the
read, 43eacf58 drops the `other` absent/null rejection clause that 054aba56
emits, for both range variants. The non-range control retains the same schema.
`crates/helm-schema/tests/local_default_join.rs` pins the complete documents for
all six cases, including the unchanged bare templates.

The existing leaf set has no representation for the distinction between an
absent slot fact and a present empty fact. Testing for a direct leaf or an
unknown node would approximate control history: assignments also install
present metadata when no fragment binding survives. `output_meta` is not a
presence alias either; `BoundValueContext::set_output_metadata` clears and
replaces it independently. Preserving the old contract requires retaining its
slot presence/join model or designing and proving a new representation.
R1b therefore restores the original field, writes, projections and join,
including the previous tests and adjacent assignment code. Production sources
are byte-identical to 054aba56; the failed transfer and its R1b landed marker
are removed. The twelve independent residual contracts above are retained.
