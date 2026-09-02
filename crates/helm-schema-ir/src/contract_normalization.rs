use std::collections::{BTreeMap, BTreeSet};

use crate::contract::ContractUse;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};
use helm_schema_core::Predicate;

/// Finalizes primary and dependency claims through one normalization pipeline.
///
/// Raw DNF rows expand once into single-conjunction rows. Subsumption,
/// append, pathless-resource merging, and merge-source rebasing preserve that
/// expanded form. The final pass compacts equal render sites back into DNF.
#[tracing::instrument(skip_all)]
pub(crate) fn normalize_contract_uses(
    mut uses: Vec<ContractUse>,
    mut dependency_uses: Vec<ContractUse>,
    fail_conditions: &[crate::eval_effect::FailCapture],
) -> Vec<ContractUse> {
    canonicalize_contract_use_inputs(&mut uses);
    canonicalize_contract_use_inputs(&mut dependency_uses);
    expand_condition_disjuncts(&mut uses);
    expand_condition_disjuncts(&mut dependency_uses);

    drop_default_guard_subsumed_duplicates(&mut uses);
    drop_self_truthy_subsumed_duplicates(&mut uses);
    merge_pathless_resource_variants(&mut uses);
    drop_self_truthy_subsumed_duplicates(&mut uses);
    canonicalize_expanded_contract_uses(&mut uses);

    drop_self_truthy_subsumed_duplicates(&mut dependency_uses);
    canonicalize_expanded_contract_uses(&mut dependency_uses);
    uses.append(&mut dependency_uses);

    drop_default_guard_subsumed_duplicates(&mut uses);
    drop_self_truthy_subsumed_duplicates(&mut uses);
    canonicalize_expanded_contract_uses(&mut uses);

    lower_string_requirement_merge_sources(&mut uses, fail_conditions);
    // Path rebasing can make formerly distinct rows equal.
    canonicalize_expanded_contract_uses(&mut uses);
    compact_contract_uses(&mut uses);
    uses
}

/// Canonicalize contract claims without dropping semantically distinct rows.
///
/// Tests and expert callers use this when they provide already-structured
/// claims and need deterministic ordering without losing raw evidence such as
/// one nullable and one non-nullable render site.
#[tracing::instrument(skip_all)]
pub(crate) fn canonicalize_contract_uses(uses: &mut Vec<ContractUse>) {
    canonicalize_contract_use_inputs(uses);
    expand_condition_disjuncts(uses);
    canonicalize_expanded_contract_uses(uses);
    compact_contract_uses(uses);
}

fn canonicalize_expanded_contract_uses(uses: &mut Vec<ContractUse>) {
    canonicalize_contract_use_inputs(uses);
    // Deep `GuardDnf` comparisons dominate this sort, and conditions
    // repeat heavily across rows. Ranking each DISTINCT condition once and
    // comparing ranks yields the identical order (rank order is condition
    // order) at integer-comparison cost.
    let condition_ranks: Vec<u32> = {
        let distinct: std::collections::BTreeSet<&helm_schema_core::GuardDnf> = uses
            .iter()
            .map(|contract_use| &contract_use.condition)
            .collect();
        let rank_by_condition: std::collections::HashMap<&helm_schema_core::GuardDnf, u32> =
            distinct
                .into_iter()
                .enumerate()
                .map(|(rank, condition)| (condition, u32::try_from(rank).unwrap_or(u32::MAX)))
                .collect();
        uses.iter()
            .map(|contract_use| {
                rank_by_condition
                    .get(&contract_use.condition)
                    .copied()
                    .unwrap_or(u32::MAX)
            })
            .collect()
    };
    let mut rows: Vec<(u32, ContractUse)> = condition_ranks
        .into_iter()
        .zip(std::mem::take(uses))
        .collect();
    rows.sort_by(|(left_rank, left), (right_rank, right)| {
        contract_use_base_cmp(left, right)
            .then_with(|| left_rank.cmp(right_rank))
            // Equal semantic rows can carry different row-scoped flags.
            // Preserve the legacy full-row winner without a preliminary sort.
            .then_with(|| left.cmp(right))
    });

    let mut semantic_rows: Vec<(u32, ContractUse)> = Vec::with_capacity(rows.len());
    for (rank, contract_use) in rows {
        if let Some((existing_rank, existing)) = semantic_rows.last_mut()
            && *existing_rank == rank
            && contract_use_base_cmp(existing, &contract_use).is_eq()
        {
            merge_contract_use_provenance(existing, contract_use.provenance);
            continue;
        }
        semantic_rows.push((rank, contract_use));
    }

    *uses = semantic_rows
        .into_iter()
        .map(|(_, contract_use)| contract_use)
        .collect();
}

