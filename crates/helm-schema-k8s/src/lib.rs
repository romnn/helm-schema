mod crd_catalog;
mod upstream;

pub use crd_catalog::CrdCatalogSchemaProvider;
pub use upstream::UpstreamK8sSchemaProvider;

use helm_schema_ir::{ResourceRef, ValueUse, YamlPath};
use serde_json::Value;

fn api_version_rank(api_version: &str) -> (u8, u8, i32, i32) {
    // Lower is better.
    // 0 = stable, 1 = beta, 2 = alpha, 3 = unknown.
    // Prefer non-extensions API groups when all else is equal.
    // Prefer higher major versions, and higher beta/alpha iterations.
    let (group, ver) = api_version.split_once('/').unwrap_or(("", api_version));

    let is_extensions = u8::from(group == "extensions");

    let (stability, pre_iter) = if ver.contains("alpha") {
        let it = ver
            .split("alpha")
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        (2u8, it)
    } else if ver.contains("beta") {
        let it = ver
            .split("beta")
            .nth(1)
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(0);
        (1u8, it)
    } else if ver.starts_with('v')
        && ver[1..].chars().next().is_some_and(|c| c.is_ascii_digit())
        && ver[1..].chars().all(|c| c.is_ascii_digit())
    {
        (0u8, 0)
    } else {
        (3u8, 0)
    };

    let major = ver
        .strip_prefix('v')
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<i32>()
                .ok()
        })
        .unwrap_or(0);

    (stability, is_extensions, -major, -pre_iter)
}

fn ordered_api_versions_for_resource(r: &ResourceRef) -> Vec<&str> {
    let mut versions: Vec<&str> = Vec::new();
    if !r.api_version.trim().is_empty() {
        versions.push(r.api_version.as_str());
    }
    for v in &r.api_version_candidates {
        if !v.trim().is_empty() {
            versions.push(v.as_str());
        }
    }
    if versions.is_empty() {
        versions.push(r.api_version.as_str());
    }

    versions.sort_by_key(|v| api_version_rank(v));
    versions.dedup();
    versions
}

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

        for v in ordered_api_versions_for_resource(r) {
            let rr = ResourceRef {
                api_version: v.to_string(),
                kind: r.kind.clone(),
                api_version_candidates: Vec::new(),
            };
            if let Some(schema) = self.schema_for_resource_path(&rr, &u.path) {
                return Some(schema);
            }
        }

        None
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
            api_version_candidates: Vec::new(),
        };
        assert_eq!(filename_for_resource(&r), "service-v1.json");
    }

    #[test]
    fn filename_for_grouped_resource() {
        let r = ResourceRef {
            api_version: "monitoring.coreos.com/v1".to_string(),
            kind: "PrometheusRule".to_string(),
            api_version_candidates: Vec::new(),
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
            api_version_candidates: Vec::new(),
        };
        assert_eq!(
            filename_for_resource(&r),
            "networkpolicy-networking-v1.json"
        );
    }
}
