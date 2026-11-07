//! Strict resource schemas derived from a release's `OpenAPI` definitions.
//!
//! The standalone-strict bundle omits served kinds whose definitions recurse, the API extension
//! server's `CustomResourceDefinition` above all. The same release's `_definitions.json` declares
//! them. Deriving the bundle's strict, nullable shape from those definitions validates such a
//! resource as a real installation's strict field validation would, instead of skipping it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

const DEFINITIONS_PREFIX: &str = "#/definitions/";

/// The single definition declaring `apiVersion`/`kind`, as a strict standalone schema.
///
/// `Ok(None)` means the release declares no such kind; an ambiguous or malformed declaration is
/// an error.
pub(crate) fn resource_schema(
    definitions: &Value,
    api_version: &str,
    kind: &str,
) -> Result<Option<Value>, String> {
    let definitions = definitions
        .get("definitions")
        .and_then(Value::as_object)
        .ok_or("release definitions lack a definitions object")?;
    let (group, version) = api_version.split_once('/').unwrap_or(("", api_version));
    let mut declaring = Vec::new();
    for (name, definition) in definitions {
        let Some(identities) = definition.get("x-kubernetes-group-version-kind") else {
            continue;
        };
        let identities = identities
            .as_array()
            .ok_or_else(|| format!("{name} has a malformed group-version-kind"))?;
        // A definition shared by many kinds (options, watch events) is no resource root.
        if let [identity] = identities.as_slice()
            && identity.get("group").and_then(Value::as_str) == Some(group)
            && identity.get("version").and_then(Value::as_str) == Some(version)
            && identity.get("kind").and_then(Value::as_str) == Some(kind)
        {
            declaring.push(name.as_str());
        }
    }
    match declaring.as_slice() {
        [] => Ok(None),
        [name] => {
            let root = json!({"$ref": format!("{DEFINITIONS_PREFIX}{name}")});
            derive(definitions, &root).map(Some)
        }
        names => Err(format!("{api_version}/{kind} is declared by {names:?}")),
    }
}

/// `root` with every reference to `definitions` expanded in the standalone-strict shape.
///
/// The shape is the bundle's: objects with properties reject undeclared ones, `int-or-string`
/// accepts either, a single-kind definition fixes its `apiVersion` and `kind`, and every type
/// admits null except the root's and a property its enclosing nested object requires.
/// A definition that refers back to itself stays a local reference, one per nullability.
pub(crate) fn derive(definitions: &Map<String, Value>, root: &Value) -> Result<Value, String> {
    let mut derivation = Derivation {
        definitions,
        recursive: BTreeMap::new(),
        pending: Vec::new(),
    };
    let mut schema = derivation.schema(root, false, true, &mut Vec::new())?;
    while let Some((name, nullable)) = derivation.pending.pop() {
        let definition = derivation.definition(&name)?;
        let derived = derivation.schema(definition, nullable, false, &mut vec![name.clone()])?;
        derivation
            .recursive
            .insert(local_name(&name, nullable), Some(derived));
    }
    if !derivation.recursive.is_empty() {
        let mut local = Map::new();
        for (name, derived) in derivation.recursive {
            local.insert(
                name,
                derived.ok_or("unfinished recursive definition derivation")?,
            );
        }
        schema
            .as_object_mut()
            .ok_or("a recursive root is not an object schema")?
            .insert("definitions".to_string(), Value::Object(local));
    }
    Ok(schema)
}

struct Derivation<'a> {
    definitions: &'a Map<String, Value>,
    /// Local definitions by name and nullability; `None` until derived.
    recursive: BTreeMap<String, Option<Value>>,
    pending: Vec<(String, bool)>,
}

