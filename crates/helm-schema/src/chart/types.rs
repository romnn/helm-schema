use std::collections::BTreeMap;
use vfs::VfsPath;

#[derive(Debug, Clone)]
pub struct ChartContext {
    pub chart_dir: VfsPath,
    pub values_prefix: Vec<String>,
    /// Exact Helm chart namespace, including every dependency alias edge.
    pub template_namespace: String,
    pub is_library: bool,
    /// Exact scalar fields Helm exposes through this chart's `.Chart` root.
    pub static_root_strings: BTreeMap<Vec<String>, String>,
    /// Ancestor-first activation levels: one entry per dependency edge on
    /// the path from the root chart that carries a condition or tags. Helm
    /// renders a nested chart only while EVERY level activates (each
    /// Chart.yaml condition is evaluated against the top-level values), so
    /// a doubly-nested chart like signoz's clickhouse→zookeeper is gated on
    /// `clickhouse.enabled` AND `clickhouse.zookeeper.enabled`.
    pub dependency_activation_chain: Vec<ChartDependencyActivation>,
}

impl ChartContext {
    /// Names an owned template in Helm's global execution namespace.
    ///
    /// Helper-body analysis also uses this name as source identity.
    /// Chart namespaces keep sibling helper files distinct while definitions remain global.
    pub(crate) fn template_name(&self, path: &VfsPath) -> String {
        let relative = path
            .as_str()
            .strip_prefix(self.chart_dir.as_str().trim_end_matches('/'))
            .and_then(|path| path.strip_prefix('/'))
            .unwrap_or(path.as_str());
        format!("{}/{relative}", self.template_namespace)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ChartDependencyActivation {
    pub condition_paths: Vec<String>,
    pub tag_paths: Vec<String>,
}
