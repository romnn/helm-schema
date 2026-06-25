use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema, lookup_root_metadata_path,
};
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaSource,
};

use super::LocalSchemaUniverse;
use super::universe::LocalSchemaDocument;

#[derive(Debug)]
pub struct ChartLocalCrdSchemaProvider {
    universe: LocalSchemaUniverse,
    allow_api_version_guess: bool,
}

impl ChartLocalCrdSchemaProvider {
    #[must_use]
    pub fn new(universe: LocalSchemaUniverse) -> Self {
        Self {
            universe,
            allow_api_version_guess: false,
        }
    }

    #[must_use]
    pub fn with_api_version_guess(mut self, enabled: bool) -> Self {
        self.allow_api_version_guess = enabled;
        self
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.universe.is_empty()
    }

    fn source_for_leaf(
        &self,
        document: &LocalSchemaDocument,
        leaf: &LocalSchemaLeaf,
    ) -> Option<ProviderSchemaSource> {
        leaf.pointer().map(|pointer| {
            ProviderSchemaSource::new(
                ProviderOrigin::ChartLocalCrd,
                document.source_id().to_string(),
                None,
                document.filename().to_string(),
                pointer.to_string(),
            )
        })
    }
}

impl K8sSchemaProvider for ChartLocalCrdSchemaProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::ChartLocalCrd
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        let Some(document) = self.universe.schema_document_for_resource(resource) else {
            return ProviderLookupResult::NotOwned;
        };

        lookup_root_metadata_path(document.schema_doc(), path, |leaf| {
            self.source_for_leaf(document, leaf)
        })
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.universe.schema_doc_for_resource(resource).is_some()
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }

        self.universe
            .resource_keys()
            .filter(|key| key.kind() == kind)
            .map(|key| ApiVersionCandidate {
                api_version: key.api_version().to_string(),
                source: InferenceSource::ChartLocalCrd,
                origin: ProviderOrigin::ChartLocalCrd,
            })
            .collect()
    }
}

/// Expand the full chart-local CRD document for regression tests and debugging.
///
/// Production provider lookup stays on the fragment-first path.
#[must_use]
pub fn debug_materialize_schema_for_resource(
    provider: &ChartLocalCrdSchemaProvider,
    resource: &ResourceRef,
) -> Option<Value> {
    let root = provider.universe.schema_doc_for_resource(resource)?;
    Some(debug_materialize_local_schema(root.root()))
}

#[cfg(test)]
#[path = "tests/provider.rs"]
mod tests;
