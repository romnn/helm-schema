use crate::abstract_document_hole::AbstractDocumentHole;
use crate::abstract_document_projection::AbstractDocumentProjection;
use crate::contract::{ContractUse, ContractUseContext};
use crate::document_hole_context::DocumentHoleContext;
use crate::document_value_analysis::DocumentValueAnalysis;
use crate::helper_analysis::HelperOutputMeta;
use crate::output_path;
use crate::{Guard, ValueKind, YamlPath};

/// A rendered manifest output site discovered while interpreting a template.
///
/// This is still a compatibility-era document artifact: it records the
/// structural position of one rendered hole and lowers through a private
/// document projection before producing contract uses. Keeping that projection
/// behind a document-shaped type gives the next A4 steps a single place to
/// attach richer contract facts before final DTO projection.
pub(crate) struct AbstractDocumentOutput {
    hole: AbstractDocumentHole,
    analysis: DocumentValueAnalysis,
}

impl AbstractDocumentOutput {
    pub(crate) fn new(
        hole_context: DocumentHoleContext,
        helper_inlined: bool,
        analysis: DocumentValueAnalysis,
    ) -> Self {
        Self {
            hole: AbstractDocumentHole::new(hole_context, helper_inlined),
            analysis,
        }
    }

    pub(crate) fn into_contract_uses(self, context: &ContractUseContext<'_>) -> Vec<ContractUse> {
        let projections = self.compatibility_projections();
        projections
            .into_iter()
            .map(|projection| projection.into_contract_use(context))
            .collect()
    }

    fn compatibility_projections(self) -> Vec<AbstractDocumentProjection> {
        let DocumentValueAnalysis {
            default_fallback_values,
            values,
            local_output_meta,
            bound_values,
            helper_output_values,
            helper_fragment_output_values,
            helper_fragment_output_uses,
            helper_dependency_values,
            helper_guard_values,
            helper_type_hints,
            suppress_direct_values,
            chart_value_defaults: _,
        } = self.analysis;
        let mut projections = Vec::new();

        for value in values {
            if suppress_direct_values.contains(&value) {
                projections.push(self.hole.document_use(
                    value,
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    Vec::new(),
                ));
                continue;
            }

            let default_guard = Guard::Default {
                path: value.clone(),
            };
            let mut extra_guards: Vec<Guard> = Vec::new();
            if let Some(meta) = local_output_meta.get(&value) {
                extra_guards.extend(meta.compatibility_guards(&value));
            }
            if default_fallback_values.contains(&value) && !extra_guards.contains(&default_guard) {
                extra_guards.push(default_guard);
            }

            let emit_path = self.hole.direct_value_path(&value);
            let emit_kind = self.hole.direct_value_kind();
            projections.push(
                self.hole
                    .document_use(value, emit_path, emit_kind, extra_guards),
            );
        }

        for value in bound_values {
            projections.push(self.hole.document_use(
                value,
                YamlPath(Vec::new()),
                ValueKind::Scalar,
                Vec::new(),
            ));
        }

        let structured_fragment_sources: std::collections::BTreeSet<String> =
            helper_fragment_output_uses
                .iter()
                .map(|output| output.source_expr.clone())
                .collect();
        let mut helper_rendered_sources = structured_fragment_sources.clone();
        helper_rendered_sources.extend(helper_output_values.keys().cloned());
        helper_rendered_sources.extend(helper_fragment_output_values.iter().cloned());
        let only_scalar_helper_outputs =
            helper_fragment_output_values.is_empty() && helper_fragment_output_uses.is_empty();

        for (value, meta) in &helper_output_values {
            if structured_fragment_sources.contains(value) {
                continue;
            }
            let has_rendered_descendant =
                output_path::values_path_has_descendant(value, &helper_rendered_sources);
            let extra_guards = helper_extra_guards(value, meta);
            if only_scalar_helper_outputs
                && self.hole.can_project_scalar_helper_to_caller_path()
                && !has_rendered_descendant
            {
                projections.push(self.hole.document_use(
                    value.clone(),
                    self.hole.path().clone(),
                    self.hole.kind(),
                    extra_guards,
                ));
            } else {
                projections.push(AbstractDocumentProjection::helper_use(
                    value.clone(),
                    ValueKind::Scalar,
                    extra_guards,
                ));
            }
        }

        for output in helper_fragment_output_uses {
            let extra_guards = helper_extra_guards(&output.source_expr, &output.meta);
            let has_rendered_descendant = output_path::values_path_has_descendant(
                &output.source_expr,
                &helper_rendered_sources,
            );
            if self.hole.can_project_structured_helper_to_caller_path() && !has_rendered_descendant
            {
                let emit_path =
                    output_path::append_relative_path(self.hole.path(), &output.relative_path);
                projections.push(self.hole.document_use(
                    output.source_expr,
                    emit_path,
                    output.kind,
                    extra_guards,
                ));
            } else {
                projections.push(AbstractDocumentProjection::helper_use(
                    output.source_expr,
                    output.kind,
                    extra_guards,
                ));
            }
        }

        for value in helper_fragment_output_values {
            if structured_fragment_sources.contains(&value) {
                continue;
            }
            let has_rendered_descendant =
                output_path::values_path_has_descendant(&value, &helper_rendered_sources);
            if self.hole.can_project_fragment_helper_to_caller_path() && !has_rendered_descendant {
                projections.push(self.hole.document_use(
                    value,
                    self.hole.path().clone(),
                    self.hole.kind(),
                    Vec::new(),
                ));
            } else {
                projections.push(AbstractDocumentProjection::helper_use(
                    value,
                    self.hole.kind(),
                    Vec::new(),
                ));
            }
        }

        for (value, meta) in helper_dependency_values {
            let extra_guards = helper_extra_guards(&value, &meta);
            projections.push(AbstractDocumentProjection::helper_use(
                value,
                ValueKind::Scalar,
                extra_guards,
            ));
        }

        for value in helper_guard_values {
            projections.push(AbstractDocumentProjection::helper_use(
                value,
                ValueKind::Scalar,
                Vec::new(),
            ));
        }

        for (path, schema_types) in helper_type_hints {
            for schema_type in schema_types {
                projections.push(self.hole.document_use(
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

        projections
    }
}

fn helper_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
    meta.compatibility_guards(source_expr)
}
