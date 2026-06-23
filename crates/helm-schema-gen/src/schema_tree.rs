use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::merge::{merge_schema_list, merge_two_schemas};
use crate::schema_model::is_empty_schema;
use crate::schema_node::{JsonSchemaType, SchemaNode};

const MAP_WILDCARD_SEGMENT: &str = "__any__";

pub(crate) struct SchemaDocument {
    root: SchemaNode,
}

impl SchemaDocument {
    pub(crate) fn new_root_object() -> Self {
        Self {
            root: SchemaNode::closed_object(),
        }
    }

    pub(crate) fn insert_path_schema(&mut self, path_segments: &[String], schema: SchemaNode) {
        if !path_segments.is_empty() {
            insert_schema_at_parts(&mut self.root, path_segments, schema);
        }
    }

    pub(crate) fn replace_path_schema(&mut self, path_segments: &[String], schema: SchemaNode) {
        replace_schema_at_parts(&mut self.root, path_segments, schema);
    }

    pub(crate) fn append_conditional(
        &mut self,
        ancestor_segments: &[String],
        condition: SchemaNode,
        then_schema: SchemaNode,
    ) {
        append_conditional_at_parts(&mut self.root, ancestor_segments, condition, then_schema);
    }

    pub(crate) fn merge_missing_values_yaml_defaults_under_roots(
        &mut self,
        values_yaml_doc: &YamlValue,
        root_paths: &[Vec<String>],
        skip_paths: &BTreeSet<Vec<String>>,
    ) {
        let mut insertions = Vec::new();
        for root_path in root_paths {
            let Some(yaml) = yaml_value_at_path(values_yaml_doc, root_path) else {
                continue;
            };
            collect_missing_yaml_default_insertions(
                &self.root,
                root_path,
                yaml,
                skip_paths,
                &mut insertions,
            );
        }
        for (path, schema) in insertions {
            self.insert_path_schema(&path, schema);
        }
    }

    pub(crate) fn into_value(self) -> Value {
        self.root.into_value()
    }
}

pub(crate) fn draft07_root_document(root_schema: Value) -> Value {
    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        Value::String("http://json-schema.org/draft-07/schema#".to_string()),
    );

    if let Value::Object(obj) = root_schema {
        for (k, v) in obj {
            out.insert(k, v);
        }
    } else {
        out.insert("type".to_string(), Value::String("object".to_string()));
        out.insert("properties".to_string(), Value::Object(Map::new()));
        out.insert("additionalProperties".to_string(), Value::Bool(false));
    }
    Value::Object(out)
}

fn conditional_entry(condition: SchemaNode, then_schema: SchemaNode) -> SchemaNode {
    SchemaNode::foreign(Value::Object(
        [
            ("if".to_string(), condition.into_value()),
            ("then".to_string(), then_schema.into_value()),
        ]
        .into_iter()
        .collect(),
    ))
}

fn append_conditional_at_parts(
    node: &mut SchemaNode,
    path_segments: &[String],
    condition: SchemaNode,
    then_schema: SchemaNode,
) {
    if path_segments.is_empty() {
        push_conditional_entry(node, condition, then_schema, false);
        return;
    }

    if matches!(node, SchemaNode::Foreign(_)) && !node.is_empty_slot() {
        let mut fragment = SchemaNode::empty();
        append_conditional_at_parts(&mut fragment, path_segments, condition, then_schema);
        merge_conditional_fragment_into_foreign_slot(node, fragment);
        return;
    }

    if path_segments.len() == 1 {
        let properties = ensure_object_properties(node);
        let child = properties
            .entry(path_segments[0].clone())
            .or_insert_with(SchemaNode::closed_object);
        push_conditional_entry(child, condition, then_schema, true);
        return;
    }

    let properties = ensure_object_properties(node);
    let child = properties
        .entry(path_segments[0].clone())
        .or_insert_with(SchemaNode::closed_object);
    append_conditional_at_parts(child, &path_segments[1..], condition, then_schema);
}

fn merge_conditional_fragment_into_foreign_slot(node: &mut SchemaNode, fragment: SchemaNode) {
    reconcile_node_host_with_branch_schema(node, &fragment);
    open_host_descendants_extended_by_schema(node, &fragment);
    merge_into_schema_slot(node, fragment);
}

