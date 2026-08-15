use super::{
    BTreeMap, BTreeSet, ConditionalGuard, ConditionalPathOverlay, ContractPathAccumulator,
    ContractPathSchemaEvidence, ContractSchemaSignals, ContractValuePathFacts, MetadataFieldKind,
    PathSchemaFactsAccumulator, ProviderSchemaUse, collect_paths_with_descendants,
    record_member_access_implications,
};

pub(super) fn finish_schema_signals(
    mut paths: BTreeMap<String, ContractPathAccumulator>,
    mut terminal_clauses: Vec<Vec<ConditionalGuard>>,
) -> ContractSchemaSignals {
    record_member_access_implications(&mut paths, &mut terminal_clauses);
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

pub(super) fn path_accumulator<'a>(
    paths: &'a mut BTreeMap<String, ContractPathAccumulator>,
    path: &str,
) -> &'a mut ContractPathAccumulator {
    paths.entry(path.to_string()).or_default()
}

/// The path-level and branch-level halves of one recorded source use's
/// facts: a structural dispatch arm keeps different facts on each side
/// (the path keeps only the dispatch tolerance, the branch the real
/// structural use).
pub(super) struct SourceUseFactSplit {
    pub(super) path: ContractValuePathFacts,
    pub(super) branch: ContractValuePathFacts,
}

impl ContractPathAccumulator {
    pub(super) fn record_source_use(
        &mut self,
        facts: &SourceUseFactSplit,
        source_null_tolerant: bool,
        lowerable_guards: Option<Vec<ConditionalGuard>>,
        provider_schema_use: Option<ProviderSchemaUse>,
        metadata_field_kind: Option<MetadataFieldKind>,
    ) {
        self.referenced = true;
        if lowerable_guards.is_none() {
            self.saw_unsupported_overlay = true;
            // The sink contract cannot escape an unencodable foreign guard,
            // but the row is still a render rather than a control-only read.
            // Retain that distinction so values.yaml remains the bounded
            // fallback shape instead of widening the path to anything.
            self.facts.facts.has_non_control_use |= facts.path.has_non_control_use;
            return;
        }
        self.facts.record_facts(facts.path);
        // A parsed-map merge may render unconditionally, but this source
        // supplies sink members only on its mapping-input partition.
        let row_forms_overlay_branch = facts.branch.has_render_use
            && (!facts.branch.has_unconditional_render_use
                || facts.branch.has_parsed_map_layered_use)
            && lowerable_guards
                .as_ref()
                .is_some_and(|guards| !guards.is_empty());
        if !row_forms_overlay_branch {
            if let Some(provider_use) = provider_schema_use.clone() {
                self.facts.record_provider_schema_use(provider_use);
            }
            self.facts.record_metadata_field_kind(metadata_field_kind);
        }
        if facts.branch.has_render_use {
            let path_is_unconditional = facts.path.has_render_use
                && ((facts.path.has_unconditional_render_use
                    && !facts.path.has_parsed_map_layered_use)
                    || lowerable_guards.as_ref().is_some_and(Vec::is_empty));
            if path_is_unconditional {
                // All predicates were the row's own structural range
                // ancestry, so its sink evidence applies to every emitted
                // member and belongs to the base rather than an empty arm.
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = lowerable_guards.filter(|guards| !guards.is_empty()) {
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
        if facts.path.has_render_use {
            self.facts.record_nullable_observation(source_null_tolerant);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    pub(super) fn into_schema_evidence(
        self,
        value_path: String,
        has_referenced_descendants: bool,
        has_item_descendants: bool,
        has_structured_item_descendants: bool,
    ) -> ContractPathSchemaEvidence {
        let ContractPathAccumulator {
            referenced,
            guard_predicates,
            facts: mut path_facts,
            requiredness,
            type_hints,
            guarded_type_hints,
            fallback_type_hints,
            guarded_fallback_type_hints,
            conditional_overlay_branches,
            mut has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            mut fail_implications,
            member_access_conditions: _,
        } = self;
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
                if matches!(
                    guards.as_slice(),
                    [ConditionalGuard::Not(inner)]
                        if matches!(
                            inner.as_ref(),
                            ConditionalGuard::Absent { path } if path == &value_path
                        )
                ) {
                    // A property schema is consulted only while that property
                    // exists, so an exact self-presence branch has no residual
                    // condition at this path. Fold its sink facts into the one
                    // base owner instead of carrying a redundant overlay or a
                    // second provider-evidence lane.
                    path_facts.merge_union(branch.clone());
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
        let facts = path_facts.facts(
            has_referenced_descendants,
            has_item_descendants,
            has_structured_item_descendants,
        );
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
                // Keep only the subset of observed hints compatible with the
                // overlay branch's own type partition. A positive
                // `TypeIs(T)` key keeps only `T`; a negated one drops `T`;
                // foreign guards leave the hints untouched.
                let mut branch_hints = branch_hint_pool.clone();
                for guard in &guards {
                    match guard {
                        ConditionalGuard::TypeIs { path, schema_type }
                            if path == value_path.as_str() =>
                        {
                            branch_hints.retain(|hint| hint == schema_type);
                        }
                        ConditionalGuard::Not(inner) => {
                            if let ConditionalGuard::TypeIs { path, schema_type } = inner.as_ref()
                                && path == value_path.as_str()
                            {
                                branch_hints.retain(|hint| hint != schema_type);
                            }
                        }
                        _ => {}
                    }
                }
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
        let unconditional_requirements = fail_implications
            .iter()
            .filter(|implication| implication.outer_guards.is_empty())
            .map(|implication| (implication.target.clone(), implication.requirements.clone()))
            .collect::<BTreeSet<_>>();
        fail_implications.retain(|implication| {
            implication.outer_guards.is_empty()
                || !unconditional_requirements
                    .contains(&(implication.target.clone(), implication.requirements.clone()))
        });
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
