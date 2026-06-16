use helm_schema_engine::{HelmAst, HelmParser as _, TreeSitterParser};
use helm_schema_k8s::{LocalResourceSchema, resource_schemas_from_crd_document_with_source};
use serde::Deserialize;
use serde_json::Value;

use crate::error::CliResult;

const TEMPLATE_CRD_SOURCE_ID: &str = "chart-template-crd";

pub(crate) fn local_resource_schemas_from_template_source(
    source: &str,
    filename: &str,
    contains_template_action: bool,
) -> CliResult<Vec<LocalResourceSchema>> {
    if !contains_template_action {
        return local_resource_schemas_from_literal_template(source, filename);
    }

    let ast = TreeSitterParser.parse(source)?;
    Ok(local_resource_schemas_from_template_ast(&ast, filename))
}

fn local_resource_schemas_from_literal_template(
    source: &str,
    filename: &str,
) -> CliResult<Vec<LocalResourceSchema>> {
    let mut resource_schemas = Vec::new();
    for document in serde_yaml::Deserializer::from_str(source) {
        let document = Value::deserialize(document)?;
        if document.is_null() {
            continue;
        }
        resource_schemas.extend(resource_schemas_from_crd_document_with_source(
            document,
            TEMPLATE_CRD_SOURCE_ID,
            filename.to_string(),
        ));
    }
    Ok(resource_schemas)
}

fn local_resource_schemas_from_template_ast(
    ast: &HelmAst,
    filename: &str,
) -> Vec<LocalResourceSchema> {
    let mut resource_schemas = Vec::new();
    collect_template_crd_schemas(ast, false, filename, &mut resource_schemas);
    resource_schemas
}

fn collect_template_crd_schemas(
    node: &HelmAst,
    in_control_flow: bool,
    filename: &str,
    resource_schemas: &mut Vec<LocalResourceSchema>,
) {
    if !in_control_flow && let Some(extracted) = crd_resource_schemas_from_ast(node, filename) {
        resource_schemas.extend(extracted);
        return;
    }

    match node {
        HelmAst::Document { items } | HelmAst::Mapping { items } | HelmAst::Sequence { items } => {
            for item in items {
                collect_template_crd_schemas(item, in_control_flow, filename, resource_schemas);
            }
        }
        HelmAst::Pair { value, .. } => {
            if let Some(value) = value.as_deref() {
                collect_template_crd_schemas(value, in_control_flow, filename, resource_schemas);
            }
        }
        HelmAst::If {
            then_branch,
            else_branch,
            ..
        }
        | HelmAst::Range {
            body: then_branch,
            else_branch,
            ..
        }
        | HelmAst::With {
            body: then_branch,
            else_branch,
            ..
        } => {
            for item in then_branch {
                collect_template_crd_schemas(item, true, filename, resource_schemas);
            }
            for item in else_branch {
                collect_template_crd_schemas(item, true, filename, resource_schemas);
            }
        }
        HelmAst::Define { body, .. } | HelmAst::Block { body, .. } => {
            for item in body {
                collect_template_crd_schemas(item, true, filename, resource_schemas);
            }
        }
        HelmAst::Scalar { .. } | HelmAst::HelmExpr { .. } | HelmAst::HelmComment { .. } => {}
    }
}

fn crd_resource_schemas_from_ast(
    node: &HelmAst,
    filename: &str,
) -> Option<Vec<LocalResourceSchema>> {
    if literal_string_for_key(node, "apiVersion")?.as_str() != "apiextensions.k8s.io/v1"
        && literal_string_for_key(node, "apiVersion")?.as_str() != "apiextensions.k8s.io/v1beta1"
    {
        return None;
    }
    if literal_string_for_key(node, "kind")?.as_str() != "CustomResourceDefinition" {
        return None;
    }

    let spec = mapping_value(node, "spec")?;
    let group = literal_string_for_key(spec, "group")?;
    let names = mapping_value(spec, "names")?;
    let kind = literal_string_for_key(names, "kind")?;

    if let Some(versions) = mapping_value(spec, "versions") {
        let mut resource_schemas = Vec::new();
        for version in sequence_items(versions)? {
            if literal_bool_for_key(version, "served").is_some_and(|served| !served) {
                continue;
            }
            let version_name = literal_string_for_key(version, "name")?;
            let schema = mapping_value(version, "schema")
                .and_then(|schema| mapping_value(schema, "openAPIV3Schema"))
                .and_then(literal_json_from_ast)?;
            resource_schemas.push(
                LocalResourceSchema::new(format!("{group}/{version_name}"), kind.clone(), schema)
                    .with_source(TEMPLATE_CRD_SOURCE_ID, filename.to_string()),
            );
        }
        return (!resource_schemas.is_empty()).then_some(resource_schemas);
    }

    let version = literal_string_for_key(spec, "version")?;
    let schema = mapping_value(spec, "validation")
        .and_then(|validation| mapping_value(validation, "openAPIV3Schema"))
        .and_then(literal_json_from_ast)?;
    Some(vec![
        LocalResourceSchema::new(format!("{group}/{version}"), kind, schema)
            .with_source(TEMPLATE_CRD_SOURCE_ID, filename.to_string()),
    ])
}

