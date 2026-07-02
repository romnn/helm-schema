use std::sync::atomic::{AtomicUsize, Ordering};
use test_util::prelude::sim_assert_eq;

use serde_json::json;

use super::*;
use crate::lookup::{ProviderOrigin, ProviderSchemaFragment};

#[derive(Debug)]
struct CountingProvider {
    calls: AtomicUsize,
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
fn repeated_provider_lookup_uses_cached_result() {
    let cache = ProviderLookupCache::default();
    let provider = CountingProvider {
        calls: AtomicUsize::new(0),
    };
    let resource = ResourceRef::concrete("v1".to_string(), "ConfigMap".to_string());
    let path = YamlPath(vec!["metadata".to_string(), "name".to_string()]);

    let first = cache.lookup(0, &provider, &resource, &path);
    let second = cache.lookup(0, &provider, &resource, &path);

    assert!(matches!(
        (first, second),
        (
            ProviderLookupResult::Found {
                resolved_k8s_version: None,
                ..
            },
            ProviderLookupResult::Found {
                resolved_k8s_version: None,
                ..
            },
        )
    ));
    sim_assert_eq!(have: provider.calls.load(Ordering::SeqCst), want: 1);
}
