use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn extracts_sorted_top_level_mapping_keys_only() {
    let roots = ValuesRoots::from_values_yaml(Some(
        r#"
z:
  nested: true
a: 1
"quoted": value
"#,
    ));

    sim_assert_eq!(
        have: roots.top_level_paths,
        want: BTreeSet::from(["a".to_string(), "quoted".to_string(), "z".to_string()])
    );
}

#[test]
fn ignores_non_mapping_documents_and_empty_keys() {
    assert!(
        ValuesRoots::from_values_yaml(Some("- item\n"))
            .top_level_paths
            .is_empty()
    );
    assert!(
        ValuesRoots::from_values_yaml(Some("\"\": value\n"))
            .top_level_paths
            .is_empty()
    );
    assert!(
        ValuesRoots::from_values_yaml(None)
            .top_level_paths
            .is_empty()
    );
}

#[test]
fn mapping_root_paths_distinguish_structured_values_roots() {
    let roots = ValuesRoots::from_values_yaml(Some(
        r#"
object:
  nested: true
empty: {}
scalar: value
list:
  - item
"#,
    ));

    sim_assert_eq!(
        have: roots.top_level_mapping_paths,
        want: BTreeSet::from(["empty".to_string(), "object".to_string()])
    );
}

#[test]
fn extracts_nested_explicit_mapping_paths() {
    let roots = ValuesRoots::from_values_yaml(Some(
        r#"
controller:
  kind: Deployment
  admissionWebhooks:
    enabled: true
tcp: {}
items:
  - name: first
"#,
    ));

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
        ValuesRoots::from_values_yaml(Some("- item\n"))
            .explicit_paths
            .is_empty()
    );
    assert!(
        ValuesRoots::from_values_yaml(Some("\"\": value\n"))
            .explicit_paths
            .is_empty()
    );
    assert!(
        ValuesRoots::from_values_yaml(None)
            .explicit_paths
            .is_empty()
    );
}
