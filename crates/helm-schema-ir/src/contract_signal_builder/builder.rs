use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, ProviderSchemaUse, ValueKind, contract::ContractUse};
use helm_schema_core::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay, ContractFailImplication,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractRequirementTarget,
    ContractSchemaSignals, ContractValuePathFacts, FailValueRequirement, MetadataFieldKind,
    Predicate,
};

#[tracing::instrument(skip_all)]
#[expect(
    clippy::too_many_arguments,
    reason = "each parameter is one interpreter fact channel; a struct would               mirror the same nine fields without adding an invariant"
)]
pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &BTreeMap<String, BTreeSet<String>>,
    guarded_type_hints: &BTreeMap<String, BTreeSet<String>>,
    fallback_type_hints: &BTreeMap<String, BTreeSet<String>>,
    guarded_fallback_type_hints: &BTreeMap<String, BTreeSet<String>>,
    shape_erased_value_paths: &BTreeSet<String>,
    string_contract_value_paths: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
    fail_conditions: &[crate::eval_effect::FailCapture],
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    let mut terminal_clauses = Vec::new();
    for contract_use in uses {
        record_contract_use(&mut paths, contract_use, range_modes);
    }
    for capture in fail_conditions {
        record_fail_conjunction(&mut paths, &mut terminal_clauses, capture, range_modes);
    }
    for value_path in dependency_values_root_fragments {
        if !value_path.trim().is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.facts.record_facts(ContractValuePathFacts {
                accepted_values_root_fragment: true,
                accepted_dependency_values_root_fragment: true,
                ..ContractValuePathFacts::default()
            });
        }
    }
    // A path the chart consumes through a total stringification tolerates
    // any input type, even when the flow is too indirect for a placed row
    // (vault's `set . "csiEnabled" (eq (.Values.csi.enabled | toString)
    // "true")`); the fact carries the same serialized dominance a
    // stringified render does.
    for value_path in shape_erased_value_paths {
        if value_path.trim().is_empty() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.referenced = true;
        acc.facts.facts.used_as_serialized = true;
    }
    // These paths' RAW values are consumed as Go strings before any
    // selection runs (a `tpl` program input piped through `default` still
    // parses first), so the contract types the path even when every placed
    // row is conditioned by the selection chain (oauth2-proxy's
    // `tpl .Values.image.registry $ | default … | default "quay.io"`).
    for value_path in string_contract_value_paths {
        if value_path.trim().is_empty() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.referenced = true;
        acc.facts.facts.has_string_contract = true;
        acc.type_hints.insert("string".to_string());
    }
    for (value_path, schema_types) in type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.type_hints.extend(schema_types);
        }
    }
    // Guarded hints hold only where their branches render: they type the
    // path's conditional overlays but never the unconditional base.
    for (value_path, schema_types) in guarded_type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.guarded_type_hints.extend(schema_types);
        }
    }
    // Fallback hints type only the truthy arm of their path: the base
    // lowering keeps the Helm-falsy set open beside them.
    for (value_path, schema_types) in fallback_type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.fallback_type_hints.extend(schema_types);
        }
    }
    // Branch-scoped fallback hints stay fallback-grade: they may type a
    // conditional overlay, but never one whose renders all totally format
    //.
    for (value_path, schema_types) in guarded_fallback_type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if !value_path.trim().is_empty() && !schema_types.is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.guarded_fallback_type_hints.extend(schema_types);
        }
    }
    finish_schema_signals(paths, terminal_clauses)
}

#[derive(Default)]
struct ContractPathAccumulator {
    referenced: bool,
    guard_predicates: Vec<ConditionalGuard>,
    facts: PathSchemaFactsAccumulator,
    requiredness: ContractRequirednessEvidence,
    /// Sink typing from guarded rows: binds at the path level only while no
    /// serialized use proves the wider contract (the overlay branches keep
    /// their own copies either way).
    guarded_provider_schema_uses: Vec<ProviderSchemaUse>,
    guarded_metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    type_hints: BTreeSet<String>,
    /// Hints observed only under branch predicates: overlay typing only.
    guarded_type_hints: BTreeSet<String>,
    /// Hints from literal `default`/`coalesce` fallbacks: they type only
    /// the truthy arm, so base lowering keeps Helm-falsy inputs open.
    fallback_type_hints: BTreeSet<String>,
    /// Branch-scoped fallback hints: fallback-grade overlay typing
    /// that a totally-formatting branch must not bind.
    guarded_fallback_type_hints: BTreeSet<String>,
    conditional_overlay_branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    has_unconditional_overlay_peer: bool,
    saw_unsupported_overlay: bool,
    fail_implications: Vec<ContractFailImplication>,
    /// Lowerable outer-guard sets of member accesses, grouped by raw kinds
    /// that an earlier proven mutation converted to an object.
    member_access_guard_sets: BTreeMap<Vec<String>, BTreeSet<Vec<ConditionalGuard>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathSchemaFactsAccumulator {
    metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    provider_schema_uses: Vec<ProviderSchemaUse>,
    facts: ContractValuePathFacts,
    all_uses_nullable: bool,
}

impl Default for PathSchemaFactsAccumulator {
    fn default() -> Self {
        Self {
            metadata_field_kinds: BTreeSet::new(),
            provider_schema_uses: Vec::new(),
            facts: ContractValuePathFacts {
                all_render_uses_self_guarded: true,
                ..ContractValuePathFacts::default()
            },
            all_uses_nullable: true,
        }
    }
}

impl PathSchemaFactsAccumulator {
    fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    fn record_metadata_field_kind(&mut self, field_kind: Option<MetadataFieldKind>) {
        if let Some(field_kind) = field_kind {
            self.metadata_field_kinds.insert(field_kind);
        }
    }

    fn record_facts(&mut self, facts: ContractValuePathFacts) {
        self.facts.used_as_fragment |= facts.used_as_fragment;
        self.facts.used_as_serialized |= facts.used_as_serialized;
        self.facts.used_as_yaml_serialized |= facts.used_as_yaml_serialized;
        self.facts.has_string_contract |= facts.has_string_contract;
        self.facts.has_string_contract_items |= facts.has_string_contract_items;
        self.facts.used_as_pathless_fragment |= facts.used_as_pathless_fragment;
        self.facts.accepted_values_root_fragment |= facts.accepted_values_root_fragment;
        self.facts.accepted_dependency_values_root_fragment |=
            facts.accepted_dependency_values_root_fragment;
        self.facts.is_ranged_source |= facts.is_ranged_source;
        self.facts.is_direct_ranged_source |= facts.is_direct_ranged_source;
        self.facts.has_destructured_range_use |= facts.has_destructured_range_use;
        self.facts.has_json_decoded_range_use |= facts.has_json_decoded_range_use;
        self.facts.is_partial_scalar_value_path |= facts.is_partial_scalar_value_path;
        self.facts.is_nullable |= facts.is_nullable;
        self.facts.merge_render_use_facts(facts);
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn merge_union(&mut self, other: Self) {
        for provider_schema_use in other.provider_schema_uses {
            self.record_provider_schema_use(provider_schema_use);
        }
        self.metadata_field_kinds.extend(other.metadata_field_kinds);
        self.record_facts(other.facts);
        self.all_uses_nullable &= other.all_uses_nullable;
    }

    fn facts(
        &self,
        has_referenced_descendants: bool,
        has_item_descendants: bool,
        has_structured_item_descendants: bool,
    ) -> ContractValuePathFacts {
        let mut facts = self.facts;
        facts.has_referenced_descendants = has_referenced_descendants;
        facts.has_item_descendants = has_item_descendants;
        facts.has_structured_item_descendants = has_structured_item_descendants;
        facts.is_nullable &= self.all_uses_nullable;
        facts
    }

    fn conditional_overlay_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let mut facts = self.facts(
            global_facts.has_referenced_descendants,
            global_facts.has_item_descendants,
            global_facts.has_structured_item_descendants,
        );
        // Iteration shape is a path-global fact (see the range-site record):
        // a branch hosting the path's rows keeps it even when the range
        // record landed under a differently keyed guard set.
        facts.has_destructured_range_use |= global_facts.has_destructured_range_use;
        facts.has_json_decoded_range_use |= global_facts.has_json_decoded_range_use;
        // A runtime string contract recorded by this branch's own rows
        // types the branch; mutually exclusive branches that render the
        // path without the contract stay unaffected.
        let mut type_hints = type_hints;
        if facts.has_string_contract {
            type_hints.insert("string".to_string());
        }
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

/// The subset of a path's observed type hints compatible with an overlay
/// branch's own type partition. A positive `TypeIs(T)` key keeps only `T`;
/// a negated one drops `T`; foreign guards leave the hints untouched.
fn partition_compatible_hints(
    hints: &BTreeSet<String>,
    guards: &[ConditionalGuard],
    value_path: &str,
) -> BTreeSet<String> {
    let mut compatible = hints.clone();
    for guard in guards {
        match guard {
            ConditionalGuard::TypeIs { path, schema_type } if path == value_path => {
                compatible.retain(|hint| hint == schema_type);
            }
            ConditionalGuard::Not(inner) => {
                if let ConditionalGuard::TypeIs { path, schema_type } = inner.as_ref()
                    && path == value_path
                {
                    compatible.retain(|hint| hint != schema_type);
                }
            }
            _ => {}
        }
    }
    compatible
}

fn record_contract_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    range_modes: &crate::range_modes::RangeModes,
) {
    if contract_use.range_key {
        record_range_key_slot_use(paths, contract_use, range_modes);
        return;
    }
    let disjuncts = contract_use.condition.disjuncts();
    let has_approximate_disjunct = disjuncts
        .iter()
        .any(|conjunction| conjunction.iter().any(Predicate::contains_approximation));
    let conjunctions = disjuncts
        .iter()
        .map(|conjunction| conjunction.iter().cloned().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    for predicates in conjunctions {
        // A constructed template projection adds self-presence to the path
        // alternatives it can still identify. When a sibling alternative is
        // approximate, that presence proves only that the selected candidate
        // exists, not that the candidate is a root values path. Promoting the
        // pathless read would turn recursive member sentinels into root keys.
        if has_approximate_disjunct
            && contract_use.path.0.is_empty()
            && !predicates.is_empty()
            && predicates
                .iter()
                .all(|predicate| predicate_is_self_presence(predicate, &contract_use.source_expr))
        {
            continue;
        }
        // A merged sink's `with` gate marks the row with every layer's
        // truthiness, but a layer's keys reach the render exactly when the
        // LAYER itself is truthy: a sibling layer's marker would file this
        // layer's typing under the wrong path's truthiness (the velero
        // securityContext guard inversion), so those markers are dropped
        // before lowering.
        let predicates: Vec<Predicate> = if let Some(merge) = &contract_use.merge_layers {
            predicates
                .into_iter()
                .filter(|predicate| {
                    !matches!(
                        predicate,
                        Predicate::Guard(Guard::Truthy { path } | Guard::With { path })
                            if path != &contract_use.source_expr
                                && merge.layers.contains(path)
                    )
                })
                .collect()
        } else {
            predicates
        };
        record_contract_use_conjunction(paths, contract_use, &predicates, range_modes);
    }
}

/// A row rendering the collection's RANGE KEY contributes exactly one fact:
/// the provider slot the key renders at, from which the generator derives
/// the key-domain requirement (a string-only slot excludes a non-empty
/// list's integer keys). It must never read as a render of the collection's
/// VALUE. Guarded or indirect sites abstain — the synthesized arm would
/// fire in states the analysis cannot scope.
fn record_range_key_slot_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    range_modes: &crate::range_modes::RangeModes,
) {
    if contract_use.path.0.is_empty() || !range_modes.mode(&contract_use.source_expr).direct {
        return;
    }
    let Some(provider_use) = provider_schema_use(contract_use, false) else {
        return;
    };
    for conjunction in contract_use.condition.disjuncts() {
        let predicates: Vec<Predicate> = conjunction.iter().cloned().collect();
        if predicates.iter().any(Predicate::contains_approximation) {
            continue;
        }
        let Some(guards) = lowerable_conditional_guard_set(contract_use, &predicates) else {
            continue;
        };
        let acc = path_accumulator(paths, &contract_use.source_expr);
        acc.referenced = true;
        if guards.is_empty() {
            acc.facts.record_provider_schema_use(provider_use.clone());
        } else {
            // The guarded use rides an overlay branch so the synthesized
            // key-domain arm carries the branch guards; the branch itself
            // resolves to nothing (range-key uses are skipped by value
            // resolution) and stays structurally inert.
            let branch = acc.conditional_overlay_branches.entry(guards).or_default();
            branch.record_provider_schema_use(provider_use.clone());
        }
    }
}

/// A use whose resource carries predicate-qualified kind branches
/// concretizes per disjunct: when the conjunction structurally entails
/// exactly one arm's selecting predicate, this row's kind IS that arm's
/// literal (airflow's `strategy:` under `not $stateful` is a Deployment
/// row). Unmatched rows keep the flat candidates; the branches themselves
/// never leave the builder.
fn kind_branch_resolved_use(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<ContractUse> {
    let resource = contract_use.resource.as_ref()?;
    if resource.kind_branches.is_empty() {
        return None;
    }
    let conjuncts: Vec<&Predicate> = predicates.iter().flat_map(flattened_conjuncts).collect();
    let mut selected = resource.kind_branches.iter().filter(|branch| {
        flattened_conjuncts(&branch.predicate)
            .iter()
            .all(|conjunct| matches!(conjunct, Predicate::True) || conjuncts.contains(conjunct))
    });
    let selected_kind = match (selected.next(), selected.next()) {
        (Some(branch), None) => Some(branch.kind.clone()),
        _ => None,
    };
    let mut resolved = contract_use.clone();
    if let Some(resource) = resolved.resource.as_mut() {
        if let Some(kind) = selected_kind {
            resource.kind = kind;
            resource.kind_candidates.clear();
        }
        resource.kind_branches.clear();
    }
    Some(resolved)
}

/// The leaf conjuncts of a predicate: nested `And`s flatten, everything
/// else (including `Not`/`Or`) is one leaf.
fn flattened_conjuncts(predicate: &Predicate) -> Vec<&Predicate> {
    match predicate {
        Predicate::And(items) => items.iter().flat_map(flattened_conjuncts).collect(),
        other => vec![other],
    }
}

fn record_contract_use_conjunction(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    predicates: &[Predicate],
    range_modes: &crate::range_modes::RangeModes,
) {
    let kind_resolved = kind_branch_resolved_use(contract_use, predicates);
    let contract_use = kind_resolved.as_ref().unwrap_or(contract_use);
    // An approximate ambient conjunct means the row's exact firing states
    // are unknown, so its NARROWING evidence (sink typing, provider uses,
    // nullability) must abstain — but the conjunction's widen-only evidence
    // survives: a positive self-type dispatch arm under an undecodable
    // liveness header still proves the chart handles that type
    // (cluster-autoscaler's `kindIs "string"` expanderPriorities arm under
    // an `include`-bearing condition).
    let has_approximate = predicates.iter().any(Predicate::contains_approximation);
    if ranged_member_parent(&contract_use.source_expr).is_some_and(|parent| {
        !range_modes.mode(parent).direct
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                matches!(
                    predicate,
                    Predicate::Guard(Guard::Range { path })
                        if !range_modes.mode(path).direct
                )
            })
    }) {
        // A `x.*` row is structural member evidence only when some direct
        // range established that identity. Derived recursive walkers may
        // carry range-shaped influence without proving `x` is a collection.
        return;
    }
    let lowerable_guards =
        lowerable_conditional_guard_set(contract_use, predicates).or_else(|| {
            (contract_use.path.0.is_empty() && range_modes.mode(&contract_use.source_expr).direct)
                .then(|| lowerable_range_outer_guards(&contract_use.source_expr, predicates))
                .flatten()
        });
    // A merge layer's sink typing rides its OWN truthiness: whichever layer
    // made the `with` gate truthy, this layer's keys render exactly when
    // the layer itself is (its falsy states contribute nothing).
    let merge_layered = contract_use
        .merge_layers
        .as_ref()
        .filter(|merge| merge.layers.get(merge.position) == Some(&contract_use.source_expr));
    let lowerable_guards = if merge_layered.is_some() {
        Some(vec![ConditionalGuard::Truthy {
            path: contract_use.source_expr.clone(),
        }])
    } else {
        lowerable_guards
    };
    if ranged_member_parent(&contract_use.source_expr).is_some()
        && predicates
            .iter()
            .any(|predicate| !matches!(predicate, Predicate::Guard(Guard::Range { .. })))
        && lowerable_guards.is_none()
    {
        // A member-local wildcard guard cannot be encoded at the document
        // root. Abstain instead of leaking its item/value shape into members
        // where the branch never runs. Exact root guards remain overlays.
        return;
    }
    let has_source = !contract_use.source_expr.trim().is_empty();
    let path_is_empty = contract_use.path.0.is_empty();
    let range_guard_paths = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    // A `x.*` member row fires BY `range x`: that Range predicate is the
    // row's own iteration, not a foreign condition gating it. It is NOT a
    // null-tolerance signal though — iteration does not skip null members.
    let member_range_parent = contract_use.source_expr.strip_suffix(".*");
    let self_range_guarded = range_guard_paths.contains(contract_use.source_expr.as_str());
    let has_matching_self_guard = predicates
        .iter()
        .any(|predicate| predicate_is_self_guarding(predicate, &contract_use.source_expr));
    let pathless_self_default_guarded = path_is_empty
        && predicates.iter().any(|predicate| {
            matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr)
        });

