use crate::helper_analysis::HelperOutputMeta;
use crate::output_node_context::OutputNodeContext;
use crate::output_path;
use crate::output_value_analysis::OutputValueAnalysis;
use crate::value_use_sink::ValueUseSink;
use crate::{Guard, ValueKind, YamlPath};

/// A rendered manifest output site discovered while interpreting a template.
///
/// This is still a compatibility-era document artifact: it records the
/// structural position of one rendered hole and projects immediately into the
/// existing `ValueUse` sink. Keeping that projection behind a document-shaped
/// type gives the next A3 steps a single place to attach richer document facts.
pub(crate) struct AbstractDocumentOutput {
    hole: AbstractDocumentHole,
    analysis: OutputValueAnalysis,
}

impl AbstractDocumentOutput {
    pub(crate) fn new(
        output_context: OutputNodeContext,
        helper_inlined: bool,
        analysis: OutputValueAnalysis,
    ) -> Self {
        Self {
            hole: AbstractDocumentHole::new(output_context, helper_inlined),
            analysis,
        }
    }

    pub(crate) fn project_to_value_uses(self, sink: &mut dyn ValueUseSink) {
        let OutputValueAnalysis {
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

        for value in values {
            if suppress_direct_values.contains(&value) {
                sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
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
            if extra_guards.is_empty() {
                sink.emit_use(value, emit_path, emit_kind);
            } else {
                sink.emit_use_with_extra_guards(value, emit_path, emit_kind, &extra_guards);
            }
        }

        for value in bound_values {
            sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
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
                sink.emit_use_with_extra_guards(
                    value.clone(),
                    self.hole.path.clone(),
                    self.hole.kind,
                    &extra_guards,
                );
            } else {
                sink.emit_helper_use_with_extra_guards(value.clone(), &extra_guards);
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
                sink.emit_use_with_extra_guards(
                    output.source_expr,
                    emit_path,
                    output.kind,
                    &extra_guards,
                );
            } else {
                sink.emit_helper_use_kind_with_extra_guards(
                    output.source_expr,
                    output.kind,
                    &extra_guards,
                );
            }
        }

        for value in helper_fragment_output_values {
            if structured_fragment_sources.contains(&value) {
                continue;
            }
            let has_rendered_descendant =
                output_path::values_path_has_descendant(&value, &helper_rendered_sources);
            if self.hole.can_project_fragment_helper_to_caller_path() && !has_rendered_descendant {
                sink.emit_use(value, self.hole.path.clone(), self.hole.kind);
            } else {
                sink.emit_helper_use_kind_with_extra_guards(value, self.hole.kind, &[]);
            }
        }

        for (value, meta) in helper_dependency_values {
            let extra_guards = helper_extra_guards(&value, &meta);
            sink.emit_helper_use_with_extra_guards(value, &extra_guards);
        }

        for value in helper_guard_values {
            sink.emit_helper_use(value);
        }

        for (path, schema_types) in helper_type_hints {
            for schema_type in schema_types {
                sink.emit_use_with_extra_guards(
                    path.clone(),
                    YamlPath(Vec::new()),
                    ValueKind::Scalar,
                    &[Guard::TypeIs {
                        path: path.clone(),
                        schema_type,
                    }],
                );
            }
        }
    }
}

struct AbstractDocumentHole {
    path: YamlPath,
    kind: ValueKind,
    in_mapping_key: bool,
    entire_scalar_value: bool,
    helper_inlined: bool,
}

impl AbstractDocumentHole {
    fn new(output_context: OutputNodeContext, helper_inlined: bool) -> Self {
        Self {
            path: output_context.path,
            kind: output_context.kind,
            in_mapping_key: output_context.in_mapping_key,
            entire_scalar_value: output_context.entire_scalar_value,
            helper_inlined,
        }
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
