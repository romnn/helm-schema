use color_eyre::eyre;
use indoc::indoc;

use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::local_resource_schemas_from_template_source;

#[test]
fn templated_metadata_crd_still_projects_local_schema() -> eyre::Result<()> {
    let source = indoc! {r#"
        apiVersion: apiextensions.k8s.io/v1
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
    "#};

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
fn dynamic_schema_subtree_is_not_projected() -> eyre::Result<()> {
    let source = indoc! {r"
        apiVersion: apiextensions.k8s.io/v1
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
    "};

    let schemas =
        local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true)?;

    assert!(schemas.is_empty());

    Ok(())
}

#[test]
fn multiple_templated_documents_keep_v1_and_v1beta1_crds() -> eyre::Result<()> {
    let source = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ .Release.Name }}
        ---
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: {{ printf "%s.example.com" "widgets" }}
        spec:
          group: example.com
          names:
            kind: Widget
          versions:
            - name: v1
              served: true
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    spec:
                      type: object
                      properties:
                        size:
                          type: integer
        ---
        apiVersion: apiextensions.k8s.io/v1beta1
        kind: CustomResourceDefinition
        metadata:
          name: gadgets.example.com
        spec:
          group: example.com
          names:
            kind: Gadget
          version: v2
          validation:
            openAPIV3Schema:
              type: object
              properties:
                spec:
                  type: object
                  properties:
                    enabled:
                      type: boolean
    "#};

    let schemas =
        local_resource_schemas_from_template_source(source, "/chart/templates/crds.yaml", true)?;
    let coordinates = schemas
        .iter()
        .map(|schema| (schema.api_version.as_str(), schema.kind.as_str()))
        .collect::<Vec<_>>();

    sim_assert_eq!(
        have: coordinates,
        want: vec![("example.com/v1", "Widget"), ("example.com/v2", "Gadget")]
    );
    sim_assert_eq!(
        have: schemas[0].schema.pointer("/properties/spec/properties/size"),
        want: Some(&json!({"type": "integer"}))
    );
    sim_assert_eq!(
        have: schemas[1]
            .schema
            .pointer("/properties/spec/properties/enabled"),
        want: Some(&json!({"type": "boolean"}))
    );

    Ok(())
}

#[test]
fn standalone_schema_holes_keep_literal_siblings() -> eyre::Result<()> {
    let source = indoc! {r"
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: widgets.example.com
        spec:
          group: example.com
          names:
            kind: Widget
          versions:
            - name: v1
              served: true
              schema:
                openAPIV3Schema:
                  type: object
                  {{ .Values.extraSchema | toYaml | nindent 16 }}
                  properties:
                    spec:
                      type: object
                      properties:
                        size:
                          type: integer
    "};

    let schemas =
        local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true)?;

    sim_assert_eq!(have: schemas.len(), want: 1);
    sim_assert_eq!(
        have: schemas[0]
            .schema
            .pointer("/properties/spec/properties/size"),
        want: Some(&json!({"type": "integer"}))
    );

    Ok(())
}
