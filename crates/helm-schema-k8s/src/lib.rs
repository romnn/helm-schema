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
mod tests {
    use super::*;
    use helm_schema_core::ResourceRef;
    use test_util::prelude::sim_assert_eq;

    #[test]
    fn filename_for_core_resource() {
        let r = ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        sim_assert_eq!(have: filename_for_resource(&r), want: "service-v1.json");
    }

    #[test]
    fn filename_for_grouped_resource() {
        let r = ResourceRef {
            api_version: "monitoring.coreos.com/v1".to_string(),
            kind: "PrometheusRule".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        sim_assert_eq!(
            have: filename_for_resource(&r),
            want: "prometheusrule-monitoring-coreos-com-v1.json"
        );
    }

    #[test]
    fn filename_for_k8s_io_group_prefers_group_prefix() {
        let r = ResourceRef {
            api_version: "networking.k8s.io/v1".to_string(),
            kind: "NetworkPolicy".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
        sim_assert_eq!(
            have: filename_for_resource(&r),
            want: "networkpolicy-networking-v1.json"
        );
    }
}
