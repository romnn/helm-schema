use std::collections::BTreeMap;

use color_eyre::eyre;
use helm_schema_json_schema_walk::GENERATED_DEFINITIONS;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use super::compact;
use crate::RootPolicy;
use crate::provider_definitions::insert_definitions_into_root;
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;

fn reference(name: &str) -> Value {
    json!({"$ref": GENERATED_DEFINITIONS.reference(name)})
}

fn referenced_property(document: &mut SchemaDocument, property: &str, reference: &str) {
    document.insert_path_schema(&[property.to_owned()], SchemaNode::reference(reference));
}

fn document_with_definitions(
    document: SchemaDocument,
    definitions: BTreeMap<String, Value>,
) -> Value {
    let mut schema = document.into_value();
    schema["$schema"] = json!("http://json-schema.org/draft-07/schema#");
    insert_definitions_into_root(&mut schema, definitions);
    schema
}

#[test]
fn suffix_references_preserve_junction_arm_indexes() -> eyre::Result<()> {
    let mut document = SchemaDocument::new_root_object(RootPolicy::Open);
    referenced_property(&mut document, "parent", "#/definitions/parent");
    referenced_property(&mut document, "arm", "#/definitions/parent/anyOf/1");
    let mut definitions = BTreeMap::from([
        (
            "parent".to_owned(),
            json!({"anyOf":[reference("child"), {"const":2}]}),
        ),
        (
            "child".to_owned(),
            json!({"anyOf":[{"const":0}, {"const":1}]}),
        ),
    ]);
    let before = definitions.clone();
    compact(&document, &mut definitions);
    sim_assert_eq!(have: definitions, want: before);
    let validator = jsonschema::validator_for(&document_with_definitions(document, definitions))?;
    assert!(validator.is_valid(&json!({"parent":0, "arm":2})));
    assert!(!validator.is_valid(&json!({"parent":0, "arm":1})));
    Ok(())
}
