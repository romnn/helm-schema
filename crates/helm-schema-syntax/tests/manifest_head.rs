//! Helm's manifest-head decoding and ambiguity after JSON-key conversion.

use helm_schema_syntax::manifest_chunk_rejection;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[test]
fn head_decoding_uses_helm_scalar_coercions_and_ignores_labels() {
    let accepted = indoc! {"
        apiVersion: 1
        kind: true
        metadata:
          name: 1
          labels: scalar
          annotations: {number: 1, boolean: true}
    "};
    sim_assert_eq!(have: manifest_chunk_rejection(accepted), want: Some(false));
    let refused = indoc! {"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
          annotations: scalar
    "};
    sim_assert_eq!(have: manifest_chunk_rejection(refused), want: Some(true));
}

#[test]
fn head_decoding_keeps_distinct_yaml_keys_ambiguous_after_json_coercion() {
    for entries in ["true: ok, 'true': [1]", "'true': [1], true: ok"] {
        let chunk = format!("metadata: {{annotations: {{{entries}}}}}");
        sim_assert_eq!(have: manifest_chunk_rejection(&chunk), want: None);
    }
}

/// The YAML decoder resolves exact duplicate keys before JSON conversion begins.
#[test]
fn head_decoding_uses_the_completed_mapping_after_duplicate_keys() {
    for chunk in [
        indoc! {"
            metadata: {name: [1]}
            metadata: {name: probe}
        "},
        "metadata: {annotations: {true: [1], true: ok}}",
    ] {
        sim_assert_eq!(have: manifest_chunk_rejection(chunk), want: Some(false));
    }
}