fn compact_contract_uses(uses: &mut Vec<ContractUse>) {
    let mut merged_sites: Vec<ContractUse> = Vec::with_capacity(uses.len());
    for contract_use in std::mem::take(uses) {
        if let Some(existing) = merged_sites.last_mut()
            && contract_use_base_cmp(existing, &contract_use).is_eq()
        {
            existing.condition.union_absorbing(contract_use.condition);
            merge_contract_use_provenance(existing, contract_use.provenance);
            continue;
        }
        merged_sites.push(contract_use);
    }
    *uses = merged_sites;
}

fn canonicalize_contract_use_inputs(uses: &mut [ContractUse]) {
    for contract_use in uses {
        contract_use.canonicalize();
    }
}

#[tracing::instrument(skip_all)]
fn merge_pathless_resource_variants(uses: &mut Vec<ContractUse>) {
    let mut merged: Vec<ContractUse> = Vec::with_capacity(uses.len());
    let mut pathless_index_by_identity: BTreeMap<
        (helm_schema_core::ValuesPath, ValueKind, BTreeSet<Predicate>),
        usize,
    > = BTreeMap::new();

    for mut contract_use in std::mem::take(uses) {
        if contract_use.path.0.is_empty() {
            let key = (
                contract_use.source_expr.clone(),
                contract_use.kind,
                contract_predicates(&contract_use),
            );
            if let Some(existing) = pathless_index_by_identity
                .get(&key)
                .and_then(|index| merged.get_mut(*index))
                && (existing.resource.is_none()
                    || contract_use.resource.is_none()
                    || existing.resource == contract_use.resource)
            {
                if existing.resource.is_none() {
                    existing.resource = contract_use.resource.take();
                }
                merge_contract_use_provenance(existing, contract_use.provenance);
                continue;
            }
            pathless_index_by_identity.insert(key, merged.len());
        }
        merged.push(contract_use);
    }

    *uses = merged;
}

#[tracing::instrument(skip_all)]
pub(crate) fn drop_default_guard_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    let defaulted_render_sites: BTreeSet<_> = uses
        .iter()
        .filter(|contract_use| has_self_default_guard(contract_use))
        .map(render_site)
        .collect();

    uses.retain(|contract_use| {
        if has_self_default_guard(contract_use) {
            return true;
        }
        !defaulted_render_sites.contains(&render_site(contract_use))
    });
}

#[tracing::instrument(skip_all)]
pub(crate) fn drop_self_truthy_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    // The subsumption scan only ever compares rows sharing one render site
    // (source, path, kind, resource), so group indices once and keep the
    // quadratic candidate scan inside those buckets instead of over all rows.
    let mut buckets: BTreeMap<
        (
            &helm_schema_core::ValuesPath,
            &YamlPath,
            ValueKind,
            Option<&ResourceRef>,
        ),
        Vec<usize>,
    > = BTreeMap::new();
    for (index, contract_use) in uses.iter().enumerate() {
        buckets
            .entry((
                &contract_use.source_expr,
                &contract_use.path,
                contract_use.kind,
                contract_use.resource.as_ref(),
            ))
            .or_default()
            .push(index);
    }

    let mut keep = vec![true; uses.len()];
    let predicates_by_index = uses.iter().map(contract_predicates).collect::<Vec<_>>();
    for indices in buckets.values() {
        if indices.len() < 2 {
            continue;
        }
        for &index in indices {
            let Some(contract_use) = uses.get(index) else {
                continue;
            };
            let source_path = contract_use.source_expr.clone();
            let predicates = predicates_by_index.get(index).cloned().unwrap_or_default();
            let has_self_truthy = predicates.iter().any(
                |predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == &source_path),
            );
            if predicates.iter().any(
                |predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Default { path }) if path == &source_path),
            ) {
                continue;
            }
            let subsumed = indices
                .iter()
                .filter_map(|&other_index| {
                    uses.get(other_index)
                        .zip(predicates_by_index.get(other_index))
                })
                // Cheapest discriminant first: a subsuming row must carry
                // strictly MORE predicates, so length filters out most of the
                // bucket before any set or provenance comparison runs.
                .filter(|(_, other_predicates)| other_predicates.len() > predicates.len())
                .any(|(other, other_predicates)| {
                    !other.provenance.is_empty()
                        && ((contract_use.provenance.is_empty()
                            && contract_use.resource.is_some())
                            || other.provenance == contract_use.provenance)
                        && predicates.is_subset(other_predicates)
                        && ((!has_self_truthy
                            && other_predicates.iter().any(|predicate| {
                                matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == &source_path)
                            }))
                            || extra_predicates_are_truthy_parents(
                                &predicates,
                                other_predicates,
                            ))
                });
            if subsumed && let Some(flag) = keep.get_mut(index) {
                *flag = false;
            }
        }
    }

    let mut index = 0;
    uses.retain(|_| {
        let kept = keep.get(index).copied().unwrap_or(true);
        index += 1;
        kept
    });
}

