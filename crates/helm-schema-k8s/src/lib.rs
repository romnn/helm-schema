//! Kubernetes / CRD schema providers.
//!
//! Composed from these cross-cutting modules:
//!   - [`fetch`]: HTTP boundary (`HttpFetcher` trait + `UreqFetcher` /
//!     `MockFetcher`).
//!   - [`cache`]: per-source layout, marker-based invalidation,
//!     negative cache.
//!   - [`diagnostic`]: typed `Diagnostic` enum + `DiagnosticSink`.
//!   - [`lookup`]: `K8sSchemaProvider` trait, `ProviderLookupResult`,
//!     `ChainLookupOutcome`, `Chain`.
//!   - [`inference`]: Feature D apiVersion guessing.
//!
//! The per-provider modules ([`kubernetes_openapi`], [`crds_catalog`],
//! [`local_override`], [`local_schema_universe`]) are slim composers of the above.

pub mod builtin_groups;
pub mod cache;
mod cache_write;
pub mod crds_catalog;
pub mod diagnostic;
mod doc_backed_schema;
pub mod fetch;
mod filename;
pub mod inference;
pub mod kubernetes_openapi;
pub mod local_override;
pub mod local_schema_universe;
pub mod lookup;
mod metadata_enrichment;
mod schema_doc;
mod source_cache;

pub use builtin_groups::is_k8s_builtin_group;
pub use cache::{
    CACHE_LAYOUT_VERSION, LAYOUT_MARKER_FILENAME, LayoutCheckOutcome, LayoutChecker, NegativeCache,
    default_source_id, source_id_for_url,
};
pub use crds_catalog::CrdsCatalogSchemaProvider;
pub use diagnostic::{
    Diagnostic, DiagnosticKey, DiagnosticSink, format_diagnostic_json, format_diagnostic_text,
};
pub use fetch::{FetchError, HttpFetcher, MockFetcher, MockResponse, UreqFetcher};
pub use filename::{
    candidate_filenames_for_resource, filename_for_resource, ordered_api_versions_for_resource,
};
pub use helm_schema_core::ResourceSchemaOracle;
pub use inference::{
    ApiVersionCandidate, ApiVersionInferenceOutcome, InferenceSource, infer_api_version,
};
pub use kubernetes_openapi::{
    K8sMirrorChain, K8sSource, K8sVersionChain, KubernetesJsonSchemaProvider,
};
pub use local_override::LocalSchemaProvider;
pub use local_schema_universe::{
    ChartLocalCrdSchemaProvider, LocalResourceSchema, LocalSchemaUniverse,
    resource_schemas_from_crd_document, resource_schemas_from_crd_document_with_source,
};
pub use lookup::{
    Chain, ChainLookupOutcome, K8sSchemaProvider, LookupTrace, LookupTraceEntry,
    LookupTraceOutcome, LookupTraceSubject, ProviderLookupResult, ProviderOrigin,
    ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment, SourceProbeTraceOutcome,
    TracedApiPresenceOutcome, TracedLookupOutcome,
};

// ---------------------------------------------------------------------------
// Convenience helpers
// ---------------------------------------------------------------------------

use serde_json::Value;

#[must_use]
pub fn type_schema(ty: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("type".to_string(), Value::String(ty.to_string()));
    Value::Object(m)
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
