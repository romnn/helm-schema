use helm_schema_mapper::schema::{generate_values_schema_vyt, DefaultVytSchemaProvider};
use helm_schema_mapper::vyt::{VYKind, VYUse, YPath};

#[test]
fn unguarded_paths_are_required_guarded_paths_are_not() {
    let uses = vec![
        // Unguarded scalar use: should make foo.bar required.
        VYUse {
            source_expr: "foo.bar".to_string(),
            path: YPath(vec!["spec".to_string()]),
            kind: VYKind::Scalar,
            guards: vec![],
            resource: None,
        },
        // Guarded scalar use: should NOT make baz.qux required.
        VYUse {
            source_expr: "baz.qux".to_string(),
            path: YPath(vec!["spec".to_string()]),
            kind: VYKind::Scalar,
            guards: vec!["feature.enabled".to_string()],
            resource: None,
        },
        // Guard-only path should exist (boolean), but should not be required.
        VYUse {
            source_expr: "dummy".to_string(),
            path: YPath(vec!["spec".to_string()]),
            kind: VYKind::Scalar,
            guards: vec!["feature.enabled".to_string()],
            resource: None,
        },
    ];

    let schema = generate_values_schema_vyt(&uses, &DefaultVytSchemaProvider::default());

    let foo_required = schema
        .pointer("/required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    assert!(foo_required.contains(&"foo"));
    assert!(!foo_required.contains(&"baz"));
    assert!(!foo_required.contains(&"feature"));

    let foo_bar_required = schema
        .pointer("/properties/foo/required")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    assert!(foo_bar_required.contains(&"bar"));

    let feature_enabled_ty = schema
        .pointer("/properties/feature/properties/enabled/type")
        .and_then(|v| v.as_str());
    assert_eq!(feature_enabled_ty, Some("boolean"));
}
