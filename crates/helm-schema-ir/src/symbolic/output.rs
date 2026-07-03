use std::collections::{BTreeMap, BTreeSet};

use helm_schema_ast::TemplateExpr;

use crate::SourceSpan;
use crate::contract::ContractIr;
use crate::contract_sink::{ContractUseContext, EmissionWitness};
use crate::eval_effect::Effects;
use crate::helper_summary::{
    HelperFragmentOutputUse, HelperSummary, merge_provenance_sites, values_paths_are_related,
};
use crate::{ContractProvenance, Guard, ValueKind, YamlPath};
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

    let suppressed_guard_path_provenance = suppressed_guard_path_provenance(&helper);
    let mut suppress_direct_values = helper.dependency_relevant_paths();
    suppress_direct_values.extend(helper.suppress_roots.iter().cloned());

    let output_values = std::mem::take(&mut output_effects.output_paths);
    for value in output_values {
        if suppress_direct_values.contains(&value)
            || suppress_direct_values
                .iter()
                .any(|root| output_path::values_path_is_descendant(&value, root))
        {
            let provenance = suppressed_guard_path_provenance
                .get(&value)
                .cloned()
                .unwrap_or_default();
            let mut witness = EmissionWitness::new(
                value,
                Some(YamlPath(Vec::new())),
                ValueKind::Scalar,
                vec![Vec::new()],
            );
            witness.provenance = provenance;
            context.emit(witness, &mut contract);
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
        if output_effects.defaults.contains(&value) {
            for extra_guards in &mut guard_sets {
                if !extra_guards.contains(&default_guard) {
                    extra_guards.push(default_guard.clone());
                }
            }
        }
        context.emit(
            EmissionWitness::new(value, Some(emit_path), emit_kind, guard_sets),
            &mut contract,
        );
    }

    let bound_output_values = std::mem::take(&mut output_effects.bound_output_paths);
    for value in bound_output_values {
        let witness = EmissionWitness::new(
            value,
            Some(YamlPath(Vec::new())),
            ValueKind::Scalar,
            vec![Vec::new()],
        );
        context.emit(witness, &mut contract);
    }

    contract.extend_type_hints(output_effects.type_hints.clone());
    append_helper_contract_uses(
        &helper,
        &suppressed_guard_path_provenance,
        &output_effects.encoded_paths,
        &site,
        &mut contract,
        context,
    );
    contract
}

fn append_helper_contract_uses(
    helper: &HelperSummary,
    suppressed_guard_path_provenance: &BTreeMap<String, Vec<ContractProvenance>>,
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
        let mut meta = output.meta.clone();
        meta.note_sibling_sources(value, &helper_output_sources);
        let witness = if helper_has_only_scalar_outputs
            && site.can_project_scalar_helper_to_caller_path()
            && !helper.has_rendered_source_descendant(value)
        {
            let emit_kind = encoded_kind(site.kind, encoded_output_values.contains(value));
            meta.emission_witness(value, Some(site.path.clone()), emit_kind)
        } else {
            meta.emission_witness(value, None, ValueKind::Scalar)
        };
        context.emit(witness, contract);
    }

    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_structured_output())
    {
        let mut meta = output.meta.clone();
        meta.prune_source_not_for_sibling_truthy(&output.source_expr, &helper_output_sources);
        if !output.relative_path.0.is_empty() {
            meta.prune_truthy_ancestors_of_source(&output.source_expr);
        }
        // `contract_guard_sets` must not see the sibling set (sibling guards
        // are derived below in `structured_output_guard_sets`), so take it out
        // of the meta here.
        let mut sibling_sources = std::mem::take(&mut meta.sibling_sources);
        if !meta.defaulted && !meta.require_sibling_guards {
            sibling_sources.clear();
        }
        if meta.defaulted {
            sibling_sources.extend(optional_ancestor_fragment_sources(output, helper));
        }
        let output_encoded = output.encoded || encoded_output_values.contains(&output.source_expr);
        let emit_path = if site.can_project_structured_helper_to_caller_path()
            && !helper.has_rendered_source_descendant(&output.source_expr)
        {
            Some(output_path::append_relative_path(
                &site.path,
                &output.relative_path,
            ))
        } else {
            None
        };
        let mut witness = meta.emission_witness(
            &output.source_expr,
            emit_path,
            encoded_kind(output.kind, output_encoded),
        );
        witness.guard_sets = structured_output_guard_sets(
            &output.source_expr,
            &witness.guard_sets,
            &sibling_sources,
            meta.require_sibling_guards,
        );
        context.emit(witness, contract);
    }

    for output in helper
        .output_uses
        .iter()
        .filter(|output| output.is_dependency())
    {
        let value = &output.source_expr;
        let mut meta = output.meta.clone();
        meta.note_sibling_sources(value, &helper_output_sources);
        // Every summary row is stamped with its helper body's provenance at
        // the end of `interpret_bound_helper_body`, so `provenance` is never
        // empty here.
        if let Some(suppressed) = suppressed_guard_path_provenance.get(value) {
            merge_provenance_sites(&mut meta.provenance, suppressed);
        }
        let mut witness = meta.emission_witness(value, None, ValueKind::Scalar);
        witness.dependency = true;
        context.emit(witness, contract);
    }

    for (value, base_meta) in &helper.guard_path_meta {
        if !suppressed_guard_path_provenance.contains_key(value)
            && suppressed_guard_path_provenance
                .keys()
                .any(|path| output_path::values_path_is_descendant(path, value))
        {
            continue;
        }
        let mut guard_path_meta = base_meta.clone();
        if let Some(suppressed) = suppressed_guard_path_provenance.get(value) {
            merge_provenance_sites(&mut guard_path_meta.provenance, suppressed);
        }
        let guard_path_meta_has_context = !guard_path_meta.predicates.is_empty()
            || guard_path_meta.defaulted
            || context.has_ambient_guards();
        if site.path.0.is_empty()
            && site.resource.is_none()
            && helper_output_sources.contains(value)
            && !guard_path_meta_has_context
        {
            continue;
        }
        let lower_guard_path_meta = site.path.0.is_empty()
            && (site.resource.is_none() || suppressed_guard_path_provenance.contains_key(value));
        let witness = if lower_guard_path_meta && guard_path_meta_has_context {
            guard_path_meta.emission_witness(value, None, ValueKind::Scalar)
        } else {
            EmissionWitness::new(value.clone(), None, ValueKind::Scalar, vec![Vec::new()])
        };
        context.emit(witness, contract);
    }
}

/// Provenance of the output rows that suppressed guard predicates for each
/// values path, so the pathless rows emitted for those paths still name the
/// helper sites that justified them.
fn suppressed_guard_path_provenance(
    helper: &HelperSummary,
) -> BTreeMap<String, Vec<ContractProvenance>> {
    let mut by_path: BTreeMap<String, Vec<ContractProvenance>> = BTreeMap::new();
    for output in &helper.output_uses {
        // Key presence doubles as the "this path was suppressed here" signal
        // for the guard-path emission checks below. Every summary row is
        // stamped with its helper body's provenance at the end of
        // `interpret_bound_helper_body`, so each entry names at least one
        // site.
        for path in &output.meta.suppress_predicate_paths {
            merge_provenance_sites(
                by_path.entry(path.clone()).or_default(),
                &output.meta.provenance,
            );
        }
    }
    by_path
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

fn provenance_is_subset(candidate: &[ContractProvenance], output: &[ContractProvenance]) -> bool {
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
