use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;
use crate::inference::{ApiVersionCandidate, InferenceSource};

#[derive(Debug)]
struct CountingProvider {
    calls: Arc<AtomicUsize>,
}

impl K8sSchemaProvider for CountingProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::KubernetesOpenApi
    }

    fn has_resource(&self, _resource: &ResourceRef) -> bool {
        true
    }

    fn lookup(&self, _resource: &ResourceRef, _path: &YamlPath) -> ProviderLookupResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        ProviderLookupResult::Found {
            schema: ProviderSchemaFragment::new(json!({"type": "string"})),
            resolved_k8s_version: None,
        }
    }
}

#[test]
fn repeated_chain_lookup_uses_cached_provider_result() {
    let calls = Arc::new(AtomicUsize::new(0));
    let chain = Chain::new(vec![Box::new(CountingProvider {
        calls: Arc::clone(&calls),
    })]);
    let resource = ResourceRef::concrete("v1".to_string(), "ConfigMap".to_string());
    let path = YamlPath(vec!["metadata".to_string(), "name".to_string()]);

    let first = chain.resolve_against_chain(&resource, &path);
    let second = chain.resolve_against_chain(&resource, &path);

    assert!(matches!(
        (first, second),
        (
            ChainLookupOutcome::Resolved(Some(_)),
            ChainLookupOutcome::Resolved(Some(_)),
        )
    ));
    sim_assert_eq!(have: calls.load(Ordering::SeqCst), want: 1);
}

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
fn repeated_inference_uses_cached_outcome() {
    let calls = Arc::new(AtomicUsize::new(0));
    let chain = Chain::new(vec![Box::new(CountingInferenceProvider {
        calls: Arc::clone(&calls),
    })])
    .with_inference_enabled(true);
    let resource = ResourceRef::concrete(String::new(), "Widget".to_string());
    let path = YamlPath(Vec::new());

    let _ = chain.schema_fragment_for_resource_needing_inference(&resource, &path);
    let _ = chain.schema_fragment_for_resource_needing_inference(&resource, &path);

    sim_assert_eq!(have: calls.load(Ordering::SeqCst), want: 1);
}