fn push_conditional_entry(
    node: &mut SchemaNode,
    condition: SchemaNode,
    then_schema: SchemaNode,
    open_host: bool,
) {
    if open_host {
        reconcile_node_host_with_branch_schema(node, &then_schema);
    }
    open_host_descendants_extended_by_schema(node, &then_schema);
    let conditional = conditional_entry(condition, then_schema);

    if !matches!(
        node,
        SchemaNode::Object { .. } | SchemaNode::Foreign(Value::Object(_))
    ) {
        *node = SchemaNode::closed_object();
    }
    node.push_all_of(conditional);
}

fn open_host_descendants_extended_by_schema(node: &mut SchemaNode, schema: &SchemaNode) {
    let branch_properties = schema.property_entries();
    if branch_properties.is_empty() {
        return;
    }

    match node {
        SchemaNode::Object { properties, .. } => {
            for (property, branch_child) in branch_properties {
                let Some(host_child) = properties.get_mut(&property) else {
                    continue;
                };

                reconcile_node_host_with_branch_schema(host_child, &branch_child);
                open_host_descendants_extended_by_schema(host_child, &branch_child);
            }
        }
        SchemaNode::Foreign(value) => {
            let Some(host_properties) = value
                .as_object_mut()
                .and_then(|object| object.get_mut("properties"))
                .and_then(Value::as_object_mut)
            else {
                return;
            };

            for (property, branch_child) in branch_properties {
                let Some(host_child_value) = host_properties.get_mut(&property) else {
                    continue;
                };
                let mut host_child = SchemaNode::foreign(std::mem::take(host_child_value));
                reconcile_node_host_with_branch_schema(&mut host_child, &branch_child);
                open_host_descendants_extended_by_schema(&mut host_child, &branch_child);
                *host_child_value = host_child.into_value();
            }
        }
        _ => {}
    }
}

fn reconcile_node_host_with_branch_schema(host: &mut SchemaNode, branch: &SchemaNode) {
    if branch.opens_unknown_object_fields() {
        host.open_object();
        return;
    }

    if !branch_schema_extends_node_host(host, branch) {
        return;
    }

    if host.is_plain_closed_values_object() {
        add_branch_property_placeholders_to_node(host, branch);
    } else {
        host.open_object();
    }
}

fn branch_schema_extends_node_host(host: &SchemaNode, branch: &SchemaNode) -> bool {
    let branch_properties = branch.property_entries();
    if branch_properties.is_empty() {
        return false;
    }

    let host_properties = host
        .property_entries()
        .into_iter()
        .map(|(property, _)| property)
        .collect::<BTreeSet<_>>();
    branch_properties
        .iter()
        .any(|(property, _)| !host_properties.contains(property))
}

fn add_branch_property_placeholders_to_node(host: &mut SchemaNode, branch: &SchemaNode) {
    let branch_properties = branch.property_entries();
    if branch_properties.is_empty() {
        return;
    }

    let properties = ensure_object_properties(host);
    for (property, _) in branch_properties {
        properties.entry(property).or_insert_with(SchemaNode::empty);
    }
}

fn yaml_value_at_path<'a>(yaml: &'a YamlValue, path_segments: &[String]) -> Option<&'a YamlValue> {
    let mut current = yaml;
    for segment in path_segments {
        let YamlValue::Mapping(mapping) = current else {
            return None;
        };
        let key = YamlValue::String(segment.clone());
        current = mapping.get(&key)?;
    }
    Some(current)
}

fn collect_missing_yaml_default_insertions(
    root_schema: &SchemaNode,
    current_path: &[String],
    yaml: &YamlValue,
    skip_paths: &BTreeSet<Vec<String>>,
    insertions: &mut Vec<(Vec<String>, SchemaNode)>,
) {
    if skip_paths.contains(current_path) {
        if !root_schema.path_exists(current_path, MAP_WILDCARD_SEGMENT) {
            insertions.push((current_path.to_vec(), SchemaNode::empty()));
        }
        return;
    }

    match yaml {
        YamlValue::Mapping(mapping) if !mapping.is_empty() => {
            if !root_schema.path_exists(current_path, MAP_WILDCARD_SEGMENT) {
                if let Some(schema) = schema_node_from_yaml_default(yaml, current_path, skip_paths)
                {
                    insertions.push((current_path.to_vec(), schema));
                }
                return;
            }

            for (key, value_yaml) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                let child_path = child_value_path(current_path, key);
                collect_missing_yaml_default_insertions(
                    root_schema,
                    &child_path,
                    value_yaml,
                    skip_paths,
                    insertions,
                );
            }
        }
        _ => {
            if !root_schema.path_exists(current_path, MAP_WILDCARD_SEGMENT)
                && let Some(schema) = schema_node_from_yaml_default(yaml, current_path, skip_paths)
            {
                insertions.push((current_path.to_vec(), schema));
            }
        }
    }
}