impl<'a> Derivation<'a> {
    fn definition(&self, name: &str) -> Result<&'a Value, String> {
        self.definitions
            .get(name)
            .ok_or_else(|| format!("unresolved definition {name}"))
    }

    fn schema(
        &mut self,
        node: &Value,
        nullable: bool,
        root: bool,
        stack: &mut Vec<String>,
    ) -> Result<Value, String> {
        let Value::Object(node) = node else {
            return Ok(node.clone());
        };
        if let Some(reference) = node.get("$ref") {
            let name = reference
                .as_str()
                .and_then(|reference| reference.strip_prefix(DEFINITIONS_PREFIX))
                .ok_or_else(|| format!("unsupported reference {reference}"))?
                .to_string();
            if stack.contains(&name) {
                let local = local_name(&name, nullable);
                if !self.recursive.contains_key(&local) {
                    self.recursive.insert(local.clone(), None);
                    self.pending.push((name, nullable));
                }
                return Ok(json!({"$ref": format!("{DEFINITIONS_PREFIX}{local}")}));
            }
            let definition = self.definition(&name)?;
            stack.push(name);
            let derived = self.schema(definition, nullable, root, stack);
            stack.pop();
            return derived;
        }
        if node.get("format").and_then(Value::as_str) == Some("int-or-string") {
            let alternatives = json!({"oneOf": [{"type": "string"}, {"type": "integer"}]});
            return self.schema(&alternatives, nullable, root, stack);
        }
        let identities = fixed_identities(node)?;
        let required: BTreeSet<&str> = node
            .get("required")
            .and_then(Value::as_array)
            .map(|required| required.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let mut derived = Map::new();
        for (keyword, value) in node {
            let value = match (keyword.as_str(), value) {
                ("type", Value::String(name)) if nullable && name != "null" => {
                    json!([name, "null"])
                }
                ("properties", Value::Object(properties)) => {
                    let mut derived_properties = Map::new();
                    for (property, schema) in properties {
                        let property_nullable = root || !required.contains(property.as_str());
                        let mut schema = self.schema(schema, property_nullable, false, stack)?;
                        if let Some((api_versions, kinds)) = &identities
                            && let Some(schema) = schema.as_object_mut()
                        {
                            match property.as_str() {
                                "apiVersion" => {
                                    schema.insert("enum".to_string(), api_versions.clone());
                                }
                                "kind" => {
                                    schema.insert("enum".to_string(), kinds.clone());
                                }
                                _ => {}
                            }
                        }
                        derived_properties.insert(property.clone(), schema);
                    }
                    Value::Object(derived_properties)
                }
                ("items" | "additionalProperties" | "not", Value::Object(_)) => {
                    self.schema(value, true, false, stack)?
                }
                ("oneOf" | "anyOf" | "allOf", Value::Array(alternatives)) => {
                    let mut derived_alternatives = Vec::new();
                    for alternative in alternatives {
                        derived_alternatives.push(self.schema(alternative, true, false, stack)?);
                    }
                    Value::Array(derived_alternatives)
                }
                _ => value.clone(),
            };
            derived.insert(keyword.clone(), value);
        }
        if derived.contains_key("properties") && !derived.contains_key("additionalProperties") {
            derived.insert("additionalProperties".to_string(), Value::Bool(false));
        }
        Ok(Value::Object(derived))
    }
}

/// The `apiVersion` and `kind` alternatives a definition's group-version-kinds fix.
fn fixed_identities(node: &Map<String, Value>) -> Result<Option<(Value, Value)>, String> {
    let Some(identities) = node.get("x-kubernetes-group-version-kind") else {
        return Ok(None);
    };
    let identities = identities
        .as_array()
        .ok_or("malformed group-version-kind")?;
    let mut api_versions = Vec::new();
    let mut kinds = Vec::new();
    for identity in identities {
        let field = |name: &str| {
            identity
                .get(name)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("group-version-kind lacks {name}"))
        };
        let (group, version, kind) = (field("group")?, field("version")?, field("kind")?);
        api_versions.push(Value::String(if group.is_empty() {
            version.to_string()
        } else {
            format!("{group}/{version}")
        }));
        kinds.push(Value::String(kind.to_string()));
    }
    Ok(Some((Value::Array(api_versions), Value::Array(kinds))))
}

fn local_name(name: &str, nullable: bool) -> String {
    if nullable {
        name.to_string()
    } else {
        format!("{name}.required")
    }
}
