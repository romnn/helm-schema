use serde::{Deserialize, Serialize};

use crate::{ContractProvenance, Guard, GuardDnf, ResourceRef, ValueKind, YamlPath};

/// A contract claim for one observed values path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ContractUse {
    pub source_expr: String,
    pub path: YamlPath,
    pub kind: ValueKind,
    pub condition: GuardDnf,
    pub resource: Option<ResourceRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provenance: Vec<ContractProvenance>,
}

impl<'de> Deserialize<'de> for ContractUse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireContractUse {
            source_expr: String,
            path: YamlPath,
            kind: ValueKind,
            condition: GuardDnf,
            resource: Option<ResourceRef>,
            #[serde(default)]
            provenance: Vec<ContractProvenance>,
        }

        let wire = WireContractUse::deserialize(deserializer)?;
        Ok(Self {
            source_expr: wire.source_expr,
            path: wire.path,
            kind: wire.kind,
            condition: wire.condition,
            resource: wire.resource,
            provenance: wire.provenance,
        })
    }
}

impl ContractUse {
    pub fn new(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
    ) -> Self {
        Self::with_provenances(source_expr, path, kind, guards, resource, None)
    }

    pub fn with_provenances(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        guards: Vec<Guard>,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        let condition = GuardDnf::from_guards(guards.iter().cloned());
        Self::with_condition_and_provenances(
            source_expr,
            path,
            kind,
            condition,
            resource,
            provenance,
        )
    }

    pub fn with_condition_and_provenances(
        source_expr: String,
        path: YamlPath,
        kind: ValueKind,
        condition: GuardDnf,
        resource: Option<ResourceRef>,
        provenance: impl IntoIterator<Item = ContractProvenance>,
    ) -> Self {
        Self {
            source_expr,
            path,
            kind,
            condition,
            resource,
            provenance: provenance.into_iter().collect(),
        }
    }

    pub fn canonicalize(&mut self) {
        self.provenance.sort();
        self.provenance.dedup();
    }

    #[must_use]
    pub fn single_guard_conjunction(&self) -> Vec<Guard> {
        self.condition
            .single_guard_conjunction()
            .unwrap_or_default()
    }

    pub fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.source_expr = map(&self.source_expr);
        self.condition.map_value_paths(map);
    }
}
