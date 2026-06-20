use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::inference::{self, ApiVersionInferenceOutcome};

use super::trait_def::K8sSchemaProvider;

#[derive(Debug, Default)]
pub(crate) struct ApiVersionInferenceCache {
    entries: Mutex<HashMap<String, ApiVersionInferenceOutcome>>,
}

impl ApiVersionInferenceCache {
    pub(crate) fn infer(
        &self,
        providers: &[Box<dyn K8sSchemaProvider>],
        kind: &str,
    ) -> ApiVersionInferenceOutcome {
        if let Some(cached) = self.entries().get(kind) {
            return cached.clone();
        }

        let inferred = inference::infer_api_version(providers, kind);
        self.entries().insert(kind.to_string(), inferred.clone());
        inferred
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<String, ApiVersionInferenceOutcome>> {
        match self.entries.lock() {
            Ok(guard) => guard,
            Err(err) => err.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use test_util::prelude::sim_assert_eq;

    use helm_schema_core::{ResourceRef, YamlPath};

    use super::*;
    use crate::inference::{ApiVersionCandidate, InferenceSource};
    use crate::lookup::ProviderOrigin;

    #[derive(Debug)]
    struct CountingInferenceProvider {
        calls: Arc<AtomicUsize>,
    }

    impl K8sSchemaProvider for CountingInferenceProvider {
        fn schema_fragment_for_resource_path(
            &self,
            _resource: &ResourceRef,
            _path: &YamlPath,
        ) -> Option<crate::lookup::ProviderSchemaFragment> {
            None
        }

        fn origin(&self) -> ProviderOrigin {
            ProviderOrigin::KubernetesOpenApi
        }

        fn has_resource(&self, _resource: &ResourceRef) -> bool {
            false
        }

        fn infer_api_version_candidates(&self, _kind: &str) -> Vec<ApiVersionCandidate> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            vec![ApiVersionCandidate {
                api_version: "example.com/v1".to_string(),
                source: InferenceSource::OnlineProbe,
                origin: ProviderOrigin::KubernetesOpenApi,
            }]
        }
    }

    #[test]
    fn repeated_inference_uses_cached_result() {
        let cache = ApiVersionInferenceCache::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = Box::new(CountingInferenceProvider {
            calls: Arc::clone(&calls),
        });
        let providers: Vec<Box<dyn K8sSchemaProvider>> = vec![provider];

        let first = cache.infer(&providers, "Widget");
        let second = cache.infer(&providers, "Widget");

        assert!(matches!(
            (first, second),
            (
                ApiVersionInferenceOutcome::Resolved { .. },
                ApiVersionInferenceOutcome::Resolved { .. },
            )
        ));
        sim_assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
