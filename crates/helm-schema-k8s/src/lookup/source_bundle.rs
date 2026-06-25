use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;

use json_schema_walk::{SchemaTraversalContext, ref_points_inside, try_map_schema_context};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceBundleLocation {
    pub(crate) document: String,
    pub(crate) pointer: String,
}

impl SourceBundleLocation {
    pub(crate) fn new(document: impl Into<String>, pointer: impl Into<String>) -> Self {
        Self {
            document: document.into(),
            pointer: pointer.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceBundleNode {
    pub(crate) location: SourceBundleLocation,
    pub(crate) schema: Value,
}

impl SourceBundleNode {
    pub(crate) fn new(
        document: impl Into<String>,
        pointer: impl Into<String>,
        schema: Value,
    ) -> Self {
        Self {
            location: SourceBundleLocation::new(document, pointer),
            schema,
        }
    }
}

pub(crate) fn bundle_source_definition(
    document: &str,
    pointer: &str,
    source_schema: &Value,
    mut resolve_external_ref: impl FnMut(&SourceBundleLocation, &str) -> Option<SourceBundleNode>,
) -> Value {
    if json_schema_walk::schema_refs_point_inside(source_schema, source_schema) {
        return source_schema.clone();
    }

    bundle_source_schema(
        SourceBundleNode::new(document, pointer, source_schema.clone()),
        |current_location, reference| {
            if let Some(pointer) = reference.strip_prefix('#')
                && source_schema.pointer(pointer).is_some()
            {
                return None;
            }
            resolve_external_ref(current_location, reference)
        },
    )
}

pub(crate) fn bundle_source_schema(
    root: SourceBundleNode,
    resolve_external_ref: impl FnMut(&SourceBundleLocation, &str) -> Option<SourceBundleNode>,
) -> Value {
    let mut bundler = SourceSchemaBundler::new(root.schema.clone(), resolve_external_ref);
    bundler.bundle_root(root)
}

struct SourceSchemaBundler<R> {
    root_schema: Value,
    resolve_external_ref: R,
    definition_names_by_location: BTreeMap<SourceBundleLocation, String>,
    definitions_by_name: BTreeMap<String, Value>,
    used_definition_names: BTreeSet<String>,
    root_location: Option<SourceBundleLocation>,
}

impl<R> SourceSchemaBundler<R>
where
    R: FnMut(&SourceBundleLocation, &str) -> Option<SourceBundleNode>,
{
    fn new(root_schema: Value, resolve_external_ref: R) -> Self {
        Self {
            root_schema,
            resolve_external_ref,
            definition_names_by_location: BTreeMap::new(),
            definitions_by_name: BTreeMap::new(),
            used_definition_names: BTreeSet::new(),
            root_location: None,
        }
    }

    fn bundle_root(&mut self, root: SourceBundleNode) -> Value {
        self.seed_existing_definition_names(&root.schema);
        self.root_location = Some(root.location.clone());
        let mut schema = self.bundle_root_node(root, 0);
        if !self.definitions_by_name.is_empty() {
            let definitions = schema.as_object_mut().and_then(|object| {
                object
                    .entry("$defs".to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
            });
            if let Some(definitions) = definitions {
                for (name, definition) in std::mem::take(&mut self.definitions_by_name) {
                    definitions.insert(name, definition);
                }
            }
        }
        schema
    }

    fn seed_existing_definition_names(&mut self, schema: &Value) {
        for key in ["$defs", "definitions"] {
            let Some(definitions) = schema.get(key).and_then(Value::as_object) else {
                continue;
            };
            self.used_definition_names
                .extend(definitions.keys().cloned());
        }
    }

    fn bundle_root_node(&mut self, node: SourceBundleNode, depth: usize) -> Value {
        if depth > 64 {
            return node.schema;
        }

        if let Some(reference) = node.schema.get("$ref").and_then(Value::as_str) {
            if ref_points_inside(&self.root_schema, reference) {
                return self.bundle_schema_value(
                    &node.schema,
                    SchemaTraversalContext::Schema,
                    &node.location,
                    depth,
                );
            }
            if let Some(target) = (self.resolve_external_ref)(&node.location, reference) {
                if self.root_location.as_ref() == Some(&target.location) {
                    return Value::Object(
                        [("$ref".to_string(), Value::String("#".to_string()))]
                            .into_iter()
                            .collect(),
                    );
                }
                return self.bundle_root_node(target, depth + 1);
            }
            return strip_ref(&node.schema);
        }

        self.bundle_schema_value(
            &node.schema,
            SchemaTraversalContext::Schema,
            &node.location,
            depth,
        )
    }

    fn bundle_schema_value(
        &mut self,
        value: &Value,
        context: SchemaTraversalContext,
        current_location: &SourceBundleLocation,
        depth: usize,
    ) -> Value {
        let mapped: Result<Value, Infallible> =
            try_map_schema_context(value, context, |value, context, relative_depth| {
                if matches!(context, SchemaTraversalContext::Data) {
                    return Ok(None);
                }
                let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
                    return Ok(None);
                };
                Ok(Some(self.bundle_schema_ref(
                    value,
                    reference,
                    current_location,
                    depth + relative_depth,
                )))
            });
        match mapped {
            Ok(value) => value,
            Err(err) => match err {},
        }
    }

    fn bundle_schema_ref(
        &mut self,
        schema: &Value,
        reference: &str,
        current_location: &SourceBundleLocation,
        depth: usize,
    ) -> Value {
        if ref_points_inside(&self.root_schema, reference) {
            return schema.clone();
        }

        let Some(target) = (self.resolve_external_ref)(current_location, reference) else {
            return strip_ref(schema);
        };

        if self.root_location.as_ref() == Some(&target.location) {
            return Value::Object(
                [("$ref".to_string(), Value::String("#".to_string()))]
                    .into_iter()
                    .collect(),
            );
        }

        let definition_name = self.definition_name_for_target(target, depth + 1);
        Value::Object(
            [(
                "$ref".to_string(),
                Value::String(format!("#/$defs/{definition_name}")),
            )]
            .into_iter()
            .collect(),
        )
    }

    fn definition_name_for_target(&mut self, target: SourceBundleNode, depth: usize) -> String {
        if let Some(name) = self.definition_names_by_location.get(&target.location) {
            return name.clone();
        }

        let name = self.next_definition_name(&target.location);
        self.definition_names_by_location
            .insert(target.location.clone(), name.clone());

        let definition_schema = self.bundle_root_node(target, depth);
        self.definitions_by_name
            .insert(name.clone(), definition_schema);
        name
    }

    fn next_definition_name(&mut self, location: &SourceBundleLocation) -> String {
        let base_name = definition_base_name(&location.pointer);
        if self.used_definition_names.insert(base_name.clone()) {
            return base_name;
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base_name}_{suffix}");
            suffix += 1;
            if self.used_definition_names.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

fn strip_ref(schema: &Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = object.clone();
    out.remove("$ref");
    Value::Object(out)
}

fn definition_base_name(pointer: &str) -> String {
    let base = pointer
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("definition");
    let sanitized: String = base
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "definition".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
#[path = "tests/source_bundle.rs"]
mod tests;
