use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{Segment, ValuesPath};
use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::merge::merge_two_schemas;
use crate::schema_node::{JsonSchemaType, SchemaNode, SchemaTypeKeyword, TypedSchemaNode};
use crate::values_yaml::{
    child_value_path, schema_node_from_yaml_value_with_skips, yaml_value_at_segments,
};

#[derive(Clone)]
pub(crate) struct SchemaDocument {
    root: SchemaNode,
    parsed_after_declared_materialization: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalConstraintOutcome {
    Applied(CanonicalConstraintApplication),
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalConstraintApplication {
    Emitted,
    Redundant,
}

impl SchemaDocument {
    pub(crate) fn new_root_object() -> Self {
        Self {
            root: SchemaNode::closed_object(),
            parsed_after_declared_materialization: false,
        }
    }

    pub(crate) fn visit_embedded_values(&self, visit: &mut impl FnMut(&Value)) {
        self.root.visit_foreign_values(visit);
    }

    pub(crate) fn insert_path_schema(
        &mut self,
        path_segments: &[String],
        schema: SchemaNode,
    ) -> usize {
        let path_segments = legacy_path_segments(path_segments);
        insert_schema_at_parts(&mut self.root, &path_segments, schema)
    }

    pub(crate) fn insert_values_path_schema(
        &mut self,
        value_path: &ValuesPath,
        schema: SchemaNode,
    ) -> usize {
        let path_segments = value_path.segments().cloned().collect::<Vec<_>>();
        insert_schema_at_parts(&mut self.root, &path_segments, schema)
    }

    /// Opens the root `global` property. Helm shares `global` values across
    /// the whole chart tree, so parent and sibling charts consume keys the
    /// analyzed chart never reads; closing the namespace to this chart's
    /// observed members would reject valid umbrella configurations.
    pub(crate) fn open_helm_global_namespace(&mut self) {
        let global = match &mut self.root {
            SchemaNode::Object { properties, .. } => properties.get_mut("global"),
            SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => keywords
                .properties
                .as_mut()
                .and_then(|properties| properties.get_mut("global")),
            _ => None,
        };
        if let Some(global) = global {
            global.open_object();
        }
    }

    pub(crate) fn replace_values_path_schema(
        &mut self,
        value_path: &ValuesPath,
        schema: SchemaNode,
    ) -> usize {
        let path_segments = value_path.segments().cloned().collect::<Vec<_>>();
        replace_schema_at_parts(&mut self.root, &path_segments, schema)
    }

    /// Conjoins an unconditional generator constraint at an existing values
    /// slot. A missing direct slot is not created: the closed root may reject
    /// that name, so the caller must retain its root-anchored fallback.
    pub(crate) fn canonicalize_constraint_at_path(
        &mut self,
        path_segments: &[String],
        constraint: &Value,
    ) -> CanonicalConstraintOutcome {
        canonicalize_constraint_at_parts(&mut self.root, path_segments, constraint)
    }

    pub(crate) fn append_conditional(
        &mut self,
        ancestor_segments: &[String],
        condition: SchemaNode,
        then_schema: SchemaNode,
    ) {
        append_conditional_at_parts(&mut self.root, ancestor_segments, condition, then_schema);
    }

    /// Drops the structural `type: object` from a host whose members are
    /// only ever read through the nil-safe grouped form: the caller's
    /// presence-guarded arm carries the object requirement, and the base
    /// must render-match helm's tolerance for an absent or null-deleted
    /// receiver. Only the tree's own `Object` nodes relax — a `Foreign`
    /// base claiming `type: object` was resolved from independent
    /// evidence and keeps it.
    pub(crate) fn relax_host_object_type(&mut self, path_segments: &[String]) {
        relax_host_object_type(&mut self.root, path_segments);
    }

    /// Applies a wildcard member schema to keys that `values.yaml` already
    /// materialized as named properties.
    ///
    /// JSON Schema excludes named `properties` from
    /// `additionalProperties`. A direct range over a member identity owns
    /// both sets, so leaving the declared slots untouched would let their
    /// example shapes override the runtime member domain.
    pub(crate) fn materialize_declared_member_schema(
        &mut self,
        path_segments: &[String],
        declared: &YamlValue,
        member_schema: &SchemaNode,
    ) {
        let YamlValue::Mapping(declared) = declared else {
            return;
        };
        let keys = declared
            .keys()
            .filter_map(YamlValue::as_str)
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return;
        }

        visit_schema_nodes_at_path_mut(&mut self.root, path_segments, &mut |schema| {
            materialize_declared_properties(schema, &keys, member_schema);
        });
        if !self.parsed_after_declared_materialization {
            let root = std::mem::replace(&mut self.root, SchemaNode::Empty);
            self.root = root.into_parsed_representation();
            self.parsed_after_declared_materialization = true;
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn merge_missing_values_yaml_defaults_under_roots(
        &mut self,
        values_yaml_doc: &YamlValue,
        root_paths: &[Vec<String>],
        skip_paths: &BTreeSet<Vec<String>>,
    ) -> usize {
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
        insertions
            .into_iter()
            .map(|(path, schema)| self.insert_path_schema(&path, schema))
            .sum()
    }

    pub(crate) fn into_value(self) -> Value {
        self.root.into_value()
    }
}

fn canonicalize_constraint_at_parts(
    node: &mut SchemaNode,
    path_segments: &[String],
    constraint: &Value,
) -> CanonicalConstraintOutcome {
    let Some((head, tail)) = path_segments.split_first() else {
        let is_object_constraint = constraint.as_object().is_some_and(|object| {
            object.len() == 1 && object.get("type").and_then(Value::as_str) == Some("object")
        });
        if is_object_constraint && let Some(outcome) = canonicalize_object_constraint(node) {
            return outcome;
        }
        if let Some(outcome) = apply_required_entries(node, constraint) {
            return outcome;
        }
        if !is_object_constraint && !is_not_null_constraint(constraint) {
            return CanonicalConstraintOutcome::NotApplicable;
        }
        if node.is_false_schema() || constraint_is_implied_by_node(node, constraint) {
            return CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant);
        }
        let existing = std::mem::replace(node, SchemaNode::empty());
        *node = SchemaNode::all_of(vec![existing, SchemaNode::foreign(constraint.clone())]);
        return CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted);
    };

    match node {
        SchemaNode::Object { properties, .. } => properties
            .get_mut(head)
            .map_or(CanonicalConstraintOutcome::NotApplicable, |child| {
                canonicalize_constraint_at_parts(child, tail, constraint)
            }),
        SchemaNode::Array { items, .. } if head == "*" => items
            .as_deref_mut()
            .map_or(CanonicalConstraintOutcome::NotApplicable, |child| {
                canonicalize_constraint_at_parts(child, tail, constraint)
            }),
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            let child = if head == "*" {
                keywords.items.as_deref_mut()
            } else {
                keywords
                    .properties
                    .as_mut()
                    .and_then(|properties| properties.get_mut(head))
            };
            child.map_or(CanonicalConstraintOutcome::NotApplicable, |child| {
                canonicalize_constraint_at_parts(child, tail, constraint)
            })
        }
        SchemaNode::Empty
        | SchemaNode::Array { .. }
        | SchemaNode::Typed(TypedSchemaNode::Boolean(_))
        | SchemaNode::Foreign(_) => CanonicalConstraintOutcome::NotApplicable,
    }
}

