use std::collections::BTreeSet;

use crate::contract::ContractIr;
use crate::contract_sink::ContractUseContext;
use crate::{Guard, ValueKind, YamlPath, output_path};

use super::hole::DocumentHole;
use super::value_analysis::DocumentHelperValueAnalysis;

pub(super) fn append_document_helper_contract_uses(
    helper: DocumentHelperValueAnalysis,
    hole: &DocumentHole,
    contract: &mut ContractIr,
    context: &ContractUseContext<'_>,
) {
    let structured_fragment_sources: BTreeSet<String> = helper
        .fragment_output_uses
        .iter()
        .map(|output| output.source_expr.clone())
        .collect();
    let mut helper_rendered_sources = structured_fragment_sources.clone();
    helper_rendered_sources.extend(helper.output_values.keys().cloned());
    helper_rendered_sources.extend(helper.fragment_output_values.iter().cloned());
    let only_scalar_helper_outputs =
        helper.fragment_output_values.is_empty() && helper.fragment_output_uses.is_empty();

    for (value, meta) in &helper.output_values {
        if structured_fragment_sources.contains(value) {
            continue;
        }
        let has_rendered_descendant =
            output_path::values_path_has_descendant(value, &helper_rendered_sources);
        let extra_guards = meta.compatibility_guards(value);
        if only_scalar_helper_outputs
            && hole.can_project_scalar_helper_to_caller_path()
            && !has_rendered_descendant
        {
            contract.push(hole.contract_use(
                context,
                value.clone(),
                hole.path().clone(),
                hole.kind(),
                extra_guards,
            ));
        } else {
            contract.push(context.pathless_contract_use(
                value.clone(),
                ValueKind::Scalar,
                &extra_guards,
            ));
        }
    }

    for output in helper.fragment_output_uses {
        let extra_guards = output.meta.compatibility_guards(&output.source_expr);
        let has_rendered_descendant =
            output_path::values_path_has_descendant(&output.source_expr, &helper_rendered_sources);
        if hole.can_project_structured_helper_to_caller_path() && !has_rendered_descendant {
            let emit_path = output_path::append_relative_path(hole.path(), &output.relative_path);
            contract.push(hole.contract_use(
                context,
                output.source_expr,
                emit_path,
                output.kind,
                extra_guards,
            ));
        } else {
            contract.push(context.pathless_contract_use(
                output.source_expr,
                output.kind,
                &extra_guards,
            ));
        }
    }

    for value in helper.fragment_output_values {
        if structured_fragment_sources.contains(&value) {
            continue;
        }
        let has_rendered_descendant =
            output_path::values_path_has_descendant(&value, &helper_rendered_sources);
        if hole.can_project_fragment_helper_to_caller_path() && !has_rendered_descendant {
            contract.push(hole.contract_use(
                context,
                value,
                hole.path().clone(),
                hole.kind(),
                Vec::new(),
            ));
        } else {
            contract.push(context.pathless_contract_use(value, hole.kind(), &[]));
        }
    }

    for (value, meta) in helper.dependency_values {
        let extra_guards = meta.compatibility_guards(&value);
        contract.push(context.pathless_contract_use(value, ValueKind::Scalar, &extra_guards));
    }

    for value in helper.guard_values {
        contract.push(context.pathless_contract_use(value, ValueKind::Scalar, &[]));
    }

    for (path, schema_types) in helper.type_hints {
        for schema_type in schema_types {
            contract.push(hole.contract_use(
                context,
                path.clone(),
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                vec![Guard::TypeIs {
                    path: path.clone(),
                    schema_type,
                }],
            ));
        }
    }
}