fn extra_predicates_are_truthy_parents(
    predicates: &BTreeSet<Predicate>,
    other_predicates: &BTreeSet<Predicate>,
) -> bool {
    other_predicates
        .iter()
        .filter(|predicate| !predicates.contains(predicate))
        .all(|predicate| {
            let helm_schema_core::PredicateKind::Guard(Guard::Truthy { path: parent }) =
                predicate.kind()
            else {
                return false;
            };
            predicates.iter().any(|existing| {
                matches!(
                    existing.kind(),
                    helm_schema_core::PredicateKind::Guard(Guard::Truthy { path: child })
                        if child.is_descendant_of(parent)
                )
            })
        })
}

fn string_requirements_by_ancestor(
    fail_conditions: &[crate::eval_effect::FailCapture],
) -> BTreeMap<String, BTreeSet<(String, helm_schema_core::Conjunction)>> {
    let mut requirements: BTreeMap<String, BTreeSet<(String, helm_schema_core::Conjunction)>> =
        BTreeMap::new();
    for capture in fail_conditions {
        let crate::eval_effect::CaptureKind::StringRequirement {
            path,
            route,
            selection,
        } = &capture.kind
        else {
            continue;
        };
        if *route == crate::eval_effect::StringRequirementRoute::Serialized {
            continue;
        }
        let mut predicates = capture.conjunction.clone();
        predicates.extend(selection.iter().cloned());
        let segments = path
            .segments()
            .map(helm_schema_core::Segment::encode_component)
            .collect::<Vec<_>>();
        for end in 1..segments.len() {
            requirements
                .entry(helm_schema_core::join_encoded_value_path(
                    segments.get(..end).unwrap_or_default().iter().cloned(),
                ))
                .or_default()
                .insert((path.encode(), predicates.clone()));
        }
    }
    requirements
}

fn lower_string_requirement_merge_sources(
    uses: &mut [ContractUse],
    fail_conditions: &[crate::eval_effect::FailCapture],
) {
    let requirements_by_ancestor = string_requirements_by_ancestor(fail_conditions);
    let mut overlap_cache = BTreeMap::new();

    for contract_use in uses {
        let Some(merge) = &contract_use.merge_layers else {
            continue;
        };
        let source_expr = contract_use.source_expr.encode();
        let Some(requirements) = requirements_by_ancestor.get(&source_expr) else {
            continue;
        };
        let source_segments = contract_use
            .source_expr
            .segments()
            .map(helm_schema_core::Segment::encode_component)
            .collect::<Vec<_>>();
        let specific_layers = merge
            .layers()
            .iter()
            .map(|layer| {
                layer
                    .path
                    .segments()
                    .map(helm_schema_core::Segment::encode_component)
                    .collect::<Vec<_>>()
            })
            .filter(|layer| {
                layer.len() > source_segments.len() && layer.starts_with(source_segments.as_slice())
            })
            .collect::<Vec<_>>();
        let [first, second, rest @ ..] = specific_layers.as_slice() else {
            continue;
        };
        let mut common_suffix_len = first
            .iter()
            .rev()
            .zip(second.iter().rev())
            .take_while(|(left, right)| left == right)
            .count();
        for layer in rest {
            common_suffix_len = common_suffix_len.min(
                first
                    .iter()
                    .rev()
                    .zip(layer.iter().rev())
                    .take_while(|(left, right)| left == right)
                    .count(),
            );
        }
        let suffix = first.get(first.len().saturating_sub(common_suffix_len)..);
        let Some(suffix) = suffix.filter(|suffix| {
            !suffix.is_empty() && suffix.iter().all(|segment| segment.as_str() != "*")
        }) else {
            continue;
        };
        let Some(requirements) =
            merge_suffix_string_requirements(&source_expr, requirements, suffix)
        else {
            continue;
        };
        let cache_key = (
            contract_use.source_expr.clone(),
            suffix.to_vec(),
            contract_use.condition.clone(),
        );
        let overlaps_requirement = overlap_cache.get(&cache_key).copied().unwrap_or_else(|| {
            let overlaps = contract_use.condition.disjuncts().iter().any(|row| {
                requirements.iter().any(|requirement| {
                    !helm_schema_core::GuardDnf::from_conjunction(
                        row.iter().cloned().chain(requirement.iter().cloned()),
                    )
                    .is_never()
                })
            });
            overlap_cache.insert(cache_key, overlaps);
            overlaps
        });
        if !overlaps_requirement {
            continue;
        }
        let source = helm_schema_core::ValuesPath::parse(&source_expr);
        let projected =
            helm_schema_core::ValuesPath::parse(&helm_schema_core::join_encoded_value_path(
                source_segments
                    .iter()
                    .cloned()
                    .chain(suffix.iter().cloned()),
            ));
        contract_use.map_value_paths(&mut |path| {
            if path == source {
                projected.clone()
            } else {
                path
            }
        });
    }
}