fn mapping_value<'a>(node: &'a HelmAst, key: &str) -> Option<&'a HelmAst> {
    let items = match node {
        HelmAst::Document { items } if items.len() == 1 => return mapping_value(&items[0], key),
        HelmAst::Mapping { items } => items,
        _ => return None,
    };

    for item in items {
        let HelmAst::Pair {
            key: pair_key,
            value,
        } = item
        else {
            continue;
        };
        if literal_string_from_ast(pair_key.as_ref()).as_deref() == Some(key) {
            return value.as_deref();
        }
    }
    None
}

fn sequence_items(node: &HelmAst) -> Option<&[HelmAst]> {
    match node {
        HelmAst::Sequence { items } => Some(items),
        HelmAst::Document { items } if items.len() == 1 => sequence_items(&items[0]),
        _ => None,
    }
}

fn literal_string_for_key(node: &HelmAst, key: &str) -> Option<String> {
    mapping_value(node, key).and_then(literal_string_from_ast)
}

fn literal_bool_for_key(node: &HelmAst, key: &str) -> Option<bool> {
    mapping_value(node, key)
        .and_then(literal_json_from_ast)?
        .as_bool()
}

fn literal_string_from_ast(node: &HelmAst) -> Option<String> {
    literal_json_from_ast(node)?
        .as_str()
        .map(std::string::ToString::to_string)
}

fn literal_json_from_ast(node: &HelmAst) -> Option<Value> {
    match node {
        HelmAst::Document { items } => {
            if items.len() == 1 {
                literal_json_from_ast(&items[0])
            } else {
                None
            }
        }
        HelmAst::Mapping { items } => {
            let mut object = serde_json::Map::new();
            for item in items {
                let HelmAst::Pair { key, value } = item else {
                    return None;
                };
                let key = literal_string_from_ast(key.as_ref())?;
                let value = match value.as_deref() {
                    Some(value) => literal_json_from_ast(value)?,
                    None => Value::Null,
                };
                object.insert(key, value);
            }
            Some(Value::Object(object))
        }
        HelmAst::Sequence { items } => items
            .iter()
            .map(literal_json_from_ast)
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        HelmAst::Scalar { text } => {
            let yaml_value = serde_yaml::from_str::<serde_yaml::Value>(text).ok()?;
            serde_json::to_value(yaml_value).ok()
        }
        HelmAst::HelmComment { .. } => None,
        HelmAst::HelmExpr { .. }
        | HelmAst::If { .. }
        | HelmAst::Range { .. }
        | HelmAst::With { .. }
        | HelmAst::Define { .. }
        | HelmAst::Block { .. }
        | HelmAst::Pair { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::local_resource_schemas_from_template_source;

    #[test]
    fn templated_metadata_crd_still_projects_local_schema() -> color_eyre::eyre::Result<()> {
        let source = r#"apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: {{ printf "%s.example.com" "widgets" }}
spec:
  group: example.com
  names:
    kind: Widget
    plural: widgets
  scope: Namespaced
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                size:
                  type: integer
"#;

        let schemas =
            local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true)?;

        assert_eq!(schemas.len(), 1);
        assert_eq!(schemas[0].api_version, "example.com/v1");
        assert_eq!(schemas[0].kind, "Widget");
        assert_eq!(
            schemas[0]
                .schema
                .pointer("/properties/spec/properties/size"),
            Some(&json!({"type": "integer"}))
        );

        Ok(())
    }

    #[test]
    fn dynamic_schema_subtree_is_not_projected() -> color_eyre::eyre::Result<()> {
        let source = r#"apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
spec:
  group: example.com
  names:
    kind: Widget
    plural: widgets
  scope: Namespaced
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: {{ .Values.schemaType }}
"#;

        let schemas =
            local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true)?;

        assert!(schemas.is_empty());

        Ok(())
    }
}
