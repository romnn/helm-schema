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
        acc.facts.facts.has_non_self_guarded_string_contract = true;
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