fn child_value_path(parent: &[String], child: &str) -> Vec<String> {
    let mut path = parent.to_vec();
    path.push(child.to_string());
    path
}

fn schema_node_from_yaml_default(
    value: &YamlValue,
    current_path: &[String],
    skip_paths: &BTreeSet<Vec<String>>,
) -> Option<SchemaNode> {
    if skip_paths.contains(current_path) {
        return None;
    }
    match value {
        YamlValue::Null | YamlValue::Tagged(_) => Some(SchemaNode::empty()),
        YamlValue::Bool(_) => Some(SchemaNode::typed(JsonSchemaType::Boolean)),
        YamlValue::Number(number) => {
            if number.as_i64().is_some() || number.as_u64().is_some() {
                Some(SchemaNode::typed(JsonSchemaType::Integer))
            } else {
                Some(SchemaNode::typed(JsonSchemaType::Number))
            }
        }
        YamlValue::String(_) => Some(SchemaNode::typed(JsonSchemaType::String)),
        YamlValue::Sequence(sequence) => {
            let items = if sequence.is_empty() {
                SchemaNode::empty()
            } else {
                SchemaNode::foreign(merge_schema_list(
                    sequence
                        .iter()
                        .filter_map(|item| {
                            schema_node_from_yaml_default(item, current_path, skip_paths)
                        })
                        .map(SchemaNode::into_value)
                        .collect(),
                ))
            };
            Some(SchemaNode::array().items(items))
        }
        YamlValue::Mapping(mapping) => {
            if mapping.is_empty() {
                return Some(SchemaNode::unknown_object());
            }
            let mut schema = SchemaNode::closed_object();
            let mut inserted = false;
            for (key, value_yaml) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                let child_path = child_value_path(current_path, key);
                let child_schema = if skip_paths.contains(&child_path) {
                    SchemaNode::empty()
                } else {
                    let Some(child_schema) =
                        schema_node_from_yaml_default(value_yaml, &child_path, skip_paths)
                    else {
                        continue;
                    };
                    child_schema
                };
                inserted = true;
                schema = schema.property(key.to_string(), child_schema);
            }
            inserted.then_some(schema)
        }
    }
}

pub(crate) fn apply_values_descriptions(root: &mut Value, descriptions: &BTreeMap<String, String>) {
    for (path, description) in descriptions {
        let path_segments: Vec<String> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(std::string::ToString::to_string)
            .collect();
        apply_description_at_path_segments(root, &path_segments, description);
    }
}

fn apply_description_at_path_segments(
    node: &mut Value,
    path_segments: &[String],
    description: &str,
) {
    if path_segments.is_empty() {
        set_schema_description(node, description);
        return;
    }

    let Some((head, tail)) = path_segments.split_first() else {
        return;
    };

    let Value::Object(obj) = node else {
        return;
    };

    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get_mut(key) {
            for variant in variants {
                apply_description_at_path_segments(variant, path_segments, description);
            }
        }
    }

    if let Some(Value::Array(variants)) = obj.get_mut("allOf") {
        for variant in variants {
            apply_description_at_path_segments(variant, path_segments, description);
        }
    }

    for key in ["then", "else"] {
        if let Some(child) = obj.get_mut(key) {
            apply_description_at_path_segments(child, path_segments, description);
        }
    }

    if head == "*" {
        if let Some(items) = obj.get_mut("items") {
            apply_description_at_path_segments(items, tail, description);
        }
        return;
    }

    if head == MAP_WILDCARD_SEGMENT {
        if let Some(additional_properties) = obj.get_mut("additionalProperties") {
            apply_description_at_path_segments(additional_properties, tail, description);
        }
        return;
    }

    let Some(properties) = obj.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(child) = properties.get_mut(head) else {
        return;
    };
    apply_description_at_path_segments(child, tail, description);
}