fn canonicalize_object_constraint(node: &mut SchemaNode) -> Option<CanonicalConstraintOutcome> {
    match node {
        SchemaNode::Empty | SchemaNode::Typed(TypedSchemaNode::Boolean(true)) => {
            *node = SchemaNode::type_named("object");
            Some(CanonicalConstraintOutcome::Applied(
                CanonicalConstraintApplication::Emitted,
            ))
        }
        SchemaNode::Object { typed, .. } => {
            let was_typed = *typed;
            *typed = true;
            Some(if was_typed {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant)
            } else {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
            })
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            if keywords.schema_type == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object)) {
                return Some(CanonicalConstraintOutcome::Applied(
                    CanonicalConstraintApplication::Redundant,
                ));
            }
            None
        }
        SchemaNode::Typed(TypedSchemaNode::Boolean(false)) => Some(
            CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant),
        ),
        SchemaNode::Array { .. } | SchemaNode::Foreign(_) => None,
    }
}

fn apply_required_entries(
    node: &mut SchemaNode,
    constraint: &Value,
) -> Option<CanonicalConstraintOutcome> {
    let constraint = constraint.as_object()?;
    if constraint.len() != 2 || constraint.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    let required = constraint.get("required")?.as_array()?;
    let required = required
        .iter()
        .map(Value::as_str)
        .collect::<Option<Vec<_>>>()?;

    match node {
        SchemaNode::Object {
            typed,
            properties,
            required: existing,
            ..
        } => {
            if required.iter().any(|name| !properties.contains_key(*name)) {
                return Some(CanonicalConstraintOutcome::NotApplicable);
            }
            let before = existing.len();
            let was_typed = *typed;
            *typed = true;
            existing.extend(required.into_iter().map(str::to_string));
            Some(if was_typed && existing.len() == before {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant)
            } else {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
            })
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
            if keywords.schema_type == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object)) =>
        {
            let all_slots_exist = keywords.properties.as_ref().is_some_and(|properties| {
                required.iter().all(|name| properties.contains_key(*name))
            });
            if !all_slots_exist {
                return Some(CanonicalConstraintOutcome::NotApplicable);
            }
            if required.is_empty() {
                return Some(CanonicalConstraintOutcome::Applied(
                    CanonicalConstraintApplication::Redundant,
                ));
            }
            let existing = keywords.required.get_or_insert_with(Vec::new);
            let before = existing.len();
            for name in required {
                let name = name.to_string();
                if !existing.contains(&name) {
                    existing.push(name);
                }
            }
            existing.sort();
            Some(if existing.len() == before {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant)
            } else {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
            })
        }
        SchemaNode::Typed(TypedSchemaNode::Boolean(false)) => Some(
            CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant),
        ),
        SchemaNode::Empty
        | SchemaNode::Array { .. }
        | SchemaNode::Typed(_)
        | SchemaNode::Foreign(_) => Some(CanonicalConstraintOutcome::NotApplicable),
    }
}

fn constraint_is_implied_by_node(node: &SchemaNode, constraint: &Value) -> bool {
    let existing = node.clone().into_value();
    if is_not_null_constraint(constraint) {
        return crate::schema_model::schema_excludes_type(&existing, "null");
    }
    false
}

fn is_not_null_constraint(constraint: &Value) -> bool {
    constraint
        == &serde_json::json!({
            "not": { "type": "null" },
        })
}

