use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::eval_effect::Effects;
use crate::helper_summary::{
    HelperFragmentOutputUse, HelperOutputMeta, HelperSummary, values_paths_are_related,
};
use crate::{Guard, ValueKind, YamlPath};
use helm_schema_ast::{OutputSlot, OutputSlotKind};
use helm_schema_core as output_path;

use super::SymbolicWalker;

impl SymbolicWalker<'_> {
    #[tracing::instrument(skip_all)]
    pub(super) fn handle_output_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        exprs: &[TemplateExpr],
    ) {
        self.inline_static_file_templates_from_helper_calls(exprs);

        let output_slot = self
            .attribution
            .output_slot_for_node(node)
            .unwrap_or_default();
        if output_slot.slot == OutputSlotKind::YamlComment {
            return;
        }

        if output_slot.kind == crate::ValueKind::Scalar {
            self.inline_exact_helper_call(exprs);
        }

        let output_effects = self.value_path_context().expression_output_effects(exprs);
        let mut helper = self.summarize_bound_helper_calls_in_exprs(exprs);
        // Stash chart-level `set X "K" (X.K | default V)` mutations discovered
        // in any helper called from this text. Subsequent contract emissions
        // in this walker attach `Guard::Default { path }` for matching reads,
        // which models that the helper's `set` has already run by the time
        // those reads are evaluated.
        let mut chart_value_defaults = helper.take_chart_value_defaults();
        self.scope
            .locals_mut()
            .append_chart_value_defaults(&mut chart_value_defaults);

        let document_contract = {
            let guards = self.contract_guards();
            let projection_context = ContractUseContext::new(
                &guards,
                &self.scope.locals().chart_value_defaults,
                self.no_output_depth > 0,
                output_slot.resource.clone(),
                self.source_path,
                Some(SourceSpan::new(
                    self.source_offset + node.start_byte(),
                    self.source_offset + node.end_byte(),
                )),
                self.provenance_helper_chain(),
            );
            output_contract(output_slot, output_effects, helper, &projection_context)
        };
        self.contract.append(document_contract);
    }
}

fn output_contract(
    site: OutputSlot,
    mut output_effects: Effects,
    helper: HelperSummary,
    context: &ContractUseContext<'_>,
) -> ContractIr {
    let mut contract = ContractIr::default();
    if site.kind == ValueKind::Scalar {
        let all_values = output_effects.output_paths.clone();
        output_effects
            .output_paths
            .retain(|path| !output_path::values_path_has_descendant(path, &all_values));
    }

    if output_effects.output_paths.is_empty()
        && output_effects.bound_output_paths.is_empty()
        && !helper.has_document_value_facts()
    {
        return contract;
    }

    let suppressed_guard_path_meta = suppressed_guard_path_meta(&helper);
    let mut suppress_direct_values = helper.dependency_relevant_paths();
    suppress_direct_values.extend(helper.suppress_roots.iter().cloned());

    let output_values = std::mem::take(&mut output_effects.output_paths);
    for value in output_values {
        if suppress_direct_values.contains(&value)
            || suppress_direct_values
                .iter()
                .any(|root| output_path::values_path_is_descendant(&value, root))
        {
            let provenance = suppressed_guard_path_meta
                .get(&value)
                .map(|meta| meta.provenance.as_slice())
                .unwrap_or_default();
            contract.push(context.contract_use_with_extra_provenance(
                value,
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                &[],
                provenance,
            ));
            continue;
        }

        let default_guard = Guard::Default {
            path: value.clone(),
        };
        let provider_path_suppressed = output_effects.encoded_paths.contains(&value);
        let emit_path = site.direct_value_path(&value);
        let emit_kind = if provider_path_suppressed {
            ValueKind::PartialScalar
        } else {
            site.direct_value_kind()
        };
        let mut guard_sets = output_effects
            .local_output_meta
            .get(&value)
            .map(|meta| meta.contract_guard_sets(&value))
            .unwrap_or_else(|| vec![Vec::new()]);
        for extra_guards in &mut guard_sets {
            if output_effects.defaults.contains(&value) && !extra_guards.contains(&default_guard) {
                extra_guards.push(default_guard.clone());
            }
            contract.push(context.contract_use(
                value.clone(),
                emit_path.clone(),
                emit_kind,
                extra_guards,
            ));
        }
    }

    let bound_output_values = std::mem::take(&mut output_effects.bound_output_paths);
    for value in bound_output_values {
        contract.push(context.contract_use(value, YamlPath(Vec::new()), ValueKind::Scalar, &[]));
    }

    contract.extend_type_hints(output_effects.schema_type_hints());
    append_helper_contract_uses(
        &helper,
        &output_effects.encoded_paths,
        &site,
        &mut contract,
        context,
    );
    contract
}

