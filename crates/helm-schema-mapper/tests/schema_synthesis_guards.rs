use helm_schema_mapper::schema::{generate_values_schema_vyt, DefaultVytSchemaProvider};
use helm_schema_mapper::vyt::{VYKind, VYUse, YPath};

#[test]
fn guard_only_paths_are_included_in_schema() {
    let uses = vec![VYUse {
        source_expr: "foo".to_string(),
        path: YPath(vec![]),
        kind: VYKind::Scalar,
        guards: vec!["feature.enabled".to_string()],
        resource: None,
    }];

    let schema = generate_values_schema_vyt(&uses, &DefaultVytSchemaProvider::default());

    let ty = schema
        .pointer("/properties/feature/properties/enabled/type")
        .and_then(|v| v.as_str());

    assert_eq!(ty, Some("boolean"));
}
