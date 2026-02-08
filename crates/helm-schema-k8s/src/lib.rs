mod navigate;
mod upstream;

pub use navigate::{JsonSchemaOps, descend_path, strengthen_leaf_schema};
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

    /// Schema for a YAML path without a specific resource (heuristic fallback).
    fn schema_for_path(&self, path: &YamlPath) -> Option<Value>;
}

// ---------------------------------------------------------------------------
// Heuristic providers
// ---------------------------------------------------------------------------

/// Hardcoded schema hints for common Kubernetes fields.
pub struct CommonK8sSchemaProvider;

impl K8sSchemaProvider for CommonK8sSchemaProvider {
    fn schema_for_resource_path(&self, _resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.schema_for_path(path)
    }

    fn schema_for_path(&self, path: &YamlPath) -> Option<Value> {
        let pat = path_pattern(path);
        match pat.as_str() {
            "apiVersion" | "kind" => Some(type_schema("string")),
            "metadata.name" | "metadata.namespace" => Some(type_schema("string")),
            "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),

            // Service
            "spec.type" => Some(type_schema("string")),
            "spec.clusterIP" => Some(type_schema("string")),
            "spec.ports[*].name" => Some(type_schema("string")),
            "spec.ports[*].protocol" => Some(type_schema("string")),
            "spec.ports[*].port" => Some(type_schema("integer")),
            "spec.ports[*].targetPort" => Some(type_schema("integer")),
            "spec.ports[*].nodePort" => Some(type_schema("integer")),

            // Workloads
            "spec.replicas" => Some(type_schema("integer")),
            "spec.selector.matchLabels" => Some(string_map_schema()),
            "spec.template.metadata.annotations" | "spec.template.metadata.labels" => {
                Some(string_map_schema())
            }
            "spec.template.spec.serviceAccountName" => Some(type_schema("string")),
            "spec.template.spec.nodeSelector" => Some(string_map_schema()),

            // Tolerations
            "spec.template.spec.tolerations[*].key" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].operator" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].value" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].effect" => Some(type_schema("string")),
            "spec.template.spec.tolerations[*].tolerationSeconds" => Some(type_schema("integer")),

            // Container
            "spec.template.spec.containers[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].image" => Some(type_schema("string")),
            "spec.template.spec.containers[*].imagePullPolicy" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].protocol" => Some(type_schema("string")),
            "spec.template.spec.containers[*].ports[*].containerPort" => {
                Some(type_schema("integer"))
            }
            "spec.template.spec.containers[*].env[*].name" => Some(type_schema("string")),
            "spec.template.spec.containers[*].env[*].value" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.limits.cpu" => Some(type_schema("string")),
            "spec.template.spec.containers[*].resources.limits.memory" => {
                Some(type_schema("string"))
            }
            "spec.template.spec.containers[*].resources.requests.cpu" => {
                Some(type_schema("string"))
            }
            "spec.template.spec.containers[*].resources.requests.memory" => {
                Some(type_schema("string"))
            }

            _ => None,
        }
    }
}

/// Hardcoded schema hints for Ingress v1 resources.
pub struct IngressSchemaProvider;

impl K8sSchemaProvider for IngressSchemaProvider {
    fn schema_for_resource_path(&self, _resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.schema_for_path(path)
    }

    fn schema_for_path(&self, path: &YamlPath) -> Option<Value> {
        let pat = path_pattern(path);
        match pat.as_str() {
            "metadata.annotations" | "metadata.labels" => Some(string_map_schema()),
            "spec.ingressClassName" => Some(type_schema("string")),
            "spec.rules[*].host" => Some(type_schema("string")),
            "spec.tls[*].hosts[*]" => Some(type_schema("string")),
            "spec.tls[*].secretName" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].path" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].pathType" => {
                let mut obj = serde_json::Map::new();
                obj.insert("type".to_string(), Value::String("string".to_string()));
                obj.insert(
                    "enum".to_string(),
                    Value::Array(
                        ["ImplementationSpecific", "Exact", "Prefix"]
                            .into_iter()
                            .map(|s| Value::String(s.to_string()))
                            .collect(),
                    ),
                );
                Some(Value::Object(obj))
            }
            "spec.rules[*].http.paths[*].backend.service.name" => Some(type_schema("string")),
            "spec.rules[*].http.paths[*].backend.service.port.number" => {
                Some(type_schema("integer"))
            }
            _ => None,
        }
    }
}

/// Default fallback: tries Ingress hints, then common K8s hints.
pub struct DefaultK8sSchemaProvider;