    // A row dispatched by a type test on its own path belongs to a
    // type-switch (`if eq (typeOf .Values.x) "string" … else …`): values of
    // unmatched types render nothing, which is valid, so the dispatch arms
    // must not close the path to the union of their tested types, and an
    // arm's sink typing holds only for its tested type, never path-wide.
    let type_dispatched = has_source
        && predicates
            .iter()
            .any(|predicate| predicate_tests_source_type(predicate, &contract_use.source_expr));
    // The catch-all COMPLEMENT arm of a type dispatch (every self-type
    // test negated: a plain `else`) executes for every unmatched type, so
    // its structural placement types that whole domain — scoped to the
    // branch key the partition rides. Arms with a positive self-type test
    // stay suppressed: their sink typing joins the union through the
    // dispatch guard predicates instead, and a `tpl`-style string arm's
    // placement says nothing about the raw value.
    let complement_dispatched = type_dispatched
        && predicates.iter().all(|predicate| {
            !predicate_tests_source_type(predicate, &contract_use.source_expr)
                || matches!(
                    predicate,
                    Predicate::Not(inner)
                        if predicate_tests_source_type(inner, &contract_use.source_expr)
                )
        });

    // The serialized-tolerance fact is itself widen-only — the use it
    // records never rejects an input, it only stops intent-grade channels
    // (declared defaults, fallback hints, standalone guard typing) from
    // narrowing — so it survives an approximate conjunct: a `typeOf`
    // dispatch or `toYaml` arm under an undecodable liveness header still
    // proves values outside its arm render nothing there (loki's
    // `kindIs "bool"` hostUsers behind a Capabilities semver check,
    // vault's server affinity helper behind `ne .mode "dev"`). Real
    // contracts from other rows keep applying.
    if has_source && has_approximate {
        let serialized_tolerant = matches!(contract_use.kind, ValueKind::Serialized)
            || (contract_use.kind == ValueKind::PartialScalar && !path_is_empty)
            || type_dispatched;
        if serialized_tolerant {
            let acc = path_accumulator(paths, &contract_use.source_expr);
            acc.referenced = true;
            acc.facts.facts.used_as_serialized = true;
        }
    }
    if has_source && !has_approximate {
        let mut facts = ContractValuePathFacts {
            used_as_fragment: matches!(
                contract_use.kind,
                ValueKind::Fragment | ValueKind::YamlSerialized
            ),
            used_as_serialized: matches!(contract_use.kind, ValueKind::Serialized)
                || (contract_use.kind == ValueKind::PartialScalar && !path_is_empty)
                || type_dispatched,
            used_as_yaml_serialized: contract_use.kind == ValueKind::YamlSerialized,
            has_string_contract: contract_use.has_string_contract && !type_dispatched,
            used_as_pathless_fragment: matches!(
                contract_use.kind,
                ValueKind::Fragment | ValueKind::YamlSerialized
            ) && path_is_empty,
            is_partial_scalar_value_path: contract_use.kind == ValueKind::PartialScalar,
            is_nullable: !path_is_empty
                || self_range_guarded
                || matches!(
                    contract_use.kind,
                    ValueKind::Fragment | ValueKind::YamlSerialized
                )
                || pathless_self_default_guarded,
            ..ContractValuePathFacts::default()
        };
        if !path_is_empty {
            facts.record_render_use(self_range_guarded, Some(has_matching_self_guard));
            facts.has_unconditional_render_use = predicates.is_empty();
        }

        let positive_header = contract_use.kind == ValueKind::Scalar
            && path_is_empty
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                predicate_is_positive_header(predicate, &contract_use.source_expr)
            });
        // A serialized splice renders text the sink cannot type back onto
        // the input, so it contributes no metadata field kind either.
        let metadata_field_kind = if matches!(
            contract_use.kind,
            ValueKind::PartialScalar | ValueKind::Serialized
        ) || type_dispatched
        {
            None
        } else {
            metadata_field_kind_from_yaml_path(&contract_use.path.0)
        };
        let acc = path_accumulator(paths, &contract_use.source_expr);
        acc.requiredness.is_positive_header |= positive_header;
        // An UNCONDITIONAL string-contract row types the path itself;
        // a conditional one types only its own overlay branch (the branch
        // facts carry it there). A member row's own iteration does not
        // scope it: `tpl` over each ranged member types every member.
        let own_iteration_only = predicates.iter().all(|predicate| {
            member_range_parent.is_some_and(|parent| {
                matches!(
                    predicate,
                    Predicate::Guard(Guard::Range { path }) if path == parent
                )
            })
        });
        if contract_use.has_string_contract && own_iteration_only {
            acc.type_hints.insert("string".to_string());
        }
        // A positive dispatch arm normally abstains from provider typing
        // (a transformed scalar arm observes derived text), but an arm that
        // splices the VALUE structurally — a fragment under its own lowered
        // structured-type partition — observes the value itself, so the
        // provider projection rides the overlay scoped to the tested type.
        // Scalar-type partitions (a `tpl` string arm rendered as a fragment)
        // still abstain: their provider projection would be vacuous under
        // the partition and only bloats the encoding.
        let structural_dispatch_arm = type_dispatched
            && matches!(
                contract_use.kind,
                ValueKind::Fragment | ValueKind::YamlSerialized
            )
            && lowerable_guards.as_ref().is_some_and(|guards| {
                guards.iter().any(|guard| {
                    matches!(
                        guard,
                        ConditionalGuard::TypeIs { schema_type, .. }
                            if schema_type == "object" || schema_type == "array"
                    )
                })
            });
        let provider_use = (!type_dispatched || complement_dispatched || structural_dispatch_arm)
            .then(|| provider_schema_use(contract_use, self_range_guarded))
            .flatten();
        // A merge layer's provider typing is synthesized by the generator
        // from the path-level use: the preferred layer becomes a whole-
        // payload arm under its own truthiness, and a SHADOWED layer
        // becomes per-key arms scoped to keys the earlier layers lack.
        // Neither shape fits the branch/base lanes here.
        let (branch_provider_use, merge_layer_provider_use) = if merge_layered.is_some() {
            (None, provider_use)
        } else {
            (provider_use, None)
        };
        if let Some(layered) = merge_layer_provider_use {
            acc.facts.record_provider_schema_use(layered);
        }
        // A structural dispatch arm splits its facts: the PATH keeps only the
        // dispatch tolerance (the arm must not hard-type the whole domain its
        // partition merely selects from), while the BRANCH keeps the real
        // structural use without the tolerance (which would dissolve the
        // arm's own provider typing into the serialized preimage).
        let (path_facts, branch_facts) = if structural_dispatch_arm {
            let mut path_facts = facts;
            path_facts.used_as_fragment = false;
            path_facts.used_as_yaml_serialized = false;
            path_facts.used_as_pathless_fragment = false;
            let mut branch_facts = facts;
            branch_facts.used_as_serialized = false;
            (path_facts, branch_facts)
        } else {
            (facts, facts)
        };
        acc.record_source_use(
            SourceUseFactSplit {
                path: path_facts,
                branch: branch_facts,
            },
            path_is_empty || has_matching_self_guard,
            lowerable_guards,
            branch_provider_use,
            metadata_field_kind,
            predicates.iter().all(|predicate| {
                predicate.value_paths().iter().all(|path| {
                    path == &contract_use.source_expr
                        || contract_use.source_expr.strip_suffix(".*") == Some(path)
                })
            }),
        );
    }

    for path in predicates
        .iter()
        .flat_map(Predicate::conditionally_optional_paths)
    {
        path_accumulator(paths, &path)
            .requiredness
            .is_conditionally_optional = true;
    }
    for path in predicates.iter().filter_map(|predicate| match predicate {
        Predicate::Guard(Guard::Default { path }) => Some(path),
        _ => None,
    }) {
        path_accumulator(paths, path)
            .requiredness
            .has_default_fallback = true;
    }
    if has_source {
        for predicate in conditional_guard_predicates(predicates) {
            for path in predicate.value_paths() {
                let acc = path_accumulator(paths, &path);
                if !acc.guard_predicates.contains(&predicate) {
                    acc.guard_predicates.push(predicate.clone());
                }
            }
        }
    }
    for path in predicates.iter().flat_map(Predicate::value_paths) {
        if has_source && path == contract_use.source_expr.as_str() {
            continue;
        }
        let acc = path_accumulator(paths, &path);
        acc.referenced |= has_source;
        if !path_is_empty {
            let mut facts = ContractValuePathFacts::default();
            facts.record_render_use(range_guard_paths.contains(&path), None);
            acc.facts.record_facts(facts);
        }
    }
    if has_source && !has_approximate {
        for path in range_guard_paths {
            let direct = range_modes.mode(&path).direct;
            let outer_guards = direct
                .then(|| lowerable_range_outer_guards(&path, predicates))
                .flatten();
            let unconditional = outer_guards.as_ref().is_some_and(Vec::is_empty);
            // No render-use flags ride along here, so record_facts leaves the
            // accumulator's self-guarded default untouched.
            let facts = ContractValuePathFacts {
                is_ranged_source: direct && unconditional,
                is_direct_ranged_source: direct && unconditional,
                // HOW the chart iterates the path (two-variable, JSON-decoded)
                // is a property of the range site itself, not of the guards
                // around it: a conditional `range $k, $v` still proves the
                // member keys are user data wherever it runs.
                has_destructured_range_use: direct && range_modes.mode(&path).destructured,
                has_json_decoded_range_use: direct && range_modes.mode(&path).json_decoded,
                is_nullable: true,
                ..ContractValuePathFacts::default()
            };
            path_accumulator(paths, &path).facts.record_facts(facts);
            if direct {
                if let Some(parent) = path.strip_suffix(".*")
                    && !path_contains_wildcard(parent)
                {
                    // A nested range over a MEMBER identity (`range
                    // $values` where `$values` holds each member of a
                    // directly ranged map): every member must itself be
                    // rangeable, or the inner range aborts rendering.
                    let parent_mode = range_modes.mode(parent);
                    let path_mode = range_modes.mode(&path);
                    record_member_range_requirement(
                        paths,
                        parent,
                        predicates,
                        !parent_mode.destructured && !parent_mode.json_decoded,
                        !path_mode.destructured && !path_mode.json_decoded,
                    );
                } else if let Some(guards) = outer_guards {
                    // Empty guards mean the range provably runs in EVERY
                    // state (complementary branches across files simplify
                    // to an unconditional row — jenkins ranges its
                    // configScripts under both sidecar-reload states), so
                    // the iterable requirement binds unconditionally.
                    let mode = range_modes.mode(&path);
                    record_guarded_range_requirement(
                        paths,
                        &path,
                        guards,
                        mode.destructured,
                        mode.json_decoded,
                    );
                }
            }
        }
    }
}

