use indoc::indoc;

use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use super::local_resource_schemas_from_template_source;

#[test]
fn templated_metadata_crd_still_projects_local_schema() {
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
        local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true);

    sim_assert_eq!(have: schemas.len(), want: 1);
    sim_assert_eq!(have: schemas[0].api_version, want: "example.com/v1");
    sim_assert_eq!(have: schemas[0].kind, want: "Widget");
    sim_assert_eq!(
        have: schemas[0]
            .schema
            .pointer("/properties/spec/properties/size"),
        want: Some(&json!({"type": "integer"}))
    );
}

#[test]
fn dynamic_schema_subtree_is_not_projected() {
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
        local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true);

    sim_assert_eq!(have: schemas, want: [] as [helm_schema_k8s::LocalResourceSchema; 0]);
}

#[test]
fn multiple_templated_documents_keep_v1_and_v1beta1_crds() {
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
        local_resource_schemas_from_template_source(source, "/chart/templates/crds.yaml", true);
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
}

#[test]
fn standalone_schema_holes_keep_literal_siblings() {
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
        local_resource_schemas_from_template_source(source, "/chart/templates/crd.yaml", true);

    sim_assert_eq!(have: schemas.len(), want: 1);
    sim_assert_eq!(
        have: schemas[0]
            .schema
            .pointer("/properties/spec/properties/size"),
        want: Some(&json!({"type": "integer"}))
    );
}

/// Each projected resource schema as `(apiVersion, kind, schema)`.
fn projected(source: &str, templated: bool) -> Vec<(String, String, Value)> {
    local_resource_schemas_from_template_source(source, "/chart/crds/crd.yaml", templated)
        .into_iter()
        .map(|schema| (schema.api_version, schema.kind, schema.schema))
        .collect()
}

#[test]
fn yaml_1_1_booleans_decode_as_helm_reads_them() {
    let source = indoc! {r#"
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
              served: yes
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    spec:
                      type: object
                      properties:
                        flag:
                          type: boolean
                          enum: [on, off]
                          default: yes
                        mode:
                          type: string
                          enum: ["on", 'off', N]
            - name: v2
              served: no
              schema:
                openAPIV3Schema:
                  type: object
    "#};

    sim_assert_eq!(
        have: projected(source, false),
        want: vec![(
            "example.com/v1".to_string(),
            "Widget".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "spec": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "flag": {"type": "boolean", "enum": [true, false], "default": true},
                            "mode": {"type": "string", "enum": ["on", "off", false]},
                        },
                    },
                },
            }),
        )]
    );
}

#[test]
fn merge_keys_apply_with_go_yaml_v2_precedence() {
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
                  properties:
                    base: &base
                      type: object
                      required: [flag]
                      properties:
                        flag: {type: boolean}
                    merged:
                      <<: *base
                      description: merged
                    listed:
                      <<: [{type: object, description: first}, {description: second, nullable: true}]
                    overridden:
                      description: before
                      <<: {description: merged}
                    tagged:
                      !!merge <<: *base
    "};
    let base = json!({
        "type": "object",
        "required": ["flag"],
        "additionalProperties": false,
        "properties": {"flag": {"type": "boolean"}},
    });
    let mut merged = base.clone();
    if let Some(object) = merged.as_object_mut() {
        object.insert("description".to_string(), json!("merged"));
    }

    sim_assert_eq!(
        have: projected(source, false),
        want: vec![(
            "example.com/v1".to_string(),
            "Widget".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "base": base.clone(),
                    "merged": merged,
                    "listed": {"type": "object", "description": "first", "nullable": true},
                    "overridden": {"description": "merged"},
                    "tagged": base,
                },
            }),
        )]
    );
}

#[test]
fn boolean_and_integer_keys_are_stringified() {
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
                  properties:
                    on: {type: string}
                    n: {type: integer}
                    1: {type: boolean}
                    0x10: {type: number}
    "};

    sim_assert_eq!(
        have: projected(source, false),
        want: vec![(
            "example.com/v1".to_string(),
            "Widget".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "true": {"type": "string"},
                    "false": {"type": "integer"},
                    "1": {"type": "boolean"},
                    "16": {"type": "number"},
                },
            }),
        )]
    );
}

#[test]
fn constructs_helm_reads_unreproducibly_widen() {
    let source = indoc! {r#"
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
                  properties:
                    size:
                      type: integer
                      maximum: !!int "3"
            - name: v2
              served: true
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    1: {type: string}
                    "1": {type: integer}
        ---
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: gadgets.example.com
        spec:
          group: example.com
          names:
            kind: Gadget
          versions:
            - name: v1
              served: true
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    ratio: {type: number, maximum: .inf}
        ---
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: gizmos.example.com
        spec:
          group: example.com
          names:
            kind: Gizmo
          versions:
            - name: v1
              served: true
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    size: {type: integer}
    "#};

    sim_assert_eq!(
        have: projected(source, false),
        want: vec![
            ("example.com/v1".to_string(), "Widget".to_string(), json!({})),
            ("example.com/v2".to_string(), "Widget".to_string(), json!({})),
            (
                "example.com/v1".to_string(),
                "Gizmo".to_string(),
                json!({"type": "object", "properties": {"size": {"type": "integer"}}}),
            ),
        ]
    );
}

#[test]
fn templated_crds_decode_yaml_1_1_and_refuse_merge_keys() {
    let booleans = indoc! {r"
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: {{ .Release.Name }}-widgets
        spec:
          group: example.com
          names:
            kind: Widget
          versions:
            - name: v1
              served: yes
              schema:
                openAPIV3Schema:
                  type: object
                  properties:
                    flag: {type: boolean, enum: [on, off]}
                    on: {type: string}
    "};
    let merge = indoc! {r"
        apiVersion: apiextensions.k8s.io/v1
        kind: CustomResourceDefinition
        metadata:
          name: {{ .Release.Name }}-widgets
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
                    flag:
                      <<: {type: string}
                      description: flag
    "};

    sim_assert_eq!(
        have: projected(booleans, true),
        want: vec![(
            "example.com/v1".to_string(),
            "Widget".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "flag": {"type": "boolean", "enum": [true, false]},
                    "true": {"type": "string"},
                },
            }),
        )]
    );
    sim_assert_eq!(have: projected(merge, true), want: Vec::new());
}
