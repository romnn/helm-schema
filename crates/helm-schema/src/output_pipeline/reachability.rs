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
        Self {
            original: root_definitions(schema),
        }
    }

    /// Whether the generator still owns root `$defs` entry `name` unchanged.
    pub(crate) fn owns_root_definition(&self, name: &str) -> bool {
        self.original.contains_key(&DefinitionId {
            keyword: "$defs".to_string(),
            name: name.to_string(),
        })
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
    for id in all_definitions.keys() {
        if !owned.original.contains_key(id) {
            reachable.insert(id.clone());
        }
    }
    let preserve_all = collect_references_outside_root_definitions(schema, &mut reachable);
    if preserve_all {
        reachable.extend(all_definitions.keys().cloned());
    }
    let mut pending = reachable.iter().cloned().collect::<VecDeque<_>>();
    while let Some(id) = pending.pop_front() {
        let Some(definition) = all_definitions.get(&id) else {
            continue;
        };
        let mut referenced = BTreeSet::new();
        if collect_references(definition, &mut referenced) {
            for id in all_definitions.keys() {
                if reachable.insert(id.clone()) {
                    pending.push_back(id.clone());
                }
            }
        }
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
) -> bool {
    let Some(root) = schema.as_object() else {
        return collect_references(schema, referenced);
    };
    let mut preserve_all = false;
    if let Some(reference) = root.get("$ref").and_then(Value::as_str) {
        preserve_all |= collect_reference(reference, referenced);
    }
    for (key, child) in root {
        if key != "$defs" && key != "definitions" {
            preserve_all |= collect_references(child, referenced);
        }
    }
    preserve_all
}

fn collect_references(schema: &Value, referenced: &mut BTreeSet<DefinitionId>) -> bool {
    match schema {
        Value::Object(object) => {
            let mut preserve_all = false;
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                preserve_all |= collect_reference(reference, referenced);
            }
            for child in object.values() {
                preserve_all |= collect_references(child, referenced);
            }
            preserve_all
        }
        Value::Array(items) => {
            let mut preserve_all = false;
            for item in items {
                preserve_all |= collect_references(item, referenced);
            }
            preserve_all
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn collect_reference(reference: &str, referenced: &mut BTreeSet<DefinitionId>) -> bool {
    match definition_id_from_reference(reference) {
        Ok(Some(id)) => {
            referenced.insert(id);
            false
        }
        Ok(None) => false,
        Err(()) => true,
    }
}

fn definition_id_from_reference(reference: &str) -> Result<Option<DefinitionId>, ()> {
    let Some(fragment) = helm_schema_json_schema_walk::local_reference_fragment(reference) else {
        // A same-document reference that is no valid URI reference may
        // address any definition.
        return if reference.starts_with('#') {
            Err(())
        } else {
            Ok(None)
        };
    };
    if !fragment.starts_with('/') {
        return Ok(None);
    }
    let decoded_segments = fragment
        .split('/')
        .map(helm_schema_json_schema_walk::unescape_json_pointer_segment)
        .collect::<Vec<_>>();
    let [root, keyword, name, ..] = decoded_segments.as_slice() else {
        return Ok(None);
    };
    if root.is_empty() && matches!(keyword.as_str(), "$defs" | "definitions") {
        return Ok(Some(DefinitionId {
            keyword: keyword.clone(),
            name: name.clone(),
        }));
    }
    Ok(None)
}

#[cfg(test)]
#[path = "tests/reachability.rs"]
mod tests;
