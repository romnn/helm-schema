use std::collections::BTreeMap;
use test_util::prelude::sim_assert_eq;

use indoc::indoc;

use super::extract_values_yaml_descriptions;

#[test]
fn extracts_leading_inline_and_nested_values_comments() {
    let yaml = indoc! {"
        # Root flag docs
        enabled: true # inline flag docs

        # -- Parent docs
        parent:
          # -- Child docs line 1
          # Child docs line 2
          child: value
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    let expected = BTreeMap::from([
        (
            "enabled".to_string(),
            "Root flag docs\ninline flag docs".to_string(),
        ),
        ("parent".to_string(), "Parent docs".to_string()),
        (
            "parent.child".to_string(),
            "Child docs line 1\nChild docs line 2".to_string(),
        ),
    ]);
    sim_assert_eq!(have: descriptions, want: expected);
}

#[test]
fn comments_inside_parent_pair_document_first_nested_key() {
    let yaml = indoc! {"
        global:
          # -- Image registry docs
          imageRegistry: null
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    sim_assert_eq!(
        have: descriptions.get("global.imageRegistry").map(String::as_str),
        want: Some("Image registry docs")
    );
    assert!(
        !descriptions.contains_key("global"),
        "child comment must not attach to the parent object"
    );
}

#[test]
fn commented_out_values_do_not_become_description_paths() {
    let yaml = indoc! {"
        config:
          # -- Disabled value docs
          # disabled: true
          enabled: true
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    assert!(!descriptions.contains_key("config.disabled"));
    sim_assert_eq!(
        have: descriptions.get("config.enabled").map(String::as_str),
        want: Some("Disabled value docs\ndisabled: true")
    );
}

#[test]
fn forward_description_without_value_does_not_attach_to_previous_key() {
    let yaml = indoc! {"
        # -- Root enabled docs
        enabled: true
        # -- Comment-only docs
        # commentedOnly: true
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    sim_assert_eq!(
        have: descriptions.get("enabled").map(String::as_str),
        want: Some("Root enabled docs")
    );
    assert!(!descriptions.contains_key("commentedOnly"));
}

#[test]
fn helm_docs_marker_splits_previous_examples_from_next_description() {
    let yaml = indoc! {"
        ingress:
          # -- Ingress annotations
          annotations: {}
          # nginx.ingress.kubernetes.io/rewrite-target: /
          # cert-manager.io/cluster-issuer: letsencrypt-prod
          # -- Ingress hosts
          hosts: []
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    sim_assert_eq!(
        have: descriptions.get("ingress.annotations").map(String::as_str),
        want: Some(
            "Ingress annotations\nnginx.ingress.kubernetes.io/rewrite-target: /\ncert-manager.io/cluster-issuer: letsencrypt-prod"
        )
    );
    sim_assert_eq!(
        have: descriptions.get("ingress.hosts").map(String::as_str),
        want: Some("Ingress hosts")
    );
}

#[test]
fn trailing_examples_at_end_of_mapping_document_previous_key() {
    let yaml = indoc! {"
        ingress:
          # -- Ingress annotations
          annotations: {}
          # nginx.ingress.kubernetes.io/rewrite-target: /

        detached: true

        # This detached comment is separated from the key above.
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    sim_assert_eq!(
        have: descriptions.get("ingress.annotations").map(String::as_str),
        want: Some("Ingress annotations\nnginx.ingress.kubernetes.io/rewrite-target: /")
    );
    sim_assert_eq!(have: descriptions.get("detached").map(String::as_str), want: None);
}

#[test]
fn decorative_comment_headings_do_not_become_descriptions() {
    let yaml = indoc! {"
        section:
          ###
          ### ---- OPERATOR ----
          ###
          operator:
            # -- Operator image docs
            image: example
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    assert!(!descriptions.contains_key("section.operator"));
    sim_assert_eq!(
        have: descriptions
            .get("section.operator.image")
            .map(String::as_str),
        want: Some("Operator image docs")
    );
}

#[test]
fn helm_docs_param_comments_attach_to_explicit_paths() {
    let yaml = indoc! {"
        ## @section Global parameters
        ## Global Docker image parameters
        ##
        ## @param global.imageRegistry Global Docker image registry
        ## @param global.imagePullSecrets Global Docker registry secret names as an array
        global:
          imageRegistry: \"\"
          imagePullSecrets: []

        auth:
          ## @param auth.enabled Enable password authentication
          ##
          enabled: true
    "};

    let descriptions = extract_values_yaml_descriptions(yaml);

    sim_assert_eq!(
        have: descriptions.get("global.imageRegistry").map(String::as_str),
        want: Some("Global Docker image registry")
    );
    sim_assert_eq!(
        have: descriptions
            .get("global.imagePullSecrets")
            .map(String::as_str),
        want: Some("Global Docker registry secret names as an array")
    );
    sim_assert_eq!(
        have: descriptions.get("auth.enabled").map(String::as_str),
        want: Some("Enable password authentication")
    );
    assert!(
        !descriptions.contains_key("global"),
        "helm-docs section comments must not attach to the parent object"
    );
}
