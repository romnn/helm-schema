use helm_schema_core::{ResourceRef, YamlPath};

use crate::doc_backed_schema::{LocalSchemaLeaf, lookup_root_metadata_path};
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaSource,
};

use super::LocalSchemaUniverse;
use super::universe::LocalSchemaDocument;

/// Provider backed by CRD schemas declared directly in the analyzed chart.
#[derive(Debug)]
pub struct ChartLocalCrdSchemaProvider {
    universe: LocalSchemaUniverse,
    allow_api_version_guess: bool,
}

impl ChartLocalCrdSchemaProvider {
    /// Creates a provider over a precomputed chart-local schema universe.
    #[must_use]
    pub fn new(universe: LocalSchemaUniverse) -> Self {
        Self {
            universe,
            allow_api_version_guess: false,
        }
    }

    /// Enables or disables API-version inference from chart-local CRDs.
    #[must_use]
    pub fn with_api_version_guess(mut self, enabled: bool) -> Self {
        self.allow_api_version_guess = enabled;
        self
    }

    /// Reports whether the provider has no chart-local schemas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.universe.is_empty()
    }

    fn source_for_leaf(
        document: &LocalSchemaDocument,
        leaf: &LocalSchemaLeaf,
    ) -> Option<ProviderSchemaSource> {
        leaf.pointer().map(|pointer| {
            ProviderSchemaSource::new(
                ProviderOrigin::ChartLocalCrd,
                document.source_id.clone(),
                None,
                document.filename.clone(),
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

        lookup_root_metadata_path(&document.doc, path, |leaf| {
            Self::source_for_leaf(document, leaf)
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
            .filter(|key| key.kind == kind)
            .map(|key| ApiVersionCandidate {
                api_version: key.api_version.clone(),
                source: InferenceSource::ChartLocalCrd,
                origin: ProviderOrigin::ChartLocalCrd,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/provider.rs"]
mod tests;
