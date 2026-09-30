use super::{
    BTreeMap, BTreeSet, ContractRequirementImplication, ContractRequirementTarget,
    ContractSchemaSignals, ContractUse, ContractValuePathFacts, FailValueRequirement,
    finish_schema_signals, path_accumulator, record_contract_use, record_fail_conjunction,
};
use crate::observed_facts::{HintIntent, HintScope, ObservedFacts};

#[tracing::instrument(skip_all)]
pub(crate) fn derive_schema_signals_from_contract_parts(
    uses: &[ContractUse],
    observed_facts: &ObservedFacts,
    dependency_values_roots: &BTreeSet<crate::DependencyValuesRoot>,
    predicate_memo: &helm_schema_core::PredicateMemo,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    let mut terminal_clauses = Vec::new();
    let string_requirement_routes = string_requirement_routes(&observed_facts.captures);
    let yaml_serialized_paths = yaml_serialized_paths(uses);
    for contract_use in uses {
        let routes = string_requirement_routes
            .get(&contract_use.source_expr)
            .map(Vec::as_slice)
            .unwrap_or_default();
        record_contract_use(
            &mut paths,
            contract_use,
            &observed_facts.range_modes,
            routes,
            yaml_serialized_paths.contains(&contract_use.source_expr),
            predicate_memo,
        );
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
    for (grade, hints) in &observed_facts.type_hints {
        for (value_path, schema_types) in hints {
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
            match (grade.scope, grade.intent) {
                (HintScope::Unconditional, HintIntent::Declared) => {
                    acc.type_hints.extend(schema_types);
                }
                (HintScope::Guarded, HintIntent::Declared) => {
                    acc.guarded_type_hints.extend(schema_types);
                }
                (HintScope::Unconditional, HintIntent::Fallback) => {
                    acc.fallback_type_hints.extend(schema_types);
                }
                (HintScope::Guarded, HintIntent::Fallback) => {
                    acc.guarded_fallback_type_hints.extend(schema_types);
                }
                (_, HintIntent::Tested) => {}
            }
        }
    }
    finish_schema_signals(paths, terminal_clauses, predicate_memo)
}

fn yaml_serialized_paths(uses: &[ContractUse]) -> BTreeSet<&helm_schema_core::ValuesPath> {
    uses.iter()
        .filter(|contract_use| {
            matches!(
                contract_use.kind,
                crate::ValueKind::YamlSerialized | crate::ValueKind::TemplatedYamlSerialized
            )
        })
        .map(|contract_use| &contract_use.source_expr)
        .collect()
}

fn string_requirement_routes(
    fail_conditions: &BTreeSet<crate::eval_effect::FailCapture>,
) -> BTreeMap<
    helm_schema_core::ValuesPath,
    Vec<(
        crate::eval_effect::StringRequirementRoute,
        helm_schema_core::Conjunction,
    )>,
> {
    let mut routes = BTreeMap::<
        helm_schema_core::ValuesPath,
        BTreeSet<(
            crate::eval_effect::StringRequirementRoute,
            helm_schema_core::Conjunction,
        )>,
    >::new();
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
        if predicates
            .iter()
            .any(helm_schema_core::Predicate::contains_approximation)
        {
            continue;
        }
        routes
            .entry(path.clone())
            .or_default()
            .insert((*route, predicates));
    }
    routes
        .into_iter()
        .map(|(path, routes)| {
            let mut routes = routes.into_iter().collect::<Vec<_>>();
            routes.sort_by(|left, right| left.1.len().cmp(&right.1.len()).then(left.cmp(right)));
            let mut minimal = Vec::<(
                crate::eval_effect::StringRequirementRoute,
                helm_schema_core::Conjunction,
            )>::new();
            for route in routes {
                if !minimal.iter().any(|broader| {
                    broader.0 == route.0
                        && broader
                            .1
                            .iter()
                            .all(|predicate| route.1.contains(predicate))
                }) {
                    minimal.push(route);
                }
            }
            (path, minimal)
        })
        .collect()
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
