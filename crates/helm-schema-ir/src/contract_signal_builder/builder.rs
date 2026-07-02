use std::collections::{BTreeMap, BTreeSet};

use crate::{Guard, ProviderSchemaUse, ValueKind, contract::ContractUse};
use helm_schema_core::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind, Predicate,
};

pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    type_hints: &BTreeMap<String, BTreeSet<String>>,
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    for contract_use in uses {
        record_contract_use(&mut paths, contract_use);
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
    finish_schema_signals(paths)
}

#[derive(Default)]
struct ContractPathAccumulator {
    referenced: bool,
    guard_predicates: Vec<ConditionalGuard>,
    facts: PathSchemaFactsAccumulator,
    requiredness: ContractRequirednessEvidence,
    type_hints: BTreeSet<String>,
    conditional_overlay_branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    has_unconditional_overlay_peer: bool,
    saw_unsupported_overlay: bool,
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
        self.facts.used_as_pathless_fragment |= facts.used_as_pathless_fragment;
        self.facts.accepted_values_root_fragment |= facts.accepted_values_root_fragment;
        self.facts.accepted_dependency_values_root_fragment |=
            facts.accepted_dependency_values_root_fragment;
        self.facts.is_ranged_source |= facts.is_ranged_source;
        self.facts.is_partial_scalar_value_path |= facts.is_partial_scalar_value_path;
        self.facts.is_nullable |= facts.is_nullable;
        self.facts.merge_render_use_facts(facts);
    }

    fn record_provider_schema_use(&mut self, provider_schema_use: ProviderSchemaUse) {
        if !self.provider_schema_uses.contains(&provider_schema_use) {
            self.provider_schema_uses.push(provider_schema_use);
        }
    }

    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        let mut facts = self.facts;
        facts.has_referenced_descendants = has_referenced_descendants;
        facts.is_nullable &= self.all_uses_nullable;
        facts
    }

    fn conditional_overlay_evidence(
        self,
        global_facts: ContractValuePathFacts,
        type_hints: BTreeSet<String>,
    ) -> ConditionalOverlayEvidence {
        let facts = self.facts(global_facts.has_referenced_descendants);
        ConditionalOverlayEvidence {
            facts,
            metadata_field_kinds: self.metadata_field_kinds,
            type_hints,
            provider_schema_uses: self.provider_schema_uses,
        }
    }
}