/// A `range` read under foreign conditions bounds an ITERABLE requirement
/// to those conditions: Go's `range` iterates collections and skips nil but
/// fails template rendering on scalars, so inside the guarded branch the
/// ranged path must be a collection. The branch stays render-free; overlay
/// lowering recognizes that shape and emits the iterable domain.
fn lowerable_range_outer_guards(
    ranged_path: &str,
    predicates: &[Predicate],
) -> Option<Vec<ConditionalGuard>> {
    let mut guards = Vec::new();
    for predicate in predicates {
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path }) if path == ranged_path
        ) || matches!(
            predicate,
            Predicate::Guard(Guard::Range { path })
                if range_guard_is_iteration_ancestor(ranged_path, path)
        ) || predicate_is_structural_ancestor_guard(predicate, ranged_path)
        {
            continue;
        }
        // `Default` marks a fallback use of the ranged value; it is not a
        // control condition. Every other predicate is load-bearing here,
        // including self-truthiness and `with`: false scalars skip the range
        // entirely and must not receive its live-branch collection contract.
        if matches!(
            predicate,
            Predicate::Guard(Guard::Default { path }) if path == ranged_path
        ) {
            continue;
        }
        let guard = predicate_to_guard(predicate, None)?;
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return None;
        }
        guards.push(guard);
    }
    guards.sort();
    guards.dedup();
    Some(guards)
}

fn record_guarded_range_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    ranged_path: &str,
    outer_guards: Vec<ConditionalGuard>,
    destructured: bool,
    json_decoded: bool,
) {
    // An unconditional row's range facts already live on the base
    // accumulator; only a real guard set opens its own overlay branch.
    if !outer_guards.is_empty() {
        let branch = path_accumulator(paths, ranged_path)
            .conditional_overlay_branches
            .entry(outer_guards.clone())
            .or_default();
        branch.facts.is_nullable = true;
        branch.record_facts(ContractValuePathFacts {
            is_ranged_source: true,
            has_destructured_range_use: destructured,
            has_json_decoded_range_use: json_decoded,
            is_nullable: true,
            ..ContractValuePathFacts::default()
        });
    }
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::Value,
        requirements: vec![FailValueRequirement::Iterable {
            allow_integer: !destructured && !json_decoded,
        }],
    };
    let acc = path_accumulator(paths, ranged_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

fn remove_redundant_approximate_conditions(conjunction: &[Predicate]) -> Vec<Predicate> {
    let exact = conjunction
        .iter()
        .filter(|predicate| !predicate.contains_approximation())
        .collect::<BTreeSet<_>>();
    conjunction
        .iter()
        .filter(|predicate| {
            if !predicate.contains_approximation() {
                return true;
            }
            !matches!(predicate, Predicate::Or(alternatives) if alternatives.iter().any(|alternative| {
                match alternative {
                    Predicate::And(items) => items.iter().all(|item| exact.contains(item)),
                    item => exact.contains(item),
                }
            }))
        })
        .cloned()
        .collect()
}

/// Lower one `fail` conjunction into a path requirement: rendering aborts
/// whenever the conjunction holds, so valid inputs must falsify the failing
/// TEST wherever the OUTER guards hold. Conjunctions whose test cannot be
/// negated structurally are skipped (truthy-fallback predicates approximate
/// undecodable conditions and must never be negated).
fn record_fail_conjunction(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    terminal_clauses: &mut Vec<Vec<ConditionalGuard>>,
    capture: &crate::eval_effect::FailCapture,
    range_modes: &crate::range_modes::RangeModes,
) {
    if let crate::eval_effect::CaptureKind::RangeKeyStrings {
        paths: range_key_string_paths,
    } = &capture.kind
    {
        record_range_key_string_requirements(paths, capture, range_key_string_paths, range_modes);
        return;
    }
    if let crate::eval_effect::CaptureKind::CollectionItems {
        paths: collection_paths,
        schema_type,
    } = &capture.kind
    {
        record_collection_item_requirements(paths, capture, collection_paths, schema_type);
        return;
    }
    if let crate::eval_effect::CaptureKind::IndexAccess { path, index } = &capture.kind {
        record_index_access_requirement(paths, capture, path, *index);
        return;
    }
    if let crate::eval_effect::CaptureKind::SplitIndexAccess {
        paths: source_paths,
        separator,
        index,
        total_text_preimage,
    } = &capture.kind
    {
        record_split_index_access_requirement(
            paths,
            capture,
            source_paths,
            separator,
            *index,
            *total_text_preimage,
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::ValueType { path, schema_type } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::SchemaType(schema_type.clone()),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::ComparableKind { path, schema_type } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::ComparableKind(schema_type.clone()),
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::ValuePattern {
        path,
        pattern,
        templated,
    } = &capture.kind
    {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::MatchesPattern {
                pattern: pattern.clone(),
                templated: *templated,
            },
        );
        return;
    }
    if let crate::eval_effect::CaptureKind::QuotedSerialization { path, style } = &capture.kind {
        record_value_requirement_capture(
            paths,
            capture,
            path,
            FailValueRequirement::QuotedSerializationSafe { style: *style },
        );
        return;
    }
    // An approximate enclosing condition abstains unless it admits a sound
    // positive strengthening (it can only ever be an OUTER guard — the
    // requirement extraction below never negates one), and a `$local` name
    // leaking into predicate paths means the condition lowering lost the
    // real subject: both make lowering unsound for the whole capture.
    let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
    if conjunction.iter().any(|predicate| {
        predicate.contains_approximation() && fail_outer_guard(predicate).is_none()
    }) {
        return;
    }
    if conjunction
        .iter()
        .flat_map(Predicate::value_paths)
        .any(|path| path.starts_with('$'))
    {
        return;
    }
    if record_range_key_prefix_requirement(paths, &capture.kind, &conjunction) {
        return;
    }
    if let crate::eval_effect::CaptureKind::MemberAccess { handled_kinds } = &capture.kind {
        record_member_access_capture(paths, capture, handled_kinds, range_modes);
        return;
    }
    // A multi-path `with` header (`with (coalesce a b)`) contributes its
    // EXACT disjunction plus one `With` row marker per path; the markers
    // annotate rows, and reading them as conjuncts would narrow the
    // failure to "every path set" when the disjunction alone is the
    // condition. Drop a marker whenever a disjunction over its path is
    // present.
    let or_covered: BTreeSet<&str> = capture
        .conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Or(items) => Some(items.iter().filter_map(|item| match item {
                Predicate::Guard(Guard::Truthy { path } | Guard::With { path }) => {
                    Some(path.as_str())
                }
                _ => None,
            })),
            _ => None,
        })
        .flatten()
        .collect();
    let conjunction: Vec<Predicate> = conjunction
        .iter()
        .filter(|predicate| {
            !matches!(
                predicate,
                Predicate::Guard(Guard::With { path }) if or_covered.contains(path.as_str())
            )
        })
        .cloned()
        .collect();
    let conjunction = &conjunction;
    let ranged: Vec<&str> = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let test_candidate_paths = conjunction
        .iter()
        .filter(|predicate| predicate_is_negatable_test(predicate))
        .flat_map(Predicate::value_paths)
        .collect::<BTreeSet<_>>();
    let ranged = ranged
        .iter()
        .copied()
        .filter(|path| range_modes.mode(path).direct || capture.ranged.mode(path).direct)
        .filter(|path| {
            let member = format!("{path}.*");
            test_candidate_paths.iter().any(|candidate| {
                candidate == &member
                    || helm_schema_core::values_path_is_descendant(candidate, &member)
            })
        })
        .max_by_key(|path| helm_schema_core::split_value_path(path).len());
    let member_scope = ranged.map(|path| format!("{path}.*"));

    let mut outer_guards = Vec::new();
    let mut member_tests: Vec<&Predicate> = Vec::new();
    let mut requirements = Vec::new();
    let mut test_paths: BTreeSet<String> = BTreeSet::new();
    for predicate in conjunction {
        if let Predicate::Guard(Guard::Range { path }) = predicate {
            if ranged == Some(path.as_str()) {
                continue;
            }
            // An iteration conjunct outside the member test: the body
            // executed, so a DIRECTLY ranged collection is Helm-truthy
            // (a truthy non-collection aborts the range and never reaches
            // a render-valid document). Indirect ranges lose that
            // implication and abstain.
            if range_modes.mode(path).direct || capture.ranged.mode(path).direct {
                outer_guards.push(ConditionalGuard::Truthy { path: path.clone() });
                continue;
            }
            return;
        }
        let paths_of = predicate.value_paths();
        // A conjunct is part of the failing TEST when it scopes to the
        // ranged member (or, without a range, to a single path) AND its
        // negation states an enforceable requirement; everything else is
        // an outer condition of the arm.
        if let Some(scope) = &member_scope {
            if !paths_of.is_empty()
                && paths_of
                    .iter()
                    .all(|path| path == scope || path.starts_with(&format!("{scope}.")))
            {
                member_tests.push(predicate);
                continue;
            }
        } else if paths_of.len() == 1 && predicate_is_negatable_test(predicate) {
            let path = paths_of.iter().next().cloned().unwrap_or_default();
            if let Some(required) =
                requirements_from_negation(predicate, &path).filter(|required| !required.is_empty())
            {
                requirements.extend(required);
                test_paths.insert(path);
                continue;
            }
        }
        let Some(guard) = fail_outer_guard(predicate) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer_guards.push(guard);
    }
    // Member tests negate against the MEMBER scope first — `required`-style
    // conjuncts name fields relative to the member — and fall back to the
    // single field path they all share (`clusters.*.name` spliced into a
    // quoted flow item), where the implication targets the members' field.
    // A member test that fits neither convention poisons the capture: the
    // requirement would be missing a dimension of the real condition.
    let mut member_field: Option<Vec<String>> = None;
    if let Some(scope) = &member_scope
        && !member_tests.is_empty()
    {
        let requirements_at = |at: &str| -> Option<Vec<FailValueRequirement>> {
            member_tests
                .iter()
                .map(|predicate| {
                    requirements_from_negation(predicate, at)
                        .filter(|required| !required.is_empty())
                })
                .collect::<Option<Vec<_>>>()
                .map(|nested| nested.into_iter().flatten().collect())
        };
        if let Some(required) = requirements_at(scope) {
            requirements.extend(required);
            test_paths.insert(scope.clone());
        } else {
            let field_path = {
                let mut paths: BTreeSet<String> = member_tests
                    .iter()
                    .flat_map(|predicate| predicate.value_paths())
                    .collect();
                match paths.pop_first() {
                    Some(path) if paths.is_empty() && !path[scope.len()..].contains('*') => {
                        Some(path)
                    }
                    _ => None,
                }
            };
            let Some(field_path) = field_path.filter(|path| path != scope) else {
                return;
            };
            let Some(required) = requirements_at(&field_path) else {
                return;
            };
            member_field = Some(helm_schema_core::split_value_path(
                &field_path[scope.len() + 1..],
            ));
            requirements.extend(required);
            test_paths.insert(field_path);
        }
    }
    if requirements.is_empty() || test_paths.len() != 1 {
        // No single-path test survived. When the WHOLE conjunction lowers
        // to conditional guards — mutual exclusions and other cross-path
        // validator formulas — it becomes a document-level terminal
        // clause: no valid values document may satisfy all of it. Ranged
        // captures have member semantics no root clause can express, and
        // an approximate enclosing condition would make the clause fire
        // too widely.
        if ranged.is_none()
            && !conjunction
                .iter()
                .any(|predicate| matches!(predicate, Predicate::Guard(Guard::Range { .. })))
            && !conjunction.is_empty()
        {
            let clause = conjunction
                .iter()
                .map(terminal_clause_guard)
                .collect::<Option<Vec<_>>>();
            if let Some(mut clause) = clause {
                clause.sort();
                clause.dedup();
                if !clause.is_empty() && !terminal_clauses.contains(&clause) {
                    terminal_clauses.push(clause);
                }
            }
        }
        return;
    }
    let target = match ranged {
        Some(path) => path.to_string(),
        None => {
            let Some(path) = test_paths.into_iter().next() else {
                return;
            };
            path
        }
    };
    requirements.sort();
    requirements.dedup();
    // A test whose requirements contradict (a type-dispatch arm's own
    // partition conjunct joins the test on the same path) can never fire;
    // its arm would encode as a tautology, so it is dropped as noise.
    let contradictory = requirements.iter().any(|requirement| {
        matches!(
            requirement,
            FailValueRequirement::SchemaType(schema_type)
                if requirements
                    .contains(&FailValueRequirement::NotSchemaType(schema_type.clone()))
        )
    });
    if contradictory {
        return;
    }
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ranged.map_or(ContractRequirementTarget::Value, |path| {
            let mode = range_modes.mode(path);
            let allow_integer = !mode.destructured && !mode.json_decoded;
            match member_field {
                Some(target_path) => ContractRequirementTarget::MembersAt {
                    target_path,
                    allow_integer,
                },
                None => ContractRequirementTarget::Members { allow_integer },
            }
        }),
        requirements,
    };
    let acc = path_accumulator(paths, &target);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

