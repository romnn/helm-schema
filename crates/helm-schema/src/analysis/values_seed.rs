use helm_schema_engine::ContractIr;

use crate::values_roots;

pub(super) fn seed_top_level_values_yaml_keys(
    contract: &mut ContractIr,
    values_yaml: Option<&str>,
) {
    for path in values_roots::top_level_value_paths(values_yaml) {
        contract.push_pathless_scalar(path);
    }
}
