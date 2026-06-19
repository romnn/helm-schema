use std::collections::BTreeSet;

use helm_schema_engine::ContractIr;

pub(super) fn seed_top_level_values_yaml_keys(
    contract: &mut ContractIr,
    top_level_value_paths: &BTreeSet<String>,
) {
    for path in top_level_value_paths {
        contract.push_pathless_scalar(path.clone());
    }
}