fn record_range_key_prefix_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    kind: &crate::eval_effect::CaptureKind,
    conjunction: &[Predicate],
) -> bool {
    if !matches!(kind, crate::eval_effect::CaptureKind::Fail) {
        return false;
    }
    let prefixes = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::RangeKeyPrefix { path, prefix }) => {
                Some((path.as_str(), prefix.as_str()))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(collection_path, prefix)] = prefixes.as_slice() else {
        return !prefixes.is_empty();
    };
    let member_scope = format!("{collection_path}.*");
    let has_matching_range = conjunction.iter().any(|predicate| {
        matches!(predicate, Predicate::Guard(Guard::Range { path }) if path == collection_path)
    });
    if !has_matching_range {
        return true;
    }

    let mut outer_guards = Vec::new();
    let mut requirements = Vec::new();
    for predicate in conjunction {
        match predicate {
            Predicate::Guard(Guard::RangeKeyPrefix {
                path,
                prefix: candidate,
            }) if path == collection_path && candidate == prefix => {}
            Predicate::Guard(Guard::Range { path }) if path == collection_path => {}
            _ if {
                let predicate_paths = predicate.value_paths();
                !predicate_paths.is_empty()
                    && predicate_paths.iter().all(|path| {
                        path == &member_scope
                            || helm_schema_core::values_path_is_descendant(path, &member_scope)
                    })
            } =>
            {
                let Some(mut required) = requirements_from_negation(predicate, &member_scope)
                else {
                    return true;
                };
                requirements.append(&mut required);
            }
            _ => {
                let Some(guard) = fail_outer_guard(predicate) else {
                    return true;
                };
                if guard
                    .value_paths()
                    .iter()
                    .any(|path| path_contains_wildcard(path))
                {
                    return true;
                }
                outer_guards.push(guard);
            }
        }
    }
    if requirements.is_empty() {
        return true;
    }
    outer_guards.sort();
    outer_guards.dedup();
    requirements.sort();
    requirements.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::MembersMatchingPrefix {
            prefix: (*prefix).to_string(),
        },
        requirements,
    };
    let acc = path_accumulator(paths, collection_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
    true
}

fn capture_outer_guards(
    capture: &crate::eval_effect::FailCapture,
) -> Option<Vec<ConditionalGuard>> {
    let conjunction = remove_redundant_approximate_conditions(&capture.conjunction);
    // A key-equality conjunct subsumes its companion iteration conjunct:
    // the has-key lowering already implies the range reaches that member.
    let key_equals_ranges: BTreeSet<&str> = conjunction
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::RangeKeyEquals { path, .. }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let mut guards = conjunction
        .iter()
        .filter(|predicate| {
            !matches!(
                predicate,
                Predicate::Guard(Guard::Range { path }) if key_equals_ranges.contains(path.as_str())
            )
        })
        .map(|predicate| match predicate {
            // An iteration conjunct: the body executed, so a DIRECTLY
            // ranged collection is Helm-truthy (a truthy non-collection
            // aborts the range and never renders). Indirect ranges lose
            // that implication and abstain.
            Predicate::Guard(Guard::Range { path }) => capture
                .ranged
                .mode(path)
                .direct
                .then(|| ConditionalGuard::Truthy { path: path.clone() }),
            predicate => fail_outer_guard(predicate),
        })
        .collect::<Option<Vec<_>>>()?;
    if guards
        .iter()
        .flat_map(ConditionalGuard::value_paths)
        .any(|path| path_contains_wildcard(&path))
    {
        return None;
    }
    guards.sort();
    guards.dedup();
    Some(guards)
}

/// ConditionalGuard for one OUTER conjunct of a FAIL implication.
///
/// Fail polarity is positive-only: the emitted guard may hold LESS often
/// than the real condition — the arm then rejects fewer inputs — but never
/// more. That admits bounded strengthenings an exact row guard cannot use:
/// a positive approximate conjunct lowers through its recognized sound
/// subset, and a NEGATED conjunction lowers as the negation of its
/// decodable conjuncts (dropping conjuncts weakens a conjunction, so
/// negating the remainder fires less often than negating all of it —
/// cilium's `else if` log-level arm behind a `has … (splitList …)` chain).
fn fail_outer_guard(predicate: &Predicate) -> Option<ConditionalGuard> {
    if !predicate.contains_approximation() {
        return predicate_to_guard(predicate, None);
    }
    match predicate {
        Predicate::Approximate { sound_subset, .. } if !sound_subset.is_empty() => {
            let guards = sound_subset
                .iter()
                .map(|guard| guard_to_conditional_guard(guard, None))
                .collect::<Option<Vec<_>>>()?;
            match guards.as_slice() {
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        Predicate::Not(inner) => {
            let Predicate::And(items) = inner.as_ref() else {
                return None;
            };
            let mut decodable: Vec<ConditionalGuard> = items
                .iter()
                .filter(|item| !item.contains_approximation())
                .filter_map(|item| predicate_to_guard(item, None))
                .collect();
            decodable.sort();
            decodable.dedup();
            let inner = match decodable.as_slice() {
                [] => return None,
                [guard] => guard.clone(),
                _ => ConditionalGuard::AllOf(decodable),
            };
            Some(ConditionalGuard::Not(Box::new(inner)))
        }
        // A disjunction lowers arm-by-arm: each strengthened arm implies
        // its real arm, so their disjunction implies the real disjunction
        // (jenkins' `or (lt $replicas 0) (gt $replicas 1)` domain check).
        Predicate::Or(items) => {
            let mut guards = items
                .iter()
                .map(fail_outer_guard)
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AnyOf(guards)),
            }
        }
        _ => None,
    }
}