fn relax_host_object_type(node: &mut SchemaNode, path_segments: &[String]) {
    let Some((head, tail)) = path_segments.split_first() else {
        match node {
            SchemaNode::Object { typed, .. } => *typed = false,
            SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords.schema_type
                    == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object)) =>
            {
                keywords.schema_type = Some(SchemaTypeKeyword::Multiple(vec![
                    JsonSchemaType::Object,
                    JsonSchemaType::Null,
                ]));
            }
            _ => {}
        }
        return;
    };
    match node {
        SchemaNode::Object { properties, .. } => {
            if let Some(child) = properties.get_mut(head) {
                relax_host_object_type(child, tail);
            }
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            if let Some(child) = keywords
                .properties
                .as_mut()
                .and_then(|properties| properties.get_mut(head))
            {
                relax_host_object_type(child, tail);
            }
        }
        _ => {}
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
) -> (Value, usize) {
    let mut root = SchemaNode::foreign(root_schema);
    let path_segments = legacy_path_segments(path_segments);
    let abstentions =
        insert_schema_at_parts(&mut root, &path_segments, SchemaNode::foreign(schema));
    (root.into_value(), abstentions)
}

fn legacy_path_segments(path_segments: &[String]) -> Vec<Segment> {
    path_segments
        .iter()
        .map(|segment| Segment::from_encoded_component(segment))
        .collect()
}

/// Conjoins one member schema with every array-item and map-value lane.
///
/// This is deliberately distinct from [`insert_path_schema_value`], whose
/// merge operation combines alternative inference evidence. A schema that
/// enriches a fail requirement is a logical conjunction; union-merging it
/// into the requirement can collapse separate `anyOf` arms.
pub(crate) fn conjoin_collection_member_schema_value(
    mut collection_schema: Value,
    member_schema: &Value,
) -> Value {
    conjoin_collection_member_schema(&mut collection_schema, member_schema);
    collection_schema
}

fn conjoin_collection_member_schema(collection_schema: &mut Value, member_schema: &Value) -> bool {
    let Some(object) = collection_schema.as_object_mut() else {
        return false;
    };

    let mut conjoined = false;
    for keyword in ["anyOf", "oneOf"] {
        if let Some(arms) = object.get_mut(keyword).and_then(Value::as_array_mut) {
            for arm in arms {
                conjoined |= conjoin_collection_member_schema(arm, member_schema);
            }
        }
    }

    if let Some(types) = object.get("type").and_then(Value::as_array) {
        let allows_array = types
            .iter()
            .any(|schema_type| schema_type.as_str() == Some("array"));
        let allows_object = types
            .iter()
            .any(|schema_type| schema_type.as_str() == Some("object"));
        if allows_array {
            let existing = object
                .remove("items")
                .unwrap_or_else(|| Value::Object(Map::new()));
            object.insert(
                "items".to_string(),
                conjoin_schema_values(existing, member_schema.clone()),
            );
        }
        if allows_object {
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                for property in properties.values_mut() {
                    let existing = std::mem::take(property);
                    *property = conjoin_schema_values(existing, member_schema.clone());
                }
            }
            let existing = object
                .remove("additionalProperties")
                .unwrap_or_else(|| Value::Object(Map::new()));
            object.insert(
                "additionalProperties".to_string(),
                conjoin_schema_values(existing, member_schema.clone()),
            );
        }
        if allows_array || allows_object {
            return true;
        }
    }

    match object.get("type").and_then(Value::as_str) {
        Some("array") => {
            let existing = object
                .remove("items")
                .unwrap_or_else(|| Value::Object(Map::new()));
            object.insert(
                "items".to_string(),
                conjoin_schema_values(existing, member_schema.clone()),
            );
            true
        }
        Some("object") if object.get("additionalProperties") != Some(&Value::Bool(false)) => {
            if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
                for property in properties.values_mut() {
                    let existing = std::mem::take(property);
                    *property = conjoin_schema_values(existing, member_schema.clone());
                }
            }
            let existing = object
                .remove("additionalProperties")
                .unwrap_or_else(|| Value::Object(Map::new()));
            object.insert(
                "additionalProperties".to_string(),
                conjoin_schema_values(existing, member_schema.clone()),
            );
            true
        }
        _ => conjoined,
    }
}

fn conjoin_schema_values(existing: Value, incoming: Value) -> Value {
    if existing == incoming || crate::schema_model::is_empty_schema(&incoming) {
        return existing;
    }
    if schemas_equal_ignoring_noop_collection_keywords(&existing, &incoming) {
        return incoming;
    }
    if crate::schema_model::is_empty_schema(&existing) {
        return incoming;
    }
    if let Some(merged) = conjoin_type_only_object_with_carrier(&existing, &incoming)
        .or_else(|| conjoin_type_only_object_with_carrier(&incoming, &existing))
    {
        return merged;
    }
    SchemaNode::all_of(vec![
        SchemaNode::foreign(existing),
        SchemaNode::foreign(incoming),
    ])
    .into_value()
}

fn conjoin_type_only_object_with_carrier(typed: &Value, carrier: &Value) -> Option<Value> {
    let typed = typed.as_object()?;
    if typed.len() != 1 || typed.get("type").and_then(Value::as_str) != Some("object") {
        return None;
    }
    let carrier = carrier.as_object()?;
    if carrier.contains_key("type")
        || !carrier.keys().all(|key| {
            matches!(
                key.as_str(),
                "additionalProperties" | "allOf" | "patternProperties" | "properties" | "required"
            )
        })
    {
        return None;
    }
    let mut merged = carrier.clone();
    merged.insert("type".to_string(), Value::String("object".to_string()));
    Some(Value::Object(merged))
}

