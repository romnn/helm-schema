use crate::contract::ContractUse;
pub use helm_schema_core::ProviderSchemaUse;

/// Contract fact that needs a Kubernetes resource schema lookup.
///
/// This is narrower than [`ContractUse`]: schema providers need only the
/// rendered resource/path target, while generator policy also needs the input
/// values path and value-kind domain.
#[must_use]
pub fn from_contract_use(contract_use: &ContractUse) -> Option<ProviderSchemaUse> {
    if contract_use.source_expr.trim().is_empty()
        || contract_use.kind == helm_schema_core::ValueKind::PartialScalar
        || contract_use.path.0.is_empty()
    {
        return None;
    }
    let resource = contract_use.resource.clone()?;

    Some(ProviderSchemaUse {
        value_path: contract_use.source_expr.clone(),
        path: contract_use.path.clone(),
        kind: contract_use.kind,
        resource,
        is_self_range_collection: use_is_self_range_collection(contract_use),
    })
}

fn use_is_self_range_collection(use_: &ContractUse) -> bool {
    use_.has_self_range_guard()
        && use_
            .path
            .0
            .last()
            .is_none_or(|segment| !segment.ends_with("[*]"))
}