fn record_value_requirement_capture(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
    requirement: FailValueRequirement,
) {
    if path.trim().is_empty() {
        return;
    }
    // `A.*.field` names one field of EVERY ranged member of `A`: the
    // requirement lowers per member at that relative path (prometheus's
    // `tpl $remoteWrite.url` over `server.remoteWrite.*.url`). Deeper or
    // repeated wildcards abstain below as before.
    let member_field_split = path.split_once(".*.").filter(|(collection, suffix)| {
        !collection.contains('*') && !suffix.is_empty() && !suffix.contains('*')
    });
    if let Some((collection_path, member_suffix)) = member_field_split {
        let key_equals_ranges: BTreeSet<&str> = capture
            .conjunction
            .iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::RangeKeyEquals { path, .. }) => Some(path.as_str()),
                _ => None,
            })
            .collect();
        let mut outer_guards = Vec::new();
        for predicate in &capture.conjunction {
            match predicate {
                Predicate::Guard(Guard::Range { path })
                    if path == collection_path || key_equals_ranges.contains(path.as_str()) => {}
                _ => {
                    let Some(guard) = predicate_to_guard(predicate, None) else {
                        return;
                    };
                    if guard
                        .value_paths()
                        .iter()
                        .any(|path| path_contains_wildcard(path))
                    {
                        return;
                    }
                    outer_guards.push(guard);
                }
            }
        }
        outer_guards.sort();
        outer_guards.dedup();
        let allow_integer = {
            let mode = capture.ranged.mode(collection_path);
            mode.direct && !mode.destructured && !mode.json_decoded
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::MembersAt {
                target_path: helm_schema_core::split_value_path(member_suffix),
                allow_integer,
            },
            requirements: vec![requirement],
        };
        let acc = path_accumulator(paths, collection_path);
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
        return;
    }
    let (target_path, target, outer_guards) = if let Some(collection_path) = path.strip_suffix(".*")
    {
        let mut outer_guards = Vec::new();
        let mut prefix = None;
        for predicate in &capture.conjunction {
            match predicate {
                Predicate::Guard(Guard::Range { path }) if path == collection_path => {}
                Predicate::Guard(Guard::RangeKeyPrefix {
                    path,
                    prefix: candidate,
                }) if path == collection_path => {
                    if prefix.replace(candidate.clone()).is_some() {
                        return;
                    }
                }
                _ => {
                    let Some(guard) = predicate_to_guard(predicate, None) else {
                        return;
                    };
                    if guard
                        .value_paths()
                        .iter()
                        .any(|path| path_contains_wildcard(path))
                    {
                        return;
                    }
                    outer_guards.push(guard);
                }
            }
        }
        outer_guards.sort();
        outer_guards.dedup();
        let allow_integer = {
            let mode = capture.ranged.mode(collection_path);
            mode.direct && !mode.destructured && !mode.json_decoded
        };
        let target = prefix.map_or(
            ContractRequirementTarget::Members { allow_integer },
            |prefix| ContractRequirementTarget::MembersMatchingPrefix { prefix },
        );
        (collection_path, target, outer_guards)
    } else {
        if path_contains_wildcard(path) {
            return;
        }
        let Some(outer_guards) = capture_outer_guards(capture) else {
            return;
        };
        (path, ContractRequirementTarget::Value, outer_guards)
    };
    let implication = ContractFailImplication {
        outer_guards,
        target,
        requirements: vec![requirement],
    };
    let acc = path_accumulator(paths, target_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

fn record_collection_item_requirements(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    collection_paths: &BTreeSet<String>,
    schema_type: &str,
) {
    let Some(outer_guards) = capture_outer_guards(capture) else {
        return;
    };
    for path in collection_paths {
        if path_contains_wildcard(path) {
            continue;
        }
        let implication = ContractFailImplication {
            outer_guards: outer_guards.clone(),
            target: ContractRequirementTarget::Members {
                allow_integer: false,
            },
            requirements: vec![FailValueRequirement::SchemaType(schema_type.to_string())],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

fn record_index_access_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
    index: usize,
) {
    if path.trim().is_empty() || path_contains_wildcard(path) {
        return;
    }
    let Some(outer_guards) = capture_outer_guards(capture) else {
        return;
    };
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::Value,
        requirements: vec![FailValueRequirement::IndexableAt(index)],
    };
    let acc = path_accumulator(paths, path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

fn record_split_index_access_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    source_paths: &BTreeSet<String>,
    separator: &str,
    index: usize,
    allow_non_string: bool,
) {
    if index == 0 || separator.is_empty() {
        return;
    }
    let outer_guards = capture_outer_guards(capture);
    for path in source_paths {
        if path.trim().is_empty() {
            continue;
        }
        if path_contains_wildcard(path) {
            record_member_relative_split_requirement(
                paths,
                capture,
                path,
                separator,
                index,
                allow_non_string,
            );
            continue;
        }
        let Some(outer_guards) = outer_guards.clone() else {
            continue;
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Value,
            requirements: vec![FailValueRequirement::SplitSegmentsAtLeast {
                separator: separator.to_string(),
                segments: index + 1,
                allow_non_string,
            }],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

fn record_member_relative_split_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    source_path: &str,
    separator: &str,
    index: usize,
    allow_non_string: bool,
) {
    let segments = helm_schema_core::split_value_path(source_path);
    let Some(member_index) = segments.iter().rposition(|segment| segment == "*") else {
        return;
    };
    if member_index == 0 || member_index + 1 >= segments.len() {
        return;
    }
    let collection_path = helm_schema_core::join_value_path(segments[..member_index].to_vec());
    let member_scope = helm_schema_core::join_value_path(segments[..=member_index].to_vec());
    let target_path = segments[(member_index + 1)..].to_vec();
    let mut member_guards = Vec::new();
    let mut outer_guards = Vec::new();

    for predicate in &capture.conjunction {
        if matches!(predicate, Predicate::Guard(Guard::Range { path })
            if path == &collection_path
                || helm_schema_core::values_path_is_descendant(&member_scope, path))
        {
            continue;
        }
        if let Predicate::Guard(Guard::Eq { path, value }) = predicate
            && let Some(relative) =
                helm_schema_core::split_value_path(path).strip_prefix(&segments[..=member_index])
            && !relative.is_empty()
            && !relative.iter().any(|segment| segment == "*")
        {
            member_guards.push((relative.to_vec(), value.clone()));
            continue;
        }
        let Some(guard) = predicate_to_guard(predicate, None) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer_guards.push(guard);
    }
    let [(guard_path, value)] = member_guards.as_slice() else {
        return;
    };
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::MembersWhereEquals {
            guard_path: guard_path.clone(),
            value: value.clone(),
            target_path,
        },
        requirements: vec![FailValueRequirement::SplitSegmentsAtLeast {
            separator: separator.to_string(),
            segments: index + 1,
            allow_non_string,
        }],
    };
    let acc = path_accumulator(paths, &collection_path);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

fn record_range_key_string_requirements(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    range_key_string_paths: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
) {
    if capture.contains_approximation() {
        return;
    }
    for path in range_key_string_paths {
        if path_contains_wildcard(path)
            || (!range_modes.mode(path).direct && !capture.ranged.mode(path).direct)
        {
            continue;
        }
        let Some(outer_guards) = lowerable_range_outer_guards(path, &capture.conjunction) else {
            continue;
        };
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Keys,
            requirements: vec![FailValueRequirement::SchemaType("string".to_string())],
        };
        let acc = path_accumulator(paths, path);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
    }
}

/// Whether a conjunct is a structurally negatable failing test. Positive
/// truthiness is excluded: the condition lowering falls back to truthy
/// approximations for conditions it cannot decode, and negating an
/// approximation would manufacture requirements the chart never stated.
fn predicate_is_negatable_test(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Not(inner) => !matches!(inner.as_ref(), Predicate::Guard(Guard::Range { .. })),
        Predicate::Guard(Guard::TypeIs { .. } | Guard::Absent { .. } | Guard::Eq { .. }) => true,
        Predicate::Or(items) => items.iter().all(predicate_is_negatable_test),
        _ => false,
    }
}

/// Requirements implied by the NEGATION of a failing test: the negation
/// must hold for the value at `scope` (a member scope `p.*` or the path
/// itself).
fn requirements_from_negation(
    predicate: &Predicate,
    scope: &str,
) -> Option<Vec<FailValueRequirement>> {
    match predicate {
        Predicate::Not(inner) => requirements_from_holding(inner, scope),
        // Negating a disjunction: every arm's negation must hold.
        Predicate::Or(items) => {
            let mut requirements = Vec::new();
            for item in items {
                requirements.append(&mut requirements_from_negation(item, scope)?);
            }
            Some(requirements)
        }
        Predicate::Guard(Guard::TypeIs { path, schema_type }) if path == scope => {
            Some(vec![FailValueRequirement::NotSchemaType(
                schema_type.clone(),
            )])
        }
        Predicate::Guard(Guard::Absent { path }) => {
            let member = path.strip_prefix(&format!("{scope}."))?;
            (!member.contains('.'))
                .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
        }
        // Dropping an equality arm weakens the requirement (fewer values
        // rejected), which is the safe direction: `required` emptiness
        // tests carry `= null` / `= ""` arms whose negations have no
        // member-schema spelling.
        Predicate::Guard(Guard::Eq { .. }) => Some(Vec::new()),
        _ => None,
    }
}

/// Requirements implied by a predicate HOLDING for the value at `scope`.
fn requirements_from_holding(
    predicate: &Predicate,
    scope: &str,
) -> Option<Vec<FailValueRequirement>> {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, schema_type }) if path == scope => {
            Some(vec![FailValueRequirement::SchemaType(schema_type.clone())])
        }
        // `regexMatch` type-asserts a string subject, so the negated fail
        // test (`if not (regexMatch …) fail`) requires a matching string.
        Predicate::Guard(Guard::MatchesPattern {
            path,
            pattern,
            templated,
        }) if path == scope => Some(vec![FailValueRequirement::MatchesPattern {
            pattern: pattern.clone(),
            templated: *templated,
        }]),
        // The tested value's own PRESENCE holding (the `hasKey` conjunct of
        // the fail path, `¬Absent(scope)` in the conjunction): an arm
        // encoded at the value's position is vacuous when the value is
        // absent, so presence needs no spelled requirement.
        Predicate::Guard(Guard::Absent { path }) if path == scope => Some(Vec::new()),
        // The tested value's own truthiness (`and $v (kindIs "string" $v)`):
        // the type requirement carries the substance; truthiness (rejecting
        // "" or 0) stays unmodeled as a bounded approximation.
        Predicate::Guard(Guard::Truthy { path }) if path == scope => Some(Vec::new()),
        Predicate::Guard(Guard::Truthy { path }) => {
            let member = path.strip_prefix(&format!("{scope}."))?;
            (!member.contains('.'))
                .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
        }
        Predicate::And(items) => {
            let mut requirements = Vec::new();
            for item in items {
                requirements.append(&mut requirements_from_holding(item, scope)?);
            }
            Some(requirements)
        }
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Guard(Guard::Absent { path }) => {
                let member = path.strip_prefix(&format!("{scope}."))?;
                (!member.contains('.'))
                    .then(|| vec![FailValueRequirement::HasMember(member.to_string())])
            }
            _ => requirements_from_negation(inner, scope),
        },
        _ => None,
    }
}