fn schemas_equal_ignoring_noop_collection_keywords(left: &Value, right: &Value) -> bool {
    fn strip_noop_collection_keywords(schema: &mut Value) {
        match schema {
            Value::Array(items) => {
                for item in items {
                    strip_noop_collection_keywords(item);
                }
            }
            Value::Object(object) => {
                for value in object.values_mut() {
                    strip_noop_collection_keywords(value);
                }
                for keyword in ["additionalProperties", "items"] {
                    if object
                        .get(keyword)
                        .is_some_and(crate::schema_model::is_empty_schema)
                    {
                        object.remove(keyword);
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }

    let mut left = left.clone();
    let mut right = right.clone();
    strip_noop_collection_keywords(&mut left);
    strip_noop_collection_keywords(&mut right);
    left == right
}

fn conditional_entry(condition: Value, then_schema: SchemaNode) -> SchemaNode {
    SchemaNode::foreign(Value::Object(
        [
            ("if".to_string(), condition),
            ("then".to_string(), then_schema.into_value()),
        ]
        .into_iter()
        .collect(),
    ))
}

/// Whether a condition schema is a pure partition of the value's own TYPE
/// (`{type: object}`, also negated or a combination of such tests). Such a
/// condition deliberately selects one runtime kind out of several the chart
/// handles, so the conditional's carrier must not assert `type: object`
/// itself; a condition that tests members instead presupposes an object
/// host.
fn condition_is_value_type_partition(condition: &Value) -> bool {
    let Value::Object(object) = condition else {
        return false;
    };
    if object.len() != 1 {
        return false;
    }
    if let Some(inner) = object.get("not") {
        return condition_is_value_type_partition(inner);
    }
    if let Some(Value::Array(items)) = object.get("anyOf").or_else(|| object.get("allOf")) {
        return items.iter().all(condition_is_value_type_partition);
    }
    object.contains_key("type")
}

fn append_conditional_at_parts(
    node: &mut SchemaNode,
    path_segments: &[String],
    condition: SchemaNode,
    then_schema: SchemaNode,
) {
    let Some((head, tail)) = path_segments.split_first() else {
        push_conditional_entry(node, condition, then_schema, false);
        return;
    };

    // A `*` segment addresses EACH member value of a ranged map: the
    // conditional lands inside the node's `additionalProperties` member
    // slot, mirroring `insert_map_member_row` — a literal `"*"` property
    // would constrain nothing a chart can render, silently dropping the
    // guarded member contract (velero `schedules.*` rows).
    if head == "*" {
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
                if tail.is_empty() {
                    push_conditional_entry(member, condition, then_schema, true);
                } else {
                    append_conditional_at_parts(member, tail, condition, then_schema);
                }
                return;
            }
        }
        if let SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) = node {
            let member = keywords
                .additional_properties
                .get_or_insert_with(|| Box::new(SchemaNode::empty()));
            if !member.is_false_schema() {
                if tail.is_empty() {
                    push_conditional_entry(member, condition, then_schema, true);
                } else {
                    append_conditional_at_parts(member, tail, condition, then_schema);
                }
                return;
            }
        }
    }

    if matches!(node, SchemaNode::Typed(_) | SchemaNode::Foreign(_)) && !node.is_empty_slot() {
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
    let child = properties.entry(head.clone()).or_insert_with(|| {
        if tail.is_empty() {
            SchemaNode::foreign(serde_json::json!({}))
        } else {
            SchemaNode::untyped_member_host()
        }
    });
    if tail.is_empty() {
        push_conditional_entry(child, condition, then_schema, true);
    } else {
        append_conditional_at_parts(child, tail, condition, then_schema);
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
    let condition = condition.into_value();
    let type_partition = condition_is_value_type_partition(&condition);
    // A trivially-true condition (an unconditional requirement, e.g. an
    // unguarded member-access contract) needs no `if`/`then` wrapper.
    let conditional = if crate::schema_model::is_empty_schema(&condition) {
        then_schema
    } else {
        conditional_entry(condition, then_schema)
    };

    if !matches!(
        node,
        SchemaNode::Object { .. } | SchemaNode::Typed(TypedSchemaNode::Keywords(_))
    ) {
        // A type-partition arm deliberately selects one runtime kind out of
        // several the chart handles, so its carrier holds vacuously for the
        // unselected kinds; every other conditional presupposes an object
        // host the same way its member tests do.
        *node = if type_partition {
            SchemaNode::untyped_member_host()
        } else {
            SchemaNode::unknown_object()
        };
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

#[derive(Default)]
struct DescriptionPathTrie<'a> {
    description: Option<&'a str>,
    children: BTreeMap<String, Self>,
}

impl<'a> DescriptionPathTrie<'a> {
    fn new(descriptions: &'a BTreeMap<String, String>) -> Self {
        let mut root = Self::default();
        for (path, description) in descriptions {
            if description.trim().is_empty() {
                continue;
            }
            let mut node = &mut root;
            for segment in crate::split_value_path(path) {
                node = node.children.entry(segment).or_default();
            }
            node.description = Some(description);
        }
        root
    }
}

pub(crate) fn apply_values_descriptions(root: &mut Value, descriptions: &BTreeMap<String, String>) {
    apply_description_trie(root, &DescriptionPathTrie::new(descriptions), true);
}

fn apply_description_trie(
    node: &mut Value,
    descriptions: &DescriptionPathTrie<'_>,
    apply_current: bool,
) {
    if apply_current && let Some(description) = &descriptions.description {
        set_schema_description(node, description);
    }
    if descriptions.children.is_empty() {
        return;
    }

    let Some(obj) = node.as_object_mut() else {
        return;
    };

    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(Value::Array(variants)) = obj.get_mut(key) {
            for variant in variants {
                apply_description_trie(variant, descriptions, false);
            }
        }
    }
    for key in ["then", "else"] {
        if let Some(child) = obj.get_mut(key) {
            apply_description_trie(child, descriptions, false);
        }
    }

    for (segment, child_descriptions) in &descriptions.children {
        if segment == "*" {
            if let Some(items) = obj.get_mut("items") {
                apply_description_trie(items, child_descriptions, true);
            }
            continue;
        }
        if let Some(child) = obj
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .and_then(|properties| properties.get_mut(segment))
        {
            apply_description_trie(child, child_descriptions, true);
        }
    }
}

fn set_schema_description(node: &mut Value, description: &str) {
    if let Value::Object(obj) = node {
        obj.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }
}

fn visit_schema_nodes_at_path_mut(
    node: &mut SchemaNode,
    path_segments: &[String],
    visit: &mut impl FnMut(&mut SchemaNode),
) {
    let Some((head, tail)) = path_segments.split_first() else {
        visit(node);
        return;
    };

    match node {
        SchemaNode::Object {
            properties, all_of, ..
        } => {
            for variant in all_of {
                visit_schema_nodes_at_path_mut(variant, path_segments, visit);
            }
            if let Some(child) = properties.get_mut(head) {
                visit_schema_nodes_at_path_mut(child, tail, visit);
            }
        }
        SchemaNode::Array { items, .. } if head == "*" => {
            if let Some(items) = items {
                visit_schema_nodes_at_path_mut(items, tail, visit);
            }
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            for variants in [
                &mut keywords.any_of,
                &mut keywords.all_of,
                &mut keywords.one_of,
            ] {
                for variant in variants.iter_mut().flatten() {
                    visit_schema_nodes_at_path_mut(variant, path_segments, visit);
                }
            }
            for child in [&mut keywords.then_schema, &mut keywords.else_schema]
                .into_iter()
                .flatten()
            {
                visit_schema_nodes_at_path_mut(child, path_segments, visit);
            }
            if head == "*" {
                if let Some(items) = &mut keywords.items {
                    visit_schema_nodes_at_path_mut(items, tail, visit);
                }
            } else if let Some(child) = keywords
                .properties
                .as_mut()
                .and_then(|properties| properties.get_mut(head))
            {
                visit_schema_nodes_at_path_mut(child, tail, visit);
            }
        }
        SchemaNode::Empty
        | SchemaNode::Array { .. }
        | SchemaNode::Typed(TypedSchemaNode::Boolean(_))
        | SchemaNode::Foreign(_) => {}
    }
}

fn materialize_declared_properties(
    schema: &mut SchemaNode,
    keys: &[&str],
    member_schema: &SchemaNode,
) {
    match schema {
        SchemaNode::Object {
            properties,
            typed,
            all_of,
            include_empty_properties,
            additional_properties,
            ..
        } => {
            for arm in all_of {
                materialize_declared_properties(arm, keys, member_schema);
            }
            if !*typed
                && !*include_empty_properties
                && properties.is_empty()
                && additional_properties.is_none()
            {
                return;
            }
            for key in keys {
                properties.insert((*key).to_string(), member_schema.clone());
            }
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            for arms in [
                &mut keywords.any_of,
                &mut keywords.all_of,
                &mut keywords.one_of,
            ] {
                for arm in arms.iter_mut().flatten() {
                    materialize_declared_properties(arm, keys, member_schema);
                }
            }
            let object_lane = keywords.schema_type
                == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
                || keywords.properties.is_some()
                || keywords.additional_properties.is_some();
            if !object_lane {
                return;
            }
            let properties = keywords.properties.get_or_insert_with(BTreeMap::new);
            for key in keys {
                properties.insert((*key).to_string(), member_schema.clone());
            }
        }
        SchemaNode::Empty
        | SchemaNode::Array { .. }
        | SchemaNode::Typed(TypedSchemaNode::Boolean(_))
        | SchemaNode::Foreign(_) => {}
    }
}

fn new_array_slot() -> SchemaNode {
    SchemaNode::array().items(SchemaNode::foreign(Value::Null))
}

fn ensure_array_items_schema(node: &mut SchemaNode) -> &mut SchemaNode {
    let is_array = matches!(node, SchemaNode::Array { .. })
        || matches!(
            node,
            SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
                if keywords.is_array_like()
        );
    if !is_array {
        *node = new_array_slot();
    }
    match node {
        SchemaNode::Array { items, .. } => items
            .get_or_insert_with(|| Box::new(SchemaNode::foreign(Value::Null)))
            .as_mut(),
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => keywords
            .items
            .get_or_insert_with(|| Box::new(SchemaNode::foreign(Value::Null)))
            .as_mut(),
        _ => ensure_array_items_schema(node),
    }
}

fn ensure_object_properties(node: &mut SchemaNode) -> &mut BTreeMap<String, SchemaNode> {
    let is_object = matches!(node, SchemaNode::Object { .. })
        || matches!(
            node,
            SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
            if keywords.schema_type
                == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
                || keywords.schema_type.is_none()
                    && (keywords.properties.is_some()
                        || keywords.additional_properties.is_some())
        );
    if !is_object {
        let is_empty_schema = matches!(
            node,
            SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) if keywords.is_empty_schema()
        );
        if is_empty_schema {
            *node = SchemaNode::untyped_member_host();
        } else {
            // Hosting a member under a non-object slot proves keys exist,
            // not that the member set is bounded: the coerced host stays
            // open (the strict root closure is created separately).
            *node = SchemaNode::unknown_object();
        }
    }
    match node {
        SchemaNode::Object { properties, .. } => properties,
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            keywords.properties.get_or_insert_with(BTreeMap::new)
        }
        _ => ensure_object_properties(node),
    }
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
    if let SchemaNode::Typed(TypedSchemaNode::Keywords(carrier)) = &schema
        && carrier.schema_type.is_none()
        && carrier.any_of.is_none()
        && carrier.one_of.is_none()
        && carrier.additional_properties.is_some()
        && carrier.properties.is_none()
        && carrier.required.is_none()
        && carrier.items.is_none()
        && carrier.all_of.is_none()
        && carrier.not.is_none()
        && carrier.if_schema.is_none()
        && carrier.then_schema.is_none()
        && carrier.else_schema.is_none()
        && carrier.min_properties.is_none()
        && carrier.max_properties.is_none()
        && carrier.min_items.is_none()
        && carrier
            .extra_keywords
            .keys()
            .all(|key| key == "description")
        && let SchemaNode::Typed(TypedSchemaNode::Keywords(object)) = slot
        && object.schema_type == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
        && object
            .additional_properties
            .as_deref()
            .is_none_or(|additional| !additional.is_false_schema())
    {
        let carrier_member = carrier
            .additional_properties
            .as_deref()
            .cloned()
            .unwrap_or_else(SchemaNode::empty);
        let member = match object.additional_properties.take().map(|value| *value) {
            None | Some(SchemaNode::Typed(TypedSchemaNode::Boolean(true))) => carrier_member,
            Some(existing) => SchemaNode::from_value(merge_two_schemas(
                existing.into_value(),
                carrier_member.into_value(),
            )),
        };
        object.additional_properties = Some(Box::new(member));
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

fn replace_schema_at_parts(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    leaf: SchemaNode,
) -> usize {
    if replace_existing_schema_at_parts(node, path_segments, &leaf) {
        return 0;
    }
    insert_schema_at_parts(node, path_segments, leaf)
}

fn replace_existing_schema_at_parts(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    leaf: &SchemaNode,
) -> bool {
    let Some((head, tail)) = path_segments.split_first() else {
        *node = leaf.clone();
        return true;
    };
    match node {
        SchemaNode::Object { properties, .. } => properties
            .get_mut(head.literal().unwrap_or("*"))
            .is_some_and(|child| replace_existing_schema_at_parts(child, tail, leaf)),
        SchemaNode::Array { items, .. } if head.is_each_member() => items
            .as_deref_mut()
            .is_some_and(|child| replace_existing_schema_at_parts(child, tail, leaf)),
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) => {
            let mut replaced = false;
            for schemas in [
                &mut keywords.any_of,
                &mut keywords.all_of,
                &mut keywords.one_of,
            ]
            .into_iter()
            .flatten()
            {
                for schema in schemas {
                    replaced |= replace_existing_schema_at_parts(schema, path_segments, leaf);
                }
            }
            for schema in [&mut keywords.then_schema, &mut keywords.else_schema]
                .into_iter()
                .filter_map(Option::as_deref_mut)
            {
                replaced |= replace_existing_schema_at_parts(schema, path_segments, leaf);
            }
            let child = if head.is_each_member() {
                keywords.items.as_deref_mut()
            } else {
                keywords
                    .properties
                    .as_mut()
                    .and_then(|properties| head.literal().and_then(|head| properties.get_mut(head)))
            };
            replaced
                | child.is_some_and(|child| replace_existing_schema_at_parts(child, tail, leaf))
        }
        _ => false,
    }
}

/// Host a `*` member row inside an object-shaped node's
/// `additionalProperties` (a ranged MAP's per-value rows). Returns false
/// when the node is not an open object-shaped schema, leaving list
/// handling to the caller.
fn insert_map_member_row(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) -> bool {
    let Some((_, tail)) = path_segments.split_first() else {
        return false;
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
            insert_map_member_schema(member.as_mut(), tail, leaf, ambiguous_union_abstentions);
            true
        }
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords))
            if keywords.schema_type == Some(SchemaTypeKeyword::Single(JsonSchemaType::Object))
                && keywords
                    .additional_properties
                    .as_deref()
                    .is_none_or(|additional| !additional.is_false_schema()) =>
        {
            let member = keywords
                .additional_properties
                .get_or_insert_with(|| Box::new(SchemaNode::empty()));
            insert_map_member_schema(member, tail, leaf, ambiguous_union_abstentions);
            true
        }
        // A union base (the runtime iterable domain) hosts the member row
        // in EVERY collection arm: `range` iterates arrays and maps alike,
        // so the member constrains array items and map values, while
        // scalar, null, and CLOSED-object arms (the exact-empty off state)
        // stay untouched. Array arms merge into their `items`
        // directly so repeated member rows fold instead of wrapping.
        SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) if keywords.any_of.is_some() => {
            let Some(arms) = keywords.any_of.as_mut() else {
                return false;
            };
            let mut hosted = false;
            for arm in arms {
                if arm.is_array_like() {
                    let items = ensure_array_items_schema(arm);
                    insert_map_member_schema(items, tail, leaf, ambiguous_union_abstentions);
                    hosted = true;
                } else {
                    hosted |= insert_map_member_row(
                        arm,
                        path_segments,
                        leaf,
                        ambiguous_union_abstentions,
                    );
                }
            }
            hosted
        }
        _ => false,
    }
}

