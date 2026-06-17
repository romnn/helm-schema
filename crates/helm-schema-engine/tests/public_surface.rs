use helm_schema_engine::{
    compatibility::{
        ContractDocumentV1, ContractDocumentV2, ContractProjection, ContractProvenanceV2,
        ContractUseV2, SourceSpanV2, ValueUse,
    },
    helpers::extract_helper_calls,
    parse::extract_values_yaml_descriptions,
    required_inference::extract_default_fallback_paths,
};
use indoc::indoc;

#[test]
fn public_engine_surface_exposes_named_parse_helper_and_compatibility_modules() {
    let values_yaml = indoc! {"
        # Root flag docs
        enabled: true # inline flag docs

        # -- Parent docs
        parent:
          # -- Child docs line 1
          # Child docs line 2
          child: value
    "};
    let descriptions = extract_values_yaml_descriptions(values_yaml).expect("extract comments");
    assert_eq!(
        descriptions.get("enabled").map(String::as_str),
        Some("Root flag docs\ninline flag docs")
    );
    assert_eq!(
        descriptions.get("parent").map(String::as_str),
        Some("Parent docs")
    );
    assert_eq!(
        descriptions.get("parent.child").map(String::as_str),
        Some("Child docs line 1\nChild docs line 2")
    );

    let template = r#"{{ .Values.serviceAccount.name | default "generated-name" }}"#;
    let fallback_paths = extract_default_fallback_paths(template);
    assert_eq!(fallback_paths, ["serviceAccount.name".to_string()]);

    assert_eq!(
        extract_helper_calls(r#"{{ include "common.fullname" . }}"#),
        vec!["common.fullname".to_string()]
    );

    let _ = std::any::type_name::<ContractProjection>();
    let _ = std::any::type_name::<ContractDocumentV1>();
    let _ = std::any::type_name::<ContractDocumentV2>();
    let _ = std::any::type_name::<ContractUseV2>();
    let _ = std::any::type_name::<ContractProvenanceV2>();
    let _ = std::any::type_name::<SourceSpanV2>();
    let _ = std::any::type_name::<ValueUse>();
}