/// One member-access capture (`[outer…, ¬object(P)]`): extract
/// the accessed path and its lowerable outer guards for per-path folding.
/// Any conjunct the guard encoding cannot represent abstains this read
/// (the arm may only under-narrow), and approximate conditions abstain
/// like every fail negation.
fn record_member_access_capture(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    handled_kinds: &BTreeSet<String>,
    range_modes: &crate::range_modes::RangeModes,
) {
    if capture.contains_approximation() {
        return;
    }
    let mut target = None;
    for predicate in &capture.conjunction {
        if let Predicate::Not(inner) = predicate
            && let Predicate::Guard(Guard::TypeIs { path, schema_type }) = inner.as_ref()
            && schema_type == "object"
        {
            target = Some(path.clone());
        }
    }
    let Some(target) = target else {
        return;
    };
    if let Some(parent) = target.strip_suffix(".*")
        && !path_contains_wildcard(parent)
    {
        if !capture.ranged.mode(parent).direct {
            return;
        }
        let mut outer_guards = Vec::new();
        for predicate in &capture.conjunction {
            if matches!(
                predicate,
                Predicate::Guard(Guard::Range { path }) if path == parent
            ) || matches!(
                predicate,
                Predicate::Not(inner)
                    if matches!(
                        inner.as_ref(),
                        Predicate::Guard(Guard::TypeIs { path, schema_type })
                            if path == &target && schema_type == "object"
                    )
            ) {
                continue;
            }
            let Some(guard) = predicate_to_guard(predicate, None) else {
                return;
            };
            if guard
                .value_paths()
                .iter()
                .any(|path| path_contains_wildcard(path))
            {
                return;
            }
            outer_guards.push(guard);
        }
        outer_guards.sort();
        outer_guards.dedup();
        let implication = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Members {
                allow_integer: {
                    let mode = range_modes.mode(parent);
                    let capture_mode = capture.ranged.mode(parent);
                    !mode.destructured
                        && !capture_mode.destructured
                        && !mode.json_decoded
                        && !capture_mode.json_decoded
                },
            },
            requirements: vec![FailValueRequirement::SchemaType("object".to_string())],
        };
        let acc = path_accumulator(paths, parent);
        acc.referenced = true;
        if !acc.fail_implications.contains(&implication) {
            acc.fail_implications.push(implication);
        }
        return;
    }
    if path_contains_wildcard(&target) {
        return;
    }
    let mut outer = Vec::new();
    for predicate in &capture.conjunction {
        match predicate {
            Predicate::Not(inner)
                if matches!(
                    inner.as_ref(),
                    Predicate::Guard(Guard::TypeIs { path, schema_type })
                        if path == &target && schema_type == "object"
                ) =>
            {
                continue;
            }
            // A `with` gate enters only when its path is truthy: the same
            // condition the guard encoding can spell.
            Predicate::Guard(Guard::With { path }) if !path_contains_wildcard(path) => {
                outer.push(ConditionalGuard::Truthy { path: path.clone() });
                continue;
            }
            _ => {}
        }
        let Some(guard) = predicate_to_guard(predicate, None) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer.push(guard);
    }
    outer.sort();
    outer.dedup();
    path_accumulator(paths, &target)
        .member_access_guard_sets
        .entry(handled_kinds.iter().cloned().collect())
        .or_default()
        .insert(outer);
}

/// Fold each path's member-access guard sets into one fail implication.
/// Unconditional accesses bind unconditionally; guarded-only accesses key
/// the arm on the any-of of their guard sets. Fanout past the cap abstains
/// rather than exploding umbrella-chart schemas.
type MemberAccessGuardSets = BTreeMap<Vec<String>, BTreeSet<Vec<ConditionalGuard>>>;

fn record_member_access_implications(paths: &mut BTreeMap<String, ContractPathAccumulator>) {
    const MEMBER_ACCESS_GUARD_FANOUT: usize = 8;
    let pending: Vec<(String, MemberAccessGuardSets)> = paths
        .iter()
        .filter(|(path, acc)| {
            !acc.member_access_guard_sets.is_empty() && !path_contains_wildcard(path)
        })
        .map(|(path, acc)| (path.clone(), acc.member_access_guard_sets.clone()))
        .collect();
    for (path, grouped_guard_sets) in pending {
        let access_count: usize = grouped_guard_sets.values().map(BTreeSet::len).sum();
        if access_count > MEMBER_ACCESS_GUARD_FANOUT {
            continue;
        }
        let fold_guards = |guard_sets: BTreeSet<Vec<ConditionalGuard>>| {
            let mut outer_guards = Vec::new();
            if guard_sets.contains(&Vec::new()) {
                return outer_guards;
            }
            let mut arms: Vec<ConditionalGuard> = guard_sets
                .into_iter()
                .map(|mut set| {
                    if set.len() == 1 {
                        set.remove(0)
                    } else {
                        ConditionalGuard::AllOf(set)
                    }
                })
                .collect();
            if arms.len() == 1 {
                match arms.remove(0) {
                    ConditionalGuard::AllOf(set) => outer_guards.extend(set),
                    guard => outer_guards.push(guard),
                }
            } else {
                outer_guards.push(ConditionalGuard::AnyOf(arms));
            }
            outer_guards.sort();
            outer_guards.dedup();
            outer_guards
        };

        for (handled_kinds, guard_sets) in &grouped_guard_sets {
            let outer_guards = fold_guards(guard_sets.clone());
            let implication = ContractFailImplication {
                outer_guards,
                target: ContractRequirementTarget::Value,
                requirements: vec![FailValueRequirement::MemberHost {
                    handled_kinds: handled_kinds.clone(),
                }],
            };
            let acc = path_accumulator(paths, &path);
            if !acc.fail_implications.contains(&implication) {
                acc.fail_implications.push(implication);
            }
        }

        let all_guard_sets = grouped_guard_sets
            .into_values()
            .flatten()
            .collect::<BTreeSet<_>>();
        let outer_guards = fold_guards(all_guard_sets);
        let mut segments = helm_schema_core::split_value_path(&path);
        let Some(member) = segments.pop() else {
            continue;
        };
        if segments.is_empty() {
            continue;
        }
        let parent = helm_schema_core::join_value_path(segments);
        let presence = ContractFailImplication {
            outer_guards,
            target: ContractRequirementTarget::Value,
            requirements: vec![FailValueRequirement::HasMember(member)],
        };
        let acc = path_accumulator(paths, &parent);
        if !acc.fail_implications.contains(&presence) {
            acc.fail_implications.push(presence);
        }
    }
}

fn finish_schema_signals(
    mut paths: BTreeMap<String, ContractPathAccumulator>,
    mut terminal_clauses: Vec<Vec<ConditionalGuard>>,
) -> ContractSchemaSignals {
    record_member_access_implications(&mut paths);
    let referenced_paths = paths
        .iter()
        .filter_map(|(path, acc)| acc.referenced.then_some(path.clone()))
        .collect();
    let (
        paths_with_referenced_descendants,
        paths_with_item_descendants,
        paths_with_structured_item_descendants,
    ) = collect_paths_with_descendants(&referenced_paths);
    for path in &paths_with_referenced_descendants {
        path_accumulator(&mut paths, path);
    }
    // A member row carrying a runtime string contract (`tpl` over each
    // ranged member) closes the parent's integer-iteration lane: integer
    // counts iterate int members, which the contract rejects.
    let string_contract_item_parents: Vec<String> = paths
        .iter()
        .filter_map(|(path, acc)| {
            let parent = path.strip_suffix(".*")?;
            (acc.facts.facts.has_string_contract || acc.type_hints.contains("string"))
                .then(|| parent.to_string())
        })
        .collect();
    for parent in string_contract_item_parents {
        path_accumulator(&mut paths, &parent)
            .facts
            .facts
            .has_string_contract_items = true;
    }

    let schema_evidence_by_value_path = paths
        .into_iter()
        .map(|(value_path, acc)| {
            let has_descendants = paths_with_referenced_descendants.contains(&value_path);
            let has_item_descendants = paths_with_item_descendants.contains(&value_path);
            let has_structured_item_descendants =
                paths_with_structured_item_descendants.contains(&value_path);
            let evidence = acc.into_schema_evidence(
                value_path.clone(),
                has_descendants,
                has_item_descendants,
                has_structured_item_descendants,
            );
            (value_path, evidence)
        })
        .collect();
    terminal_clauses.sort();
    terminal_clauses.dedup();
    ContractSchemaSignals::new(schema_evidence_by_value_path, terminal_clauses)
}

fn path_accumulator<'a>(
    paths: &'a mut BTreeMap<String, ContractPathAccumulator>,
    path: &str,
) -> &'a mut ContractPathAccumulator {
    paths.entry(path.to_string()).or_default()
}

/// The path-level and branch-level halves of one recorded source use's
/// facts: a structural dispatch arm keeps different facts on each side
/// (the path keeps only the dispatch tolerance, the branch the real
/// structural use).
struct SourceUseFactSplit {
    path: ContractValuePathFacts,
    branch: ContractValuePathFacts,
}

impl ContractPathAccumulator {
    fn record_source_use(
        &mut self,
        facts: SourceUseFactSplit,
        source_null_tolerant: bool,
        lowerable_guards: Option<Vec<ConditionalGuard>>,
        provider_schema_use: Option<ProviderSchemaUse>,
        metadata_field_kind: Option<MetadataFieldKind>,
        self_scoped: bool,
    ) {
        self.referenced = true;
        if lowerable_guards.is_none() {
            self.saw_unsupported_overlay = true;
            return;
        }
        self.facts.record_facts(facts.path);
        let row_forms_overlay_branch = facts.path.has_render_use
            && !facts.path.has_unconditional_render_use
            && lowerable_guards
                .as_ref()
                .is_some_and(|guards| !guards.is_empty());
        if row_forms_overlay_branch {
            // A guarded row's sink typing rides its overlay branch; whether
            // it also binds at the path level is decided once the path's
            // serialized uses are known (see `into_schema_evidence`).
            if self_scoped && let Some(provider_use) = provider_schema_use.clone() {
                self.guarded_provider_schema_uses.push(provider_use);
            }
            if self_scoped && let Some(field_kind) = metadata_field_kind {
                self.guarded_metadata_field_kinds.insert(field_kind);
            }
        } else {
            if let Some(provider_use) = provider_schema_use.clone() {
                self.facts.record_provider_schema_use(provider_use);
            }
            self.facts.record_metadata_field_kind(metadata_field_kind);
        }
        if facts.path.has_render_use {
            if facts.path.has_unconditional_render_use
                || lowerable_guards.as_ref().is_some_and(Vec::is_empty)
            {
                // All predicates were the row's own structural range
                // ancestry, so its sink evidence applies to every emitted
                // member and belongs to the base rather than an empty arm.
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = lowerable_guards {
                let branch = self.conditional_overlay_branches.entry(guards).or_default();
                branch.facts.is_nullable = true;
                branch.record_nullable_observation(source_null_tolerant);
                branch.record_metadata_field_kind(metadata_field_kind);
                branch.record_facts(facts.branch);

                if let Some(provider_schema_use) = provider_schema_use {
                    branch.record_provider_schema_use(provider_schema_use);
                }
            }
        }
        self.facts.record_nullable_observation(source_null_tolerant);
    }

    fn into_schema_evidence(
        self,
        value_path: String,
        has_referenced_descendants: bool,
        has_item_descendants: bool,
        has_structured_item_descendants: bool,
    ) -> ContractPathSchemaEvidence {
        let facts = self.facts.facts(
            has_referenced_descendants,
            has_item_descendants,
            has_structured_item_descendants,
        );
        let ContractPathAccumulator {
            referenced,
            guard_predicates,
            facts: mut path_facts,
            requiredness,
            type_hints,
            guarded_type_hints,
            fallback_type_hints,
            guarded_fallback_type_hints,
            guarded_provider_schema_uses,
            guarded_metadata_field_kinds,
            conditional_overlay_branches,
            mut has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            mut fail_implications,
            member_access_guard_sets: _,
        } = self;
        // Only self-scoped rows enter these collections. An unsupported
        // foreign overlay still suppresses conditional overlays below, but
        // cannot invalidate an independently self-scoped sink contract.
        if !facts.used_as_serialized {
            for provider_use in guarded_provider_schema_uses {
                path_facts.record_provider_schema_use(provider_use);
            }
            path_facts
                .metadata_field_kinds
                .extend(guarded_metadata_field_kinds);
        }
        let overlay_type_hints: BTreeSet<String> = type_hints
            .iter()
            .chain(guarded_type_hints.iter())
            .chain(fallback_type_hints.iter())
            .chain(guarded_fallback_type_hints.iter())
            .cloned()
            .collect();
        // Fallback-grade hints are intent, not consumer contracts: a branch
        // whose renders ALL totally format (an embedded partial-scalar
        // splice like `--log-level={{ x | default "info" }}`) proves the
        // chart tolerates any input kind there, so those hints must not
        // close it (flux2's `--log-level=` arguments). Contract-grade hints
        // keep typing it.
        let contract_type_hints: BTreeSet<String> = type_hints
            .iter()
            .chain(guarded_type_hints.iter())
            .cloned()
            .collect();
        let mut evidence_groups: Vec<(PathSchemaFactsAccumulator, Vec<Vec<ConditionalGuard>>)> =
            Vec::new();
        for (guards, branch) in conditional_overlay_branches {
            if let Some((_, guard_sets)) = evidence_groups
                .iter_mut()
                .find(|(evidence, _)| evidence == &branch)
            {
                guard_sets.push(guards);
            } else {
                evidence_groups.push((branch, vec![guards]));
            }
        }
        let mut conditional_overlay_branches: BTreeMap<
            Vec<ConditionalGuard>,
            PathSchemaFactsAccumulator,
        > = BTreeMap::new();
        for (branch, guard_sets) in evidence_groups {
            for guards in
                helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(guard_sets)
            {
                if guards.is_empty() {
                    has_unconditional_overlay_peer = true;
                    continue;
                }
                match conditional_overlay_branches.entry(guards) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().merge_union(branch.clone());
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(branch.clone());
                    }
                }
            }
        }
        // Exact branches remain useful when a sibling guard is unlowerable.
        // The unknown sibling is represented by preserving the base domain;
        // discarding exact branches as well would lose structural facts that
        // are sound whenever their own guards hold.
        let conditional_overlays = conditional_overlay_branches
            .into_iter()
            .map(|(guards, branch)| {
                // A branch keyed on the path's own type partition hosts
                // only the hints compatible with that partition: the
                // map arm's object hint must never type the slice arm's
                // `then` (and vice versa), or a live arm becomes
                // internally contradictory.
                //
                // A branch whose renders ALL totally format (an embedded
                // partial-scalar splice like `--log-level={{ x | default
                // "info" }}`) proves the chart tolerates any input kind
                // there, so branch-scoped hint-grade typing — a literal
                // fallback's documented intent routed through the guarded
                // channel — must not close it (flux2). Path-level
                // hints keep typing the branch: they carry real consumer
                // contracts (flux2's own `substr` tag check) that hold
                // wherever the path renders.
                let branch_hint_pool =
                    if branch.facts.used_as_serialized && !branch.facts.has_string_contract {
                        &contract_type_hints
                    } else {
                        &overlay_type_hints
                    };
                let branch_hints =
                    partition_compatible_hints(branch_hint_pool, &guards, value_path.as_str());
                ConditionalPathOverlay {
                    guards,
                    evidence: branch.conditional_overlay_evidence(facts, branch_hints),
                    preserve_base_schema: has_unconditional_overlay_peer || saw_unsupported_overlay,
                }
            })
            .collect();
        // Branch-scoped hints ride the overlays' evidence copies. When no
        // overlay can host them (none lowered, or an unsupported or
        // approximate guard poisoned them), they stay branch-scoped
        // wideners rather than degrading to path-level typing: the guards
        // the encoding could not represent decide when those branches run,
        // so binding their typing path-wide would narrow states the branch
        // never reaches.
        fail_implications.sort();
        fail_implications.dedup();
        let mut guarded_type_hints = guarded_type_hints;
        guarded_type_hints.extend(guarded_fallback_type_hints);
        ContractPathSchemaEvidence {
            value_path,
            is_referenced_value_path: referenced,
            facts,
            guard_predicates,
            metadata_field_kinds: path_facts.metadata_field_kinds,
            type_hints,
            guarded_type_hints,
            fallback_type_hints,
            provider_schema_uses: path_facts.provider_schema_uses,
            requiredness,
            conditional_overlays,
            fail_implications,
        }
    }
}