fn append_helper_contract_uses(
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &OutputSlot,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    contract.extend_type_hints(helper.type_hints.clone());
    let helper_output_sources = helper
        .output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect::<BTreeSet<_>>();
    let suppressed_guard_path_meta = suppressed_guard_path_meta(helper);
    let helper_has_only_scalar_outputs = helper
        .output_uses
        .iter()
        .filter(|output| output.is_rendered())
        .all(HelperFragmentOutputUse::is_scalar_summary_output);
    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_scalar_summary_output())
    {
        let value = &output.source_expr;
        if helper.has_structured_fragment_source(value) {
            continue;
        }
        let mut meta = output.meta.clone();
        meta.note_sibling_sources(value, &helper_output_sources);
        for extra_guards in meta.contract_guard_sets(value) {
            let emit_kind = encoded_kind(site.kind, encoded_output_values.contains(value));
            if helper_has_only_scalar_outputs
                && site.can_project_scalar_helper_to_caller_path()
                && !helper.has_rendered_source_descendant(value)
            {
                contract.push(context.contract_use_with_extra_provenance(
                    value.clone(),
                    site.path.clone(),
                    emit_kind,
                    &extra_guards,
                    &output.meta.provenance,
                ));
            } else {
                contract.push(context.pathless_contract_use_with_extra_provenance(
                    value.clone(),
                    ValueKind::Scalar,
                    &extra_guards,
                    &output.meta.provenance,
                ));
            }
        }
    }

    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_structured_output())
    {
        append_fragment_output_contract_use(
            output,
            helper,
            encoded_output_values,
            site,
            contract,
            context,
        );
    }

    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_dependency())
    {
        let value = &output.source_expr;
        let mut meta = output.meta.clone();
        meta.note_sibling_sources(value, &helper_output_sources);
        let mut provenance = meta.provenance.clone();
        if let Some(parent_meta) = suppressed_guard_path_meta.get(value) {
            for site in &parent_meta.provenance {
                if !provenance.contains(site) {
                    provenance.push(site.clone());
                }
            }
        }
        let structured_scalar_guard_sets = structured_scalar_output_guard_sets(helper, value);
        let defaulted_scalar_guard_sets = defaulted_scalar_summary_guard_sets(helper, value);
        for extra_guards in meta.contract_guard_sets(value) {
            if provenance.is_empty()
                && (structured_scalar_guard_sets.contains(&extra_guards)
                    || defaulted_scalar_guard_sets.contains(&extra_guards))
            {
                continue;
            }
            contract.push_dependency_use(context.pathless_contract_use_with_extra_provenance(
                value.clone(),
                ValueKind::Scalar,
                &extra_guards,
                &provenance,
            ));
        }
    }

    for value in helper.guard_path_meta.keys() {
        if !suppressed_guard_path_meta.contains_key(value)
            && suppressed_guard_path_meta
                .iter()
                .any(|(path, _meta)| output_path::values_path_is_descendant(path, value))
        {
            continue;
        }
        let guard_path_meta = merged_guard_path_meta(helper, &suppressed_guard_path_meta, value);
        let guard_path_has_own_context = guard_path_meta
            .as_ref()
            .is_some_and(|meta| !meta.predicates.is_empty() || meta.defaulted);
        if !guard_path_has_own_context
            && defaulted_scalar_summary_guard_sets(helper, value)
                .iter()
                .any(Vec::is_empty)
        {
            continue;
        }
        let guard_path_meta_has_context = guard_path_meta.as_ref().is_some_and(|meta| {
            !meta.predicates.is_empty() || meta.defaulted || context.has_ambient_guards()
        });
        if site.path.0.is_empty()
            && site.resource.is_none()
            && helper_output_sources.contains(value)
            && !guard_path_meta_has_context
        {
            continue;
        }
        let lower_guard_path_meta = site.path.0.is_empty()
            && (site.resource.is_none() || suppressed_guard_path_meta.contains_key(value));
        if lower_guard_path_meta
            && let Some(meta) = guard_path_meta.as_ref()
            && guard_path_meta_has_context
        {
            for extra_guards in meta.contract_guard_sets(value) {
                contract.push(context.pathless_contract_use_with_extra_provenance(
                    value.clone(),
                    ValueKind::Scalar,
                    &extra_guards,
                    &meta.provenance,
                ));
            }
        } else {
            contract.push(context.pathless_contract_use(value.clone(), ValueKind::Scalar, &[]));
        }
    }
}

