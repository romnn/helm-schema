use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

use helm_schema_core::{ResourceRef, YamlPath};

use super::*;
use crate::ProviderLookupResult;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::ProviderOrigin;

#[derive(Debug)]
struct CountingInferenceProvider {
    calls: Arc<AtomicUsize>,
}

impl K8sSchemaProvider for CountingInferenceProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    fn lookup(&self, _resource: &ResourceRef, _path: &YamlPath) -> ProviderLookupResult {
        ProviderLookupResult::NotOwned
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
    sim_assert_eq!(have: calls.load(Ordering::SeqCst), want: 1);
}
