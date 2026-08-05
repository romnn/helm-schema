use super::{
    ApproximationRole, BTreeMap, BTreeSet, ConditionalGuard, ConditionalOverlayEvidence,
    ContractFailImplication, ContractRequirednessEvidence, ContractRequirementTarget, ContractUse,
    ContractValuePathFacts, FailValueRequirement, Guard, GuardDnf, MetadataFieldKind, Predicate,
    ProviderSchemaUse, SourceUseFactSplit, ValueKind, collapse_layered_truthy_gates,
    conditional_guard_predicates, extend_lowerable_predicate, hard_negation_paths,
    lowerable_conditional_guard_set, lowerable_conditional_guard_subset,
    metadata_field_kind_from_yaml_path, path_accumulator, path_contains_wildcard,
    predicate_is_positive_header, predicate_is_self_guarding, predicate_is_self_presence,
    predicate_is_structural_ancestor_guard, predicate_skips_falsy_source,
    predicate_tests_source_type, predicate_to_guard, provider_schema_use,
    range_guard_is_iteration_ancestor, ranged_member_parent, record_member_range_requirement,
};

#[derive(Default)]
pub(super) struct ContractPathAccumulator {
    pub(super) referenced: bool,
    pub(super) guard_predicates: Vec<ConditionalGuard>,
    pub(super) facts: PathSchemaFactsAccumulator,
    pub(super) requiredness: ContractRequirednessEvidence,
    pub(super) type_hints: BTreeSet<String>,
    /// Hints observed only under branch predicates: overlay typing only.
    pub(super) guarded_type_hints: BTreeSet<String>,
    /// Hints from literal `default`/`coalesce` fallbacks: they type only
    /// the truthy arm, so base lowering keeps Helm-falsy inputs open.
    pub(super) fallback_type_hints: BTreeSet<String>,
    /// Branch-scoped fallback hints: fallback-grade overlay typing
    /// that a totally-formatting branch must not bind.
    pub(super) guarded_fallback_type_hints: BTreeSet<String>,
    pub(super) conditional_overlay_branches:
        BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    pub(super) has_unconditional_overlay_peer: bool,
    pub(super) saw_unsupported_overlay: bool,
    pub(super) fail_implications: Vec<ContractFailImplication>,
    pub(super) member_access_conditions: MemberAccessConditions,
}

#[derive(Clone, Default)]
pub(super) struct MemberAccessConditions {
    /// Exact execution conditions, grouped by raw kinds that an earlier
    /// proven mutation converted to an object.
    pub(super) exact_by_handled_kinds: BTreeMap<Vec<String>, GuardDnf>,
    /// Sound subsets of executions whose full condition remains unknown.
    /// These may emit rejection arms but never own the host's base.
    pub(super) partial_by_handled_kinds: BTreeMap<Vec<String>, GuardDnf>,
    /// At least one access site could not be represented as an exact
    /// execution condition, so the exact arms do not own the whole domain.
    pub(super) saw_incomplete_access: bool,
}

impl MemberAccessConditions {
    pub(super) fn record(
        &mut self,
        handled_kinds: Vec<String>,
        condition: GuardDnf,
        complete: bool,
    ) {
        self.saw_incomplete_access |= !complete;
        if condition.is_never() {
            return;
        }
        let conditions = if complete {
            &mut self.exact_by_handled_kinds
        } else {
            &mut self.partial_by_handled_kinds
        };
        conditions
            .entry(handled_kinds)
            .and_modify(|known| known.union_absorbing(condition.clone()))
            .or_insert(condition);
    }

    pub(super) fn mark_incomplete(&mut self) {
        self.saw_incomplete_access = true;
    }

