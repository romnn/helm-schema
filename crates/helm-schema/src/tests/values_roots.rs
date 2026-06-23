use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn extracts_sorted_top_level_mapping_keys_only() {
    let paths = top_level_value_paths(Some(
        r#"
z:
  nested: true
a: 1
"quoted": value
"#,
    ));

    sim_assert_eq!(
        have: paths,
        want: BTreeSet::from(["a".to_string(), "quoted".to_string(), "z".to_string()])
    );
}

#[test]
fn ignores_non_mapping_documents_and_empty_keys() {
    assert!(top_level_value_paths(Some("- item\n")).is_empty());
    assert!(top_level_value_paths(Some("\"\": value\n")).is_empty());
    assert!(top_level_value_paths(None).is_empty());
}

#[test]
fn mapping_root_paths_distinguish_structured_values_roots() {
    let paths = top_level_mapping_value_paths(Some(
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
        have: paths,
        want: BTreeSet::from(["empty".to_string(), "object".to_string()])
    );
}

#[test]
fn extracts_nested_explicit_mapping_paths() {
    let paths = explicit_value_paths(Some(
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
        have: paths,
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
    assert!(explicit_value_paths(Some("- item\n")).is_empty());
    assert!(explicit_value_paths(Some("\"\": value\n")).is_empty());
    assert!(explicit_value_paths(None).is_empty());
}