fn metadata_field_kind_from_yaml_path(path: &[String]) -> Option<MetadataFieldKind> {
    if path.get(path.len().checked_sub(2)?)?.as_str() != "metadata" {
        return None;
    }

    match path.last()?.as_str() {
        "labels" | "annotations" => Some(MetadataFieldKind::StringMap),
        "name" => Some(MetadataFieldKind::Name),
        "namespace" => Some(MetadataFieldKind::Namespace),
        _ => None,
    }
}

fn conditional_guard_predicates(predicates: &[Predicate]) -> Vec<ConditionalGuard> {
    let mut guards = predicates
        .iter()
        .filter_map(|predicate| predicate_to_guard(predicate, None))
        .collect::<Vec<_>>();
    guards.sort();
    guards.dedup();
    guards
}

fn lowerable_conditional_guard_set(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<Vec<ConditionalGuard>> {
    // A key-equality conjunct subsumes its companion iteration conjunct:
    // the has-key lowering already implies the range reaches that member
    // (prometheus's serverFiles dispatch around the remoteWrite rows).
    let key_equals_ranges: BTreeSet<&str> = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::RangeKeyEquals { path, .. }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let mut guards = Vec::new();
    for predicate in predicates {
        // The row's own iteration (`range .Values.x` around a render of
        // `.Values.x` itself) is how the row fires, not a foreign
        // condition; the overlay keys on the residual conjuncts. A range
        // over a DIFFERENT path stays unlowerable unless a key-equality
        // pins the exact member the iteration must contain.
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path })
                if path == &contract_use.source_expr
                    || range_guard_is_iteration_ancestor(&contract_use.source_expr, path)
                    || key_equals_ranges.contains(path.as_str())
        ) {
            continue;
        }
        extend_lowerable_predicate(predicate, &contract_use.source_expr, &mut guards)?;
    }
    guards.sort();
    guards.dedup();
    Some(guards)
}

fn provider_schema_use(
    contract_use: &ContractUse,
    self_range_guarded: bool,
) -> Option<ProviderSchemaUse> {
    if contract_use.source_expr.trim().is_empty()
        || matches!(
            contract_use.kind,
            ValueKind::PartialScalar | ValueKind::Serialized
        )
        || contract_use.path.0.is_empty()
        // A string-consuming transform produced this rendered text, so the
        // slot observes the TRANSFORM's output, never the raw spelling: a
        // provider preimage on the raw value would reject programs and
        // pre-transform spellings that render fine (loki's
        // `tpl .Values.loki.configObjectName .` at a secretName slot). The
        // transform's own string-input contract still types the path. A
        // split-segment splice is the exception: its declared provenance is
        // exactly which part of the raw string the slot observes.
        || (contract_use.has_string_contract
            && contract_use.kind == ValueKind::Scalar
            && contract_use.split_segment.is_none())
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        resource,
        template_supplied_member_keys: contract_use.template_supplied_member_keys.clone(),
        split_segment: contract_use.split_segment.clone(),
        merge_layers: contract_use.merge_layers.clone(),
        range_key: contract_use.range_key,
        is_self_range_collection: self_range_guarded
            && contract_use
                .path
                .0
                .last()
                .is_none_or(|segment| !segment.ends_with("[*]")),
    })
}

fn predicate_to_guard(
    predicate: &Predicate,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    match predicate {
        Predicate::True | Predicate::False | Predicate::Approximate { .. } => None,
        Predicate::Guard(guard) => guard_to_conditional_guard(guard, target_value_path),
        // Predicates inside a negation are load-bearing even when they test
        // the target itself. Dropping a target conjunct from `not (a &&
        // target)` widens the branch into states where the render never
        // occurs (for example a `default` fallback shadowed by its primary).
        // A negated range-key equality has no document-level encoding: the
        // else-arm runs for every OTHER member even when the named key is
        // also present, so inverting the has-key lowering would be unsound.
        Predicate::Not(inner) => {
            if matches!(
                inner.as_ref(),
                Predicate::Guard(Guard::RangeKeyEquals { .. })
            ) {
                return None;
            }
            Some(ConditionalGuard::Not(Box::new(predicate_to_guard(
                inner, None,
            )?)))
        }
        Predicate::And(predicates) => {
            // `Range(P) ∧ Eq(P.*.M, V)` at the document level means SOME
            // iterated item's member equals the literal — the joined form
            // of a range-sentinel flag (`$found = true` under the member
            // test) — and lowers to the `contains` guard.
            if let Some(contains) = existential_member_guard(predicates) {
                return Some(contains);
            }
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
            guards.sort();
            guards.dedup();
            match guards.as_slice() {
                [] => None,
                [guard] => Some(guard.clone()),
                _ => Some(ConditionalGuard::AllOf(guards)),
            }
        }
        Predicate::Or(predicates) => {
            // Inside a disjunction a guard on the target itself is
            // load-bearing (`or .Values.other (and .Values.self .flag)`),
            // unlike a top-level self conjunct (the row's own firing
            // condition), so arms encode their paths literally.
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, None))
                .collect::<Option<Vec<_>>>()?;
            if guards
                .iter()
                .flat_map(ConditionalGuard::value_paths)
                .any(|path| path_contains_wildcard(&path))
            {
                return None;
            }
            guards.sort();
            guards.dedup();
            (target_value_path.is_some() || !guards.is_empty())
                .then_some(ConditionalGuard::AnyOf(guards))
        }
    }
}

fn extend_lowerable_predicate(
    predicate: &Predicate,
    target_value_path: &str,
    out: &mut Vec<ConditionalGuard>,
) -> Option<()> {
    match predicate {
        Predicate::True | Predicate::False | Predicate::Approximate { .. } => return None,
        Predicate::Guard(Guard::With { path }) if path == target_value_path => {}
        Predicate::Guard(Guard::With { .. }) => {
            out.push(predicate_to_guard(predicate, None)?);
        }
        Predicate::And(predicates) => {
            for predicate in predicates {
                extend_lowerable_predicate(predicate, target_value_path, out)?;
            }
        }
        Predicate::Guard(Guard::Range { .. }) => return None,
        Predicate::Guard(Guard::Default { path }) if path == target_value_path => {}
        // The row's own truthiness is nullability evidence (captured as
        // source null-tolerance), not a conditional shape over *other*
        // paths; like the self-default and self-negation arms it must not
        // poison the foreign overlay keys. Root-to-leaf guard stacks put it
        // on every `with .Values.x`-wrapped render since the fragment
        // interpreter landed.
        Predicate::Guard(Guard::Truthy { path }) if path == target_value_path => {}
        // Self-negation carries the branch's own-arm exclusion, not a
        // conditional shape over *other* paths; the overlay keys stay on the
        // foreign conditions.
        Predicate::Not(inner)
            if matches!(
                inner.as_ref(),
                Predicate::Guard(Guard::Truthy { path }) if path == target_value_path
            ) => {}
        // A type test on the row's own path (also negated or a disjunction
        // of such tests) partitions its domain (a type-switch arm). The
        // partition is load-bearing: the arm's sink typing holds only for
        // its tested types, and an executing complement arm's requirements
        // hold exactly for the untested ones — so it stays ON the overlay
        // key rather than leaking the arm's shape over the whole domain.
        // On a `.*` member row the partition keys the MEMBER overlay: the
        // wildcard guard path is encodable at the member slot, exactly like
        // its negated complement (signoz's per-member object-versus-scalar
        // EnvVar dispatch).
        other if predicate_is_self_type_partition(other, target_value_path) => {
            let target = if path_contains_wildcard(target_value_path) {
                None
            } else {
                Some(target_value_path)
            };
            out.push(predicate_to_guard(other, target)?);
        }
        other => {
            out.push(predicate_to_guard(other, Some(target_value_path))?);
        }
    }
    Some(())
}

