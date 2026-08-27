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
    dependency_values_root_fragments: &BTreeSet<String>,
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
            yaml_serialized_paths.contains(contract_use.source_expr.as_str()),
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
    for value_path in dependency_values_root_fragments {
        if !value_path.trim().is_empty() {
            let acc = path_accumulator(&mut paths, value_path);
            acc.referenced = true;
            acc.facts.record_facts(ContractValuePathFacts {
                accepted_values_root_fragment: true,
                accepted_dependency_values_root_fragment: true,
                ..ContractValuePathFacts::default()
            });
            // Helm's dependency coalescing type-asserts every loaded
            // dependency's values root BEFORE any rendering and regardless
            // of the dependency's own activation: a present non-table
            // aborts with "type mismatch on <name>" (verified against a
            // condition-disabled dependency). The chart declares the key
            // as a mapping, so a user null is deleted by the values
            // coalesce that runs first and reaches the check as absent —
            // hence the null-tolerant form.
            let requires_table = ContractRequirementImplication {
                outer_guards: Vec::new(),
                target: ContractRequirementTarget::Value,
                requirements: vec![FailValueRequirement::SchemaType("object".to_string())],
            };
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
        if value_path.trim().is_empty() {
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
            if value_path.trim().is_empty() || schema_types.is_empty() {
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
    finish_schema_signals(paths, terminal_clauses)
}

fn yaml_serialized_paths(uses: &[ContractUse]) -> BTreeSet<&str> {
    uses.iter()
        .filter(|contract_use| {
            matches!(
                contract_use.kind,
                crate::ValueKind::YamlSerialized | crate::ValueKind::TemplatedYamlSerialized
            )
        })
        .map(|contract_use| contract_use.source_expr.as_str())
        .collect()
}

fn string_requirement_routes(
    fail_conditions: &BTreeSet<crate::eval_effect::FailCapture>,
) -> BTreeMap<
    String,
    Vec<(
        crate::eval_effect::StringRequirementRoute,
        Vec<helm_schema_core::Predicate>,
    )>,
> {
    let mut routes = BTreeMap::<
        String,
        BTreeSet<(
            crate::eval_effect::StringRequirementRoute,
            Vec<helm_schema_core::Predicate>,
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
        predicates.sort();
        predicates.dedup();
        if predicates
            .iter()
            .any(helm_schema_core::Predicate::contains_approximation)
        {
            continue;
        }
        routes
            .entry(path.encode())
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
                Vec<helm_schema_core::Predicate>,
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
