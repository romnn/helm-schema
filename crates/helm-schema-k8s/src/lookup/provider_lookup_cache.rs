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
#[path = "tests/provider_lookup_cache.rs"]
mod tests;
