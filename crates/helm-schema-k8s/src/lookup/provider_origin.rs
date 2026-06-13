use serde::{Deserialize, Serialize};

/// Identifies which provider produced a lookup result. Used by the
/// chain layer to apply origin-specific rules (e.g. hard-fail vs
/// fall-through on `ResourceDocMissing`) and by diagnostics to point
/// the user at the right layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderOrigin {
    /// `LocalSchemaProvider` — hand-maintained CRD overrides under
    /// `--crd-override-dir`.
    LocalOverride,
    /// `ChartLocalCrdSchemaProvider` — static CRDs bundled under a chart's
    /// `crds/` directory.
    ChartLocalCrd,
    /// `CrdsCatalogSchemaProvider` — default datreeio catalog plus any
    /// `--crd-catalog-mirror` URLs.
    DefaultCatalog,
    /// `KubernetesJsonSchemaProvider` — upstream yannh K8s OpenAPI
    /// schemas plus any `--k8s-schema-mirror` URLs.
    KubernetesOpenApi,
}