fn insert_map_member_schema(
    member: &mut SchemaNode,
    tail: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) {
    if tail.is_empty() {
        merge_into_schema_slot(member, leaf.clone());
    } else {
        insert_schema_at_parts_tracking(member, tail, leaf.clone(), ambiguous_union_abstentions);
    }
}

fn insert_schema_at_parts(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    leaf: SchemaNode,
) -> usize {
    let mut ambiguous_union_abstentions = 0;
    insert_schema_at_parts_tracking(node, path_segments, leaf, &mut ambiguous_union_abstentions);
    ambiguous_union_abstentions
}

fn insert_schema_at_parts_tracking(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    leaf: SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) {
    let Some((head, tail)) = path_segments.split_first() else {
        return;
    };

    // `range` iterates maps as well as lists, so a member row's `*` segment
    // means "each member value" of whatever container the node already is:
    // an object-shaped node hosts it under `additionalProperties` instead
    // of growing an array alternative.
    if head.is_each_member()
        && insert_map_member_row(node, path_segments, &leaf, ambiguous_union_abstentions)
    {
        return;
    }

    if insert_into_nonempty_schema_slot(
        node,
        path_segments,
        head,
        tail,
        &leaf,
        ambiguous_union_abstentions,
    ) {
        return;
    }

    if head.is_each_member() {
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
            let hosted =
                insert_map_member_row(node, path_segments, &leaf, ambiguous_union_abstentions);
            debug_assert!(hosted, "two-lane member seed must host the row");
            return;
        }
        if !node.is_array_like() {
            let existing = std::mem::replace(node, SchemaNode::empty());
            let mut array_variant = new_array_slot();
            insert_schema_at_parts_tracking(
                &mut array_variant,
                path_segments,
                leaf,
                ambiguous_union_abstentions,
            );
            *node = SchemaNode::any_of(vec![existing, array_variant]);
            return;
        }
        let items = ensure_array_items_schema(node);
        if tail.is_empty() {
            merge_into_schema_slot(items, leaf);
        } else {
            insert_schema_at_parts_tracking(items, tail, leaf, ambiguous_union_abstentions);
        }
        return;
    }

    if !tail.is_empty() {
        node.clear_exact_empty_constraint_for_descendant();
    }
    let Some(head) = head.literal() else {
        return;
    };
    if tail.is_empty() {
        let properties = ensure_object_properties(node);
        merge_into_schema_slot(
            properties
                .entry(head.to_owned())
                .or_insert_with(SchemaNode::empty),
            leaf,
        );
        return;
    }

    let key = head.to_owned();
    let next_is_array = tail.first().is_some_and(Segment::is_each_member);
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
    insert_schema_at_parts_tracking(child, tail, leaf, ambiguous_union_abstentions);
}

