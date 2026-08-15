use super::{
    BTreeMap, BTreeSet, ContractFailImplication, ContractRequirementTarget, ContractSchemaSignals,
    ContractUse, ContractValuePathFacts, FailValueRequirement, finish_schema_signals,
    path_accumulator, record_contract_use, record_fail_conjunction,
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
    range_modes: &crate::range_modes::RangeModes,
    fail_conditions: &[crate::eval_effect::FailCapture],
    dependency_values_root_fragments: &BTreeSet<String>,
) -> ContractSchemaSignals {
    let mut paths = BTreeMap::new();
    let mut terminal_clauses = Vec::new();
    let string_requirement_routes = string_requirement_routes(fail_conditions);
    let yaml_serialized_paths = yaml_serialized_paths(uses);
    for contract_use in uses {
        let routes = string_requirement_routes
            .get(&contract_use.source_expr)
            .map(Vec::as_slice)
            .unwrap_or_default();
        record_contract_use(
            &mut paths,
            contract_use,
            range_modes,
            routes,
            yaml_serialized_paths.contains(contract_use.source_expr.as_str()),
        );
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
            // Helm's dependency coalescing type-asserts every loaded
            // dependency's values root BEFORE any rendering and regardless
            // of the dependency's own activation: a present non-table
            // aborts with "type mismatch on <name>" (verified against a
            // condition-disabled dependency). The chart declares the key
            // as a mapping, so a user null is deleted by the values
            // coalesce that runs first and reaches the check as absent —
            // hence the null-tolerant form.
            let requires_table = ContractFailImplication {
                outer_guards: Vec::new(),
                target: ContractRequirementTarget::Value,
                requirements: vec![FailValueRequirement::SchemaType("object".to_string())],
            };
            if !acc.fail_implications.contains(&requires_table) {
                acc.fail_implications.push(requires_table);
            }
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
    fail_conditions: &[crate::eval_effect::FailCapture],
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
