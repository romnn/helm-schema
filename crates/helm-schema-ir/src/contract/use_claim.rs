use crate::{ContractProvenance, Guard, ResourceRef, ValueKind, YamlPath};

/// A contract claim for one observed values path.
///
/// This is the semantic contract claim shape owned by the contract layer.
/// [`ContractDocumentUse`] is the serialized inspection DTO at the export
/// boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub guards: Vec<Guard>,
    pub resource: Option<ResourceRef>,
    pub provenance: Vec<ContractProvenance>,
}

impl ContractUse {
    pub(crate) fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self::with_provenance(source_expr, path, kind, guards, resource, None)
    }

    pub(crate) fn with_provenance(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
        provenance: Option<ContractProvenance>,
    ) -> Self {
        Self::with_provenances(source_expr, path, kind, guards, resource, provenance)
    }

    pub(crate) fn with_provenances(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            guards,
            resource,
            provenance: provenance.into_iter().collect(),
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
