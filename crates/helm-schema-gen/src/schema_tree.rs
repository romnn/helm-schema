use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use serde_yaml::Value as YamlValue;

use crate::merge::merge_two_schemas;
use crate::schema_model::is_empty_schema;
use crate::schema_node::SchemaNode;
use crate::values_yaml::{
    child_value_path, schema_node_from_yaml_value_with_skips, yaml_value_at_segments,
};

#[derive(Clone)]
pub(crate) struct SchemaDocument {
    root: SchemaNode,
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
        }
    }

    pub(crate) fn visit_embedded_values(&self, visit: &mut impl FnMut(&Value)) {
        self.root.visit_foreign_values(visit);
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
        member_schema: &Value,
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

        let root = std::mem::replace(&mut self.root, SchemaNode::empty());
        let mut root = root.into_value();
        visit_schema_values_at_path_mut(&mut root, path_segments, &mut |schema| {
            materialize_declared_properties(schema, &keys, member_schema);
        });
        self.root = SchemaNode::foreign(root);
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
        if matches!(node, SchemaNode::Foreign(Value::Bool(false)))
            || constraint_is_implied_by_node(node, constraint)
        {
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
        SchemaNode::Foreign(Value::Object(object)) => {
            let Some(child_value) = object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|properties| properties.get_mut(head))
            else {
                return CanonicalConstraintOutcome::NotApplicable;
            };
            let mut child = SchemaNode::foreign(std::mem::take(child_value));
            let outcome = canonicalize_constraint_at_parts(&mut child, tail, constraint);
            *child_value = child.into_value();
            outcome
        }
        SchemaNode::Empty | SchemaNode::Array { .. } | SchemaNode::Foreign(_) => {
            CanonicalConstraintOutcome::NotApplicable
        }
    }
}