    pub(super) fn is_empty(&self) -> bool {
        self.exact_by_handled_kinds.is_empty() && self.partial_by_handled_kinds.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PathSchemaFactsAccumulator {
    pub(super) metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    pub(super) provider_schema_uses: Vec<ProviderSchemaUse>,
    pub(super) facts: ContractValuePathFacts,
    pub(super) all_uses_nullable: bool,
}

impl Default for PathSchemaFactsAccumulator {
    fn default() -> Self {
        Self {
            metadata_field_kinds: BTreeSet::new(),
            provider_schema_uses: Vec::new(),
            facts: ContractValuePathFacts {
                all_render_uses_self_guarded: true,
                all_render_uses_falsy_tolerant: true,
                ..ContractValuePathFacts::default()
            },
            all_uses_nullable: true,
        }
    }
}

impl PathSchemaFactsAccumulator {
    pub(super) fn record_nullable_observation(&mut self, nullable: bool) {
        self.all_uses_nullable &= nullable;
    }

    pub(super) fn record_metadata_field_kind(&mut self, field_kind: Option<MetadataFieldKind>) {
        if let Some(field_kind) = field_kind {
            self.metadata_field_kinds.insert(field_kind);
        }
    }

    pub(super) fn record_facts(&mut self, facts: ContractValuePathFacts) {
        self.facts.used_as_fragment |= facts.used_as_fragment;
        self.facts.used_as_serialized |= facts.used_as_serialized;
        self.facts.used_as_yaml_serialized |= facts.used_as_yaml_serialized;
        self.facts.has_string_contract |= facts.has_string_contract;
        self.facts.has_non_self_guarded_string_contract |=
            facts.has_non_self_guarded_string_contract;
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
        self.facts.has_non_control_use |= facts.has_non_control_use;
        self.facts.has_unlayered_non_control_use |= facts.has_unlayered_non_control_use;
        self.facts.merge_render_use_facts(facts);
    }

    pub(super) fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    pub(super) fn merge_union(&mut self, other: Self) {
        for provider_schema_use in other.provider_schema_uses {
            self.record_provider_schema_use(provider_schema_use);
        }
        self.metadata_field_kinds.extend(other.metadata_field_kinds);
        self.record_facts(other.facts);
        self.all_uses_nullable &= other.all_uses_nullable;
    }

    pub(super) fn facts(
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

    pub(super) fn conditional_overlay_evidence(
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
pub(super) fn partition_compatible_hints(
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

pub(super) fn record_contract_use(
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
        // Audited positive-evidence subsets replace their approximate
        // conjunct before recording. The first-iteration collection bound is
        // structural. Other CONTROL subsets apply where a source reaches a
        // resolved provider sink: they identify states where the row
        // certainly renders, regardless of whether its payload is scalar or
        // structured. OUTPUT-SELECTION subsets stay approximate here because
        // influence on a selected value does not prove source identity.
        let predicates: Vec<Predicate> = predicates
            .into_iter()
            .map(|predicate| match &predicate {
                Predicate::Approximate {
                    role: ApproximationRole::Control,
                    sound_subset: Some(sound_subset),
                    ..
                } if at_most_one_member_predicate(sound_subset)
                    || (contract_use.resource.is_some()
                        && !contract_use.path.0.is_empty()
                        && predicate_to_guard(sound_subset, None).is_some()) =>
                {
                    sound_subset.as_ref().clone()
                }
                _ => predicate,
            })
            .collect();
        record_contract_use_conjunction(paths, contract_use, &predicates, range_modes);
    }
}

pub(super) fn at_most_one_member_predicate(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Guard(Guard::AtMostOneMember { .. }) => true,
        Predicate::And(predicates) => {
            !predicates.is_empty() && predicates.iter().all(at_most_one_member_predicate)
        }
        _ => false,
    }
}

pub(super) fn predicate_is_truthy_disjunction_over(
    predicate: &Predicate,
    paths: &[String],
) -> bool {
    let Predicate::Or(alternatives) = predicate else {
        return false;
    };
    let candidates = alternatives
        .iter()
        .filter_map(|alternative| match alternative {
            Predicate::Guard(Guard::Truthy { path }) => Some(path.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    candidates.len() == alternatives.len()
        && candidates == paths.iter().cloned().collect::<BTreeSet<_>>()
}

/// A row rendering the collection's RANGE KEY contributes exactly one fact:
/// the provider slot the key renders at, from which the generator derives
/// the key-domain requirement (a string-only slot excludes a non-empty
/// list's integer keys). It must never read as a render of the collection's
/// VALUE. Guarded or indirect sites abstain — the synthesized arm would
/// fire in states the analysis cannot scope.
pub(super) fn record_range_key_slot_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
    range_modes: &crate::range_modes::RangeModes,
) {
    if contract_use.path.0.is_empty()
        || !range_modes.mode(&contract_use.source_expr).member_identity
    {
        return;
    }
    let Some(provider_use) = provider_schema_use(contract_use, false, false) else {
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
/// row). Unmatched rows keep both flat candidates and branch provenance so
/// emission can distinguish selector-dependent from ordinary uses.
pub(super) fn kind_branch_resolved_use(
    contract_use: &ContractUse,
    predicates: &[Predicate],
) -> Option<ContractUse> {
    let resource = contract_use.resource.as_ref()?;
    if resource.kind_branches.is_empty() {
        return None;
    }
    let row_conjuncts = predicates
        .iter()
        .flat_map(flattened_conjuncts)
        .collect::<Vec<_>>();
    let mut selected = resource.kind_branches.iter().filter(|branch| {
        flattened_conjuncts(&branch.predicate)
            .iter()
            .all(|conjunct| matches!(conjunct, Predicate::True) || row_conjuncts.contains(conjunct))
    });
    let selected_kind = match (selected.next(), selected.next()) {
        (Some(branch), None) => Some(branch.kind.clone()),
        _ => None,
    };
    let mut resolved = contract_use.clone();
    if let Some(resource) = resolved.resource.as_mut()
        && let Some(kind) = selected_kind
    {
        resource.kind = kind;
        resource.kind_candidates.clear();
    }
    Some(resolved)
}

/// The leaf conjuncts of a predicate: nested `And`s flatten, everything
/// else (including `Not`/`Or`) is one leaf.
pub(super) fn flattened_conjuncts(predicate: &Predicate) -> Vec<&Predicate> {
    match predicate {
        Predicate::And(items) => items.iter().flat_map(flattened_conjuncts).collect(),
        other => vec![other],
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn record_contract_use_conjunction(
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
        !range_modes.mode(parent).member_identity
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                matches!(
                    predicate,
                    Predicate::Guard(Guard::Range { path })
                        if !range_modes.mode(path).member_identity
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
            (contract_use.path.0.is_empty()
                && range_modes.mode(&contract_use.source_expr).member_identity)
                .then(|| lowerable_range_outer_guards(&contract_use.source_expr, predicates))
                .flatten()
        });
    // A merge layer's sink typing rides its OWN truthiness: whichever layer
    // made the `with` gate truthy, this layer's keys render exactly when
    // the layer itself is (its falsy states contribute nothing). The
    // ORIGINAL decoded gates still scope the synthesized layer arms — a
    // dormant render gate must silence them — so they travel on the
    // provider use itself.
    let merge_layered = contract_use
        .merge_layers
        .as_ref()
        .filter(|merge| merge.layers.get(merge.position) == Some(&contract_use.source_expr))
        // Binding-carried layer facts reroute only when the merge involves
        // a structural transform: there the member typing must scope to the
        // states each layer actually supplies. Shadowed members stay open,
        // nil-scrubbed members null-relax, and parsed-map layers type only
        // mapping inputs.
        // Ordinary binding-carried merges keep the pre-layered routing:
        // their sibling dispatch arms (bitnami's `tplvalues.render` string
        // lane) rely on the branch alternatives the rerouting suppresses.
        .filter(|merge| {
            !merge.via_binding
                || merge
                    .transforms
                    .iter()
                    .any(|transform| *transform != helm_schema_core::MergeLayerTransform::Identity)
        })
        // Positive row conditions the ungated arm drops only widen its
        // firing states within the render's own selection facts (the
        // documented member-local-wildcard widening). A dropped HARD
        // NEGATION is different: the row renders only while some OTHER
        // candidate family is dormant, and an arm that fires regardless
        // rejects states whose renders never consume this row (airflow's
        // deprecated `securityContext` fallback behind a live
        // `securityContexts.pod`). Rows whose unlowerable conditions
        // negate foreign-family selections keep the pre-layered routing,
        // whose ambient approximates abstain instead of narrowing.
        .filter(|merge| {
            lowerable_guards.is_some()
                || predicates.iter().all(|predicate| {
                    let mut guards = Vec::new();
                    if extend_lowerable_predicate(predicate, &contract_use.source_expr, &mut guards)
                        .is_some()
                    {
                        return true;
                    }
                    let mut negated_paths = BTreeSet::new();
                    hard_negation_paths(predicate, &mut negated_paths);
                    negated_paths.iter().all(|path| {
                        merge.layers.iter().any(|layer| {
                            path == layer
                                || helm_schema_core::values_path_is_descendant(path, layer)
                                || helm_schema_core::values_path_is_descendant(layer, path)
                        })
                    })
                })
        });
    // The synthesized layer arms tolerate a PARTIAL gate: every lowered
    // conjunct is an exact decode of one row condition, so a live render
    // satisfies the subset and the arms still fire; dropped conjuncts only
    // leave the arms firing in some dormant states (the documented
    // member-local-wildcard widening, now shrunk to the genuinely
    // unlowerable conjuncts). The full-set decode stays preferred so an
    // exactly-decodable row keeps its complete gate. Either way the
    // per-layer spellings of one MERGED read (the historic all-paths
    // conjunction) must not gate conjunctively — the merged value is
    // truthy when ANY layer supplies it, so the conjunction silences the
    // arm on live renders (airflow's `workers.waitForMigrations.enabled`
    // read of the celery-merged map, absent from the celery defaults).
    let merge_outer_guards = merge_layered.map(|merge| {
        let guards = lowerable_guards
            .clone()
            .unwrap_or_else(|| lowerable_conditional_guard_subset(contract_use, predicates));
        collapse_layered_truthy_gates(guards, &merge.layers)
    });
    let lowerable_guards = merge_layered.map_or(lowerable_guards, |merge| {
        let guard = match merge.own_transform() {
            helm_schema_core::MergeLayerTransform::ParsedMap => ConditionalGuard::TypeIs {
                path: contract_use.source_expr.clone(),
                schema_type: "object".to_string(),
            },
            helm_schema_core::MergeLayerTransform::Identity
            | helm_schema_core::MergeLayerTransform::NilScrubbed => ConditionalGuard::Truthy {
                path: contract_use.source_expr.clone(),
            },
        };
        Some(vec![guard])
    });
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
    let total_stringified = contract_use.stringified
        && !matches!(
            contract_use.kind,
            ValueKind::YamlSerialized | ValueKind::TemplatedYamlSerialized
        );

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
            || total_stringified
            || type_dispatched;
        if serialized_tolerant {
            let acc = path_accumulator(paths, &contract_use.source_expr);
            acc.referenced = true;
            acc.facts.facts.used_as_serialized = true;
            acc.facts.facts.has_non_control_use = true;
        }
    }
    if has_source && !has_approximate {
        let mut facts = ContractValuePathFacts {
            used_as_fragment: matches!(
                contract_use.kind,
                ValueKind::Fragment
                    | ValueKind::YamlSerialized
                    | ValueKind::TemplatedYamlSerialized
            ),
            used_as_serialized: matches!(contract_use.kind, ValueKind::Serialized)
                || (contract_use.kind == ValueKind::PartialScalar && !path_is_empty)
                || total_stringified
                || type_dispatched,
            used_as_yaml_serialized: matches!(
                contract_use.kind,
                ValueKind::YamlSerialized | ValueKind::TemplatedYamlSerialized
            ),
            has_string_contract: contract_use.has_string_contract && !type_dispatched,
            has_non_self_guarded_string_contract: contract_use.has_string_contract
                && !type_dispatched
                && !predicates.iter().any(|predicate| {
                    predicate_skips_falsy_source(predicate, &contract_use.source_expr)
                }),
            used_as_pathless_fragment: matches!(
                contract_use.kind,
                ValueKind::Fragment
                    | ValueKind::YamlSerialized
                    | ValueKind::TemplatedYamlSerialized
            ) && path_is_empty,
            has_parsed_map_layered_use: merge_layered.is_some_and(|merge| {
                merge.own_transform() == helm_schema_core::MergeLayerTransform::ParsedMap
            }),
            is_partial_scalar_value_path: contract_use.kind == ValueKind::PartialScalar,
            is_nullable: !path_is_empty
                || self_range_guarded
                || matches!(
                    contract_use.kind,
                    ValueKind::Fragment
                        | ValueKind::YamlSerialized
                        | ValueKind::TemplatedYamlSerialized
                )
                || pathless_self_default_guarded,
            ..ContractValuePathFacts::default()
        };
        let scoped_pathless_string_contract = path_is_empty
            && contract_use.has_string_contract
            && ranged_member_parent(&contract_use.source_expr).is_none();
        if !path_is_empty || scoped_pathless_string_contract {
            // Merge operands and digest rows do not consume a falsy input at
            // this use. Textual placement alone does not prove that: strict
            // formatters such as `%s` still abort on empty maps and arrays.
            let falsy_tolerant_use =
                merge_layered.is_some() || contract_use.digest || contract_use.merge_operand;
            facts.record_render_use(
                self_range_guarded,
                Some(has_matching_self_guard),
                Some(has_matching_self_guard || falsy_tolerant_use),
            );
            facts.has_unconditional_render_use = predicates.is_empty();
        }

        let positive_header = contract_use.kind == ValueKind::Scalar
            && path_is_empty
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                predicate_is_positive_header(predicate, &contract_use.source_expr)
            });
        // A pathless scalar row is the value's influence on a control
        // region, including its negated and branch-collapsed forms. It does
        // not render the source value into the resource. Other pathless
        // kinds carry actual fragment/text output.
        facts.has_non_control_use = !path_is_empty || contract_use.kind != ValueKind::Scalar;
        facts.has_unlayered_non_control_use = facts.has_non_control_use && merge_layered.is_none();
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
                ValueKind::Fragment
                    | ValueKind::YamlSerialized
                    | ValueKind::TemplatedYamlSerialized
            )
            && (matches!(
                contract_use.kind,
                ValueKind::YamlSerialized | ValueKind::TemplatedYamlSerialized
            ) && complement_dispatched
                || lowerable_guards.as_ref().is_some_and(|guards| {
                    guards.iter().any(|guard| {
                        matches!(
                            guard,
                            ConditionalGuard::TypeIs { schema_type, .. }
                                if schema_type == "object" || schema_type == "array"
                        )
                    })
                }));
        // A serialized splice renders text the sink cannot type back onto
        // the input, so it contributes no metadata field kind either. A
        // structural type partition is different: the selected object or
        // array itself reaches the sink, so metadata typing belongs inside
        // that arm even when the resource kind could not be resolved.
        let metadata_field_kind = if matches!(
            contract_use.kind,
            ValueKind::PartialScalar | ValueKind::Serialized
        ) || (contract_use.stringified
            && !matches!(
                contract_use.kind,
                ValueKind::YamlSerialized | ValueKind::TemplatedYamlSerialized
            ))
            || (type_dispatched && !structural_dispatch_arm)
        {
            None
        } else {
            metadata_field_kind_from_yaml_path(&contract_use.path.0)
        };
        let source_null_tolerant = path_is_empty || has_matching_self_guard;
        let provider_use = (!type_dispatched || complement_dispatched || structural_dispatch_arm)
            .then(|| provider_schema_use(contract_use, self_range_guarded, source_null_tolerant))
            .flatten()
            // A row whose layer facts stay UN-rerouted (binding-carried,
            // no structural transform involved) types through the ordinary
            // branch/base lanes; its use must not also seed synthesized
            // layer arms.
            // Rows whose position mismatches their own path (member
            // projections of a layered parent) keep the info — their
            // synthesized member arms are exactly round 18's ungated lanes.
            .map(|mut provider_use| {
                let keeps_layer_arms = provider_use.merge_layers.as_ref().is_some_and(|merge| {
                    !merge.via_binding
                        || merge.transforms.iter().any(|transform| {
                            *transform != helm_schema_core::MergeLayerTransform::Identity
                        })
                });
                if !keeps_layer_arms {
                    provider_use.merge_layers = None;
                }
                provider_use
            });
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
        let layer_arms_carry_sink_typing = merge_layer_provider_use.is_some();
        if let Some(mut layered) = merge_layer_provider_use {
            // Decoded render gates scope the synthesized layer arms — a
            // dormant gate must silence them (KPS's `defaultRules.create:
            // false`, airflow's empty worker-set range). Conjuncts that
            // cannot lower (member-local wildcard conditions on airflow's
            // per-set rows) drop from the gate individually, keeping the
            // arms live wherever the render is; their exact encoding is
            // the F80 existential member-guard residual.
            layered.outer_guards = merge_outer_guards.unwrap_or_default();
            acc.facts.facts.has_merge_layered_use = true;
            acc.facts.record_provider_schema_use(layered);
        }
        // The sink's metadata field kind is layer-scoped the same way: a
        // shadowed layer's member reaches the rendered map only where the
        // earlier layers leave it visible, so the string-map typing rides
        // the synthesized layer arms, never the base lanes (KPS's
        // group-level rule annotations beneath the per-alert layer).
        // Suppression presupposes the synthesized arms exist: a layered row
        // whose resource never resolved has no provider use to carry the
        // typing (minio's ternary-kinded workload), so its string-map kind
        // keeps typing the base lanes as an unlayered row would.
        let metadata_field_kind = if merge_layered.is_some() && layer_arms_carry_sink_typing {
            None
        } else {
            metadata_field_kind
        };
        // A structural dispatch arm splits its facts: the PATH keeps only the
        // dispatch tolerance (the arm must not hard-type the whole domain its
        // partition merely selects from), while the BRANCH keeps the real
        // structural use without the tolerance (which would dissolve the
        // arm's own provider typing into the serialized preimage).
        let guarded_string_contract = scoped_pathless_string_contract
            && lowerable_guards
                .as_ref()
                .is_some_and(|guards| !guards.is_empty());
        let (path_facts, branch_facts) = if structural_dispatch_arm {
            let mut path_facts = facts;
            path_facts.used_as_fragment = false;
            path_facts.used_as_yaml_serialized = false;
            path_facts.used_as_pathless_fragment = false;
            let mut branch_facts = facts;
            branch_facts.used_as_serialized = false;
            (path_facts, branch_facts)
        } else if guarded_string_contract {
            // A pathless strict-consumer claim under a foreign gate types
            // only that branch. Keeping the same fact on the path base would
            // reject values while the consumer's chart is dormant.
            let mut path_facts = facts;
            path_facts.has_string_contract = false;
            (path_facts, facts)
        } else if contract_use.digest {
            // A digest observes fresh derived text, not the raw input. Its
            // tolerance belongs only to a live overlay branch; path-level
            // render facts would let an unconditional checksum own the
            // input base even when the helper's real sink is dormant.
            (ContractValuePathFacts::default(), facts)
        } else {
            (facts, facts)
        };
        if layer_arms_carry_sink_typing {
            // The self-selection guard routes merge arms; it is not an
            // independent sink branch whose declared default may be typed.
            acc.referenced = true;
            acc.facts.record_facts(path_facts);
            acc.facts.record_nullable_observation(source_null_tolerant);
        } else {
            acc.record_source_use(
                &SourceUseFactSplit {
                    path: path_facts,
                    branch: branch_facts,
                },
                source_null_tolerant,
                lowerable_guards,
                branch_provider_use,
                metadata_field_kind,
            );
        }
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
    }
    if has_source && !has_approximate {
        for path in range_guard_paths {
            let facts = ContractValuePathFacts {
                is_nullable: true,
                ..ContractValuePathFacts::default()
            };
            path_accumulator(paths, &path).facts.record_facts(facts);
        }
    }
}

/// A `range` read under foreign conditions bounds an ITERABLE requirement
/// to those conditions: Go's `range` iterates collections and skips nil but
/// fails template rendering on scalars, so inside the guarded branch the
/// ranged path must be a collection. The branch stays render-free; overlay
/// lowering recognizes that shape and emits the iterable domain.
/// Whether a conjunction carries a disjunctive with-header's marker stamp:
/// `with A | default B` stamps a conjunctive `With` marker per path beside
/// the real `Or` condition. The markers encode as truthiness downstream, so
/// lowering them conjunctively would scope a RANGE requirement to "every
/// candidate truthy" — the exact state where a selected sibling collection
/// keeps a truthy scalar co-candidate unranged — and dropping them would
/// fire it on other unselected states instead. Only the selection-chain
/// capture knows which candidate ranges; requirement lowering abstains on
/// the stamp.
pub(super) fn has_selection_chain_marker_stamp(predicates: &[Predicate]) -> bool {
    let with_marker_paths: BTreeSet<&str> = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::With { path }) => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let disjunction_paths = |predicate: &Predicate| -> Option<Vec<String>> {
        match predicate {
            Predicate::Guard(Guard::Or { paths }) => Some(paths.clone()),
            Predicate::Or(alternatives) => alternatives
                .iter()
                .map(|alternative| match alternative {
                    Predicate::Guard(Guard::Truthy { path }) => Some(path.clone()),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    };
    with_marker_paths.len() > 1
        && predicates.iter().any(|predicate| {
            disjunction_paths(predicate).is_some_and(|paths| {
                with_marker_paths
                    .iter()
                    .all(|marker| paths.iter().any(|path| path == marker))
            })
        })
}

pub(super) fn lowerable_range_outer_guards(
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

pub(super) fn record_guarded_range_requirement(
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

pub(super) fn record_range_input_capture(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    capture: &crate::eval_effect::FailCapture,
    path: &str,
    destructured: bool,
    json_decoded: bool,
) {
    if path.trim().is_empty() || capture.contains_approximation() {
        return;
    }
    let outer_guards = (!has_selection_chain_marker_stamp(&capture.conjunction))
        .then(|| lowerable_range_outer_guards(path, &capture.conjunction))
        .flatten();
    let unconditional = outer_guards.as_ref().is_some_and(Vec::is_empty);
    let facts = ContractValuePathFacts {
        is_ranged_source: unconditional,
        is_direct_ranged_source: unconditional,
        has_destructured_range_use: destructured,
        has_json_decoded_range_use: json_decoded,
        is_nullable: true,
        ..ContractValuePathFacts::default()
    };
    path_accumulator(paths, path).facts.record_facts(facts);

    if let Some(parent) = path.strip_suffix(".*")
        && !path_contains_wildcard(parent)
    {
        let parent_mode = capture.ranged.mode(parent);
        if !parent_mode.member_identity {
            return;
        }
        record_member_range_requirement(
            paths,
            parent,
            &capture.conjunction,
            !parent_mode.destructured && !parent_mode.json_decoded,
            !destructured && !json_decoded,
        );
    } else if let Some(guards) = outer_guards {
        record_guarded_range_requirement(paths, path, guards, destructured, json_decoded);
    }
}

pub(super) fn remove_redundant_approximate_conditions(conjunction: &[Predicate]) -> Vec<Predicate> {
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
