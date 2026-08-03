use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DefinitionId {
    keyword: String,
    name: String,
}

#[derive(Debug, Default)]
pub(crate) struct OwnedDefinitions {
    original: BTreeMap<DefinitionId, Value>,
}

impl OwnedDefinitions {
    pub(crate) fn capture(schema: &Value) -> Self {
        let mut original = BTreeMap::new();
        for keyword in ["$defs", "definitions"] {
            let Some(definitions) = schema.get(keyword).and_then(Value::as_object) else {
                continue;
            };
            for (name, definition) in definitions {
                original.insert(
                    DefinitionId {
                        keyword: keyword.to_string(),
                        name: name.clone(),
                    },
                    definition.clone(),
                );
            }
        }
        Self { original }
    }

    pub(crate) fn retain_unchanged(mut self, schema: &Value) -> Self {
        self.original.retain(|id, definition| {
            schema
                .get(&id.keyword)
                .and_then(Value::as_object)
                .and_then(|definitions| definitions.get(&id.name))
                == Some(definition)
        });
        self
    }
}

/// Removes only definitions still owned by the generator and unreachable
/// from the final document. Caller-added or caller-modified definitions are
/// never candidates, even when they are dead.
pub(crate) fn prune_unreachable_owned_definitions(
    schema: &mut Value,
    owned: &OwnedDefinitions,
) -> usize {
    if owned.original.is_empty() {
        return 0;
    }
    let all_definitions = root_definitions(schema);
    let mut reachable = BTreeSet::new();
    collect_references_outside_root_definitions(schema, &mut reachable);
    let mut pending = reachable.iter().cloned().collect::<VecDeque<_>>();
    while let Some(id) = pending.pop_front() {
        let Some(definition) = all_definitions.get(&id) else {
            continue;
        };
        let mut referenced = BTreeSet::new();
        collect_references(definition, &mut referenced);
        for referenced_id in referenced {
            if reachable.insert(referenced_id.clone()) {
                pending.push_back(referenced_id);
            }
        }
    }

    let removable = owned
        .original
        .keys()
        .filter(|id| !reachable.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    for id in &removable {
        if let Some(definitions) = schema.get_mut(&id.keyword).and_then(Value::as_object_mut) {
            definitions.remove(&id.name);
        }
    }
    for keyword in ["$defs", "definitions"] {
        let remove_empty = schema
            .get(keyword)
            .and_then(Value::as_object)
            .is_some_and(serde_json::Map::is_empty)
            && removable.iter().any(|id| id.keyword == keyword);
        if remove_empty && let Some(object) = schema.as_object_mut() {
            object.remove(keyword);
        }
    }
    removable.len()
}

fn root_definitions(schema: &Value) -> BTreeMap<DefinitionId, Value> {
    let mut out = BTreeMap::new();
    for keyword in ["$defs", "definitions"] {
        let Some(definitions) = schema.get(keyword).and_then(Value::as_object) else {
            continue;
        };
        for (name, definition) in definitions {
            out.insert(
                DefinitionId {
                    keyword: keyword.to_string(),
                    name: name.clone(),
                },
                definition.clone(),
            );
        }
    }
    out
}

fn collect_references_outside_root_definitions(
    schema: &Value,
    referenced: &mut BTreeSet<DefinitionId>,
) {
    let Some(root) = schema.as_object() else {
        collect_references(schema, referenced);
        return;
    };
    if let Some(reference) = root.get("$ref").and_then(Value::as_str)
        && let Some(id) = definition_id_from_reference(reference)
    {
        referenced.insert(id);
    }
    for (key, child) in root {
        if key != "$defs" && key != "definitions" {
            collect_references(child, referenced);
        }
    }
}

fn collect_references(schema: &Value, referenced: &mut BTreeSet<DefinitionId>) {
    match schema {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                && let Some(id) = definition_id_from_reference(reference)
            {
                referenced.insert(id);
            }
            for child in object.values() {
                collect_references(child, referenced);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_references(item, referenced);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn definition_id_from_reference(reference: &str) -> Option<DefinitionId> {
    for keyword in ["$defs", "definitions"] {
        let prefix = format!("#/{keyword}/");
        if let Some(encoded_name) = reference
            .strip_prefix(&prefix)
            .and_then(|suffix| suffix.split('/').next())
        {
            return Some(DefinitionId {
                keyword: keyword.to_string(),
                name: decode_json_pointer_segment(encoded_name),
            });
        }
    }
    None
}

fn decode_json_pointer_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

#[cfg(test)]
#[path = "tests/reachability.rs"]
mod tests;