fn set_schema_description(node: &mut Value, description: &str) {
    if description.trim().is_empty() {
        return;
    }

    if let Value::Object(obj) = node {
        obj.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
}

fn ensure_array_schema(node: &mut SchemaNode) {
    if !matches!(node, SchemaNode::Array { .. }) {
        *node = SchemaNode::array().items(SchemaNode::foreign(Value::Null));
    }
}

fn ensure_items_schema(array_schema: &mut SchemaNode) -> &mut SchemaNode {
    let SchemaNode::Array { items, .. } = array_schema else {
        unreachable!("array schema must be normalized before accessing items");
    };
    items
        .get_or_insert_with(|| Box::new(SchemaNode::foreign(Value::Null)))
        .as_mut()
}

fn ensure_object_properties(node: &mut SchemaNode) -> &mut BTreeMap<String, SchemaNode> {
    match node {
        SchemaNode::Object { .. } => {}
        SchemaNode::Foreign(value) if is_empty_schema(value) => {
            *node = SchemaNode::object()
                .with_empty_properties()
                .with_additional_properties(SchemaNode::empty());
        }
        _ => {
            *node = SchemaNode::closed_object();
        }
    }
    let SchemaNode::Object { properties, .. } = node else {
        unreachable!("object schema must be normalized before accessing properties");
    };
    properties
}

fn ensure_additional_properties(node: &mut SchemaNode) -> &mut SchemaNode {
    if !matches!(node, SchemaNode::Object { .. }) {
        *node = SchemaNode::closed_object();
    }
    let SchemaNode::Object {
        additional_properties,
        ..
    } = node
    else {
        unreachable!("object schema must be normalized before accessing additionalProperties");
    };
    let additional_properties =
        additional_properties.get_or_insert_with(|| Box::new(SchemaNode::empty()));
    if additional_properties.is_false_schema() {
        **additional_properties = SchemaNode::empty();
    }
    additional_properties.as_mut()
}

fn merge_into_schema_slot(slot: &mut SchemaNode, schema: SchemaNode) {
    if slot.is_empty_slot() {
        *slot = schema;
        return;
    }

    if schema.opens_unknown_object_fields()
        || (schema.is_exact_empty_object() && slot.has_object_descendants())
    {
        slot.open_object();
    }

    let slot_opens_unknown_object_fields = slot.opens_unknown_object_fields();
    let existing = std::mem::replace(slot, SchemaNode::empty()).into_value();
    *slot = SchemaNode::foreign(merge_two_schemas(existing, schema.into_value()));
    if slot_opens_unknown_object_fields {
        slot.open_object();
    }
}

fn insert_schema_into_union_variants(
    variants: &mut [SchemaNode],
    path_segments: &[String],
    leaf: &SchemaNode,
) -> bool {
    let head = path_segments[0].as_str();
    let mut touched = false;
    for variant in variants {
        let compatible = if head == "*" {
            variant.is_array_like()
        } else {
            variant.is_object_like()
        };

        if compatible {
            insert_schema_at_parts(variant, path_segments, leaf.clone());
            touched = true;
        }
    }
    touched
}

fn new_union_variant_for_head(head: &str) -> SchemaNode {
    if head == "*" {
        SchemaNode::array().items(SchemaNode::foreign(Value::Null))
    } else {
        SchemaNode::closed_object()
    }
}

fn insert_schema_at_union(
    variants: &mut Vec<SchemaNode>,
    path_segments: &[String],
    leaf: SchemaNode,
) {
    let touched = insert_schema_into_union_variants(variants, path_segments, &leaf);
    if !touched {
        let mut new_variant = new_union_variant_for_head(path_segments[0].as_str());
        insert_schema_at_parts(&mut new_variant, path_segments, leaf);
        variants.push(new_variant);
    }
}

fn replace_schema_at_parts(node: &mut SchemaNode, path_segments: &[String], leaf: SchemaNode) {
    if path_segments.is_empty() {
        *node = leaf;
        return;
    }

    let head = path_segments[0].as_str();
    let replaced = match node {
        SchemaNode::Object {
            additional_properties,
            ..
        } if head == MAP_WILDCARD_SEGMENT => {
            if let Some(child) = additional_properties.as_deref_mut() {
                replace_schema_at_parts(child, &path_segments[1..], leaf.clone());
                true
            } else {
                false
            }
        }
        SchemaNode::Object { properties, .. } => {
            if let Some(child) = properties.get_mut(head) {
                replace_schema_at_parts(child, &path_segments[1..], leaf.clone());
                true
            } else {
                false
            }
        }
        SchemaNode::Array { items, .. } if head == "*" => {
            if let Some(child) = items.as_deref_mut() {
                replace_schema_at_parts(child, &path_segments[1..], leaf.clone());
                true
            } else {
                false
            }
        }
        SchemaNode::AnyOf(variants) | SchemaNode::AllOf(variants) => {
            let mut replaced = false;
            for variant in variants {
                let compatible = if head == "*" {
                    variant.is_array_like()
                } else {
                    variant.is_object_like()
                };
                if compatible {
                    replace_schema_at_parts(variant, path_segments, leaf.clone());
                    replaced = true;
                }
            }
            replaced
        }
        SchemaNode::Foreign(value) => {
            replace_schema_value_at_path(value, path_segments, &leaf.clone().into_value())
        }
        _ => false,
    };

    if !replaced {
        insert_schema_at_parts(node, path_segments, leaf);
    }
}

fn replace_schema_value_at_path(
    value: &mut Value,
    path_segments: &[String],
    replacement: &Value,
) -> bool {
    if path_segments.is_empty() {
        *value = replacement.clone();
        return true;
    }

    let Some(object) = value.as_object_mut() else {
        return false;
    };

    let mut replaced = false;
    for composition_key in ["anyOf", "allOf", "oneOf"] {
        if let Some(entries) = object
            .get_mut(composition_key)
            .and_then(Value::as_array_mut)
        {
            for entry in entries {
                replaced |= replace_schema_value_at_path(entry, path_segments, replacement);
            }
        }
    }

    let head = path_segments[0].as_str();
    if head == "*" {
        if let Some(items) = object.get_mut("items") {
            replaced |= replace_schema_value_at_path(items, &path_segments[1..], replacement);
        }
        return replaced;
    }
    if head == MAP_WILDCARD_SEGMENT {
        if let Some(additional_properties) = object.get_mut("additionalProperties") {
            replaced |= replace_schema_value_at_path(
                additional_properties,
                &path_segments[1..],
                replacement,
            );
        }
        return replaced;
    }

    if let Some(child) = object
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .and_then(|properties| properties.get_mut(head))
    {
        replaced |= replace_schema_value_at_path(child, &path_segments[1..], replacement);
    }
    replaced
}

fn insert_schema_at_parts(node: &mut SchemaNode, path_segments: &[String], leaf: SchemaNode) {
    if path_segments.is_empty() {
        return;
    }

    if matches!(node, SchemaNode::Foreign(_)) && !node.is_empty_slot() {
        let mut fragment = SchemaNode::empty();
        insert_schema_at_parts(&mut fragment, path_segments, leaf);
        merge_into_schema_slot(node, fragment);
        return;
    }

    if let SchemaNode::AnyOf(variants) = node {
        insert_schema_at_union(variants, path_segments, leaf);
        return;
    }

    if path_segments[0] == MAP_WILDCARD_SEGMENT {
        if path_segments.len() > 1 {
            node.clear_exact_empty_constraint_for_descendant();
        }
        let additional_properties = ensure_additional_properties(node);
        if path_segments.len() == 1 {
            merge_into_schema_slot(additional_properties, leaf);
        } else {
            additional_properties.clear_exact_empty_constraint_for_descendant();
            insert_schema_at_parts(additional_properties, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments[0] == "*" {
        if !node.is_empty_slot() && !node.is_array_like() {
            let existing = std::mem::replace(node, SchemaNode::empty());
            let mut array_variant = new_union_variant_for_head("*");
            insert_schema_at_parts(&mut array_variant, path_segments, leaf);
            *node = SchemaNode::any_of(vec![existing, array_variant]);
            return;
        }
        ensure_array_schema(node);
        let items = ensure_items_schema(node);
        if path_segments.len() == 1 {
            merge_into_schema_slot(items, leaf);
        } else {
            insert_schema_at_parts(items, &path_segments[1..], leaf);
        }
        return;
    }

    if path_segments.len() > 1 {
        node.clear_exact_empty_constraint_for_descendant();
    }
    if path_segments.len() == 1 {
        let properties = ensure_object_properties(node);
        let key = path_segments[0].clone();
        merge_into_schema_slot(
            properties.entry(key).or_insert_with(SchemaNode::empty),
            leaf,
        );
        return;
    }

    let key = path_segments[0].clone();
    let next_is_array = path_segments.get(1).is_some_and(|segment| segment == "*");
    let properties = ensure_object_properties(node);
    let child = properties.entry(key).or_insert_with(|| {
        if next_is_array {
            new_union_variant_for_head("*")
        } else {
            SchemaNode::closed_object()
        }
    });
    if !next_is_array {
        child.clear_exact_empty_constraint_for_descendant();
    }
    insert_schema_at_parts(child, &path_segments[1..], leaf);
}
