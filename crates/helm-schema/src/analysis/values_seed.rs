use std::collections::BTreeSet;

use helm_schema_ir::ContractIr;

pub(super) fn seed_top_level_values_yaml_keys(
    contract: &mut ContractIr,
    top_level_value_paths: &BTreeSet<String>,
    top_level_mapping_value_paths: &BTreeSet<String>,
    dependency_root_paths: &BTreeSet<String>,
) {
    for path in top_level_value_paths {
        if top_level_mapping_value_paths.contains(path) && dependency_root_paths.contains(path) {
            contract.push_pathless_dependency_fragment(path.clone());
        } else {
            contract.push_pathless_scalar(path.clone());
        }
    }
}
