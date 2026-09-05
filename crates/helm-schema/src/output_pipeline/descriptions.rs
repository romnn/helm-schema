use helm_schema_json_schema_walk::visit_subschemas_mut;
use serde_json::Value;

pub(super) fn strip_schema_descriptions(schema: &mut Value) {
    if let Some(object) = schema.as_object_mut() {
        object.remove("description");
    }
    visit_subschemas_mut(
        schema,
        helm_schema_json_schema_walk::ReferenceSiblings::Skip,
        &mut strip_schema_descriptions,
    );
}

#[cfg(test)]
#[path = "tests/descriptions.rs"]
mod tests;
