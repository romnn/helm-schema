use serde::{Deserialize, Serialize};

/// Identifies which provider produced a lookup result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProviderOrigin {
    LocalOverride,
    ChartLocalCrd,
    DefaultCatalog,
    KubernetesOpenApi,
}