fn merged_guard_path_meta(
    helper: &HelperSummary,
    suppressed_guard_path_meta: &BTreeMap<String, HelperOutputMeta>,
    value: &str,
) -> Option<HelperOutputMeta> {
    let mut meta = helper.guard_path_meta.get(value).cloned();
    if let Some(suppressed_meta) = suppressed_guard_path_meta.get(value) {
        match &mut meta {
            Some(meta) => meta.merge_ref(suppressed_meta),
            None => meta = Some(suppressed_meta.clone()),
        }
    }
    meta
}

fn suppressed_guard_path_meta(helper: &HelperSummary) -> BTreeMap<String, HelperOutputMeta> {
    let mut by_path = BTreeMap::new();
    for output in &helper.output_uses {
        if output.meta.provenance.is_empty() {
            continue;
        }
        for path in &output.meta.suppress_predicate_paths {
            let mut meta = HelperOutputMeta::default();
            for provenance in &output.meta.provenance {
                meta.add_provenance_site(provenance.clone());
            }
            by_path
                .entry(path.clone())
                .or_insert_with(HelperOutputMeta::default)
                .merge(meta);
        }
    }
    by_path
}

fn structured_scalar_output_guard_sets(helper: &HelperSummary, value: &str) -> Vec<Vec<Guard>> {
    let mut guard_sets = Vec::new();
    for output in helper.output_uses.iter().filter(|output| {
        output.is_rendered()
            && output.source_expr == value
            && output.kind == ValueKind::Scalar
            && !output.relative_path.0.is_empty()
    }) {
        for guards in output.meta.contract_guard_sets(value) {
            if !guard_sets.contains(&guards) {
                guard_sets.push(guards);
            }
        }
    }
    guard_sets
}

fn defaulted_scalar_summary_guard_sets(helper: &HelperSummary, value: &str) -> Vec<Vec<Guard>> {
    let mut guard_sets = Vec::new();
    for output in helper.output_uses.iter().filter(|output| {
        output.is_scalar_summary_output() && output.source_expr == value && output.meta.defaulted
    }) {
        for guards in output.meta.contract_guard_sets(value) {
            let stripped = guards
                .into_iter()
                .filter(|guard| !is_self_default_or_truthy_guard(guard, value))
                .collect::<Vec<_>>();
            if !guard_sets.contains(&stripped) {
                guard_sets.push(stripped);
            }
        }
    }
    guard_sets
}

fn is_self_default_or_truthy_guard(guard: &Guard, value: &str) -> bool {
    matches!(
        guard,
        Guard::Default { path } | Guard::Truthy { path } if path == value
    )
}

