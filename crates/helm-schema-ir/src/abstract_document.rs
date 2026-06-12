use crate::document_hole_context::DocumentHoleContext;
use crate::document_value_analysis::DocumentValueAnalysis;
use crate::helper_analysis::HelperOutputMeta;
use crate::output_path;
use crate::{Guard, ValueKind, ValueUse, YamlPath};
use std::collections::BTreeSet;

/// A rendered manifest output site discovered while interpreting a template.
///
/// This is still a compatibility-era document artifact: it records the
/// structural position of one rendered hole and lowers through a private
/// document projection before producing the existing `ValueUse` compatibility
/// DTO. Keeping that projection behind a document-shaped type gives the next
/// A3 steps a single place to attach richer document facts.
pub(crate) struct AbstractDocumentOutput {
    hole: AbstractDocumentHole,
    analysis: DocumentValueAnalysis,
    context: AbstractDocumentProjectionContext,
}

impl AbstractDocumentOutput {
    pub(crate) fn new(
        hole_context: DocumentHoleContext,
        helper_inlined: bool,
        analysis: DocumentValueAnalysis,
        context: AbstractDocumentProjectionContext,
    ) -> Self {
        Self {
            hole: AbstractDocumentHole::new(hole_context, helper_inlined),
            analysis,
            context,
        }
    }

    pub(crate) fn into_value_uses(self) -> Vec<ValueUse> {
        let projections = self.compatibility_projections();
        projections
            .into_iter()
            .map(|projection| projection.into_value_use())
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
                    self.hole.path.clone(),
                    self.hole.kind,
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
                    output_path::append_relative_path(&self.hole.path, &output.relative_path);
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
                    self.hole.path.clone(),
                    self.hole.kind,
                    Vec::new(),
                ));
            } else {
                projections.push(AbstractDocumentProjection::helper_use(
                    value,
                    self.hole.kind,
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
            .into_iter()
            .map(|projection| projection.with_context(&self.context))
            .collect()
    }
}

#[derive(Clone)]
pub(crate) struct AbstractDocumentProjectionContext {
    guards: Vec<Guard>,
    chart_value_defaults: BTreeSet<String>,
    suppress_document_path: bool,
}

impl AbstractDocumentProjectionContext {
    pub(crate) fn new(
        guards: Vec<Guard>,
        chart_value_defaults: BTreeSet<String>,
        suppress_document_path: bool,
    ) -> Self {
        Self {
            guards,
            chart_value_defaults,
            suppress_document_path,
        }
    }
}

enum AbstractDocumentProjection {
    DocumentUse(AbstractDocumentUse),
    HelperUse {
        source_expr: String,
        kind: ValueKind,
        guards: Vec<Guard>,
    },
}

impl AbstractDocumentProjection {
    fn helper_use(source_expr: String, kind: ValueKind, guards: Vec<Guard>) -> Self {
        Self::HelperUse {
            source_expr,
            kind,
            guards,
        }
    }

    fn with_context(mut self, context: &AbstractDocumentProjectionContext) -> Self {
        match &mut self {
            Self::DocumentUse(use_) => use_.apply_context(context),
            Self::HelperUse { guards, .. } => {
                *guards = guards_with_context(&context.guards, guards);
            }
        }
        self
    }

    fn into_value_use(self) -> ValueUse {
        match self {
            Self::DocumentUse(use_) => use_.into_value_use(),
            Self::HelperUse {
                source_expr,
                kind,
                guards,
            } => ValueUse {
                source_expr,
                path: YamlPath(Vec::new()),
                kind: if kind == ValueKind::PartialScalar {
                    ValueKind::Scalar
                } else {
                    kind
                },
                guards,
                resource: None,
            },
        }
    }
}

struct AbstractDocumentUse {
    source_expr: String,
    path: YamlPath,
    kind: ValueKind,
    guards: Vec<Guard>,
    resource: Option<crate::ResourceRef>,
}

impl AbstractDocumentUse {
    fn apply_context(&mut self, context: &AbstractDocumentProjectionContext) {
        if context.suppress_document_path {
            self.path = YamlPath(Vec::new());
        }
        if self.kind == ValueKind::PartialScalar && self.path.0.is_empty() {
            self.kind = ValueKind::Scalar;
        }
        self.guards = guards_with_context(&context.guards, &self.guards);
        if !self.path.0.is_empty() && context.chart_value_defaults.contains(&self.source_expr) {
            let default_guard = Guard::Default {
                path: self.source_expr.clone(),
            };
            if !self.guards.contains(&default_guard) {
                self.guards.push(default_guard);
            }
        }
    }

    fn into_value_use(self) -> ValueUse {
        ValueUse {
            source_expr: self.source_expr,
            path: self.path,
            kind: self.kind,
            guards: self.guards,
            resource: self.resource,
        }
    }
}

fn guards_with_context(context_guards: &[Guard], extra_guards: &[Guard]) -> Vec<Guard> {
    let mut guards = context_guards.to_vec();
    merge_guards(&mut guards, extra_guards);
    guards
}

fn merge_guards(target: &mut Vec<Guard>, extra_guards: &[Guard]) {
    for guard in extra_guards {
        if !target.contains(guard) {
            target.push(guard.clone());
        }
    }
}

struct AbstractDocumentHole {
    path: YamlPath,
    kind: ValueKind,
    in_mapping_key: bool,
    entire_scalar_value: bool,
    helper_inlined: bool,
    resource: Option<crate::ResourceRef>,
}

impl AbstractDocumentHole {
    fn new(hole_context: DocumentHoleContext, helper_inlined: bool) -> Self {
        Self {
            path: hole_context.path,
            kind: hole_context.kind,
            in_mapping_key: hole_context.in_mapping_key,
            entire_scalar_value: hole_context.entire_scalar_value,
            helper_inlined,
            resource: hole_context.resource,
        }
    }

    fn document_use(
        &self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
    ) -> AbstractDocumentProjection {
        AbstractDocumentProjection::DocumentUse(AbstractDocumentUse {
            source_expr,
            path,
            kind,
            guards,
            resource: self.resource.clone(),
        })
    }

    fn direct_value_kind(&self) -> ValueKind {
        if self.kind == ValueKind::Scalar && !self.entire_scalar_value && !self.path.0.is_empty() {
            ValueKind::PartialScalar
        } else {
            self.kind
        }
    }

    fn direct_value_path(&self, source_expr: &str) -> YamlPath {
        if source_expr.ends_with(".*") && !self.in_sequence_item() {
            YamlPath(Vec::new())
        } else {
            self.path.clone()
        }
    }

    fn in_sequence_item(&self) -> bool {
        self.path
            .0
            .last()
            .map(std::string::String::as_str)
            .is_some_and(|segment| segment.ends_with("[*]"))
    }

    fn can_project_scalar_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && self.kind == ValueKind::Scalar
            && self.entire_scalar_value
    }

    fn can_project_fragment_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && self.kind == ValueKind::Fragment
    }

    fn can_project_structured_helper_to_caller_path(&self) -> bool {
        !self.helper_inlined
            && !self.in_mapping_key
            && !self.path.0.is_empty()
            && (self.kind == ValueKind::Fragment
                || (self.kind == ValueKind::Scalar && self.entire_scalar_value))
    }
}

fn helper_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
    meta.compatibility_guards(source_expr)
}
