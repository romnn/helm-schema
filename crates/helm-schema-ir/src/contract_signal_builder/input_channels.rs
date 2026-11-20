use super::{
    BTreeMap, BTreeSet, ContractRequirementImplication, ContractRequirementTarget,
    ContractSchemaSignals, ContractUse, ContractValuePathFacts, FailValueRequirement,
    finish_schema_signals, path_accumulator, record_contract_use, record_fail_conjunction,
};
use crate::observed_facts::ObservedFacts;

#[tracing::instrument(skip_all)]
pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    observed_facts: &ObservedFacts,
    dependency_values_roots: &BTreeSet<crate::DependencyValuesRoot>,
    predicate_memo: &helm_schema_core::PredicateMemo,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    let mut terminal_clauses = Vec::new();
    for contract_use in uses {
        record_contract_use(&mut paths, contract_use, &observed_facts.range_modes);
    }
    for capture in &observed_facts.captures {
        record_fail_conjunction(
            &mut paths,
            &mut terminal_clauses,
            capture,
            &observed_facts.range_modes,
        );
    }
    for root in dependency_values_roots {
        if root.path.segments().next().is_none() {
            continue;
        }
        let acc = path_accumulator(&mut paths, &root.path);
        acc.referenced = true;
        acc.facts.record_facts(ContractValuePathFacts {
            accepted_values_root_fragment: true,
            accepted_dependency_values_root_fragment: true,
            ..ContractValuePathFacts::default()
        });
        for requires_table in dependency_root_table_requirements(root) {
            if !acc.requirement_implications.contains(&requires_table) {
                acc.requirement_implications.push(requires_table);
            }
        }
    }
    // A path the chart consumes through a total stringification tolerates
    // any input type, even when the flow is too indirect for a placed row
    // (vault's `set . "csiEnabled" (eq (.Values.csi.enabled | toString)
    // "true")`); the fact carries the same serialized dominance a
    // stringified render does.
    for value_path in &observed_facts.shape_erased_paths {
        if value_path.segments().next().is_none() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.referenced = true;
        acc.facts.facts.used_as_serialized = true;
    }
    for (value_path, schema_types) in &observed_facts.type_hints {
        let schema_types = schema_types
            .iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();
        if value_path.segments().next().is_none() || schema_types.is_empty() {
            continue;
        }
        let acc = path_accumulator(&mut paths, value_path);
        acc.referenced = true;
        acc.type_hints.extend(schema_types);
    }
    finish_schema_signals(
        paths,
        terminal_clauses,
        &observed_facts.consumptions,
        predicate_memo,
    )
    .with_resource_observations(resource_observations(observed_facts, predicate_memo))
}

fn resource_observations<'a>(
    observed_facts: &'a ObservedFacts,
    predicate_memo: &'a helm_schema_core::PredicateMemo,
) -> impl Iterator<Item = helm_schema_core::ResourceObservation> + 'a {
    observed_facts.resources.iter().filter_map(
        |crate::observed_facts::ObservedResource {
             condition,
             resource,
         }| {
            let condition = predicate_memo.normalize(condition.clone());
            if condition == helm_schema_core::Predicate::False {
                return None;
            }
            let activation = if condition == helm_schema_core::Predicate::True {
                Some(Vec::new())
            } else if condition.contains_approximation()
                || condition.value_paths().iter().any(|path| {
                    path.segments()
                        .any(helm_schema_core::Segment::is_each_member)
                })
            {
                None
            } else {
                super::terminal_clause_guard(&condition).map(|guard| vec![guard])
            };
            Some(helm_schema_core::ResourceObservation {
                resource: resource.clone(),
                activation,
            })
        },
    )
}

/// Helm's assertions on a dependency values root, as they bind the final
/// document (see [`crate::DependencyValuesRoot`]).
///
/// While the instance is active every present non-table aborts, null
/// included: the rendering pass deletes a user null only where a default
/// declares the key, and a null that survived that far failed the first pass
/// already. While it is pruned only the first pass applies: it rejects a
/// scalar or list when the root is asserted there, and a null too when the
/// parent declares no default that deletes it first.
fn dependency_root_table_requirements(
    root: &crate::DependencyValuesRoot,
) -> Vec<ContractRequirementImplication> {
    let requirement = |outer_guards, requirement| {
        ContractRequirementImplication::new(
            outer_guards,
            ContractRequirementTarget::Value,
            vec![requirement],
        )
    };
    let table_even_null = || FailValueRequirement::SchemaTypeEvenNull("object".to_string());
    if root.active.is_empty() || (root.asserted_before_pruning && !root.declared_by_parent) {
        return vec![requirement(Vec::new(), table_even_null())];
    }
    let mut requirements = Vec::new();
    if root.asserted_before_pruning {
        requirements.push(requirement(
            Vec::new(),
            FailValueRequirement::SchemaType("object".to_string()),
        ));
    }
    for guards in &root.active {
        // An alternative the schema cannot express keeps its null accepted.
        if let Ok(outer_guards) = guards
            .iter()
            .map(helm_schema_core::ConditionalGuard::try_from)
            .collect::<Result<Vec<_>, _>>()
        {
            requirements.push(requirement(outer_guards, table_even_null()));
        }
    }
    requirements
}
