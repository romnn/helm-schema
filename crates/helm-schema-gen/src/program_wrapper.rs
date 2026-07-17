//! Wrapper alternatives for chart-authored values-program conventions.
//!
//! A chart whose engine rewrites its values tree (nats' `$tplYaml`) accepts
//! a singleton `{KEY: PROGRAM}` map at ANY node: the engine executes the
//! program with `tpl`, reparses the output as YAML, and substitutes the
//! typed result before consumers read the tree. Every value-position schema
//! node therefore gains a wrapper alternative. A static program (no
//! template action) resolves to its own YAML decoding, so integer-typed
//! nodes constrain static programs to integer literals; dynamic programs
//! stay an explicit open alternative rather than being rejected as the
//! node's raw object kind.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ValuesProgramWrapper;
use serde_json::{Map, Value, json};

pub(crate) fn apply_program_wrapper_alternatives(
    root: &mut Value,
    wrappers: &BTreeSet<ValuesProgramWrapper>,
) {
    if wrappers.is_empty() {
        return;
    }
    let mut keys_by_scope: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for wrapper in wrappers {
        keys_by_scope
            .entry(wrapper.scope_path.as_str())
            .or_default()
            .insert(wrapper.key.as_str());
    }
    for (scope, keys) in keys_by_scope {
        if scope.is_empty() {
            rewrite_document(root, &keys);
        } else if let Some(node) = properties_node_mut(root, scope) {
            rewrite_value_edges(node, &keys);
        }
    }
}

/// The schema node declared for `scope` under the document's base
/// properties tree, when every segment resolves.
fn properties_node_mut<'a>(root: &'a mut Value, scope: &str) -> Option<&'a mut Value> {
    let mut node = root;
    for segment in crate::split_value_path(scope) {
        node = node.get_mut("properties")?.get_mut(&segment)?;
    }
    Some(node)
}

/// Root-document edges: the base properties tree, conditional arms, and
/// shared definitions. Definitions encoding TESTS rather than accepted
/// values (helm truthiness, quoted-content grammars) stay untouched — a
/// wrapper alternative inside a test would change what the test means.
fn rewrite_document(root: &mut Value, keys: &BTreeSet<&str>) {
    let Some(object) = root.as_object_mut() else {
        return;
    };
    for (keyword, child) in object.iter_mut() {
        match keyword.as_str() {
            "properties" | "patternProperties" => wrap_member_values(child, keys),
            "additionalProperties" if child.is_object() => wrap_node(child, keys),
            "allOf" | "anyOf" | "oneOf" => {
                if let Some(arms) = child.as_array_mut() {
                    for arm in arms {
                        rewrite_value_edges(arm, keys);
                    }
                }
            }
            "$defs" => {
                if let Some(definitions) = child.as_object_mut() {
                    for (name, definition) in definitions.iter_mut() {
                        if name.starts_with("helm-") {
                            continue;
                        }
                        wrap_node(definition, keys);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Descend one value node's value-position edges, wrapping each child.
/// Condition-position keywords (`if`, `not`, `propertyNames`) encode tests
/// and are left alone; union alternatives recurse without a wholesale wrap
/// because the node owning the union is wrapped at ITS edge.
fn rewrite_value_edges(node: &mut Value, keys: &BTreeSet<&str>) {
    let Some(object) = node.as_object_mut() else {
        return;
    };
    for (keyword, child) in object.iter_mut() {
        match keyword.as_str() {
            "properties" | "patternProperties" => wrap_member_values(child, keys),
            "additionalProperties" | "items" if child.is_object() => wrap_node(child, keys),
            "anyOf" | "allOf" | "oneOf" => {
                if let Some(alternatives) = child.as_array_mut() {
                    for alternative in alternatives {
                        rewrite_value_edges(alternative, keys);
                    }
                }
            }
            "then" | "else" => rewrite_value_edges(child, keys),
            "$defs" | "definitions" => {
                if let Some(definitions) = child.as_object_mut() {
                    for definition in definitions.values_mut() {
                        wrap_node(definition, keys);
                    }
                }
            }
            _ => {}
        }
    }
}

fn wrap_member_values(members: &mut Value, keys: &BTreeSet<&str>) {
    if let Some(members) = members.as_object_mut() {
        for member in members.values_mut() {
            wrap_node(member, keys);
        }
    }
}

/// Recurse into a value node's edges, then union the node with the wrapper
/// alternative. Nodes that already accept everything gain nothing.
fn wrap_node(node: &mut Value, keys: &BTreeSet<&str>) {
    rewrite_value_edges(node, keys);
    if schema_accepts_everything(node) {
        return;
    }
    let alternative = wrapper_schema(node, keys);
    let original = std::mem::take(node);
    *node = json!({ "anyOf": [original, alternative] });
}

fn schema_accepts_everything(node: &Value) -> bool {
    match node {
        Value::Bool(accepts) => *accepts,
        Value::Object(object) => object
            .keys()
            .all(|keyword| matches!(keyword.as_str(), "description" | "default")),
        _ => false,
    }
}

/// The singleton wrapper-map alternative for one node: exactly one sentinel
/// member whose value is the program string. A static program's YAML
/// decoding must inhabit the node, which is decidable lexically for
/// integer-typed nodes; a program carrying a template action stays open.
fn wrapper_schema(node: &Value, keys: &BTreeSet<&str>) -> Value {
    let integer_typed = node.get("type").and_then(Value::as_str) == Some("integer");
    let program: Value = if integer_typed {
        json!({
            "type": "string",
            "pattern": "(^[+-]?(0x[0-9A-Fa-f]+|0o[0-7]+|[0-9]+)$)|\\{\\{",
        })
    } else {
        json!({ "type": "string" })
    };
    let mut properties = Map::new();
    for key in keys {
        properties.insert((*key).to_string(), program.clone());
    }
    json!({
        "type": "object",
        "minProperties": 1,
        "maxProperties": 1,
        "additionalProperties": false,
        "properties": properties,
    })
}
