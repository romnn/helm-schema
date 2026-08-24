use helm_schema_core::ResourceSchemaOracle;
use helm_schema_gen::{PreparedValuesDocuments, ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::ContractIr;
use serde_json::Value;

pub fn generate_schema_with_values_yaml(
    contract: ContractIr,
    provider: &dyn ResourceSchemaOracle,
    values_yaml: Option<&str>,
) -> Value {
    let schema_signals = contract.finalize().into_schema_signals();
    let composed = values_yaml
        .and_then(|source| serde_yaml::from_str(source).ok())
        .unwrap_or(serde_yaml::Value::Null);
    let documents =
        PreparedValuesDocuments::new(composed, serde_yaml::Value::Null, serde_yaml::Value::Null);
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, provider).with_values_documents(&documents),
    )
}