fn canonicalize_object_constraint(node: &mut SchemaNode) -> Option<CanonicalConstraintOutcome> {
    match node {
        SchemaNode::Empty | SchemaNode::Foreign(Value::Bool(true)) => {
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
        SchemaNode::Foreign(value) if crate::schema_model::schema_type(value) == Some("object") => {
            Some(CanonicalConstraintOutcome::Applied(
                CanonicalConstraintApplication::Redundant,
            ))
        }
        SchemaNode::Foreign(Value::Bool(false)) => Some(CanonicalConstraintOutcome::Applied(
            CanonicalConstraintApplication::Redundant,
        )),
        SchemaNode::Array { .. }
        | SchemaNode::Foreign(
            Value::Null | Value::Number(_) | Value::String(_) | Value::Array(_) | Value::Object(_),
        ) => None,
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
        SchemaNode::Foreign(Value::Object(object))
            if object.get("type").and_then(Value::as_str) == Some("object") =>
        {
            let all_slots_exist = object
                .get("properties")
                .and_then(Value::as_object)
                .is_some_and(|properties| {
                    required.iter().all(|name| properties.contains_key(*name))
                });
            if !all_slots_exist {
                return Some(CanonicalConstraintOutcome::NotApplicable);
            }
            let existing = object
                .entry("required".to_string())
                .or_insert_with(|| Value::Array(Vec::new()));
            let Some(existing) = existing.as_array_mut() else {
                return Some(CanonicalConstraintOutcome::NotApplicable);
            };
            let before = existing.len();
            for name in required {
                let name = Value::String(name.to_string());
                if !existing.contains(&name) {
                    existing.push(name);
                }
            }
            existing.sort_by(|left, right| left.as_str().cmp(&right.as_str()));
            Some(if existing.len() == before {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant)
            } else {
                CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
            })
        }
        SchemaNode::Foreign(Value::Bool(false)) => Some(CanonicalConstraintOutcome::Applied(
            CanonicalConstraintApplication::Redundant,
        )),
        SchemaNode::Empty
        | SchemaNode::Array { .. }
        | SchemaNode::Foreign(
            Value::Null
            | Value::Bool(true)
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_)
            | Value::Object(_),
        ) => Some(CanonicalConstraintOutcome::NotApplicable),
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
            // A declared mapping default resolves to a foreign base pinning
            // `type: object`; keep its scalar rejection and admit only the
            // null spelling helm deletes before any consumer runs.
            SchemaNode::Foreign(Value::Object(object))
                if object.get("type").and_then(Value::as_str) == Some("object") =>
            {
                object.insert(
                    "type".to_string(),
                    Value::Array(vec![
                        Value::String("object".to_string()),
                        Value::String("null".to_string()),
                    ]),
                );
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
        SchemaNode::Foreign(Value::Object(object)) => {
            if let Some(child_value) = object
                .get_mut("properties")
                .and_then(Value::as_object_mut)
                .and_then(|properties| properties.get_mut(head))
            {
                let mut child = SchemaNode::foreign(std::mem::take(child_value));
                relax_host_object_type(&mut child, tail);
                *child_value = child.into_value();
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
) -> Value {
    let mut root = SchemaNode::foreign(root_schema);
    insert_schema_at_parts(&mut root, path_segments, SchemaNode::foreign(schema));
    root.into_value()
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
        SchemaNode::Object { .. } | SchemaNode::Foreign(Value::Object(_))
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

fn materialize_declared_properties(schema: &mut Value, keys: &[&str], member_schema: &Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for keyword in ["anyOf", "allOf", "oneOf"] {
        if let Some(arms) = object.get_mut(keyword).and_then(Value::as_array_mut) {
            for arm in arms {
                materialize_declared_properties(arm, keys, member_schema);
            }
        }
    }

    let object_lane = object.get("type").and_then(Value::as_str) == Some("object")
        || object.contains_key("properties")
        || object.contains_key("additionalProperties");
    if !object_lane {
        return;
    }
    let properties = object
        .entry("properties")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(properties) = properties.as_object_mut() else {
        return;
    };
    for key in keys {
        properties.insert((*key).to_string(), member_schema.clone());
    }
}

fn new_array_slot() -> SchemaNode {
    SchemaNode::array().items(SchemaNode::foreign(Value::Null))
}

fn ensure_array_items_schema(node: &mut SchemaNode) -> &mut SchemaNode {
    if !matches!(node, SchemaNode::Array { .. }) {
        *node = new_array_slot();
    }
    match node {
        SchemaNode::Array { items, .. } => items
            .get_or_insert_with(|| Box::new(SchemaNode::foreign(Value::Null)))
            .as_mut(),
        _ => ensure_array_items_schema(node),
    }
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
    match node {
        SchemaNode::Object { properties, .. } => properties,
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
    let Some((head, tail)) = path_segments.split_first() else {
        *node = leaf;
        return;
    };

    let head = head.as_str();
    let replaced = match node {
        SchemaNode::Object { properties, .. } => {
            if let Some(child) = properties.get_mut(head) {
                replace_schema_at_parts(child, tail, leaf.clone());
                true
            } else {
                false
            }
        }
        SchemaNode::Array { items, .. } if head == "*" => {
            if let Some(child) = items.as_deref_mut() {
                replace_schema_at_parts(child, tail, leaf.clone());
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
    let Some((_, tail)) = path_segments.split_first() else {
        return false;
    };
    let insert = |member: &mut SchemaNode| {
        if tail.is_empty() {
            merge_into_schema_slot(member, leaf.clone());
        } else {
            insert_schema_at_parts(member, tail, leaf.clone());
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
    let Some((head, tail)) = path_segments.split_first() else {
        return;
    };

    // `range` iterates maps as well as lists, so a member row's `*` segment
    // means "each member value" of whatever container the node already is:
    // an object-shaped node hosts it under `additionalProperties` instead
    // of growing an array alternative.
    if head == "*" && insert_map_member_row(node, path_segments, &leaf) {
        return;
    }

    if matches!(node, SchemaNode::Foreign(_)) && !node.is_empty_slot() {
        // Descend through properties the resolved value already declares, so
        // the descendant merges at the deepest existing node and that node's
        // own openness decides the outcome (a top-level merge of a nested
        // carrier would let the carrier's materialized closure shut an open
        // map one level down).
        if !tail.is_empty()
            && head != "*"
            && let SchemaNode::Foreign(value) = node
            && let Some(child_value) = value
                .get_mut("properties")
                .and_then(|properties| properties.get_mut(head))
        {
            let mut child_node = SchemaNode::foreign(std::mem::take(child_value));
            insert_schema_at_parts(&mut child_node, tail, leaf);
            *child_value = child_node.into_value();
            return;
        }
        if insert_into_canonical_object_conjunct(node, head, tail, &leaf) {
            return;
        }
        // A union base (an off-state arm plus one open object arm, e.g. a
        // declared-empty serialized map) hosts descendants in its open arm:
        // merging a carrier at the union level would replace the open arm
        // with the carrier's materialized closure.
        if head != "*"
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

    if head == "*" {
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
        if tail.is_empty() {
            merge_into_schema_slot(items, leaf);
        } else {
            insert_schema_at_parts(items, tail, leaf);
        }
        return;
    }

    if !tail.is_empty() {
        node.clear_exact_empty_constraint_for_descendant();
    }
    if tail.is_empty() {
        let properties = ensure_object_properties(node);
        merge_into_schema_slot(
            properties
                .entry(head.clone())
                .or_insert_with(SchemaNode::empty),
            leaf,
        );
        return;
    }

    let key = head.clone();
    let next_is_array = tail.first().is_some_and(|segment| segment == "*");
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
    insert_schema_at_parts(child, tail, leaf);
}

fn insert_into_canonical_object_conjunct(
    node: &mut SchemaNode,
    head: &str,
    tail: &[String],
    leaf: &SchemaNode,
) -> bool {
    // A canonical object constraint may leave the slot's type inside
    // `allOf`. Descendant default evidence still belongs to that same
    // object lane; union-merging a new carrier would let either side
    // bypass the other.
    let SchemaNode::Foreign(value) = node else {
        return false;
    };
    if insert_into_canonical_mixed_object_lane(value, head, tail, leaf) {
        return true;
    }
    let mut path_segments = Vec::with_capacity(tail.len() + 1);
    path_segments.push(head.to_string());
    path_segments.extend_from_slice(tail);
    if multi_arm_object_union_has_equivalent_descendant(value, &path_segments) == Some(false) {
        // Default backfill cannot identify which alternative supplied a
        // branch-specific member constraint. Retaining the structural
        // alternatives is safer than conjoining the default across them.
        return true;
    }
    if !schema_only_allows_object(value) {
        return false;
    }
    let Some(object) = value.as_object_mut() else {
        return false;
    };
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(properties) = properties.as_object_mut() else {
        return false;
    };
    let next_is_array = tail.first().is_some_and(|segment| segment == "*");
    let child = properties.entry(head.to_string()).or_insert_with(|| {
        if tail.is_empty() || next_is_array {
            SchemaNode::empty().into_value()
        } else {
            SchemaNode::unknown_object().into_value()
        }
    });
    let mut child = SchemaNode::foreign(std::mem::take(child));
    if tail.is_empty() {
        merge_into_schema_slot(&mut child, leaf.clone());
    } else {
        child.clear_exact_empty_constraint_for_descendant();
        insert_schema_at_parts(&mut child, tail, leaf.clone());
    }
    properties.insert(head.to_string(), child.into_value());
    true
}

fn schema_only_allows_object(schema: &Value) -> bool {
    crate::schema_model::schema_allows_type(schema, "object")
        && ["array", "boolean", "integer", "null", "number", "string"]
            .into_iter()
            .all(|schema_type| crate::schema_model::schema_excludes_type(schema, schema_type))
}

fn insert_into_canonical_mixed_object_lane(
    schema: &mut Value,
    head: &str,
    tail: &[String],
    leaf: &SchemaNode,
) -> bool {
    let Some(conjuncts) = schema.get_mut("allOf").and_then(Value::as_array_mut) else {
        return false;
    };
    let [first, second] = conjuncts.as_slice() else {
        return false;
    };
    let base_index = if is_not_null_constraint(first) {
        1
    } else if is_not_null_constraint(second) {
        0
    } else {
        return false;
    };
    let Some(base) = conjuncts.get(base_index).and_then(Value::as_object) else {
        return false;
    };
    let Some(types) = base.get("type").and_then(Value::as_array).cloned() else {
        return false;
    };
    if !types.iter().any(|schema_type| schema_type == "object")
        || !types.iter().any(|schema_type| {
            schema_type
                .as_str()
                .is_some_and(|schema_type| !matches!(schema_type, "null" | "object"))
        })
    {
        return false;
    }

    let mut base_keywords = base.clone();
    base_keywords.remove("type");
    let mut path_segments = Vec::with_capacity(tail.len() + 1);
    path_segments.push(head.to_string());
    path_segments.extend_from_slice(tail);
    let mut arms = Vec::with_capacity(types.len());
    for schema_type in types {
        let mut arm = base_keywords.clone();
        arm.insert("type".to_string(), schema_type.clone());
        let mut arm = SchemaNode::foreign(Value::Object(arm));
        if schema_type == "object" {
            insert_schema_at_parts(&mut arm, &path_segments, leaf.clone());
        }
        arms.push(arm);
    }
    let Some(base) = conjuncts.get_mut(base_index) else {
        return false;
    };
    *base = SchemaNode::any_of(arms).into_value();
    true
}

fn multi_arm_object_union_has_equivalent_descendant(
    schema: &Value,
    path_segments: &[String],
) -> Option<bool> {
    let object = schema.as_object()?;
    for keyword in ["anyOf", "oneOf"] {
        let Some(arms) = object.get(keyword).and_then(Value::as_array) else {
            continue;
        };
        if arms.len() <= 1 || !arms.iter().all(schema_only_allows_object) {
            continue;
        }
        let first = arms
            .first()
            .and_then(|arm| schema_descendant_at_path(arm, path_segments));
        return Some(
            arms.iter()
                .skip(1)
                .all(|arm| schema_descendant_at_path(arm, path_segments) == first),
        );
    }
    object
        .get("allOf")
        .and_then(Value::as_array)
        .and_then(|arms| {
            arms.iter().find_map(|arm| {
                multi_arm_object_union_has_equivalent_descendant(arm, path_segments)
            })
        })
}

fn schema_descendant_at_path<'a>(
    mut schema: &'a Value,
    path_segments: &[String],
) -> Option<&'a Value> {
    for segment in path_segments {
        if segment == "*" {
            return None;
        }
        schema = schema.get("properties")?.get(segment)?;
    }
    Some(schema)
}