fn record_contract_use(
    paths: &mut BTreeMap<String, ContractPathAccumulator>,
    contract_use: &ContractUse,
) {
    let predicates = contract_use
        .guards
        .iter()
        .cloned()
        .map(Predicate::from)
        .collect::<Vec<_>>();
    let has_source = !contract_use.source_expr.trim().is_empty();
    let path_is_empty = contract_use.path.0.is_empty();
    let range_guard_paths = predicates
        .iter()
        .filter_map(|predicate| match predicate {
            Predicate::Guard(Guard::Range { path }) => Some(path.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let self_range_guarded = range_guard_paths.contains(contract_use.source_expr.as_str());
    let has_matching_self_guard = predicates
        .iter()
        .any(|predicate| predicate_is_self_guarding(predicate, &contract_use.source_expr));
    let pathless_self_default_guarded = path_is_empty
        && predicates.iter().any(|predicate| {
            matches!(predicate, Predicate::Guard(Guard::Default { path }) if path == &contract_use.source_expr)
        });

    if has_source {
        let mut facts = ContractValuePathFacts {
            used_as_fragment: contract_use.kind == ValueKind::Fragment,
            used_as_pathless_fragment: contract_use.kind == ValueKind::Fragment && path_is_empty,
            is_partial_scalar_value_path: contract_use.kind == ValueKind::PartialScalar
                && !path_is_empty,
            is_nullable: !path_is_empty
                || self_range_guarded
                || contract_use.kind == ValueKind::Fragment
                || pathless_self_default_guarded,
            ..ContractValuePathFacts::default()
        };
        if !path_is_empty {
            facts.record_render_use(self_range_guarded, Some(has_matching_self_guard));
            facts.has_unconditional_render_use = contract_use.guards.is_empty();
        }

        let positive_header = contract_use.kind == ValueKind::Scalar
            && path_is_empty
            && !predicates.is_empty()
            && predicates.iter().all(|predicate| {
                predicate_is_positive_header(predicate, &contract_use.source_expr)
            });
        let metadata_field_kind = metadata_field_kind_from_yaml_path(&contract_use.path.0);
        let acc = path_accumulator(paths, &contract_use.source_expr);
        acc.requiredness.is_positive_header |= positive_header;
        acc.facts.record_metadata_field_kind(metadata_field_kind);
        acc.record_source_use(
            facts,
            path_is_empty || has_matching_self_guard,
            lowerable_conditional_guard_set(contract_use, &predicates),
            provider_schema_use(contract_use, self_range_guarded),
            metadata_field_kind,
        );
        acc.facts.record_facts(facts);
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
        for predicate in conditional_guard_predicates(&predicates) {
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
    if has_source {
        for path in range_guard_paths {
            let mut facts = ContractValuePathFacts {
                is_ranged_source: true,
                is_nullable: true,
                ..ContractValuePathFacts::default()
            };
            facts.all_render_uses_self_guarded = true;
            path_accumulator(paths, &path).facts.record_facts(facts);
        }
    }
}

fn finish_schema_signals(
    mut paths: BTreeMap<String, ContractPathAccumulator>,
) -> ContractSchemaSignals {
    let referenced_paths = paths
        .iter()
        .filter_map(|(path, acc)| acc.referenced.then_some(path.clone()))
        .collect();
    let paths_with_referenced_descendants = collect_paths_with_descendants(&referenced_paths);
    for path in &paths_with_referenced_descendants {
        path_accumulator(&mut paths, path);
    }

    let schema_evidence_by_value_path = paths
        .into_iter()
        .map(|(value_path, acc)| {
            let facts = acc.facts(paths_with_referenced_descendants.contains(&value_path));
            let evidence = acc.into_schema_evidence(value_path.clone(), facts);
            (value_path, evidence)
        })
        .collect();
    ContractSchemaSignals::new(schema_evidence_by_value_path)
}

fn path_accumulator<'a>(
    paths: &'a mut BTreeMap<String, ContractPathAccumulator>,
    path: &str,
) -> &'a mut ContractPathAccumulator {
    paths.entry(path.to_string()).or_default()
}

impl ContractPathAccumulator {
    fn record_source_use(
        &mut self,
        facts: ContractValuePathFacts,
        source_null_tolerant: bool,
        lowerable_guards: Option<Vec<ConditionalGuard>>,
        provider_schema_use: Option<ProviderSchemaUse>,
        metadata_field_kind: Option<MetadataFieldKind>,
    ) {
        if let Some(provider_use) = provider_schema_use.clone() {
            self.facts.record_provider_schema_use(provider_use);
        }
        self.referenced = true;
        if facts.has_render_use {
            if facts.has_unconditional_render_use {
                self.has_unconditional_overlay_peer = true;
            } else if let Some(guards) = lowerable_guards {
                let branch = self.conditional_overlay_branches.entry(guards).or_default();
                branch.facts.is_nullable = true;
                branch.record_nullable_observation(source_null_tolerant);
                branch.record_metadata_field_kind(metadata_field_kind);
                branch.record_facts(facts);

                if let Some(provider_schema_use) = provider_schema_use {
                    branch.record_provider_schema_use(provider_schema_use);
                }
            } else {
                self.saw_unsupported_overlay = true;
            }
        }
        self.facts.record_nullable_observation(source_null_tolerant);
    }

    fn facts(&self, has_referenced_descendants: bool) -> ContractValuePathFacts {
        self.facts.facts(has_referenced_descendants)
    }

    fn into_schema_evidence(
        self,
        value_path: String,
        facts: ContractValuePathFacts,
    ) -> ContractPathSchemaEvidence {
        let ContractPathAccumulator {
            referenced,
            guard_predicates,
            facts: path_facts,
            requiredness,
            type_hints,
            conditional_overlay_branches,
            has_unconditional_overlay_peer,
            saw_unsupported_overlay,
        } = self;
        let conditional_overlays = conditional_overlays(
            conditional_overlay_branches,
            has_unconditional_overlay_peer,
            saw_unsupported_overlay,
            &type_hints,
            facts,
        );
        ContractPathSchemaEvidence {
            value_path,
            is_referenced_value_path: referenced,
            facts,
            guard_predicates,
            metadata_field_kinds: path_facts.metadata_field_kinds,
            type_hints,
            provider_schema_uses: path_facts.provider_schema_uses,
            requiredness,
            conditional_overlays,
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
    if path_contains_wildcard(&contract_use.source_expr) {
        return None;
    }

    let mut guards = Vec::new();
    for predicate in predicates {
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
        || contract_use.kind == ValueKind::PartialScalar
        || contract_use.path.0.is_empty()
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        resource,
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
        Predicate::True | Predicate::False => None,
        Predicate::Guard(guard) => guard_to_conditional_guard(guard, target_value_path),
        Predicate::Not(inner) => Some(ConditionalGuard::Not(Box::new(predicate_to_guard(
            inner,
            target_value_path,
        )?))),
        Predicate::And(predicates) => {
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
            let mut guards = predicates
                .iter()
                .map(|predicate| predicate_to_guard(predicate, target_value_path))
                .collect::<Option<Vec<_>>>()?;
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
        Predicate::True | Predicate::False => return None,
        Predicate::Guard(Guard::With { .. }) => {}
        Predicate::And(predicates) => {
            for predicate in predicates {
                extend_lowerable_predicate(predicate, target_value_path, out)?;
            }
        }
        Predicate::Guard(Guard::Range { .. }) => return None,
        Predicate::Guard(Guard::Default { path }) if path == target_value_path => {}
        other => {
            out.push(predicate_to_guard(other, Some(target_value_path))?);
        }
    }
    Some(())
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
        Guard::TypeIs {
            path: value_path,
            schema_type,
        } => Some(ConditionalGuard::TypeIs {
            path: path(value_path)?,
            schema_type: schema_type.clone(),
        }),
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
    path.split('.').any(|segment| segment == "*")
}

fn conditional_overlays(
    branches: BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>,
    preserve_base_schema: bool,
    saw_unsupported_overlay: bool,
    type_hints: &BTreeSet<String>,
    global_facts: ContractValuePathFacts,
) -> Vec<ConditionalPathOverlay> {
    if saw_unsupported_overlay {
        return Vec::new();
    }
    branches
        .into_iter()
        .map(|(guards, branch)| ConditionalPathOverlay {
            guards,
            evidence: branch.conditional_overlay_evidence(global_facts, type_hints.clone()),
            preserve_base_schema,
        })
        .collect()
}

fn collect_paths_with_descendants(paths: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for path in paths {
        let mut segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        while segments.len() > 1 {
            segments.pop();
            out.insert(segments.join("."));
        }
    }
    out
}