impl K8sSchemaProvider for DefaultK8sSchemaProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        IngressSchemaProvider
            .schema_for_resource_path(resource, path)
            .or_else(|| CommonK8sSchemaProvider.schema_for_resource_path(resource, path))
    }

    fn schema_for_path(&self, path: &YamlPath) -> Option<Value> {
        IngressSchemaProvider
            .schema_for_path(path)
            .or_else(|| CommonK8sSchemaProvider.schema_for_path(path))
    }
}

/// Combines upstream (downloaded) schemas with heuristic fallback.
pub struct UpstreamThenDefaultProvider {
    pub upstream: UpstreamK8sSchemaProvider,
    pub fallback: DefaultK8sSchemaProvider,
}

impl Default for UpstreamThenDefaultProvider {
    fn default() -> Self {
        Self {
            upstream: UpstreamK8sSchemaProvider::new(DEFAULT_K8S_SCHEMA_VERSION_DIR),
            fallback: DefaultK8sSchemaProvider,
        }
    }
}

impl K8sSchemaProvider for UpstreamThenDefaultProvider {
    fn schema_for_use(&self, u: &ValueUse) -> Option<Value> {
        self.upstream
            .schema_for_use(u)
            .or_else(|| self.fallback.schema_for_use(u))
    }

    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        self.upstream
            .schema_for_resource_path(resource, path)
            .or_else(|| self.fallback.schema_for_resource_path(resource, path))
    }

    fn schema_for_path(&self, path: &YamlPath) -> Option<Value> {
        self.fallback.schema_for_path(path)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub const DEFAULT_K8S_SCHEMA_VERSION_DIR: &str = "v1.29.0-standalone-strict";

pub fn path_pattern(path: &YamlPath) -> String {
    path.0.join(".")
}

pub fn type_schema(ty: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("type".to_string(), Value::String(ty.to_string()));
    Value::Object(m)
}

pub fn string_map_schema() -> Value {
    let mut ap = serde_json::Map::new();
    ap.insert("type".to_string(), Value::String("string".to_string()));

    let mut m = serde_json::Map::new();
    m.insert("type".to_string(), Value::String("object".to_string()));
    m.insert("additionalProperties".to_string(), Value::Object(ap));
    Value::Object(m)
}

pub fn filename_for_resource(resource: &ResourceRef) -> String {
    let kind = resource.kind.to_ascii_lowercase();
    let (group, version) = match resource.api_version.split_once('/') {
        Some((g, v)) => (g.to_ascii_lowercase(), v.to_ascii_lowercase()),
        None => ("".to_string(), resource.api_version.to_ascii_lowercase()),
    };

    if group.is_empty() {
        format!("{}-{}.json", kind, version)
    } else {
        let group = group.replace('.', "-");
        format!("{}-{}-{}.json", kind, group, version)
    }
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
    fn common_schema_metadata_name() {
        let path = YamlPath(vec!["metadata".to_string(), "name".to_string()]);
        let schema = CommonK8sSchemaProvider.schema_for_path(&path);
        assert!(schema.is_some());
        assert_eq!(
            schema.unwrap().get("type").and_then(|t| t.as_str()),
            Some("string")
        );
    }

    #[test]
    fn common_schema_replicas() {
        let path = YamlPath(vec!["spec".to_string(), "replicas".to_string()]);
        let schema = CommonK8sSchemaProvider.schema_for_path(&path);
        assert!(schema.is_some());
        assert_eq!(
            schema.unwrap().get("type").and_then(|t| t.as_str()),
            Some("integer")
        );
    }

    #[test]
    fn ingress_schema_rules_host() {
        let path = YamlPath(vec![
            "spec".to_string(),
            "rules[*]".to_string(),
            "host".to_string(),
        ]);
        let schema = IngressSchemaProvider.schema_for_path(&path);
        assert!(schema.is_some());
    }

    #[test]
    fn default_provider_uses_fallback() {
        let path = YamlPath(vec!["metadata".to_string(), "labels".to_string()]);
        let schema = DefaultK8sSchemaProvider.schema_for_path(&path);
        assert!(schema.is_some());
        assert_eq!(
            schema.unwrap().get("type").and_then(|t| t.as_str()),
            Some("object")
        );
    }

    #[test]
    fn strengthen_leaf_bool() {
        let any_of = serde_json::json!({
            "anyOf": [
                {"type": "boolean"},
                {"type": "string"}
            ]
        });
        let result = strengthen_leaf_schema("metrics.enabled", any_of);
        assert_eq!(result, serde_json::json!({"type": "boolean"}));
    }

    #[test]
    fn strengthen_leaf_integer() {
        let any_of = serde_json::json!({
            "anyOf": [
                {"type": "integer"},
                {"type": "string"}
            ]
        });
        let result = strengthen_leaf_schema("spec.replicas", any_of);
        assert_eq!(result, serde_json::json!({"type": "integer"}));
    }
}
