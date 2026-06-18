use helm_schema_engine::{
    contract::{
        ContractDocument, ContractDocumentGuard, ContractDocumentProvenance, ContractDocumentSpan,
        ContractDocumentUse, ContractProjection,
    },
    helpers::extract_helper_calls,
    parse::extract_values_yaml_descriptions,
};
use indoc::indoc;

#[test]
fn public_engine_surface_exposes_named_parse_helper_and_contract_modules() {
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

    assert_eq!(
        extract_helper_calls(r#"{{ include "common.fullname" . }}"#),
        vec!["common.fullname".to_string()]
    );

    let _ = std::any::type_name::<ContractProjection>();
    let _ = std::any::type_name::<ContractDocument>();
    let _ = std::any::type_name::<ContractDocumentGuard>();
    let _ = std::any::type_name::<ContractDocumentUse>();
    let _ = std::any::type_name::<ContractDocumentProvenance>();
    let _ = std::any::type_name::<ContractDocumentSpan>();
}
