mod crd_catalog;
mod upstream;

pub use crd_catalog::CrdCatalogSchemaProvider;
pub use upstream::UpstreamK8sSchemaProvider;

use helm_schema_ir::{ResourceRef, ValueUse, YamlPath};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Provides JSON Schema fragments for Kubernetes resource fields.
///
/// Given a [`ValueUse`] (which carries resource type + YAML path), returns the
/// JSON Schema for that field in the upstream K8s API, if known.
pub trait K8sSchemaProvider {
    /// Schema for a specific value use (resource + YAML path).
    fn schema_for_use(&self, u: &ValueUse) -> Option<Value> {
        let r = u.resource.as_ref()?;
        self.schema_for_resource_path(r, &u.path)
    }

    /// Schema for a specific resource type + YAML path.
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value>;
}

pub struct ChainSchemaProvider<A, B> {
    pub first: A,
    pub second: B,
}

impl<A, B> K8sSchemaProvider for ChainSchemaProvider<A, B>
where
    A: K8sSchemaProvider,
    B: K8sSchemaProvider,
{
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.first
            .schema_for_resource_path(resource, path)
            .or_else(|| self.second.schema_for_resource_path(resource, path))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[must_use]
pub fn type_schema(ty: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("type".to_string(), Value::String(ty.to_string()));
    Value::Object(m)
}

#[must_use]
pub fn candidate_filenames_for_resource(resource: &ResourceRef) -> Vec<String> {
    let kind = resource.kind.to_ascii_lowercase();
    let (group, version) = match resource.api_version.split_once('/') {
        Some((g, v)) => (g.to_ascii_lowercase(), v.to_ascii_lowercase()),
        None => (String::new(), resource.api_version.to_ascii_lowercase()),
    };

    let mut out = Vec::new();
    if group.is_empty() {
        out.push(format!("{kind}-{version}.json"));
        return out;
    }

    let dashed_full_group = group.replace('.', "-");
    let group_prefix = group.split('.').next().unwrap_or(&group).to_string();

    if group.ends_with(".k8s.io") {
        out.push(format!("{kind}-{group_prefix}-{version}.json"));
    }

    out.push(format!("{kind}-{dashed_full_group}-{version}.json"));

    if !group.ends_with(".k8s.io") {
        out.push(format!("{kind}-{group_prefix}-{version}.json"));
    }

    out
}

#[must_use]
pub fn filename_for_resource(resource: &ResourceRef) -> String {
    candidate_filenames_for_resource(resource)
        .into_iter()
        .next()
        .unwrap_or_else(|| "unknown.json".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_schema_ir::ResourceRef;

    #[test]
    fn filename_for_core_resource() {
        let r = ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
        };
        assert_eq!(filename_for_resource(&r), "service-v1.json");
    }

    #[test]
    fn filename_for_grouped_resource() {
        let r = ResourceRef {
            api_version: "monitoring.coreos.com/v1".to_string(),
            kind: "PrometheusRule".to_string(),
        };
        assert_eq!(
            filename_for_resource(&r),
            "prometheusrule-monitoring-coreos-com-v1.json"
        );
    }

    #[test]
    fn filename_for_k8s_io_group_prefers_group_prefix() {
        let r = ResourceRef {
            api_version: "networking.k8s.io/v1".to_string(),
            kind: "NetworkPolicy".to_string(),
        };
        assert_eq!(
            filename_for_resource(&r),
            "networkpolicy-networking-v1.json"
        );
    }
}