fn insert_into_nonempty_schema_slot(
    node: &mut SchemaNode,
    path_segments: &[Segment],
    head: &Segment,
    tail: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) -> bool {
    if !matches!(node, SchemaNode::Typed(_)) || node.is_empty_slot() {
        return false;
    }
    // Descend through properties the resolved value already declares, so
    // the descendant merges at the deepest existing node and that node's
    // own openness decides the outcome (a top-level merge of a nested
    // carrier would let the carrier's materialized closure shut an open
    // map one level down).
    if !tail.is_empty()
        && let Some(head) = head.literal()
        && let SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) = node
        && let Some(child) = keywords
            .properties
            .as_mut()
            .and_then(|properties| properties.get_mut(head))
    {
        insert_schema_at_parts_tracking(child, tail, leaf.clone(), ambiguous_union_abstentions);
        // Re-enter through the parent's ingestion boundary so later passes
        // do not mistake a newly materialized legacy host for a tree-owned one.
        let child_value = std::mem::replace(child, SchemaNode::empty()).into_value();
        *child = SchemaNode::from_value(child_value);
        return true;
    }
    if insert_into_canonical_object_conjunct(node, head, tail, leaf, ambiguous_union_abstentions) {
        return true;
    }
    // A union base (an off-state arm plus one open object arm, e.g. a
    // declared-empty serialized map) hosts descendants in its open arm:
    // merging a carrier at the union level would replace the open arm
    // with the carrier's materialized closure.
    if !head.is_each_member()
        && let SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) = node
        && let Some(arms) = keywords.any_of.as_mut()
    {
        let mut open_arms = arms
            .iter_mut()
            .filter(|arm| arm.opens_unknown_object_fields());
        if let (Some(arm), None) = (open_arms.next(), open_arms.next()) {
            insert_schema_at_parts_tracking(
                arm,
                path_segments,
                leaf.clone(),
                ambiguous_union_abstentions,
            );
            return true;
        }
    }
    let mut fragment = SchemaNode::empty();
    insert_schema_at_parts_tracking(
        &mut fragment,
        path_segments,
        leaf.clone(),
        ambiguous_union_abstentions,
    );
    merge_into_schema_slot(node, fragment);
    true
}

