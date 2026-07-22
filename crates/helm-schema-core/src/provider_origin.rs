use serde::{Deserialize, Serialize};

/// Identifies which provider produced a lookup result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderOrigin {
    /// User-configured local schema file.
    LocalOverride,
    /// `CRD` schema declared by the analyzed chart.
    ChartLocalCrd,
    /// Default or mirrored CRD catalog.
    DefaultCatalog,
    /// Versioned `Kubernetes OpenAPI` schema source.
    KubernetesOpenApi,
}
