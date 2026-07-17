use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::merge::merge_two_schemas;
use crate::schema_model::is_empty_schema;
use crate::schema_node::SchemaNode;
use crate::values_yaml::{
    child_value_path, schema_node_from_yaml_value_with_skips, yaml_value_at_segments,
};

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
        insert_schema_at_parts(&mut self.root, path_segments, schema);
    }

    /// Opens the root `global` property. Helm shares `global` values across
    /// the whole chart tree, so parent and sibling charts consume keys the
    /// analyzed chart never reads; closing the namespace to this chart's
    /// observed members would reject valid umbrella configurations.
    pub(crate) fn open_helm_global_namespace(&mut self) {
        if let SchemaNode::Object { properties, .. } = &mut self.root
            && let Some(global) = properties.get_mut("global")
        {
            global.open_object();
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

    pub(crate) fn constrain_existing_path_to_object(&mut self, path_segments: &[String]) -> bool {
        constrain_existing_path_to_object(&mut self.root, path_segments)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn merge_missing_values_yaml_defaults_under_roots(
        &mut self,
        values_yaml_doc: &YamlValue,
        root_paths: &[Vec<String>],
        skip_paths: &BTreeSet<Vec<String>>,
    ) {
        let mut insertions = Vec::new();
        for root_path in root_paths {
            let Some(yaml) = yaml_value_at_segments(values_yaml_doc, root_path) else {
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

fn constrain_existing_path_to_object(node: &mut SchemaNode, path_segments: &[String]) -> bool {
    let Some((head, tail)) = path_segments.split_first() else {
        return match node {
            SchemaNode::Empty => {
                *node = SchemaNode::unknown_object();
                true
            }
            SchemaNode::Object { typed, .. } => {
                *typed = true;
                true
            }
            SchemaNode::Foreign(Value::Object(object)) => match object.get("type") {
                None => {
                    object.insert("type".to_string(), Value::String("object".to_string()));
                    true
                }
                Some(Value::String(schema_type)) => schema_type == "object",
                Some(Value::Array(schema_types))
                    if schema_types
                        .iter()
                        .any(|schema_type| schema_type == "object") =>
                {
                    object.insert("type".to_string(), Value::String("object".to_string()));
                    true
                }
                _ => false,
            },
            SchemaNode::Foreign(Value::Bool(false)) => true,
            SchemaNode::Foreign(Value::Bool(true)) => {
                *node = SchemaNode::unknown_object();
                true
            }
            SchemaNode::Array { .. } | SchemaNode::Foreign(_) => false,
        };
    };

    match node {
        SchemaNode::Object { properties, .. } => properties
            .get_mut(head)
            .is_some_and(|child| constrain_existing_path_to_object(child, tail)),
        SchemaNode::Foreign(Value::Object(object)) => {
            let Some(child_value) = object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|properties| properties.get_mut(head))
            else {
                return false;
            };
            let mut child = SchemaNode::foreign(std::mem::take(child_value));
            let constrained = constrain_existing_path_to_object(&mut child, tail);
            *child_value = child.into_value();
            constrained
        }
        SchemaNode::Empty | SchemaNode::Array { .. } | SchemaNode::Foreign(_) => false,
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

pub(crate) fn insert_path_schema_value(
    root_schema: Value,
    path_segments: &[String],
    schema: Value,
) -> Value {
    let mut root = SchemaNode::foreign(root_schema);
    insert_schema_at_parts(&mut root, path_segments, SchemaNode::foreign(schema));
    root.into_value()
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

    // A `*` segment addresses EACH member value of a ranged map: the
    // conditional lands inside the node's `additionalProperties` member
    // slot, mirroring `insert_map_member_row` — a literal `"*"` property
    // would constrain nothing a chart can render, silently dropping the
    // guarded member contract (velero `schedules.*` rows).
    if path_segments[0] == "*" {
        if node.is_empty_slot() {
            *node = SchemaNode::untyped_member_host();
        }
        if let SchemaNode::Object {
            additional_properties,
            ..
        } = node
        {
            let member = additional_properties.get_or_insert_with(|| Box::new(SchemaNode::empty()));
            if !member.is_false_schema() {
                if path_segments.len() == 1 {
                    push_conditional_entry(member, condition, then_schema, true);
                } else {
                    append_conditional_at_parts(
                        member,
                        &path_segments[1..],
                        condition,
                        then_schema,
                    );
                }
                return;
            }
        }
    }

    if matches!(node, SchemaNode::Foreign(_)) && !node.is_empty_slot() {
        let mut fragment = SchemaNode::empty();
        append_conditional_at_parts(&mut fragment, path_segments, condition, then_schema);
        merge_conditional_fragment_into_foreign_slot(node, fragment);
        return;
    }

    // Pure navigation toward the conditional's anchor: a `with` chain skips
    // falsy ancestors, so every carrier level must hold vacuously for them
    // instead of asserting `type: object`.
    if node.is_empty_slot() {
        *node = SchemaNode::untyped_member_host();
    }
    let properties = ensure_object_properties(node);
    let child = properties
        .entry(path_segments[0].clone())
        .or_insert_with(|| {
            if path_segments.len() == 1 {
                SchemaNode::foreign(serde_json::json!({}))
            } else {
                SchemaNode::untyped_member_host()
            }
        });
    if path_segments.len() == 1 {
        push_conditional_entry(child, condition, then_schema, true);
    } else {
        append_conditional_at_parts(child, &path_segments[1..], condition, then_schema);
    }
}

fn merge_conditional_fragment_into_foreign_slot(node: &mut SchemaNode, fragment: SchemaNode) {
    reconcile_node_host_with_branch_schema(node, &fragment);
    open_host_descendants_extended_by_schema(node, &fragment);
    // The fragment is a conditional constraint on the existing slot, not an
    // alternative value lane. A union here lets the old slot bypass every
    // descendant `if`/`then` carried by the fragment.
    node.push_all_of(fragment);
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
    // A trivially-true condition (an unconditional requirement, e.g. an
    // unguarded member-access contract) needs no `if`/`then` wrapper.
    let conditional = if condition.is_empty_slot() {
        then_schema
    } else {
        conditional_entry(condition, then_schema)
    };

    if !matches!(
        node,
        SchemaNode::Object { .. } | SchemaNode::Foreign(Value::Object(_))
    ) {
        *node = SchemaNode::unknown_object();
    }
    node.push_all_of(conditional);
}

fn open_host_descendants_extended_by_schema(node: &mut SchemaNode, schema: &SchemaNode) {
    let branch_properties = schema.property_entries();
    if branch_properties.is_empty() {
        return;
    }

    for (property, branch_child) in branch_properties {
        let Some(mut host_child) = node.take_property(&property) else {
            continue;
        };
        reconcile_node_host_with_branch_schema(&mut host_child, &branch_child);
        open_host_descendants_extended_by_schema(&mut host_child, &branch_child);
        node.put_property(property, host_child);
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

fn collect_missing_yaml_default_insertions(
    root_schema: &SchemaNode,
    current_path: &[String],
    yaml: &YamlValue,
    skip_paths: &BTreeSet<Vec<String>>,
    insertions: &mut Vec<(Vec<String>, SchemaNode)>,
) {
    if skip_paths.iter().any(|skip_path| {
        skip_path.len() < current_path.len() && current_path.starts_with(skip_path)
    }) {
        return;
    }
    if skip_paths.contains(current_path) {
        if !root_schema.path_exists(current_path) {
            insertions.push((current_path.to_vec(), SchemaNode::empty()));
        }
        return;
    }

    if let YamlValue::Mapping(mapping) = yaml
        && !mapping.is_empty()
        && root_schema.path_exists(current_path)
    {
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
        return;
    }

    if !root_schema.path_exists(current_path)
        && let Some(schema) = schema_node_from_yaml_value_with_skips(yaml, current_path, skip_paths)
    {
        insertions.push((current_path.to_vec(), schema));
    }
}

pub(crate) fn apply_values_descriptions(root: &mut Value, descriptions: &BTreeMap<String, String>) {
    for (path, description) in descriptions {
        if description.trim().is_empty() {
            continue;
        }
        let path_segments = crate::split_value_path(path);
        visit_schema_values_at_path_mut(root, &path_segments, &mut |node| {
            set_schema_description(node, description);
        });
    }
}

fn visit_schema_values_at_path_mut(
    node: &mut Value,
    path_segments: &[String],
    visit: &mut impl FnMut(&mut Value),
) -> bool {
    if path_segments.is_empty() {
        visit(node);
        return true;
    }

    let Some(obj) = node.as_object_mut() else {
        return false;
    };

    let mut visited = false;
    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get_mut(key) {
            for variant in variants {
                visited |= visit_schema_values_at_path_mut(variant, path_segments, visit);
            }
        }
    }
    for key in ["then", "else"] {
        if let Some(child) = obj.get_mut(key) {
            visited |= visit_schema_values_at_path_mut(child, path_segments, visit);
        }
    }

    let Some((head, tail)) = path_segments.split_first() else {
        return visited;
    };
    if head == "*" {
        if let Some(items) = obj.get_mut("items") {
            visited |= visit_schema_values_at_path_mut(items, tail, visit);
        }
        return visited;
    }

    if let Some(child) = obj
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .and_then(|properties| properties.get_mut(head))
    {
        visited |= visit_schema_values_at_path_mut(child, tail, visit);
    }
    visited
}

fn set_schema_description(node: &mut Value, description: &str) {
    if let Value::Object(obj) = node {
        obj.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
}

fn new_array_slot() -> SchemaNode {
    SchemaNode::array().items(SchemaNode::foreign(Value::Null))
}

fn ensure_array_items_schema(node: &mut SchemaNode) -> &mut SchemaNode {
    if !matches!(node, SchemaNode::Array { .. }) {
        *node = new_array_slot();
    }
    let SchemaNode::Array { items, .. } = node else {
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
            // An EMPTY slot coerced to host members carries no shape claim
            // of its own: it stays open and untyped (guarded member reads
            // supply the conditional object requirement).
            *node = SchemaNode::untyped_member_host();
        }
        _ => {
            // Hosting a member under a non-object slot proves keys exist,
            // not that the member set is bounded: the coerced host stays
            // open (the strict root closure is created separately).
            *node = SchemaNode::unknown_object();
        }
    }
    let SchemaNode::Object { properties, .. } = node else {
        unreachable!("object schema must be normalized before accessing properties");
    };
    properties
}

fn merge_into_schema_slot(slot: &mut SchemaNode, schema: SchemaNode) {
    if slot.is_empty_slot() {
        *slot = schema;
        return;
    }

    // A TYPELESS member-host carrier (`{"additionalProperties": …}` with no
    // type of its own) merged into an object-typed slot must not degrade
    // the slot into a union whose typeless alternative matches scalars
    // (jenkins' additionalAgents member-map contract): conjoin the
    // carrier's member slot into the object instead.
    if let SchemaNode::Foreign(Value::Object(carrier)) = &schema
        && !carrier.contains_key("type")
        && !carrier.contains_key("anyOf")
        && !carrier.contains_key("oneOf")
        && carrier.contains_key("additionalProperties")
        && carrier
            .keys()
            .all(|key| matches!(key.as_str(), "additionalProperties" | "description"))
        && let SchemaNode::Foreign(Value::Object(object)) = slot
        && object.get("type").and_then(Value::as_str) == Some("object")
        && object
            .get("additionalProperties")
            .is_none_or(|additional| additional != &Value::Bool(false))
    {
        let carrier_member = carrier
            .get("additionalProperties")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
        let member = match object.remove("additionalProperties") {
            None | Some(Value::Bool(true)) => carrier_member,
            Some(existing) => crate::merge::merge_two_schemas(existing, carrier_member),
        };
        object.insert("additionalProperties".to_string(), member);
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

fn replace_schema_at_parts(node: &mut SchemaNode, path_segments: &[String], leaf: SchemaNode) {
    if path_segments.is_empty() {
        *node = leaf;
        return;
    }

    let head = path_segments[0].as_str();
    let replaced = match node {
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
    visit_schema_values_at_path_mut(value, path_segments, &mut |node| {
        *node = replacement.clone();
    })
}

/// Host a `*` member row inside an object-shaped node's
/// `additionalProperties` (a ranged MAP's per-value rows). Returns false
/// when the node is not an open object-shaped schema, leaving list
/// handling to the caller.
fn insert_map_member_row(
    node: &mut SchemaNode,
    path_segments: &[String],
    leaf: &SchemaNode,
) -> bool {
    let insert = |member: &mut SchemaNode| {
        if path_segments.len() == 1 {
            merge_into_schema_slot(member, leaf.clone());
        } else {
            insert_schema_at_parts(member, &path_segments[1..], leaf.clone());
        }
    };
    match node {
        SchemaNode::Object {
            additional_properties,
            ..
        } => {
            let member = additional_properties.get_or_insert_with(|| Box::new(SchemaNode::empty()));
            if member.is_false_schema() {
                return false;
            }
            insert(member.as_mut());
            true
        }
        SchemaNode::Foreign(Value::Object(object))
            if object.get("type").and_then(Value::as_str) == Some("object")
                && object
                    .get("additionalProperties")
                    .is_none_or(|additional| additional != &Value::Bool(false)) =>
        {
            let mut member = object
                .get("additionalProperties")
                .cloned()
                .map_or_else(SchemaNode::empty, SchemaNode::foreign);
            insert(&mut member);
            object.insert("additionalProperties".to_string(), member.into_value());
            true
        }
        // A union base (the runtime iterable domain) hosts the member row
        // in EVERY collection arm: `range` iterates arrays and maps alike,
        // so the member constrains array items and map values, while
        // scalar, null, and CLOSED-object arms (the exact-empty off state)
        // stay untouched. Array arms merge into their `items`
        // directly so repeated member rows fold instead of wrapping.
        SchemaNode::Foreign(Value::Object(object)) if object.contains_key("anyOf") => {
            let Some(arms) = object.get_mut("anyOf").and_then(Value::as_array_mut) else {
                return false;
            };
            let mut hosted = false;
            for arm in arms {
                if let Some(arm_object) = arm.as_object_mut() {
                    let is_array_arm = arm_object.get("type").and_then(Value::as_str)
                        == Some("array")
                        || arm_object.contains_key("items");
                    if is_array_arm {
                        let mut items = arm_object
                            .get("items")
                            .cloned()
                            .map_or_else(SchemaNode::empty, SchemaNode::foreign);
                        insert(&mut items);
                        arm_object.insert("items".to_string(), items.into_value());
                        hosted = true;
                        continue;
                    }
                }
                let mut arm_node = SchemaNode::foreign(std::mem::take(arm));
                hosted |= insert_map_member_row(&mut arm_node, path_segments, leaf);
                *arm = arm_node.into_value();
            }
            hosted
        }
        _ => false,
    }
}

fn insert_schema_at_parts(node: &mut SchemaNode, path_segments: &[String], leaf: SchemaNode) {
    if path_segments.is_empty() {
        return;
    }

    // `range` iterates maps as well as lists, so a member row's `*` segment
    // means "each member value" of whatever container the node already is:
    // an object-shaped node hosts it under `additionalProperties` instead
    // of growing an array alternative.
    if path_segments[0] == "*" && insert_map_member_row(node, path_segments, &leaf) {
        return;
    }

    if matches!(node, SchemaNode::Foreign(_)) && !node.is_empty_slot() {
        // Descend through properties the resolved value already declares, so
        // the descendant merges at the deepest existing node and that node's
        // own openness decides the outcome (a top-level merge of a nested
        // carrier would let the carrier's materialized closure shut an open
        // map one level down).
        if path_segments.len() > 1
            && path_segments[0] != "*"
            && let SchemaNode::Foreign(value) = node
            && let Some(child_value) = value
                .get_mut("properties")
                .and_then(|properties| properties.get_mut(&path_segments[0]))
        {
            let mut child_node = SchemaNode::foreign(std::mem::take(child_value));
            insert_schema_at_parts(&mut child_node, &path_segments[1..], leaf);
            *child_value = child_node.into_value();
            return;
        }
        // A union base (an off-state arm plus one open object arm, e.g. a
        // declared-empty serialized map) hosts descendants in its open arm:
        // merging a carrier at the union level would replace the open arm
        // with the carrier's materialized closure.
        if path_segments[0] != "*"
            && let SchemaNode::Foreign(value) = node
            && let Some(arms) = value.get_mut("anyOf").and_then(Value::as_array_mut)
        {
            let arm_is_open_object = |arm: &Value| {
                arm.get("additionalProperties")
                    .is_some_and(|ap| ap.as_bool() != Some(false))
            };
            let mut open_arms = arms.iter_mut().filter(|arm| arm_is_open_object(arm));
            if let (Some(arm), None) = (open_arms.next(), open_arms.next()) {
                let mut arm_node = SchemaNode::foreign(std::mem::take(arm));
                insert_schema_at_parts(&mut arm_node, path_segments, leaf);
                *arm = arm_node.into_value();
                return;
            }
        }
        let mut fragment = SchemaNode::empty();
        insert_schema_at_parts(&mut fragment, path_segments, leaf);
        merge_into_schema_slot(node, fragment);
        return;
    }

    if path_segments[0] == "*" {
        if node.is_empty_slot() {
            // A bare `*` member row proves members exist, not which
            // collection lane hosts them: `range` iterates arrays and maps
            // alike, so a slot with no other shape evidence opens both lanes
            // instead of inventing an array-only shape.
            *node = SchemaNode::foreign(serde_json::json!({
                "anyOf": [
                    { "items": {}, "type": "array" },
                    { "additionalProperties": {}, "type": "object" },
                ]
            }));
            let hosted = insert_map_member_row(node, path_segments, &leaf);
            debug_assert!(hosted, "two-lane member seed must host the row");
            return;
        }
        if !node.is_array_like() {
            let existing = std::mem::replace(node, SchemaNode::empty());
            let mut array_variant = new_array_slot();
            insert_schema_at_parts(&mut array_variant, path_segments, leaf);
            *node = SchemaNode::any_of(vec![existing, array_variant]);
            return;
        }
        let items = ensure_array_items_schema(node);
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
            // Left empty so the `*` handling above seeds both collection
            // lanes for the member row instead of an array-only slot.
            SchemaNode::empty()
        } else {
            // An ancestor object materialized only because a DESCENDANT is
            // referenced: member reads prove keys exist, they do not bound
            // the member set, so the interior node stays open (the strict
            // root closure is created separately).
            SchemaNode::unknown_object()
        }
    });
    if !next_is_array {
        child.clear_exact_empty_constraint_for_descendant();
    }
    insert_schema_at_parts(child, &path_segments[1..], leaf);
}
