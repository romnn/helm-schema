macro_rules! impl_resource_schema_oracle_via_k8s_provider {
    ($ty:ty) => {
        impl helm_schema_core::ResourceSchemaOracle for $ty {
            fn schema_fragment_for_use(
                &self,
                use_: &helm_schema_core::ProviderSchemaUse,
            ) -> Option<helm_schema_core::ProviderSchemaFragment> {
                <Self as crate::lookup::K8sSchemaProvider>::schema_fragment_for_use(self, use_)
            }

            fn schema_fragment_for_resource_path(
                &self,
                resource: &helm_schema_core::ResourceRef,
                path: &helm_schema_core::YamlPath,
            ) -> Option<helm_schema_core::ProviderSchemaFragment> {
                <Self as crate::lookup::K8sSchemaProvider>::schema_fragment_for_resource_path(
                    self, resource, path,
                )
            }
        }
    };
}

pub(crate) use impl_resource_schema_oracle_via_k8s_provider;

mod api_presence_executor;
mod api_version_inference_cache;
mod chain;
mod chain_outcome;
mod miss_diagnostics;
mod provider_lookup_cache;
mod provider_origin;
mod provider_result;
mod provider_schema_fragment;
mod resource_lookup_executor;
mod resource_lookup_plan;
pub(crate) mod source_bundle;
mod trace;
mod trait_def;

pub use chain::Chain;
pub use chain_outcome::ChainLookupOutcome;
pub use helm_schema_core::ApiPresenceQuery;
pub use provider_origin::ProviderOrigin;
pub use provider_result::ProviderLookupResult;
pub use provider_schema_fragment::{
    ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment,
};
pub use trace::{
    LookupTrace, LookupTraceEntry, LookupTraceOutcome, LookupTraceSubject, SourceProbeTraceOutcome,
    TracedApiPresenceOutcome, TracedLookupOutcome,
};
pub use trait_def::K8sSchemaProvider;