fn insert_into_canonical_object_conjunct(
    node: &mut SchemaNode,
    head: &Segment,
    tail: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) -> bool {
    // A canonical object constraint may leave the slot's type inside
    // `allOf`. Descendant default evidence still belongs to that same
    // object lane; union-merging a new carrier would let either side
    // bypass the other.
    insert_into_typed_canonical_object_conjunct(node, head, tail, leaf, ambiguous_union_abstentions)
}

fn insert_into_typed_canonical_object_conjunct(
    node: &mut SchemaNode,
    head: &Segment,
    tail: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) -> bool {
    if insert_into_typed_canonical_mixed_object_lane(
        node,
        head,
        tail,
        leaf,
        ambiguous_union_abstentions,
    ) {
        return true;
    }
    let head_literal = head.literal().unwrap_or("*");
    let value = node.clone().into_value();
    let mut path_segments = Vec::with_capacity(tail.len() + 1);
    path_segments.push(head.clone());
    path_segments.extend_from_slice(tail);
    if multi_arm_object_union_has_equivalent_descendant(&value, &path_segments) == Some(false) {
        *ambiguous_union_abstentions += 1;
        return true;
    }
    if !schema_only_allows_object(&value) {
        return false;
    }
    let SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) = node else {
        return false;
    };
    let properties = keywords.properties.get_or_insert_with(BTreeMap::new);
    let next_is_array = tail.first().is_some_and(Segment::is_each_member);
    let child = properties
        .entry(head_literal.to_owned())
        .or_insert_with(|| {
            if tail.is_empty() || next_is_array {
                SchemaNode::empty()
            } else {
                SchemaNode::unknown_object()
            }
        });
    if tail.is_empty() {
        merge_into_schema_slot(child, leaf.clone());
    } else {
        child.clear_exact_empty_constraint_for_descendant();
        insert_schema_at_parts_tracking(child, tail, leaf.clone(), ambiguous_union_abstentions);
    }
    true
}

