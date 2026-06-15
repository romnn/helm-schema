use crate::contract::ContractUse;
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

/// Contract fact that needs a Kubernetes resource schema lookup.
///
/// This is narrower than [`ContractUse`]: schema providers need only the
/// rendered resource/path target, while generator policy also needs the input
/// values path and value-kind domain.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderSchemaUse {
    pub value_path: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub resource: ResourceRef,
    pub is_self_range_collection: bool,
}

impl ProviderSchemaUse {
    #[must_use]
    pub fn from_contract_use(contract_use: &ContractUse) -> Option<Self> {
        if contract_use.source_expr.trim().is_empty()
            || contract_use.kind == ValueKind::PartialScalar
            || contract_use.path.0.is_empty()
        {
            return None;
        }
        let resource = contract_use.resource.clone()?;

        Some(Self {
            value_path: contract_use.source_expr.clone(),
            path: contract_use.path.clone(),
            kind: contract_use.kind,
            resource,
            is_self_range_collection: use_is_self_range_collection(contract_use),
        })
    }
}

fn use_is_self_range_collection(use_: &ContractUse) -> bool {
    use_.guards
        .iter()
        .any(|guard| matches!(guard, Guard::Range { path } if path == &use_.source_expr))
        && use_
            .path
            .0
            .last()
            .is_none_or(|segment| !segment.ends_with("[*]"))
}
