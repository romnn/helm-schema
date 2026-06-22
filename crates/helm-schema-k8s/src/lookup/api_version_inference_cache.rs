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
#[path = "tests/api_version_inference_cache.rs"]
mod tests;
