use std::collections::BTreeSet;

use crate::contract::ContractUse;

// `ContractFact` is a transient by-value fact, never stored en masse, so the
// size gap between `Use` and `TypeHint` does not justify boxing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ContractFact {
    Use(ContractUse),
    TypeHint(ContractTypeHint),
}

impl ContractFact {
    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        match self {
            Self::Use(contract_use) => contract_use.map_value_paths(map),
            Self::TypeHint(type_hint) => type_hint.map_value_paths(map),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ContractTypeHint {
    pub(crate) value_path: String,
    pub(crate) schema_types: BTreeSet<String>,
}

impl ContractTypeHint {
    pub(crate) fn new(
        value_path: impl Into<String>,
        schema_types: impl IntoIterator<Item = String>,
    ) -> Option<Self> {
        let value_path = value_path.into();
        if value_path.trim().is_empty() {
            return None;
        }

        let schema_types = schema_types
            .into_iter()
            .filter(|schema_type| !schema_type.trim().is_empty())
            .collect::<BTreeSet<_>>();
        if schema_types.is_empty() {
            return None;
        }

        Some(Self {
            value_path,
            schema_types,
        })
    }

    pub(crate) fn map_value_paths<F>(&mut self, map: &mut F)
    where
        F: FnMut(&str) -> String,
    {
        self.value_path = map(&self.value_path);
    }
}
