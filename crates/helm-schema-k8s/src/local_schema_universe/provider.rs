use helm_schema_core::{ResourceRef, ResourceSchemaOracle, YamlPath};
use serde_json::Value;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema,
    descend_schema_path_expanding_leaf_with_root_metadata_source, fragment_for_source_leaf,
};
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::schema_doc::SchemaDoc;

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

    fn schema_leaf_for_resource_path_from_doc(
        &self,
        root: &SchemaDoc,
        path: &YamlPath,
    ) -> Option<LocalSchemaLeaf> {
        descend_schema_path_expanding_leaf_with_root_metadata_source(root.root(), &path.0)
    }

    fn fragment_for_leaf(
        &self,
        document: &LocalSchemaDocument,
        leaf: LocalSchemaLeaf,
    ) -> ProviderSchemaFragment {
        let source = leaf.pointer().map(|pointer| {
            ProviderSchemaSource::new(
                ProviderOrigin::ChartLocalCrd,
                document.source_id().to_string(),
                None,
                document.filename().to_string(),
                pointer.to_string(),
            )
        });
        fragment_for_source_leaf(document.schema_doc(), source, leaf)
    }
}

impl ResourceSchemaOracle for ChartLocalCrdSchemaProvider {
    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let document = self.universe.schema_document_for_resource(resource)?;
        self.schema_leaf_for_resource_path_from_doc(document.schema_doc(), path)
            .map(|leaf| self.fragment_for_leaf(document, leaf))
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

        match self.schema_leaf_for_resource_path_from_doc(document.schema_doc(), path) {
            Some(leaf) => ProviderLookupResult::Found {
                schema: self.fragment_for_leaf(document, leaf),
                resolved_k8s_version: None,
            },
            None => ProviderLookupResult::PathUnresolved,
        }
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
