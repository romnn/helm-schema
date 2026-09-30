use helm_schema_ir::{ContractIr, DependencyValuesRoot};

use crate::values_roots::ValuesRoots;

/// Makes every declared top-level key visible as a pathless claim.
///
/// Dependency instance roots are registered as dependency fragments by the
/// caller. Helm's `global` namespace is reserved by schema emission at every
/// chart values root; a declaration there is the chart's own default, not
/// evidence that the chart consumes the namespace.
pub(super) fn seed_top_level_values_yaml_keys(
    contract: &mut ContractIr,
    values_roots: &ValuesRoots,
    dependency_roots: &[DependencyValuesRoot],
) {
    for path in &values_roots.top_level_paths {
        let is_global = path
            .segments()
            .eq([&helm_schema_core::Segment::from("global")]);
        if is_global || dependency_roots.iter().any(|root| root.path == *path) {
            continue;
        }
        contract.push_pathless_scalar(path.clone());
    }
}
