//! The release-definitions derivation reproduces the pinned standalone-strict bundle.

use std::fs;

use color_eyre::eyre::{self, OptionExt as _};
use jsonschema::Draft;
use serde_json::{Map, Value, json};
use test_util::prelude::sim_assert_eq;

use crate::helm::adjudication::{KUBERNETES_RELEASE, ResourceSchema};
use crate::helm::assertion_identity::{canonical_schema_location, verify_registry_integrity};
use crate::helm::definitions::resource_schema;

const ANONYMOUS_SCHEMA_URI: &str = "https://helm-schema.invalid/resource";

fn release_definitions() -> eyre::Result<Value> {
    let path = test_util::workspace_testdata()
        .join("provider-bundle/kubernetes-json-schema-cache/default/v1.29.0/_definitions.json");
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

/// `schema` without annotation keywords, which never change what a schema accepts.
fn validating(schema: &Value) -> Value {
    match schema {
        Value::Object(node) => {
            let mut kept = Map::new();
            for (keyword, value) in node {
                if keyword == "description" || keyword == "$schema" || keyword.starts_with("x-") {
                    continue;
                }
                let value = if keyword == "properties"
                    && let Value::Object(properties) = value
                {
                    Value::Object(
                        properties
                            .iter()
                            .map(|(name, schema)| (name.clone(), validating(schema)))
                            .collect(),
                    )
                } else {
                    validating(value)
                };
                kept.insert(keyword.clone(), value);
            }
            Value::Object(kept)
        }
        Value::Array(items) => Value::Array(items.iter().map(validating).collect()),
        other => other.clone(),
    }
}

/// Every standalone-strict schema the bundle pins is exactly what the derivation yields from
/// the same release's definitions, so a kind the bundle omits is judged by the same rules.
#[test]
fn derived_schemas_equal_every_pinned_standalone_strict_schema() -> eyre::Result<()> {
    let definitions = release_definitions()?;
    let directory = test_util::workspace_testdata()
        .join("provider-bundle/kubernetes-json-schema-cache/default")
        .join(KUBERNETES_RELEASE);
    let mut compared = 0;
    let mut entries: Vec<_> = fs::read_dir(directory)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let pinned: Value = serde_json::from_slice(&fs::read(&path)?)?;
        let [identity] = pinned
            .get("x-kubernetes-group-version-kind")
            .and_then(Value::as_array)
            .ok_or_eyre("pinned schema lacks its group-version-kind")?
            .as_slice()
        else {
            eyre::bail!("{} pins several kinds", path.display());
        };
        let field = |name: &str| identity.get(name).and_then(Value::as_str).unwrap_or("");
        let api_version = match field("group") {
            "" => field("version").to_string(),
            group => format!("{group}/{}", field("version")),
        };
        let derived = resource_schema(&definitions, &api_version, field("kind"))
            .map_err(|error| eyre::eyre!(error))?
            .ok_or_eyre("the release does not declare a pinned kind")?;
        sim_assert_eq!(have: validating(&derived), want: validating(&pinned));
        compared += 1;
    }
    sim_assert_eq!(have: compared, want: 36);
    Ok(())
}

/// The CRD the standalone bundle omits derives with its recursion kept as local references,
/// compiles, and types annotations as strings.
#[test]
fn the_custom_resource_definition_kind_derives_a_recursive_strict_schema() -> eyre::Result<()> {
    let definitions = release_definitions()?;
    let schema = resource_schema(
        &definitions,
        "apiextensions.k8s.io/v1",
        "CustomResourceDefinition",
    )
    .map_err(|error| eyre::eyre!(error))?
    .ok_or_eyre("the release does not declare CustomResourceDefinition")?;
    let local: Vec<&str> = schema
        .get("definitions")
        .and_then(Value::as_object)
        .ok_or_eyre("recursion left no local definitions")?
        .keys()
        .map(String::as_str)
        .collect();
    sim_assert_eq!(
        have: local,
        want: vec!["io.k8s.apiextensions-apiserver.pkg.apis.apiextensions.v1.JSONSchemaProps"],
    );
    verify_registry_integrity(&schema, Draft::Draft7, ANONYMOUS_SCHEMA_URI)?;
    let compiled = ResourceSchema::compile(&schema, Draft::Draft7)?;
    let crd = |annotations: Value, property: Value| {
        json!({
            "apiVersion": "apiextensions.k8s.io/v1",
            "kind": "CustomResourceDefinition",
            "metadata": {"name": "widgets.example.test", "annotations": annotations},
            "spec": {
                "group": "example.test",
                "names": {"kind": "Widget", "plural": "widgets"},
                "scope": "Namespaced",
                "versions": [{
                    "name": "v1", "served": true, "storage": true,
                    "schema": {"openAPIV3Schema": {
                        "type": "object",
                        "properties": {"spec": {"type": "object", "properties": {"size": property}}},
                    }},
                }],
            },
        })
    };
    let errors = |document: &Value| -> eyre::Result<Vec<String>> {
        let mut found = Vec::new();
        for error in compiled.validator.iter_errors(document) {
            let location =
                canonical_schema_location(&compiled.registry, ANONYMOUS_SCHEMA_URI, &error)?;
            found.push(format!("{} @ {location}", error.instance_path()));
        }
        Ok(found)
    };
    let valid = crd(
        json!({"helm.sh/resource-policy": "keep"}),
        json!({"type": "integer"}),
    );
    sim_assert_eq!(have: errors(&valid)?, want: Vec::<String>::new());
    let boolean_annotation = crd(json!({"unknown": true}), json!({"type": "integer"}));
    sim_assert_eq!(
        have: errors(&boolean_annotation)?,
        want: vec![format!(
            "/metadata/annotations/unknown @ {ANONYMOUS_SCHEMA_URI}#/properties/metadata/properties/annotations/additionalProperties/type"
        )],
    );
    // Nested JSONSchemaProps are strict too: an undeclared keyword is an unknown field.
    let unknown_keyword = crd(json!({}), json!({"type": "integer", "bogus": 1}));
    // The nested property recurses into the local JSONSchemaProps definition.
    sim_assert_eq!(
        have: errors(&unknown_keyword)?,
        want: vec![format!(
            "/spec/versions/0/schema/openAPIV3Schema/properties/spec/properties/size @ \
             {ANONYMOUS_SCHEMA_URI}#/definitions/\
             io.k8s.apiextensions-apiserver.pkg.apis.apiextensions.v1.JSONSchemaProps/\
             additionalProperties"
        )],
    );
    Ok(())
}
