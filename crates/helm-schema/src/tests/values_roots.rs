use super::*;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn values_roots_from_yaml(source: Option<&str>) -> ValuesRoots {
    let document = source
        .and_then(|source| serde_yaml::from_str(source).ok())
        .unwrap_or(serde_yaml::Value::Null);
    ValuesRoots::from_values_document(&document)
}

#[test]
fn extracts_sorted_top_level_mapping_keys_only() {
    let roots = values_roots_from_yaml(Some(indoc! {r#"
        z:
          nested: true
        a: 1
        "quoted": value
    "#}));

    sim_assert_eq!(
        have: roots.top_level_paths,
        want: ["a", "quoted", "z"]
            .into_iter()
            .map(helm_schema_core::ValuesPath::parse)
            .collect::<BTreeSet<_>>()
    );
}

#[test]
fn ignores_non_mapping_documents_and_empty_keys() {
    assert!(
        values_roots_from_yaml(Some("- item\n"))
            .top_level_paths
            .is_empty()
    );
    assert!(
        values_roots_from_yaml(Some("\"\": value\n"))
            .top_level_paths
            .is_empty()
    );
    assert!(values_roots_from_yaml(None).top_level_paths.is_empty());
}

#[test]
fn extracts_nested_explicit_mapping_paths() {
    let roots = values_roots_from_yaml(Some(indoc! {r"
        controller:
          kind: Deployment
          admissionWebhooks:
            enabled: true
        tcp: {}
        items:
          - name: first
    "}));

    sim_assert_eq!(
        have: roots.explicit_paths,
        want: BTreeSet::from([
            "controller".to_string(),
            "controller.admissionWebhooks".to_string(),
            "controller.admissionWebhooks.enabled".to_string(),
            "controller.kind".to_string(),
            "items".to_string(),
            "tcp".to_string(),
        ])
    );
}

#[test]
fn explicit_paths_ignore_non_mapping_documents_and_empty_keys() {
    assert!(
        values_roots_from_yaml(Some("- item\n"))
            .explicit_paths
            .is_empty()
    );
    assert!(
        values_roots_from_yaml(Some("\"\": value\n"))
            .explicit_paths
            .is_empty()
    );
    assert!(values_roots_from_yaml(None).explicit_paths.is_empty());
}
