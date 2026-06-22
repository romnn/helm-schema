use serde_json::json;
use test_util::prelude::sim_assert_eq;

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

    sim_assert_eq!(have: schemas.len(), want: 1);
    sim_assert_eq!(have: schemas[0].api_version, want: "example.com/v1");
    sim_assert_eq!(have: schemas[0].kind, want: "Widget");
    sim_assert_eq!(
        have: schemas[0]
            .schema
            .pointer("/properties/spec/properties/size"),
        want: Some(&json!({"type": "integer"}))
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
