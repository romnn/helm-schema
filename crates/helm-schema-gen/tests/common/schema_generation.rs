use helm_schema_core::ResourceSchemaOracle;
use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::ContractIr;
use serde_json::Value;

pub fn generate_schema_with_values_yaml(
    contract: ContractIr,
    provider: &dyn ResourceSchemaOracle,
    values_yaml: Option<&str>,
) -> Value {
    let schema_signals = contract.finalize().into_schema_signals();
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, provider).with_values_yaml(values_yaml),
    )
}