fn append_fragment_output_contract_use(
    output: &HelperFragmentOutputUse,
    helper: &HelperSummary,
    encoded_output_values: &BTreeSet<String>,
    site: &OutputSlot,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    let helper_output_sources = helper
        .output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect::<BTreeSet<_>>();
    let mut meta = output.meta.clone();
    meta.prune_source_not_for_sibling_truthy(&output.source_expr, &helper_output_sources);
    if !output.relative_path.0.is_empty() {
        meta.prune_truthy_ancestors_of_source(&output.source_expr);
    }
    let mut sibling_sources = if meta.defaulted || meta.require_sibling_guards {
        meta.sibling_sources.clone()
    } else {
        BTreeSet::new()
    };
    if meta.defaulted {
        sibling_sources.extend(optional_ancestor_fragment_sources(output, helper));
    }
    let require_sibling_guards = meta.require_sibling_guards;
    meta.sibling_sources.clear();
    let guard_sets = structured_output_guard_sets(
        &output.source_expr,
        &meta.contract_guard_sets(&output.source_expr),
        &sibling_sources,
        require_sibling_guards,
    );
    for extra_guards in guard_sets {
        let output_encoded = output.encoded || encoded_output_values.contains(&output.source_expr);
        let emit_kind = encoded_kind(output.kind, output_encoded);
        if site.can_project_structured_helper_to_caller_path()
            && !helper.has_rendered_source_descendant(&output.source_expr)
        {
            let emit_path = output_path::append_relative_path(&site.path, &output.relative_path);
            contract.push(context.contract_use_with_extra_provenance(
                output.source_expr.clone(),
                emit_path,
                emit_kind,
                &extra_guards,
                &output.meta.provenance,
            ));
        } else {
            contract.push(context.pathless_contract_use_with_extra_provenance(
                output.source_expr.clone(),
                emit_kind,
                &extra_guards,
                &output.meta.provenance,
            ));
        }
    }
}

fn optional_ancestor_fragment_sources(
    output: &HelperFragmentOutputUse,
    helper: &HelperSummary,
) -> BTreeSet<String> {
    if output.meta.require_sibling_guards {
        return BTreeSet::new();
    }
    helper
        .output_uses
        .iter()
        .filter(|candidate| {
            candidate.is_structured_output()
                && candidate.source_expr != output.source_expr
                && candidate.kind == ValueKind::Fragment
                && !candidate.meta.require_sibling_guards
                && yaml_path_is_ancestor(&candidate.relative_path, &output.relative_path)
                && !provenance_is_subset(&candidate.meta.provenance, &output.meta.provenance)
        })
        .map(|candidate| candidate.source_expr.clone())
        .collect()
}

fn provenance_is_subset(
    candidate: &[crate::ContractProvenance],
    output: &[crate::ContractProvenance],
) -> bool {
    !candidate.is_empty()
        && candidate
            .iter()
            .all(|provenance| output.contains(provenance))
}

fn yaml_path_is_ancestor(ancestor: &YamlPath, descendant: &YamlPath) -> bool {
    ancestor.0.len() < descendant.0.len() && descendant.0.starts_with(&ancestor.0)
}

fn structured_output_guard_sets(
    source_expr: &str,
    base_sets: &[Vec<Guard>],
    sibling_sources: &BTreeSet<String>,
    require_sibling_guards: bool,
) -> Vec<Vec<Guard>> {
    let mut guard_sets = if require_sibling_guards {
        Vec::new()
    } else {
        base_sets.to_vec()
    };
    for sibling in sibling_sources {
        if sibling == source_expr || values_paths_are_related(sibling, source_expr) {
            continue;
        }
        for base_set in base_sets {
            let sibling_guard = Guard::Truthy {
                path: sibling.clone(),
            };
            let mut guard_set = base_set.clone();
            if !guard_set.contains(&sibling_guard) {
                guard_set.insert(0, sibling_guard);
            }
            if !guard_sets.contains(&guard_set) {
                guard_sets.push(guard_set);
            }
        }
    }
    if guard_sets.is_empty() {
        base_sets.to_vec()
    } else {
        guard_sets
    }
}

fn encoded_kind(kind: ValueKind, encoded: bool) -> ValueKind {
    if encoded {
        ValueKind::PartialScalar
    } else {
        kind
    }
}
