use crate::helper_analysis::HelperOutputMeta;
use crate::output_node_context::OutputNodeContext;
use crate::output_path;
use crate::output_value_analysis::OutputValueAnalysis;
use crate::{Guard, ValueKind, YamlPath};

pub(crate) trait ValueUseSink {
    fn emit_use(&mut self, source_expr: String, path: YamlPath, kind: ValueKind);

    fn emit_use_with_extra_guards(
        &mut self,
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        extra_guards: &[Guard],
    );

    fn emit_helper_use(&mut self, source_expr: String);

    fn emit_helper_use_with_extra_guards(&mut self, source_expr: String, extra_guards: &[Guard]);

    fn emit_helper_use_kind_with_extra_guards(
        &mut self,
        source_expr: String,
        kind: ValueKind,
        extra_guards: &[Guard],
    );
}

pub(crate) fn emit_output_value_analysis(
    sink: &mut dyn ValueUseSink,
    output_context: &OutputNodeContext,
    helper_inlined: bool,
    analysis: OutputValueAnalysis,
) {
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
    } = analysis;

    for value in values {
        if suppress_direct_values.contains(&value) {
            sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
            continue;
        }
        let in_sequence_item = output_context
            .path
            .0
            .last()
            .map(std::string::String::as_str)
            .is_some_and(|segment| segment.ends_with("[*]"));
        let kind = if output_context.kind == ValueKind::Scalar
            && !output_context.entire_scalar_value
            && !output_context.path.0.is_empty()
        {
            ValueKind::PartialScalar
        } else {
            output_context.kind
        };

        let emit_path = if value.ends_with(".*") && !in_sequence_item {
            YamlPath(Vec::new())
        } else {
            output_context.path.clone()
        };
        let default_guard = Guard::Default {
            path: value.clone(),
        };
        let mut extra_guards: Vec<Guard> = Vec::new();
        if let Some(meta) = local_output_meta.get(&value) {
            extra_guards.extend(meta.compatibility_guards(&value));
        }
        if default_fallback_values.contains(&value) {
            if !extra_guards.contains(&default_guard) {
                extra_guards.push(default_guard);
            }
        }
        if extra_guards.is_empty() {
            sink.emit_use(value, emit_path, kind);
        } else {
            sink.emit_use_with_extra_guards(value, emit_path, kind, &extra_guards);
        }
    }

    for value in bound_values {
        sink.emit_use(value, YamlPath(Vec::new()), ValueKind::Scalar);
    }

    let helper_call_caller_scalar_path = !helper_inlined
        && !output_context.in_mapping_key
        && !output_context.path.0.is_empty()
        && !helper_output_values.is_empty()
        && helper_fragment_output_values.is_empty()
        && helper_fragment_output_uses.is_empty()
        && output_context.kind == ValueKind::Scalar
        && output_context.entire_scalar_value;
    let helper_call_caller_fragment_path = !helper_inlined
        && !output_context.in_mapping_key
        && !output_context.path.0.is_empty()
        && (!helper_fragment_output_values.is_empty() || !helper_fragment_output_uses.is_empty())
        && output_context.kind == ValueKind::Fragment;
    let helper_call_caller_structured_path = !helper_inlined
        && !output_context.in_mapping_key
        && !output_context.path.0.is_empty()
        && !helper_fragment_output_uses.is_empty()
        && (output_context.kind == ValueKind::Fragment
            || (output_context.kind == ValueKind::Scalar && output_context.entire_scalar_value));
    let structured_fragment_sources: std::collections::BTreeSet<String> =
        helper_fragment_output_uses
            .iter()
            .map(|output| output.source_expr.clone())
            .collect();
    let mut helper_rendered_sources = structured_fragment_sources.clone();
    helper_rendered_sources.extend(helper_output_values.keys().cloned());
    helper_rendered_sources.extend(helper_fragment_output_values.iter().cloned());

    for (value, meta) in &helper_output_values {
        if structured_fragment_sources.contains(value) {
            continue;
        }
        let has_rendered_descendant =
            output_path::values_path_has_descendant(value, &helper_rendered_sources);
        if helper_call_caller_scalar_path && !has_rendered_descendant {
            let extra_guards = helper_extra_guards(value, meta);
            sink.emit_use_with_extra_guards(
                value.clone(),
                output_context.path.clone(),
                output_context.kind,
                &extra_guards,
            );
        } else {
            let extra_guards = helper_extra_guards(value, meta);
            sink.emit_helper_use_with_extra_guards(value.clone(), &extra_guards);
        }
    }

    for output in helper_fragment_output_uses {
        let extra_guards = helper_extra_guards(&output.source_expr, &output.meta);
        let has_rendered_descendant =
            output_path::values_path_has_descendant(&output.source_expr, &helper_rendered_sources);
        if helper_call_caller_structured_path && !has_rendered_descendant {
            let emit_path =
                output_path::append_relative_path(&output_context.path, &output.relative_path);
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
        if helper_call_caller_fragment_path && !has_rendered_descendant {
            sink.emit_use(value, output_context.path.clone(), output_context.kind);
        } else {
            sink.emit_helper_use_kind_with_extra_guards(value, output_context.kind, &[]);
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

fn helper_extra_guards(source_expr: &str, meta: &HelperOutputMeta) -> Vec<Guard> {
    meta.compatibility_guards(source_expr)
}
