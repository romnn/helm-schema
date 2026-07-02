mod capability_probe;
mod provider;
pub(crate) mod resolve_ctx;
mod version_chain;

pub use provider::KubernetesJsonSchemaProvider;
pub use version_chain::K8sVersionChain;