fn insert_into_typed_canonical_mixed_object_lane(
    node: &mut SchemaNode,
    head: &Segment,
    tail: &[Segment],
    leaf: &SchemaNode,
    ambiguous_union_abstentions: &mut usize,
) -> bool {
    let SchemaNode::Typed(TypedSchemaNode::Keywords(keywords)) = node else {
        return false;
    };
    let Some(conjuncts) = keywords.all_of.as_mut() else {
        return false;
    };
    let [first, second] = conjuncts.as_slice() else {
        return false;
    };
    let base_index = if is_not_null_constraint(&first.clone().into_value()) {
        1
    } else if is_not_null_constraint(&second.clone().into_value()) {
        0
    } else {
        return false;
    };
    let Some(SchemaNode::Typed(TypedSchemaNode::Keywords(base))) = conjuncts.get(base_index) else {
        return false;
    };
    let Some(SchemaTypeKeyword::Multiple(types)) = &base.schema_type else {
        return false;
    };
    if !types.contains(&JsonSchemaType::Object)
        || !types.iter().any(|schema_type| {
            !matches!(schema_type, JsonSchemaType::Null | JsonSchemaType::Object)
        })
    {
        return false;
    }
    let types = types.clone();
    let mut base_keywords = (**base).clone();
    base_keywords.schema_type = None;
    let mut path_segments = Vec::with_capacity(tail.len() + 1);
    path_segments.push(head.clone());
    path_segments.extend_from_slice(tail);
    let mut arms = Vec::with_capacity(types.len());
    for schema_type in types {
        let mut arm_keywords = base_keywords.clone();
        arm_keywords.schema_type = Some(SchemaTypeKeyword::Single(schema_type));
        let mut arm = SchemaNode::Typed(TypedSchemaNode::Keywords(Box::new(arm_keywords)));
        if schema_type == JsonSchemaType::Object {
            insert_schema_at_parts_tracking(
                &mut arm,
                &path_segments,
                leaf.clone(),
                ambiguous_union_abstentions,
            );
        }
        arms.push(arm);
    }
    let Some(base) = conjuncts.get_mut(base_index) else {
        return false;
    };
    *base = SchemaNode::any_of(arms);
    true
}

fn schema_only_allows_object(schema: &Value) -> bool {
    crate::schema_model::schema_allows_type(schema, "object")
        && ["array", "boolean", "integer", "null", "number", "string"]
            .into_iter()
            .all(|schema_type| crate::schema_model::schema_excludes_type(schema, schema_type))
}

fn multi_arm_object_union_has_equivalent_descendant(
    schema: &Value,
    path_segments: &[Segment],
) -> Option<bool> {
    let object = schema.as_object()?;
    for keyword in ["anyOf", "oneOf"] {
        let Some(arms) = object.get(keyword).and_then(Value::as_array) else {
            continue;
        };
        if arms.len() <= 1 || !arms.iter().all(schema_only_allows_object) {
            continue;
        }
        let Some(first) = arms
            .first()
            .and_then(|arm| schema_descendant_at_path(arm, path_segments))
        else {
            return Some(false);
        };
        return Some(arms.iter().skip(1).all(|arm| {
            schema_descendant_at_path(arm, path_segments)
                .is_some_and(|descendant| descendant == first)
        }));
    }
    let arms = object.get("allOf").and_then(Value::as_array)?;
    let mut verdicts = arms
        .iter()
        .filter_map(|arm| multi_arm_object_union_has_equivalent_descendant(arm, path_segments));
    let first = verdicts.next()?;
    Some(first && verdicts.all(|verdict| verdict == first))
}

fn schema_descendant_at_path<'a>(
    mut schema: &'a Value,
    path_segments: &[Segment],
) -> Option<&'a Value> {
    for segment in path_segments {
        schema = schema.get("properties")?.get(segment.literal()?)?;
    }
    Some(schema)
}
