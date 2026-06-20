use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use helm_schema_core::{ResourceRef, YamlPath};

use super::provider_result::ProviderLookupResult;
use super::trait_def::K8sSchemaProvider;

#[derive(Debug, Default)]
pub(crate) struct ProviderLookupCache {
    entries: Mutex<HashMap<ProviderLookupCacheKey, ProviderLookupResult>>,
}

impl ProviderLookupCache {
    pub(crate) fn lookup(
        &self,
        provider_index: usize,
        provider: &dyn K8sSchemaProvider,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> ProviderLookupResult {
        let key = ProviderLookupCacheKey::new(provider_index, resource, path);

        if let Some(cached) = self.entries().get(&key) {
            return cached.clone();
        }

        let result = provider.lookup(resource, path);
        self.entries().insert(key, result.clone());
        result
    }

    fn entries(&self) -> MutexGuard<'_, HashMap<ProviderLookupCacheKey, ProviderLookupResult>> {
        match self.entries.lock() {
            Ok(guard) => guard,
            Err(err) => err.into_inner(),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ProviderLookupCacheKey {
    provider_index: usize,
    api_version: String,
    kind: String,
    path: Vec<String>,
}

impl ProviderLookupCacheKey {
    fn new(provider_index: usize, resource: &ResourceRef, path: &YamlPath) -> Self {
        Self {
            provider_index,
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
            path: path.0.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
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
        fn schema_fragment_for_resource_path(
            &self,
            _resource: &ResourceRef,
            _path: &YamlPath,
        ) -> Option<ProviderSchemaFragment> {
            None
        }

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
        let resource = ResourceRef {
            api_version: "v1".to_string(),
            kind: "ConfigMap".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        };
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
}
