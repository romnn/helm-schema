use crate::YamlPath;
use crate::bound_value_analysis::parse_literal_list_range;
use crate::fragment_binding::FragmentBinding;
use crate::fragment_range_scope::{
    range_body_emits_sequence_item_from_source, range_body_renders_mapping_entries_from_ast,
    range_body_renders_scalar_sequence_items_from_source,
    range_has_destructured_variable_definition, range_header_text_from_source,
};
use crate::value_path_context::ValuePathContext;

pub(crate) struct RangeActionPlan {
    pub(crate) header_text: Option<String>,
    pub(crate) source_paths: Vec<String>,
    pub(crate) literal_range: Option<(String, Vec<String>)>,
    pub(crate) guard_path: YamlPath,
    pub(crate) emit_header_use: bool,
    pub(crate) renders_mapping_entries: bool,
    pub(crate) dot_binding: Option<FragmentBinding>,
    pub(crate) apply_dot_binding: bool,
}

pub(crate) fn plan_range_action(
    node: tree_sitter::Node<'_>,
    source: &str,
    value_path_context: &ValuePathContext<'_>,
    current_path: &YamlPath,
) -> RangeActionPlan {
    let has_variable_definition = range_has_destructured_variable_definition(node);
    let body_emits_sequence_item = range_body_emits_sequence_item_from_source(node, source);
    let body_renders_mapping_entries = range_body_renders_mapping_entries_from_ast(node, source);
    let body_renders_scalar_sequence_items = !has_variable_definition
        && range_body_renders_scalar_sequence_items_from_source(node, source);

    let Some(header_text) = range_header_text_from_source(node, source) else {
        return RangeActionPlan {
            header_text: None,
            source_paths: Vec::new(),
            literal_range: None,
            guard_path: YamlPath(Vec::new()),
            emit_header_use: false,
            renders_mapping_entries: false,
            dot_binding: None,
            apply_dot_binding: true,
        };
    };

    let direct_iterable_header_path = direct_iterable_header_path(&header_text, value_path_context);
    let source_paths = value_path_context.resolved_values_paths(&header_text);
    let guard_path = if has_variable_definition {
        YamlPath(Vec::new())
    } else if body_emits_sequence_item
        && body_renders_scalar_sequence_items
        && direct_iterable_header_path.is_some()
    {
        current_path.clone()
    } else {
        YamlPath(Vec::new())
    };
    let emit_header_use = has_variable_definition
        || !body_emits_sequence_item
        || (body_renders_scalar_sequence_items && direct_iterable_header_path.is_some());
    let renders_mapping_entries = has_variable_definition
        && !body_emits_sequence_item
        && body_renders_mapping_entries
        && !current_path.0.is_empty()
        && current_path
            .0
            .last()
            .is_some_and(|segment| !segment.ends_with("[*]"));
    let dot_binding =
        direct_iterable_header_path.map(|path| FragmentBinding::ValuesPath(format!("{path}.*")));
    let literal_range = parse_literal_list_range(&header_text);

    RangeActionPlan {
        header_text: Some(header_text),
        source_paths,
        literal_range,
        guard_path,
        emit_header_use,
        renders_mapping_entries,
        dot_binding,
        apply_dot_binding: true,
    }
}

fn direct_iterable_header_path(
    header_text: &str,
    value_path_context: &ValuePathContext<'_>,
) -> Option<String> {
    value_path_context.single_direct_iterable_range_path(header_text)
}