fn merge_suffix_string_requirements(
    source: &str,
    requirements: &BTreeSet<(String, helm_schema_core::Conjunction)>,
    suffix: &[String],
) -> Option<BTreeSet<helm_schema_core::Conjunction>> {
    let source_segments = helm_schema_core::split_value_path(source);
    let mut paths = BTreeSet::new();
    let mut conjunctions = BTreeSet::new();
    for (path, predicates) in requirements {
        let segments = helm_schema_core::split_value_path(path);
        if segments.len() <= source_segments.len()
            || !segments.starts_with(source_segments.as_slice())
            || !segments.ends_with(suffix)
        {
            continue;
        }
        paths.insert(segments);
        conjunctions.insert(predicates.clone());
    }
    (paths.len() >= 2).then_some(conjunctions)
}

type RenderSite = (
    helm_schema_core::ValuesPath,
    YamlPath,
    ValueKind,
    Option<ResourceRef>,
);

fn render_site(contract_use: &ContractUse) -> RenderSite {
    (
        contract_use.source_expr.clone(),
        contract_use.path.clone(),
        contract_use.kind,
        contract_use.resource.clone(),
    )
}

fn has_self_default_guard(contract_use: &ContractUse) -> bool {
    let source_path = contract_use.source_expr.clone();
    contract_predicates(contract_use)
        .iter()
        .any(|predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Default { path }) if path == &source_path))
}

fn contract_predicates(contract_use: &ContractUse) -> BTreeSet<Predicate> {
    contract_use
        .condition
        .disjuncts()
        .iter()
        .next()
        .cloned()
        .unwrap_or_default()
}

fn expand_condition_disjuncts(uses: &mut Vec<ContractUse>) {
    let mut expanded = Vec::new();
    for contract_use in std::mem::take(uses) {
        // The single-disjunct row (the common case after the first expansion)
        // moves through unchanged instead of being re-cloned per pass.
        if contract_use.condition.disjuncts().len() <= 1 {
            expanded.push(contract_use);
            continue;
        }
        for conjunction in contract_use.condition.disjuncts() {
            let mut branch = contract_use.clone();
            branch.condition =
                helm_schema_core::GuardDnf::from_conjunction(conjunction.iter().cloned());
            expanded.push(branch);
        }
    }
    *uses = expanded;
}

fn contract_use_base_cmp(left: &ContractUse, right: &ContractUse) -> std::cmp::Ordering {
    left.source_expr
        .cmp(&right.source_expr)
        .then_with(|| left.path.0.cmp(&right.path.0))
        .then_with(|| (left.kind as u8).cmp(&(right.kind as u8)))
        .then_with(|| left.resource.cmp(&right.resource))
        // These row-scoped semantics must not fold into a plain row at the
        // same site: the fold keeps one row's marker for ALL unioned
        // disjuncts and incorrectly attributes the other's semantics.
        // Range keys are especially distinct from values: their provider
        // slots constrain the collection's key domain, never its payload.
        .then_with(|| left.merge_layers.cmp(&right.merge_layers))
        .then_with(|| left.digest.cmp(&right.digest))
        .then_with(|| left.merge_operand.cmp(&right.merge_operand))
        .then_with(|| left.range_key.cmp(&right.range_key))
}

fn merge_contract_use_provenance(
    target: &mut ContractUse,
    incoming: Vec<crate::ContractProvenance>,
) {
    target.provenance.extend(incoming);
    target.provenance.sort();
    target.provenance.dedup();
}

#[cfg(test)]
#[path = "tests/contract_normalization.rs"]
mod tests;