/// The exact conjunction shape a joined range-sentinel flag produces: one
/// direct iteration conjunct plus one literal equality on a single member
/// of the iterated item, and nothing else. Any extra conjunct abstains —
/// the existential reading holds only when the flag's truthiness is
/// exactly "some item's member equals the literal".
fn existential_member_guard(predicates: &[Predicate]) -> Option<ConditionalGuard> {
    fn flatten<'a>(predicates: &'a [Predicate], out: &mut Vec<&'a Predicate>) {
        for predicate in predicates {
            match predicate {
                Predicate::True => {}
                Predicate::And(inner) => flatten(inner, out),
                other => out.push(other),
            }
        }
    }
    let mut conjuncts = Vec::new();
    flatten(predicates, &mut conjuncts);
    let [a, b] = conjuncts.as_slice() else {
        return None;
    };
    let (range_path, eq_path, value) = match (a, b) {
        (
            Predicate::Guard(Guard::Range { path: range_path }),
            Predicate::Guard(Guard::Eq { path, value }),
        )
        | (
            Predicate::Guard(Guard::Eq { path, value }),
            Predicate::Guard(Guard::Range { path: range_path }),
        ) => (range_path, path, value),
        _ => return None,
    };
    let member = eq_path
        .strip_prefix(range_path.as_str())?
        .strip_prefix(".*.")?;
    if member.is_empty() || member.contains('.') || member.contains('*') {
        return None;
    }
    Some(ConditionalGuard::ContainsMemberEquals {
        path: range_path.clone(),
        member: member.to_string(),
        value: value.clone(),
    })
}

/// A terminal-clause conjunct may lower through an approximate predicate's
/// recognized SOUND SUBSET: the clause then rejects a subset of the real
/// failing states (firing less often is safe in this positive position).
/// The subset must never lower through a negation — `predicate_to_guard`'s
/// `Not` arm keeps returning `None` for approximate inners.
fn terminal_clause_guard(predicate: &Predicate) -> Option<ConditionalGuard> {
    if let Predicate::Approximate { sound_subset, .. } = predicate
        && !sound_subset.is_empty()
    {
        let mut guards = sound_subset
            .iter()
            .map(|guard| guard_to_conditional_guard(guard, None))
            .collect::<Option<Vec<_>>>()?;
        guards.sort();
        guards.dedup();
        return match guards.as_slice() {
            [] => None,
            [guard] => Some(guard.clone()),
            _ => Some(ConditionalGuard::AllOf(guards)),
        };
    }
    // A disjunction of strengthened arms implies the real disjunction, so
    // it stays inside the clause's positive position (jenkins' two-sided
    // `$replicas` domain check).
    if let Predicate::Or(items) = predicate
        && predicate.contains_approximation()
    {
        let mut guards = items
            .iter()
            .map(terminal_clause_guard)
            .collect::<Option<Vec<_>>>()?;
        guards.sort();
        guards.dedup();
        return match guards.as_slice() {
            [] => None,
            [guard] => Some(guard.clone()),
            _ => Some(ConditionalGuard::AnyOf(guards)),
        };
    }
    predicate_to_guard(predicate, None)
}

fn guard_to_conditional_guard(
    guard: &Guard,
    target_value_path: Option<&str>,
) -> Option<ConditionalGuard> {
    let path = |path: &str| match target_value_path {
        Some(target_value_path) => lowerable_guard_path(path, target_value_path),
        None => Some(path.to_string()),
    };

    match guard {
        Guard::Truthy { path: value_path } => Some(ConditionalGuard::Truthy {
            path: path(value_path)?,
        }),
        Guard::With { path: value_path } if target_value_path.is_none() => {
            Some(ConditionalGuard::With {
                path: path(value_path)?,
            })
        }
        Guard::Eq {
            path: value_path,
            value,
        } => Some(ConditionalGuard::Eq {
            path: path(value_path)?,
            value: value.clone(),
        }),
        Guard::NotEq {
            path: value_path,
            value,
        } => Some(ConditionalGuard::NotEq {
            path: path(value_path)?,
            value: value.clone(),
        }),
        Guard::Absent { path: value_path } => Some(ConditionalGuard::Absent {
            path: path(value_path)?,
        }),
        Guard::MatchesPattern {
            path: value_path,
            pattern,
            templated: false,
        } => Some(ConditionalGuard::MatchesPattern {
            path: path(value_path)?,
            pattern: pattern.clone(),
        }),
        Guard::MatchesPattern { .. } | Guard::RangeKeyPrefix { .. } => None,
        Guard::TypeIs {
            path: value_path,
            schema_type,
        } => {
            // A type test on the TARGET itself is load-bearing dispatch
            // structure (the `else` of `if typeIs "string" x` scopes an
            // object overlay to non-strings); only truthiness self-guards
            // are the row's own firing condition and stay stripped.
            let path = if target_value_path == Some(value_path.as_str()) {
                (!path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::TypeIs {
                path,
                schema_type: schema_type.clone(),
            })
        }
        Guard::NotTypeIs {
            path: value_path,
            schema_type,
        } => {
            // The dispatch complement is load-bearing on the target for the
            // same reason as the positive test above.
            let path = if target_value_path == Some(value_path.as_str()) {
                (!path_contains_wildcard(value_path)).then(|| value_path.clone())?
            } else {
                path(value_path)?
            };
            Some(ConditionalGuard::Not(Box::new(ConditionalGuard::TypeIs {
                path,
                schema_type: schema_type.clone(),
            })))
        }
        Guard::IntGt {
            path: value_path,
            bound,
        } => Some(ConditionalGuard::IntGt {
            path: path(value_path)?,
            bound: *bound,
        }),
        Guard::IntLt {
            path: value_path,
            bound,
        } => Some(ConditionalGuard::IntLt {
            path: path(value_path)?,
            bound: *bound,
        }),
        // The POSITIVE key-equality selects exactly one member, so at the
        // document level it holds iff the collection HAS that key (// prometheus's `eq $key "prometheus.yml"` serverFiles arm). The
        // negated form runs for every OTHER member and must not lower —
        // `predicate_to_guard`'s Not arm rejects it.
        Guard::RangeKeyEquals {
            path: value_path,
            key,
        } => {
            if key.is_empty() {
                return None;
            }
            Some(ConditionalGuard::HasKey {
                path: path(value_path)?,
                key: key.clone(),
            })
        }
        Guard::Range { .. }
        | Guard::With { .. }
        | Guard::Default { .. }
        | Guard::Not { .. }
        | Guard::Or { .. }
        | Guard::AnyOf { .. } => None,
    }
}

fn predicate_is_self_guarding(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(
            Guard::Truthy { path }
                | Guard::Eq { path, .. }
                | Guard::Range { path }
                | Guard::With { path }
                | Guard::Default { path }
        ) if path == source_expr
    )
}

fn predicate_is_self_presence(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Not(inner)
            if matches!(
                inner.as_ref(),
                Predicate::Guard(Guard::Absent { path }) if path == source_expr
            )
    )
}

/// A nested range over each member of `parent` (`p.*` ranged): members
/// must be rangeable wherever the outer conditions hold.
fn record_member_range_requirement(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    parent: &str,
    predicates: &[Predicate],
    outer_allows_integer: bool,
    inner_allows_integer: bool,
) {
    let mut outer_guards = Vec::new();
    for predicate in predicates {
        if matches!(
            predicate,
            Predicate::Guard(Guard::Range { path }) if path == parent || path == &format!("{parent}.*")
        ) {
            continue;
        }
        let Some(guard) = predicate_to_guard(predicate, None) else {
            return;
        };
        if guard
            .value_paths()
            .iter()
            .any(|path| path_contains_wildcard(path))
        {
            return;
        }
        outer_guards.push(guard);
    }
    outer_guards.sort();
    outer_guards.dedup();
    let implication = ContractFailImplication {
        outer_guards,
        target: ContractRequirementTarget::Members {
            allow_integer: outer_allows_integer,
        },
        requirements: vec![FailValueRequirement::Iterable {
            allow_integer: inner_allows_integer,
        }],
    };
    let acc = path_accumulator(paths, parent);
    acc.referenced = true;
    if !acc.fail_implications.contains(&implication) {
        acc.fail_implications.push(implication);
    }
}

/// Whether a conjunct tests the TYPE of `source_expr`, positively or under
/// negation.
fn predicate_tests_source_type(predicate: &Predicate, source_expr: &str) -> bool {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, .. }) => path == source_expr,
        Predicate::Not(inner) => predicate_tests_source_type(inner, source_expr),
        Predicate::And(items) | Predicate::Or(items) => items
            .iter()
            .any(|item| predicate_tests_source_type(item, source_expr)),
        Predicate::True
        | Predicate::False
        | Predicate::Approximate { .. }
        | Predicate::Guard(_) => false,
    }
}

/// Whether every leaf of `predicate` is a type test on `target_value_path`
/// itself: such a predicate partitions the row's own domain instead of
/// conditioning it on other paths.
fn predicate_is_self_type_partition(predicate: &Predicate, target_value_path: &str) -> bool {
    match predicate {
        Predicate::Guard(Guard::TypeIs { path, .. }) => path == target_value_path,
        Predicate::Not(inner) => predicate_is_self_type_partition(inner, target_value_path),
        Predicate::And(items) | Predicate::Or(items) => {
            !items.is_empty()
                && items
                    .iter()
                    .all(|item| predicate_is_self_type_partition(item, target_value_path))
        }
        Predicate::True
        | Predicate::False
        | Predicate::Approximate { .. }
        | Predicate::Guard(_) => false,
    }
}

fn predicate_is_positive_header(predicate: &Predicate, source_expr: &str) -> bool {
    matches!(
        predicate,
        Predicate::Guard(Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::TypeIs { path, .. }) if path == source_expr
    )
}

fn lowerable_guard_path(path: &str, target_value_path: &str) -> Option<String> {
    (!path_contains_wildcard(path) && path != target_value_path).then(|| path.to_string())
}

fn path_contains_wildcard(path: &str) -> bool {
    helm_schema_core::split_value_path(path)
        .iter()
        .any(|segment| segment == "*")
}

fn ranged_member_parent(path: &str) -> Option<&str> {
    path.strip_suffix(".*")
        .or_else(|| path.split_once(".*.").map(|(parent, _)| parent))
}

fn range_guard_is_iteration_ancestor(source_path: &str, guard_path: &str) -> bool {
    let source_segments = helm_schema_core::split_value_path(source_path);
    let guard_segments = helm_schema_core::split_value_path(guard_path);
    source_segments.len() > guard_segments.len()
        && source_segments.starts_with(&guard_segments)
        && source_segments
            .get(guard_segments.len())
            .is_some_and(|segment| segment == "*")
}

fn predicate_is_structural_ancestor_guard(predicate: &Predicate, source_path: &str) -> bool {
    let Predicate::Guard(Guard::Truthy { path } | Guard::With { path }) = predicate else {
        return false;
    };
    let source_segments = helm_schema_core::split_value_path(source_path);
    let guard_segments = helm_schema_core::split_value_path(path);
    source_segments.len() > guard_segments.len() && source_segments.starts_with(&guard_segments)
}

/// All strict ancestors of the referenced paths, the subset whose
/// descendant continues through a `*` item segment (a ranged collection's
/// element rows, as opposed to a literal member read), and the subset
/// whose `*` descendant continues INTO element structure (`p.*.field`) —
/// a bare `p.*` value row proves no LIST shape, since `range` iterates
/// maps too.
fn collect_paths_with_descendants(
    paths: &BTreeSet<String>,
) -> (BTreeSet<String>, BTreeSet<String>, BTreeSet<String>) {
    let mut ancestors = BTreeSet::new();
    let mut item_ancestors = BTreeSet::new();
    let mut structured_item_ancestors = BTreeSet::new();
    for path in paths {
        let segments = helm_schema_core::split_value_path(path);
        for prefix_len in 1..segments.len() {
            let ancestor = helm_schema_core::join_value_path(&segments[..prefix_len]);
            if segments[prefix_len] == "*" {
                item_ancestors.insert(ancestor.clone());
                if prefix_len + 1 < segments.len() {
                    structured_item_ancestors.insert(ancestor.clone());
                }
            }
            ancestors.insert(ancestor);
        }
    }
    (ancestors, item_ancestors, structured_item_ancestors)
}
