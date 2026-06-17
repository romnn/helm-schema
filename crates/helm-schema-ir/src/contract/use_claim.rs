use crate::{Guard, ResourceRef, ValueKind, ValueUse, YamlPath};

/// A contract claim for one observed values path.
///
/// This is still the migration-era claim shape, but it is owned by the
/// contract layer. [`ValueUse`] remains the serialized fixture DTO at the
/// inspection boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<Guard>,
    pub resource: Option<ResourceRef>,
}

impl ContractUse {
    pub(crate) fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
        }
    }

    pub(super) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.source_expr = map(&self.source_expr);
        self.guards = std::mem::take(&mut self.guards)
            .into_iter()
            .map(|guard| guard.map_value_paths(map))
            .collect();
    }
}

impl From<ContractUse> for ValueUse {
    fn from(contract_use: ContractUse) -> Self {
        Self {
            source_expr: contract_use.source_expr,
            path: contract_use.path,
            kind: contract_use.kind,
            guards: contract_use.guards,
            resource: contract_use.resource,
        }
    }
}
