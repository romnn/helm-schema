use std::collections::BTreeSet;

use helm_schema_ir::ContractIr;

use crate::values_roots::ValuesRoots;

pub(super) fn seed_top_level_values_yaml_keys(
    contract: &mut ContractIr,
    values_roots: &ValuesRoots,
    dependency_root_paths: &BTreeSet<String>,
) {
    for path in &values_roots.top_level_paths {
        if values_roots.top_level_mapping_paths.contains(path)
            && dependency_root_paths.contains(path)
        {
            contract.push_pathless_dependency_fragment(path.clone());
        } else {
            contract.push_pathless_scalar(path.clone());
        }
    }
}
